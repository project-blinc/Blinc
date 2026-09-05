//! Render context for blinc_app
//!
//! Wraps the GPU rendering pipeline with a clean API.

use blinc_core::{
    Brush, Color, CornerRadius, DrawCommand, DrawContext, DrawContextExt, Rect, Stroke,
};
use blinc_gpu::{
    FontRegistry, GenericFont as GpuGenericFont, GpuGlyph, GpuImage, GpuImageInstance,
    GpuPaintContext, GpuPrimitive, GpuRenderer, ImageRenderingContext, PendingMesh, PrimitiveBatch,
    TextAlignment, TextAnchor, TextRenderingContext,
};
use blinc_layout::div::{FontFamily, FontWeight, GenericFont, TextAlign, TextVerticalAlign};
use blinc_layout::prelude::*;
use blinc_layout::render_state::Overlay;
use blinc_layout::renderer::ElementType;
use blinc_svg::{RasterizedSvg, SvgDocument};
use lru::LruCache;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use crate::error::Result;
use crate::svg_atlas::SvgAtlas;

/// Maximum number of images to keep in cache (prevents unbounded memory growth).
///
/// Sized to comfortably hold the simultaneously-visible image set of typical
/// content-heavy views (galleries, emoji grids, chat backlogs). Going below
/// the visible-set size causes scroll-driven thrashing where currently-visible
/// images are evicted to make room for newly-loaded ones.
const IMAGE_CACHE_CAPACITY: usize = 256;

/// Maximum number of parsed SVG documents to cache
const SVG_CACHE_CAPACITY: usize = 128;

/// Decide whether an image entering the cache should be BC-compressed.
///
/// BC1/BC3's 4×4 block quantization is designed for large
/// photographic content where artifacts hide in detail. It's a
/// visibly bad fit for:
///
/// - **Emoji sprites** (`emoji://...`) — tiny color glyphs with
///   smooth gradients, sharp edges, and feathered alpha. Blocking
///   artifacts produce obvious banding and ringing.
/// - **Small icons / logos** — same failure mode at smaller
///   scale. Anywhere a designer cared about crispness is somewhere
///   BC will look wrong.
///
/// Gate compression behind a minimum dimension floor of 256 px
/// and a hard skip for the `emoji://` scheme. Large photographic
/// content (product shots, avatars at reasonable size, wallpaper)
/// still goes through BC and keeps the VRAM win; small UI sprites
/// stay lossless.
fn bc_eligible(source_uri: &str, width: u32, height: u32) -> bool {
    const MIN_DIM: u32 = 256;
    if source_uri.starts_with("emoji://") {
        return false;
    }
    // BC blocks are 4×4 — wgpu's `Device::create_texture` rejects
    // any texture whose dimensions aren't multiples of 4 for the
    // BC formats. Real photos (camera captures, album art, stock
    // images) frequently land on odd dimensions like 1920×1201.
    // Padding the source buffer + adjusting UVs would reclaim
    // these, but the 2D image pipeline samples `[0,1]` across the
    // texture with no slop — cheapest correct fix is to fall
    // through to Rgba8 when the image isn't block-aligned.
    if width % 4 != 0 || height % 4 != 0 {
        return false;
    }
    width >= MIN_DIM && height >= MIN_DIM
}

/// Intersect two axis-aligned clip rects [x, y, w, h], returning their overlap.
/// Union AABB (`[x, y, w, h]` in screen pixels) of every primitive
/// in `range`, computed from each primitive's `bounds`. Skips zero-
/// size primitives so they don't pull the union to the origin.
/// Returns `None` for empty ranges or ranges of degenerate prims.
fn bounds_union_of_range(
    primitives: &[blinc_gpu::primitives::GpuPrimitive],
    range: &std::ops::Range<usize>,
) -> Option<[f32; 4]> {
    if range.start >= range.end || range.end > primitives.len() {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for p in &primitives[range.start..range.end] {
        let [x, y, w, h] = p.bounds;
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    }
    if min_x.is_finite() && max_x > min_x && max_y > min_y {
        Some([min_x, min_y, max_x - min_x, max_y - min_y])
    } else {
        None
    }
}

/// Union of two optional AABBs. Returns `None` when both are `None`,
/// the populated one when only one is set, and the bounding box of
/// both otherwise.
fn union_aabbs(a: Option<[f32; 4]>, b: Option<[f32; 4]>) -> Option<[f32; 4]> {
    match (a, b) {
        (None, None) => None,
        (Some(r), None) | (None, Some(r)) => Some(r),
        (Some([ax, ay, aw, ah]), Some([bx, by, bw, bh])) => {
            let min_x = ax.min(bx);
            let min_y = ay.min(by);
            let max_x = (ax + aw).max(bx + bw);
            let max_y = (ay + ah).max(by + bh);
            Some([min_x, min_y, max_x - min_x, max_y - min_y])
        }
    }
}

/// Union AABB of a slice of damage rects. Returns `None` for empty
/// or all-degenerate input.
fn damage_union(rects: &[[f32; 4]]) -> Option<[f32; 4]> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for [x, y, w, h] in rects.iter().copied() {
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    }
    if min_x.is_finite() && max_x > min_x && max_y > min_y {
        Some([min_x, min_y, max_x - min_x, max_y - min_y])
    } else {
        None
    }
}

/// Convert a union AABB into a `set_scissor_rect`-ready
/// `(x, y, w, h)` tuple, clamped to the renderer's static layer
/// extent. Returns `None` if the rect is empty after clamping.
fn damage_scissor_from_union(
    rect: Option<[f32; 4]>,
    renderer: &blinc_gpu::GpuRenderer,
) -> Option<(u32, u32, u32, u32)> {
    let [x, y, w, h] = rect?;
    let (lw, lh) = renderer.viewport_size();
    let layer_width = lw as f32;
    let layer_height = lh as f32;
    let scissor_x = x.max(0.0).floor() as u32;
    let scissor_y = y.max(0.0).floor() as u32;
    let scissor_right = (x + w).min(layer_width).ceil() as u32;
    let scissor_bottom = (y + h).min(layer_height).ceil() as u32;
    if scissor_right <= scissor_x || scissor_bottom <= scissor_y {
        return None;
    }
    Some((
        scissor_x,
        scissor_y,
        scissor_right - scissor_x,
        scissor_bottom - scissor_y,
    ))
}

/// `true` when `bounds` (x, y, w, h) intersects any rect in `rects`.
/// Used to filter cached glyphs / SVGs / images down to those that
/// fall inside the damage region before scissored re-dispatch.
fn aabb_intersects_any(bounds: [f32; 4], rects: &[[f32; 4]]) -> bool {
    let [bx, by, bw, bh] = bounds;
    if bw <= 0.0 || bh <= 0.0 {
        return false;
    }
    rects.iter().any(|[rx, ry, rw, rh]| {
        if *rw <= 0.0 || *rh <= 0.0 {
            return false;
        }
        bx + bw > *rx && rx + rw > bx && by + bh > *ry && ry + rh > by
    })
}

fn intersect_clip_rects(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = (a[0] + a[2]).min(b[0] + b[2]);
    let y2 = (a[1] + a[3]).min(b[1] + b[3]);
    [x1, y1, (x2 - x1).max(0.0), (y2 - y1).max(0.0)]
}

/// Merge a new clip rect with an optional existing one via intersection.
fn merge_scroll_clip(new_clip: [f32; 4], existing: Option<[f32; 4]>) -> Option<[f32; 4]> {
    match existing {
        Some(ex) => Some(intersect_clip_rects(new_clip, ex)),
        None => Some(new_clip),
    }
}

/// Compute effective clip for elements that support only a single clip rect (text, SVG).
/// Intersects primary clip and scroll clip so nested scroll containers are respected.
fn effective_single_clip(primary: Option<[f32; 4]>, scroll: Option<[f32; 4]>) -> Option<[f32; 4]> {
    match (primary, scroll) {
        (Some(c), Some(s)) => Some(intersect_clip_rects(c, s)),
        (c, s) => c.or(s),
    }
}

// Rasterized SVG textures are now packed into SvgAtlas (single shared GPU texture)

/// Everything a canvas overlay pass needs to dispatch in one frame.
///
/// Compositor-mode rendering skips the canvas's `render_fn` during
/// the static-cache paint (the walker's `skip_canvas_drawing` flag)
/// and re-invokes the closure into a scratch
/// `GpuPaintContext` each frame inside `collect_canvas_overlay`.
/// A canvas closure can emit several different kinds of draw output,
/// and ALL of them need to reach the GPU for the canvas to render
/// correctly:
///
/// - **`primitives`** — SDF / glass / text primitives, the
///   common case (drawn rounded boxes, gradients, glyphs).
/// - **`foreground_primitives`** — SDF / mesh / text primitives
///   emitted while the canvas has `set_foreground_layer(true)` active.
/// - **`dynamic_images`** — raw-RGBA blits via
///   `ctx.draw_rgba_pixels(...)`. Used by video players (one
///   blit per video frame) and the camera-preview demos.
/// - **`meshes`** — 3D mesh draws via `ctx.draw_mesh_data(...)`.
///   `blinc_canvas_kit::SceneKit3D` and `mesh_3d_demo.rs` go
///   through this path.
///
/// Before this struct existed, the overlay collection only drained
/// `primitives` from the scratch batch; the other channels
/// were dropped on the floor when the scratch context dropped at
/// end-of-frame, so foreground canvas content, video frames, and 3D
/// content never reached the GPU under compositor mode.
#[derive(Default)]
pub struct CanvasOverlay {
    pub primitives: Vec<blinc_gpu::primitives::GpuPrimitive>,
    pub foreground_primitives: Vec<blinc_gpu::primitives::GpuPrimitive>,
    pub dynamic_images: Vec<blinc_gpu::primitives::DynamicImage>,
    pub meshes: Vec<blinc_gpu::PendingMesh>,
    /// User-defined GPU passes scheduled via `DrawContext::run_gpu_pass`
    /// inside canvas closures. Drained from each canvas's scratch paint
    /// context and dispatched alongside `meshes`, after the SDF static
    /// cache has been blitted onto the frame target.
    pub gpu_passes: Vec<blinc_gpu::paint::PendingGpuPass>,
    /// Aux-data emitted by canvas closures (polygon-clip vertices, 3D
    /// group shape descriptors). Concatenated across closures; per-
    /// primitive offsets that referenced the closure's own
    /// `PrimitiveBatch::aux_data` are shifted by the accumulated
    /// length so they index into the merged buffer.
    pub aux_data: Vec<[f32; 4]>,
}

/// Outcome of [`RenderContext::apply_css_deltas`]. Tells the caller
/// which follow-up work the CSS-only fast path needs this frame.
///
/// The `last_css_damage_rects` slot is written before the function
/// returns regardless of variant — callers can read it for both
/// [`Self::Patched`] and [`Self::PatchedWithReemits`]. On
/// [`Self::Bail`] the field is cleared (caller must take the slow
/// path anyway, so damage rects are irrelevant).
#[derive(Debug, Clone)]
pub enum CssDeltaResult {
    /// Every record was patched in place. Caller dispatches the
    /// damaged-cache repaint and the cached text / SVG / image
    /// re-dispatch.
    Patched,
    /// In-scope records were patched, but one or more records carried
    /// an out-of-scope property (width / height / padding / margin /
    /// gap / clip-path geometry / filter blur / backdrop blur). The
    /// listed subtrees still hold stale primitives in
    /// `cached_bg_batch`; the caller must re-emit them via a scratch
    /// walker pass and splice the new primitives back into the cache
    /// before dispatching the damaged-cache repaint.
    PatchedWithReemits { reemit_nodes: Vec<LayoutNodeId> },
    /// The patch path couldn't proceed (cache missing, lock poisoned,
    /// opacity-zero ratio, layer-push-index drift). Caller takes the
    /// full slow path.
    Bail,
}

/// Captures the cached_bg_batch state of one CSS-animated record at
/// the moment `try_reemit_and_splice_css_subtrees` starts. The
/// walker overwrites the live record during the re-walk; we keep
/// these snapshots to (a) position each splice into the right
/// `primitive_range` after the walker has clobbered the live values
/// with scratch-batch indices, and (b) AABB-intersect cached glyphs
/// for the Phase 4 text-bail check.
type ReemitSnapshot = (LayoutNodeId, std::ops::Range<usize>, Option<[f32; 4]>);

/// Internal render context that manages GPU resources and rendering
pub struct RenderContext {
    renderer: GpuRenderer,
    pub(crate) text_ctx: TextRenderingContext,
    image_ctx: ImageRenderingContext,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    sample_count: u32,
    // Single texture for glass backdrop (rendered to and sampled from)
    backdrop_texture: Option<CachedTexture>,
    // Cached MSAA texture for anti-aliased rendering
    msaa_texture: Option<CachedTexture>,
    // LRU cache for images (prevents unbounded memory growth)
    image_cache: LruCache<String, GpuImage>,
    // Tracks when each image first appeared in the cache (for fade-in animation)
    image_load_times: std::collections::HashMap<String, web_time::Instant>,
    /// Damage rectangles populated by the most recent
    /// `apply_binding_deltas` call. Each entry is the union of a
    /// motion-bound subtree's previous on-screen AABB and its new
    /// AABB after delta patching, in screen pixels (post-DPI).
    ///
    /// The compositor v2 damage-rect path reads this after the fast
    /// path runs and re-renders just these regions of the static
    /// cache (with scissor + `LoadOp::Load`) instead of invalidating
    /// the whole layer.
    last_binding_damage_rects: Vec<[f32; 4]>,
    /// Damage rectangles populated by the most recent
    /// `apply_css_deltas` call. One entry per `CssAnimPaintMeta`
    /// whose properties actually changed this frame; each is the
    /// node's `last_screen_aabb` (screen pixels, post-DPI) captured
    /// by the walker. The Phase 4d Opt 2 CSS-damage-rect path
    /// passes these to `render_static_layer_damaged` so the cache
    /// repaint is scissored to just the animated regions instead of
    /// re-rendering the whole layer.
    ///
    /// Visual-only Phase 4c properties (opacity / colour /
    /// corner_radius / shadow / filter / rotate_x / rotate_y) keep
    /// the AABB stable, so the walker's recorded last_screen_aabb
    /// covers both the pre- and post-patch pixel footprints — no
    /// union with a new AABB needed.
    last_css_damage_rects: Vec<[f32; 4]>,
    /// Prepared text glyphs from the most recent full paint, keyed
    /// by `z_index`. Populated by `render_tree_with_motion_opt`'s
    /// body right after the text-shaping loop, drained by the
    /// damage-rect fast path so text inside the damaged region can
    /// be re-dispatched (the scissored clear in
    /// `render_static_layer_damaged` would otherwise wipe those
    /// pixels). Invalidated alongside `cached_texts`.
    cached_glyphs_by_layer: Option<std::collections::BTreeMap<u32, Vec<GpuGlyph>>>,
    /// Same as `cached_glyphs_by_layer` but for the foreground-text
    /// pass — glyphs inside `.foreground()` elements, dispatched
    /// after the rest of the scene.
    cached_fg_glyphs: Option<Vec<GpuGlyph>>,
    /// Cached text *primitives* (PRIM_TEXT GpuPrimitives) for text that
    /// lives inside a motion-bound subtree. Stored as primitives (not
    /// glyphs) so the SDF pipeline can apply `local_affine` — that's
    /// where the motion container's scale / translate is baked in. The
    /// glyph pipeline (`render_text`) doesn't carry per-glyph affine,
    /// so a fade-in animation that scales the bg would leave plain
    /// glyphs at full size while the bg shrank.
    ///
    /// Dispatched via `render_primitives_overlay` onto the surface
    /// *after* `composite_frame`'s overlay pass so they sit on top of
    /// the motion-bound bg primitives (which route through the dynamic
    /// batch / overlay). Without this, text inside motion containers
    /// gets covered by their bg paint.
    cached_motion_subtree_text_prims: Option<Vec<GpuPrimitive>>,
    /// Live-patch records for `cached_motion_subtree_text_prims`:
    /// (prim index range, composite-promoted ancestor stable id). The
    /// glyph prims in the range were baked at BASE alpha (the
    /// ancestor's CSS animation opacity excluded); the dispatch helper
    /// multiplies the live store opacity in per frame so overlay text
    /// fades in lockstep with the composited bg blit, on fast-path
    /// frames included. Cleared together with the pool.
    motion_subtree_text_patch: Vec<(std::ops::Range<usize>, blinc_layout::tree::StableNodeId)>,
    /// Per-promoted-ancestor baseline captured at collect time:
    /// (region screen AABB, translate, scale) that the glyph geometry
    /// was baked with. The dispatch patch re-anchors glyph centers
    /// from this baseline to the live values using the same top-left
    /// math as `blit_tight_texture_to_target`, so text tracks the
    /// panel's translate/scale during the animation.
    #[allow(clippy::type_complexity)]
    composite_patch_baselines: std::collections::HashMap<
        blinc_layout::tree::StableNodeId,
        ([f32; 4], (f32, f32), (f32, f32)),
    >,
    /// SVGs inside motion subtrees — same routing as the text prims
    /// above. Re-dispatched via `render_rasterized_svgs` after the
    /// overlay so the icon lands on top of its motion container's bg.
    cached_motion_subtree_svgs: Option<Vec<SvgElement>>,
    /// Same as `cached_glyphs_by_layer` but for CSS-transformed text
    /// primitives (text with a `transform:` style — routed through
    /// the SDF pipeline, not the glyph pipeline).
    cached_css_transformed_text_prims: Option<Vec<GpuPrimitive>>,
    /// Per-source fade end deadline (`loaded_at + fade_duration_ms`).
    /// Populated only when the element that triggered the load specified
    /// `fade_duration_ms > 0`. Used at the frame boundary to decide
    /// whether the redraw chain should stay alive — once `now >= deadline`
    /// the fade has visually settled and we can stop firing frames.
    ///
    /// Stored separately from `image_load_times` (which is just the
    /// load timestamp consumed by `fade_factor` calc) so existing
    /// fade-factor code paths stay unchanged.
    image_fade_deadlines: std::collections::HashMap<String, web_time::Instant>,
    // LRU cache for parsed SVG documents (avoids re-parsing)
    svg_cache: LruCache<u64, SvgDocument>,
    // Texture atlas for rasterized SVGs (single shared GPU texture, shelf-packed)
    svg_atlas: SvgAtlas,
    // Scratch buffers for per-frame allocations (reused to avoid allocations)
    scratch_glyphs: Vec<GpuGlyph>,
    scratch_texts: Vec<TextElement>,
    scratch_svgs: Vec<SvgElement>,
    scratch_images: Vec<ImageElement>,
    // Current cursor position in physical pixels (for @flow pointer input)
    cursor_pos: [f32; 2],
    // Whether the last render contained @flow shader elements (triggers continuous redraw)
    has_active_flows: bool,
    /// Whether any image emitted during the last render is in the
    /// middle of its load-time fade-in (`fade_factor < 1.0`). Kept
    /// separate from `has_active_flows` because the flows flag is
    /// authoritatively reset to `!flow_elements.is_empty()` at the
    /// end of dispatch — image-fade signals set in the middle of
    /// dispatch get overwritten otherwise. Read by the windowed
    /// runner's redraw-gate via `has_pending_image_fade()` so the
    /// chain keeps firing until every image's fade completes.
    has_pending_image_fade: bool,
    // Frame counter for periodic cache stats logging
    frame_count: u64,
    // Alpha value used when clearing the main render target. 1.0 for
    // opaque windows (the default); 0.0 when the window surface is
    // configured for transparent composition. Set via
    // [`Self::set_clear_alpha`] before each window's render — the
    // desktop runner updates it per-window so a mix of opaque and
    // transparent windows can share the same RenderContext.
    clear_alpha: f32,
    // Cached `PrimitiveBatch` from the most recent full Phase 4
    // paint. Lives across frames so the compositor-path fast Phase 4
    // can patch primitives in place (using
    // `RenderTree::composite_bindings` ranges) and re-render without
    // re-walking the tree.
    //
    // Invalidated (set to `None`) any time the next frame can't
    // safely reuse the cache — tree rebuild, layout change, CSS state
    // change, stylesheet reparse, scroll offset change. The fast
    // path is opt-in per frame: callers must check
    // `cached_bg_batch.is_some()` AND that no invalidator fired this
    // frame before consuming.
    //
    // Memory: roughly `bytes_per_primitive * primitive_count`. For
    // cn_demo (~400 primitives at ~256 bytes each = 100 KB), this
    // is a small fixed cost in exchange for skipping a 1 ms paint
    // walker + 1 ms collect every frame.
    cached_bg_batch: Option<blinc_gpu::PrimitiveBatch>,
    /// Compositor v2 dynamic batch — primitives emitted inside
    /// motion-bound subtrees, separated by the walker via
    /// `push_motion_subtree`/`pop_motion_subtree`. Stays out of the
    /// static cache so motion-binding animations don't have to
    /// invalidate it; dispatched per-frame as an overlay after the
    /// cache blit. `apply_binding_deltas` patches its primitives in
    /// place, so subsequent frames show the new positions without
    /// re-running the walker.
    cached_dynamic_batch: Option<blinc_gpu::PrimitiveBatch>,
    /// True while `try_render_with_compositor` is running its inner
    /// `render_tree_with_motion_opt(target=static_view, target_texture=None)`
    /// step. The inner step paints the static cache; the compositor
    /// then drives `composite_frame` separately to overlay the
    /// `cached_dynamic_batch` onto the surface. Without this flag the
    /// inner step also dispatched `cached_dynamic_batch` onto the
    /// static-layer texture, so the static cache ended up containing
    /// the motion-bound primitives at the slow-paint rotation —
    /// subsequent frames blitted that into the surface AND
    /// composite_frame painted them again at the current rotation,
    /// producing two visible motion-bound primitives (the cn::spinner
    /// double-arc symptom). The non-compositor path needs the
    /// dispatch; the compositor path doesn't.
    in_compositor_inner_call: bool,
    /// Per-node `LayerTexture` cache for composite-promoted CSS
    /// subtrees. The walker peels each promoted subtree's primitives
    /// into a per-node scratch batch (see
    /// `GpuPaintContext::push_composite_layer`); at end of paint we
    /// rasterize each scratch batch into a tight texture and stash
    /// it here keyed by the node's slotmap key
    /// (`LayoutNodeId::data().as_ffi()`).
    ///
    /// On animation frames the per-frame composite reads these
    /// textures by key (looked up via the matching `DynamicRegion`)
    /// and blits each with the active CSS animation transform
    /// applied — no walker re-entry. Textures stay resident across
    /// frames until the subtree's intrinsic content changes (Phase
    /// 5 dirty tracking) or the node demotes out of the dynamic
    /// set.
    /// The `(u32, u32)` is the ALLOCATED texture size — bucket-rounded
    /// up from the natural content size by `render_subtree_to_layer_texture`
    /// (64-px increments for atlas reuse). The blit's `source_size`
    /// argument must be this allocated size, NOT the natural content
    /// size, because the layer-composite shader's `source_rect` is in
    /// normalized 0–1 UVs of the ALLOCATED texture. Passing natural
    /// size collapses the bucket padding into the dest rect — a 16-px
    /// indicator in a 64-px bucket renders at 25% height (the visible
    /// thin-bar artefact users hit when the dest happens to land at a
    /// dimension below the bucket granularity).
    css_composited_textures:
        std::collections::HashMap<u64, (blinc_gpu::renderer::LayerTexture, (u32, u32))>,
    /// Hash of the scratch `PrimitiveBatch` that produced the texture
    /// currently in [`Self::css_composited_textures`] for each node.
    /// On every slow-path frame the walker re-emits primitives into a
    /// fresh scratch batch; comparing the new hash against the cached
    /// one tells us whether the subtree's intrinsic content actually
    /// changed. When it hasn't — the steady-state case for a pulse /
    /// glow node whose only animation is composite-promotable opacity
    /// — we skip the GPU rasterize and reuse the existing texture.
    /// Saves ~render_layer_range_tight per promoted region per slow-
    /// path frame; on cn_demo with a single pulse on screen and a
    /// non-promotable sibling driving the slow path that's ~200 µs
    /// GPU time per frame.
    css_composited_scratch_hash: std::collections::HashMap<u64, u64>,
    /// Per-node `LayerTexture` cache for motion-bound subtrees that the
    /// P4.1 predicate promoted into the `subtree_texture_candidates`
    /// set. Mirror of [`Self::css_composited_textures`] for the motion
    /// path — the walker peels each promoted motion-bound subtree's
    /// primitives into a per-node scratch batch (the walker pushes the
    /// composite layer at the same site that the CSS path uses), and
    /// the end-of-paint bake hook below rasterizes each batch into a
    /// tight texture and stashes it here keyed by the node's slotmap
    /// key.
    ///
    /// On motion-driven animation frames the per-frame motion overlay
    /// (`composite_motion_layers_overlay`) blits each texture with the
    /// current `MotionBindings` (translate / scale / rotation /
    /// opacity) applied at composite time — no walker re-entry, no
    /// SDF re-emission. Textures stay resident across frames until the
    /// subtree's intrinsic content changes or the node demotes out of
    /// the bake registry (see
    /// `RenderTree::demote_lapsed_motion_bake_records`).
    motion_subtree_textures:
        std::collections::HashMap<u64, (blinc_gpu::renderer::LayerTexture, (u32, u32))>,
    /// Hash of the scratch `PrimitiveBatch` that produced the texture
    /// currently in [`Self::motion_subtree_textures`] for each node.
    /// Same dirty-skip mechanism the CSS path uses: on every paint the
    /// walker re-emits the subtree's intrinsic content into a fresh
    /// scratch batch; if the hash matches we skip the GPU rasterize
    /// and reuse the existing texture. Steady-state motion frames — a
    /// `cn::switch` thumb at a stable colour, a toast at its base
    /// background — pay zero raster cost per frame; only the blit
    /// runs.
    motion_subtree_scratch_hash: std::collections::HashMap<u64, u64>,
    // Collected text / SVG / image elements from the most recent
    // full paint. Lives alongside `cached_bg_batch` so the
    // compositor fast path can skip `collect_render_elements_with_state`
    // (a 0.8–1.0 ms pass on cn_demo). On a successful fast path the
    // cached vecs are cloned into the per-frame scratch and used
    // exactly as if the collector had just run — translate-delta
    // patching of text/SVG positions is folded into the same
    // `apply_binding_deltas` helper that patches GPU primitives.
    //
    // Cloning costs ~bytes-per-element × element-count per frame:
    // ~150 B × N for text, less for SVG / image. cn_demo's progress
    // section has <30 elements total, so the clone is well under
    // 10 µs.
    //
    // `Some` only when [`Self::cached_bg_batch`] is also `Some` —
    // they're populated and invalidated together.
    cached_texts: Option<Vec<TextElement>>,
    cached_svgs: Option<Vec<SvgElement>>,
    cached_images: Option<Vec<ImageElement>>,
    cached_flows: Option<Vec<FlowElement>>,
}

struct CachedTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

/// Info about a 3D-transformed ancestor layer. When text/SVGs/images are inside a parent
/// with `perspective` + `rotate-x`/`rotate-y`, this info is used to render them to an
/// offscreen texture and blit with the same perspective transform.
#[derive(Clone, Debug)]
struct Transform3DLayerInfo {
    /// Node ID of the 3D-transformed ancestor (used as layer grouping key)
    node_id: LayoutNodeId,
    /// Screen-space bounds of the 3D layer [x, y, w, h] (DPI-scaled)
    layer_bounds: [f32; 4],
    /// Perspective transform parameters
    transform_3d: blinc_core::Transform3DParams,
    /// Layer opacity
    opacity: f32,
}

/// Text element data for rendering
#[derive(Clone)]
struct TextElement {
    content: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    font_size: f32,
    color: [f32; 4],
    align: TextAlign,
    weight: FontWeight,
    /// Whether to use italic style
    italic: bool,
    /// Vertical alignment within bounding box
    v_align: TextVerticalAlign,
    /// Clip bounds from parent scroll container (x, y, width, height)
    clip_bounds: Option<[f32; 4]>,
    /// Motion opacity inherited from parent motion container
    motion_opacity: f32,
    /// Whether to wrap text at container bounds
    wrap: bool,
    /// Line height multiplier
    line_height: f32,
    /// Measured width (before layout constraints) - used to determine if wrap is needed
    measured_width: f32,
    /// Font family category
    font_family: FontFamily,
    /// Word spacing in pixels (0.0 = normal)
    word_spacing: f32,
    /// Letter spacing in pixels (0.0 = normal)
    letter_spacing: f32,
    /// Z-index for rendering order (higher = on top)
    z_index: u32,
    /// Font ascender in pixels (distance from baseline to top)
    ascender: f32,
    /// Whether text has strikethrough decoration
    strikethrough: bool,
    /// Whether text has underline decoration
    underline: bool,
    /// CSS text-decoration-color override (RGBA)
    decoration_color: Option<[f32; 4]>,
    /// CSS text-decoration-thickness override in pixels
    decoration_thickness: Option<f32>,
    /// Inherited CSS transform from ancestor elements (full 6-element affine in layout coords)
    /// [a, b, c, d, tx, ty] where new_x = a*x + c*y + tx, new_y = b*x + d*y + ty
    css_affine: Option<[f32; 6]>,
    /// Text shadow (offset_x, offset_y, blur, color) from CSS text-shadow property
    text_shadow: Option<blinc_core::Shadow>,
    /// 3D layer info if this text is inside a perspective-transformed parent
    transform_3d_layer: Option<Transform3DLayerInfo>,
    /// Whether this text is inside a foreground-layer element (rendered after foreground primitives)
    is_foreground: bool,
    /// Whether this text sits inside a motion-bound subtree (one that
    /// has `motion_bindings` or a motion FSM config on any ancestor).
    /// Motion-subtree primitives are routed to the dynamic batch and
    /// dispatched as an overlay on top of the static cache, so text
    /// rendered into the static cache would otherwise be covered by
    /// the overlay's bg paint. Texts with this flag are deferred from
    /// the static-cache text pass and re-dispatched after the overlay
    /// in `composite_frame`'s flow so they land on top of the motion
    /// bg primitives. Same routing principle as foreground text but
    /// triggered by ancestor motion instead of a foreground layer.
    in_motion_subtree: bool,
    /// Nearest composite-promoted ancestor (CSS-animated subtree baked
    /// to a layer texture and blitted per frame). When `Some`, the
    /// composite animation's opacity is NOT baked into
    /// `motion_opacity` at collect — the dispatch-time patch in
    /// `dispatch_motion_subtree_text_overlay` multiplies the LIVE
    /// store opacity into the glyph alpha instead, so text fades in
    /// lockstep with the blitted bg on every frame (fast or slow).
    composite_ancestor: Option<blinc_layout::tree::StableNodeId>,
}

/// Image element data for rendering
#[derive(Clone)]
struct ImageElement {
    source: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    object_fit: u8,
    object_position: [f32; 2],
    opacity: f32,
    border_radius: f32,
    tint: [f32; 4],
    /// Clip bounds from parent (x, y, width, height)
    clip_bounds: Option<[f32; 4]>,
    /// Clip corner radii (tl, tr, br, bl)
    clip_radius: [f32; 4],
    /// Which layer this image renders in
    layer: RenderLayer,
    /// Loading strategy: 0 = Eager (load immediately), 1 = Lazy (load when visible)
    loading_strategy: u8,
    /// Placeholder type: 0 = None, 1 = Color, 2 = Image, 3 = Skeleton
    placeholder_type: u8,
    /// Placeholder color [r, g, b, a]
    placeholder_color: [f32; 4],
    /// Placeholder image source (only used when placeholder_type == 2)
    placeholder_image: Option<String>,
    /// Fade-in duration in milliseconds (0 = no fade)
    fade_duration_ms: u32,
    /// Z-layer index for interleaved rendering with primitives
    z_index: u32,
    /// Border width (0 = no border)
    border_width: f32,
    /// Border color
    border_color: blinc_core::Color,
    /// CSS transform as 6-element affine [a, b, c, d, tx, ty] (None = no transform)
    css_affine: Option<[f32; 6]>,
    /// Drop shadow from CSS
    shadow: Option<blinc_core::Shadow>,
    /// CSS filter A (grayscale, invert, sepia, hue_rotate_rad) — identity = `[0,0,0,0]`
    filter_a: [f32; 4],
    /// CSS filter B (brightness, contrast, saturate, unused) — identity = `[1,1,1,0]`
    filter_b: [f32; 4],
    /// Secondary clip (scroll container boundary) — sharp rect, no radius.
    /// Kept separate from primary clip_bounds so rounded corners don't morph
    /// when the primary clip rect shrinks at scroll boundaries.
    scroll_clip: Option<[f32; 4]>,
    /// Mask gradient params: linear=(x1,y1,x2,y2), radial=(cx,cy,r,0) in OBB space
    mask_params: [f32; 4],
    /// Mask info: [mask_type, start_alpha, end_alpha, 0] (0=none, 1=linear, 2=radial)
    mask_info: [f32; 4],
    /// 3D layer info if this image is inside a perspective-transformed parent
    transform_3d_layer: Option<Transform3DLayerInfo>,
}

/// SVG element data for rendering
#[derive(Clone)]
struct SvgElement {
    source: Arc<str>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// Tint color to apply to SVG fill/stroke (from CSS `color`)
    tint: Option<blinc_core::Color>,
    /// CSS `fill` override for SVG
    fill: Option<blinc_core::Color>,
    /// CSS `stroke` override for SVG
    stroke: Option<blinc_core::Color>,
    /// CSS `stroke-width` override for SVG
    stroke_width: Option<f32>,
    /// CSS `stroke-dasharray` pattern for SVG
    stroke_dasharray: Option<Vec<f32>>,
    /// CSS `stroke-dashoffset` for SVG
    stroke_dashoffset: Option<f32>,
    /// SVG path `d` attribute data (for path morphing)
    svg_path_data: Option<String>,
    /// Clip bounds from parent scroll container (x, y, width, height)
    clip_bounds: Option<[f32; 4]>,
    /// Motion opacity inherited from parent motion container
    motion_opacity: f32,
    /// Inherited CSS transform from ancestor elements (full 6-element affine in layout coords)
    /// [a, b, c, d, tx, ty] where new_x = a*x + c*y + tx, new_y = b*x + d*y + ty
    css_affine: Option<[f32; 6]>,
    /// Per-SVG-tag style overrides from CSS tag-name selectors (e.g., `path { fill: red; }`)
    tag_overrides: std::collections::HashMap<String, blinc_layout::element::SvgTagStyle>,
    /// 3D layer info if this SVG is inside a perspective-transformed parent
    transform_3d_layer: Option<Transform3DLayerInfo>,
    /// Whether this SVG sits inside a motion-bound subtree. Same
    /// routing principle as `TextElement.in_motion_subtree`: deferred
    /// from the static-cache SVG pass and re-dispatched after the
    /// motion overlay so the SVG icon lands on top of its motion-bound
    /// container's bg paint instead of being covered.
    in_motion_subtree: bool,
    /// Stack-layer z used to interleave with primitives + text. Higher
    /// values render on top. Captured from the walker's `z_layer`
    /// counter — bumped by `stack_layer()` (overlay layers, toast
    /// tray, foreground content) and by explicit `z_index`. Without
    /// this, icons inside an overlay subtree would all collapse to
    /// z=0 and render *under* the dialog/sheet/drawer card their
    /// container z-layered above the main UI.
    z_layer: u32,
    /// Nearest composite-promoted ancestor — same live-patch contract
    /// as `TextElement.composite_ancestor`: the composite animation's
    /// opacity is excluded from `motion_opacity` at collect and
    /// applied from the live store at dispatch.
    composite_ancestor: Option<blinc_layout::tree::StableNodeId>,
}

/// Flow shader element — an element with `flow: <name>` that renders via a custom GPU pipeline
#[derive(Clone)]
struct FlowElement {
    /// Name referencing a @flow DAG in the stylesheet
    flow_name: String,
    /// Direct FlowGraph (from `flow!` macro), bypasses stylesheet lookup
    flow_graph: Option<std::sync::Arc<blinc_core::FlowGraph>>,
    /// Bounds in physical pixels (DPI-scaled)
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// Z-layer for rendering order
    z_index: u32,
    /// Corner radius in physical pixels
    corner_radius: f32,
}

/// Debug bounds element for layout visualization
#[derive(Clone)]
struct DebugBoundsElement {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// Element type name for labeling
    element_type: String,
    /// Depth in the tree (for color coding)
    depth: u32,
}

impl RenderContext {
    /// Create a new render context
    pub(crate) fn new(
        renderer: GpuRenderer,
        text_ctx: TextRenderingContext,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        sample_count: u32,
    ) -> Self {
        let image_ctx = ImageRenderingContext::new(device.clone(), queue.clone());
        let svg_atlas = SvgAtlas::new(&device);
        Self {
            renderer,
            text_ctx,
            image_ctx,
            device,
            queue,
            sample_count,
            backdrop_texture: None,
            msaa_texture: None,
            image_cache: LruCache::new(NonZeroUsize::new(IMAGE_CACHE_CAPACITY).unwrap()),
            image_load_times: std::collections::HashMap::new(),
            image_fade_deadlines: std::collections::HashMap::new(),
            last_binding_damage_rects: Vec::new(),
            last_css_damage_rects: Vec::new(),
            cached_glyphs_by_layer: None,
            cached_fg_glyphs: None,
            cached_motion_subtree_text_prims: None,
            motion_subtree_text_patch: Vec::new(),
            composite_patch_baselines: std::collections::HashMap::new(),
            cached_motion_subtree_svgs: None,
            cached_css_transformed_text_prims: None,
            svg_cache: LruCache::new(NonZeroUsize::new(SVG_CACHE_CAPACITY).unwrap()),
            svg_atlas,
            scratch_glyphs: Vec::with_capacity(1024), // Pre-allocate for typical text
            scratch_texts: Vec::with_capacity(64),    // Pre-allocate for text elements
            scratch_svgs: Vec::with_capacity(32),     // Pre-allocate for SVG elements
            scratch_images: Vec::with_capacity(32),   // Pre-allocate for image elements
            cursor_pos: [0.0; 2],
            has_active_flows: false,
            has_pending_image_fade: false,
            frame_count: 0,
            clear_alpha: 1.0,
            cached_bg_batch: None,
            cached_dynamic_batch: None,
            in_compositor_inner_call: false,
            css_composited_textures: std::collections::HashMap::new(),
            css_composited_scratch_hash: std::collections::HashMap::new(),
            motion_subtree_textures: std::collections::HashMap::new(),
            motion_subtree_scratch_hash: std::collections::HashMap::new(),
            cached_texts: None,
            cached_svgs: None,
            cached_images: None,
            cached_flows: None,
        }
    }

    /// Set the alpha component used when clearing the main render target.
    ///
    /// 1.0 (default) gives opaque clears — correct for regular windows
    /// and any surface configured with `CompositeAlphaMode::Opaque`.
    /// 0.0 is used for windows whose surface was created with a
    /// premultiplied/postmultiplied alpha mode so the OS compositor
    /// can see through to whatever is behind the window.
    pub fn set_clear_alpha(&mut self, alpha: f32) {
        self.clear_alpha = alpha;
    }

    /// Drop the cached primitive batch + companion element lists. Called
    /// by the windowed runner any time the next frame can't safely
    /// reuse the cache — tree rebuild, layout recompute, CSS state
    /// transition, stylesheet reparse, scroll offset change, scale
    /// factor change, surface resize. The next paint will run the
    /// full walker and repopulate the cache.
    ///
    /// Safe to call even when no cache is set (clears the companion
    /// vecs unconditionally for symmetry).
    pub fn invalidate_render_cache(&mut self) {
        self.invalidate_render_cache_tagged("unspecified");
    }

    pub fn invalidate_render_cache_tagged(&mut self, source: &'static str) {
        if self.cached_bg_batch.is_some() {
            tracing::trace!(
                target: "blinc_app::frame_timing",
                source,
                "invalidate_render_cache",
            );
        }
        self.cached_bg_batch = None;
        self.cached_dynamic_batch = None;
        self.cached_texts = None;
        self.cached_svgs = None;
        self.cached_images = None;
        self.cached_flows = None;
        self.cached_glyphs_by_layer = None;
        self.cached_fg_glyphs = None;
        self.cached_motion_subtree_text_prims = None;
        self.motion_subtree_text_patch.clear();
        self.cached_motion_subtree_svgs = None;
        self.cached_css_transformed_text_prims = None;
        // Compositor static cache rides on the same lifecycle as the
        // primitive-batch cache — anything that would invalidate the
        // batch (rebuild, layout change, scroll, hover state change,
        // CSS state change) also makes the cached texture stale.
        self.renderer.invalidate_static_layer();
        // NOTE: `css_composited_textures` / `motion_subtree_textures`
        // are intentionally NOT drained here. The per-key dirty-skip
        // at the bake site (see `hash_composite_scratch`) already
        // detects changed primitives / layer-command opacity per
        // node, so an animating subtree re-bakes via hash mismatch.
        // An earlier draft drained both maps on every invalidation
        // to fix a stale-bake repro; that tanked steady-state perf
        // by tearing down every live composite/motion texture in
        // the scene on every hover flip, pointer move with handlers,
        // scroll tick, and stateful refresh. If a stale-bake bug
        // ever resurfaces, fix it at the hash function or at the
        // specific node's bake key — never via a tree-wide drain.
    }

    /// Drop the cached `LayerTexture` AND its scratch hash for each
    /// listed `composite_promotion` / `motion_subtree_texture` key.
    /// Forces the next paint to re-bake the listed subtrees regardless
    /// of whether the walker emits byte-identical primitives.
    ///
    /// **Why this exists, separate from `invalidate_render_cache_tagged`:**
    /// the per-key `hash_composite_scratch` dirty-skip locks in the
    /// FIRST bake's texture and refuses to re-bake while the walker
    /// emits the same primitives every frame. For a freshly-mounted
    /// overlay (cn::context_menu / cn::dropdown / cn::popover etc.) on
    /// a canvas-bearing page, the first bake happens INSIDE a frame
    /// where the canvas overlay has already overwritten the GPU
    /// `aux_data` buffer, and the overlay-stack's aux re-upload runs
    /// AFTER the bake. So the texture captures the menu's SDF primitives
    /// bound to whatever stale aux the canvas left behind — the menu
    /// renders garbage / invisible / wrong colour until something
    /// else (mouse motion → hover state change → cache invalidate)
    /// trips a different invalidation path that finally drops the
    /// texture so the next paint can re-bake against fresh aux.
    ///
    /// The fix: hook the structural-subtree-rebuild path
    /// (windowed.rs:5612-ish) and, immediately after the rebuild
    /// installs the new overlay subtree, drain the affected layer
    /// textures so the next paint re-bakes against the now-current
    /// GPU state.
    ///
    /// Steady-state animations (an existing menu animating each
    /// frame) DO NOT pass through this call site — only freshly-mounted
    /// subtrees do, so the per-frame perf cost is zero.
    pub fn invalidate_overlay_mount_textures(&mut self, keys: &[u64]) {
        if keys.is_empty() {
            return;
        }
        for key in keys {
            if let Some((old, _)) = self.css_composited_textures.remove(key) {
                self.renderer.layer_texture_cache_mut().release(old);
            }
            self.css_composited_scratch_hash.remove(key);
            if let Some((old, _)) = self.motion_subtree_textures.remove(key) {
                self.renderer.layer_texture_cache_mut().release(old);
            }
            self.motion_subtree_scratch_hash.remove(key);
        }
        tracing::trace!(
            target: "blinc_app::frame_timing",
            count = keys.len(),
            "invalidate_overlay_mount_textures",
        );
    }

    /// Drain every cached `css_composited_textures` / `motion_subtree_textures`
    /// entry and release its `LayerTexture` back to the renderer's pool.
    /// Use when EVERY texture is provably stale — a viewport pan/zoom on
    /// a canvas-bearing page, for instance: each texture's content was
    /// rasterised against a content-space affine that no longer applies,
    /// and the per-key hash-mismatch heuristic doesn't detect viewport
    /// changes (the bake's input primitives can be byte-identical when
    /// the canvas closure's geometry is content-space). Don't reach for
    /// this on routine state changes — it tears down every live
    /// composite / motion texture, which is the exact perf cost the
    /// surgical `invalidate_render_cache_tagged` path is designed to
    /// avoid. The web runner calls this on scroll because viewport
    /// changes there don't otherwise route through any signal that
    /// would re-bake the textures.
    pub fn drain_all_layer_textures(&mut self, source: &'static str) {
        if self.css_composited_textures.is_empty() && self.motion_subtree_textures.is_empty() {
            return;
        }
        let css_count = self.css_composited_textures.len();
        let motion_count = self.motion_subtree_textures.len();
        for (_, (tex, _)) in self.css_composited_textures.drain() {
            self.renderer.layer_texture_cache_mut().release(tex);
        }
        self.css_composited_scratch_hash.clear();
        for (_, (tex, _)) in self.motion_subtree_textures.drain() {
            self.renderer.layer_texture_cache_mut().release(tex);
        }
        self.motion_subtree_scratch_hash.clear();
        tracing::trace!(
            target: "blinc_app::frame_timing",
            source,
            css_count,
            motion_count,
            "drain_all_layer_textures",
        );
    }

    /// Whether the renderer has a usable cached batch from the most
    /// recent full paint. The Phase-4 fast path checks this before
    /// attempting the compositor route — if `false` (first paint
    /// after rebuild, after invalidate, etc.) the runner must take
    /// the full path.
    pub fn has_render_cache(&self) -> bool {
        self.cached_bg_batch.is_some()
    }

    /// Apply motion-binding deltas to the cached primitive batch in
    /// place. Mirrors what the paint walker would have re-emitted if
    /// it ran this frame, but in O(bound-nodes × primitives-per-node)
    /// instead of O(whole tree). The caller drives the fast Phase-4
    /// path:
    ///
    ///   1. The runner detects that only motion bindings changed
    ///      this frame (no rebuild / no layout / etc.) and that
    ///      [`Self::has_render_cache`] returns `true`.
    ///   2. The runner calls `apply_binding_deltas(tree, scale)`.
    ///   3. On `Ok(())`, the runner re-renders from the patched
    ///      cached batch (currently still through the full GPU
    ///      pipeline — see follow-ups for partial uploads / command
    ///      list reuse).
    ///   4. On `Err(())`, the runner falls back to the full paint
    ///      path. Returning `Err` is conservative: any binding that
    ///      changed a property the patcher doesn't handle yet
    ///      (scale, rotation, anything affecting bounds of children
    ///      with their own CSS transforms) routes to the full path.
    ///
    /// Properties handled today: `translate_x` / `translate_y`
    /// (delta-applied to `bounds.xy`) and `opacity` (ratio-applied
    /// to every `color.a` / `border_color.a` / `shadow_color.a` in
    /// the binding's primitive range). Scale / rotation are
    /// deferred — they need `local_affine` updates plus
    /// bounds-around-centre recomputation that interacts with
    /// existing CSS transforms in non-trivial ways.
    ///
    /// `scale` is the DPI scale factor — needed because
    /// `last_translate` is recorded in logical pixels (matches the
    /// motion-binding API) but `bounds.xy` is in physical pixels.
    /// The function multiplies the delta by `scale` before
    /// applying.
    ///
    /// After a successful patch, the function updates each
    /// binding's `last_translate` / `last_opacity` on the
    /// `composite_bindings` map so the next frame's delta is
    /// computed against the value just written to the GPU, not the
    /// original paint-time value (otherwise multiple fast-path
    /// Re-invoke every recorded canvas's `render_fn` and splice the
    /// resulting primitives back into the cached `bg_batch` at the
    /// range the walker recorded for that canvas.
    ///
    /// Lets the compositor fast path stay engaged when there's an
    /// in-viewport canvas: only the ~tens of primitives the canvas
    /// emits get refreshed, the surrounding ~thousand primitives in
    /// the cache stay untouched, and the full paint walker doesn't
    /// run. On `cn_demo` (3 spinners on screen, ~1000 primitives in
    /// the tree) this is the difference between ~46 % CPU and a
    /// fraction of that.
    ///
    /// Bails (returns `false`, caller falls back to full paint) when:
    ///   - There's no cached batch.
    ///   - A canvas emits a different primitive count this frame than
    ///     it did on the last full paint. A count change shifts every
    ///     subsequent range stored in `composite_bindings` /
    ///     `canvas_paint_records`, and rebuilding that bookkeeping
    ///     correctly is more work than re-walking the tree.
    ///
    /// Returns `true` when every canvas replayed successfully OR
    /// there were no canvases to redraw (no-op fast-path).
    pub fn redraw_canvases(
        &mut self,
        tree: &blinc_layout::RenderTree,
        width: u32,
        height: u32,
    ) -> bool {
        use blinc_core::{DrawContext, Rect, Transform, layer::Affine2D};
        use blinc_gpu::GpuPaintContext;

        let batch = match self.cached_bg_batch.as_mut() {
            Some(b) => b,
            None => return false,
        };

        let records_ref = tree.canvas_paint_records();
        if records_ref.is_empty() {
            return true;
        }
        // Clone records out of the RefCell so we don't hold the
        // borrow across the canvas closure invocation (closures can
        // reach back into the scheduler / state and we don't want
        // to deadlock the renderer's bookkeeping).
        let mut sorted: Vec<blinc_layout::renderer::CanvasPaintRecord> =
            records_ref.values().cloned().collect();
        drop(records_ref);
        sorted.sort_by_key(|r| r.primitive_range.start);

        for record in &sorted {
            // Scratch context. Width / height match the real frame
            // so any viewport-dependent calculations inside fill_*
            // (currently none, but defensively correct) get the
            // right reference frame.
            let mut scratch = GpuPaintContext::new(width as f32, height as f32);

            // Replay the transform stack. `push_transform` composes
            // with `Affine2D::IDENTITY` from the fresh stack, so
            // pushing the saved affine reproduces the exact affine
            // the walker had at canvas paint time.
            scratch.push_transform(Transform::Affine2D(Affine2D {
                elements: record.affine,
            }));
            scratch.push_opacity(record.opacity);
            scratch.set_z_layer(record.z_layer);

            let local_clip_rect = if record.clips_content {
                let clip_rect = Rect::new(0.0, 0.0, record.bounds_wh.0, record.bounds_wh.1);
                scratch.push_clip(blinc_core::layer::ClipShape::rect(clip_rect));
                Some(clip_rect)
            } else {
                None
            };
            let _ = local_clip_rect;

            let canvas_bounds = blinc_layout::canvas::CanvasBounds {
                x: 0.0,
                y: 0.0,
                width: record.bounds_wh.0,
                height: record.bounds_wh.1,
            };
            (record.render_fn)(&mut scratch as &mut dyn DrawContext, canvas_bounds);

            if record.clips_content {
                scratch.pop_clip();
            }

            // Capture the freshly emitted primitives. The scratch
            // context's batch only contains what `render_fn` just
            // emitted (we pushed nothing else into it that produces
            // primitives) so we can splice the whole `primitives`
            // vec back into the cached batch.
            let new_batch = scratch.take_batch();
            let new_count = new_batch.primitives.len();
            let old_count = record.primitive_range.end - record.primitive_range.start;
            if new_count != old_count {
                // Splice would shift every subsequent range — bail.
                return false;
            }
            // Defensive bounds check: a tree rebuild that landed
            // between the original record + this fast-path replay
            // can leave the stored range pointing past the current
            // batch's primitive count. Without this guard the
            // indexed write below panicked with `index out of
            // bounds` (observed when an inline overlay popped open
            // mid-frame: the overlay-stack growth shrank the
            // surrounding subtree's primitive set, invalidating
            // every later canvas record's range). Bail so the
            // caller re-records from scratch instead.
            if record.primitive_range.end > batch.primitives.len() {
                return false;
            }
            for (i, p) in new_batch.primitives.into_iter().enumerate() {
                batch.primitives[record.primitive_range.start + i] = p;
            }
        }

        true
    }

    /// Re-invoke every recorded canvas closure into a fresh scratch
    /// context and return the union of their emitted primitives.
    ///
    /// Used by the layer compositor as the dynamic overlay batch:
    /// every frame, the static texture (containing the
    /// canvas-skipped paint) is blitted onto the surface, and the
    /// fresh `Vec<GpuPrimitive>` this returns is then dispatched on
    /// top with `LoadOp::Load` — so the canvas content animates at
    /// vsync while the surrounding tree never re-renders.
    pub fn collect_canvas_overlay_primitives(
        &mut self,
        tree: &blinc_layout::RenderTree,
        width: u32,
        height: u32,
    ) -> Vec<blinc_gpu::primitives::GpuPrimitive> {
        self.collect_canvas_overlay(tree, width, height).primitives
    }

    /// Full canvas overlay collection: re-invokes every recorded
    /// canvas's `render_fn` into a scratch context and returns
    /// every kind of draw output the closure produced:
    ///
    /// - `primitives`: SDF / mesh / text primitives (the main batch)
    /// - `foreground_primitives`: SDF / mesh / text primitives emitted
    ///   while `set_foreground_layer(true)` is active
    /// - `dynamic_images`: raw-RGBA blits like video frames
    ///   (`ctx.draw_rgba_pixels(...)`)
    /// - `meshes`: 3D mesh draws (`ctx.draw_mesh_data(...)`) from
    ///   `blinc_canvas_kit::SceneKit3D` or direct API users
    ///
    /// The legacy `collect_canvas_overlay_primitives` is preserved
    /// as a thin shim that returns only `.primitives` — callers
    /// that need the full overlay state (video, 3D) should call
    /// this method instead.
    ///
    /// Canvas paint records are sorted by `z_layer` before
    /// invocation so canvases at higher z-indexes render on top
    /// of canvases at lower z-indexes within the overlay batch.
    pub fn collect_canvas_overlay(
        &mut self,
        tree: &blinc_layout::RenderTree,
        width: u32,
        height: u32,
    ) -> CanvasOverlay {
        let records_ref = tree.canvas_paint_records();
        let records: Vec<blinc_layout::renderer::CanvasPaintRecord> =
            records_ref.values().cloned().collect();
        drop(records_ref);
        self.collect_canvas_overlay_from(records, width, height)
    }

    /// Sibling collector for canvases nested inside an
    /// `is_overlay_root` ancestor. Returns the per-frame primitives
    /// the renderer dispatches AFTER `dispatch_dynamic_batch_overlay`
    /// so a caret canvas inside a focused cn::input stays painted
    /// above its host popover's bg/border (which lives in the
    /// dyn-batch overlay).
    pub fn collect_overlay_canvas_overlay(
        &mut self,
        tree: &blinc_layout::RenderTree,
        width: u32,
        height: u32,
    ) -> CanvasOverlay {
        let records_ref = tree.overlay_canvas_paint_records();
        let records: Vec<blinc_layout::renderer::CanvasPaintRecord> =
            records_ref.values().cloned().collect();
        drop(records_ref);
        self.collect_canvas_overlay_from(records, width, height)
    }

    /// Shared body of [`Self::collect_canvas_overlay`] and
    /// [`Self::collect_overlay_canvas_overlay`]. Takes an already-
    /// extracted set of records so each pool can be drained
    /// independently without re-walking the tree.
    fn collect_canvas_overlay_from(
        &mut self,
        mut records: Vec<blinc_layout::renderer::CanvasPaintRecord>,
        width: u32,
        height: u32,
    ) -> CanvasOverlay {
        use blinc_core::{DrawContext, Rect, Transform, layer::Affine2D};
        use blinc_gpu::GpuPaintContext;

        if records.is_empty() {
            return CanvasOverlay::default();
        }

        // Sort by z_layer so overlays render bottom-up. Stable
        // sort on (z_layer, primitive_range.start) keeps siblings
        // at the same z in tree-emit order (the order the walker
        // recorded them).
        records.sort_by(|a, b| {
            a.z_layer
                .cmp(&b.z_layer)
                .then_with(|| a.primitive_range.start.cmp(&b.primitive_range.start))
        });

        // Reuse a single scratch `GpuPaintContext` across every
        // canvas in the frame. `GpuPaintContext::new` allocates a
        // handful of `Vec`s + a `PrimitiveBatch`, all of which
        // benefit from reuse — cn_demo's three spinners running at
        // 30 fps would otherwise build 90 fresh contexts per
        // second. `take_batch` returns the emitted primitives and
        // leaves the batch empty for the next iteration; the
        // transform / opacity stacks get explicit `pop`s after each
        // canvas so the scratch ends every loop iteration in the
        // same fresh state.
        //
        // **Wire the text context**: `draw_text` early-returns when
        // `ctx.text_ctx.is_none()` — without this `set_text_context`
        // call the scratch context silently dropped every
        // `ctx.draw_text(...)` call inside a canvas closure, so the
        // overlay pass emitted zero text primitives even though the
        // composite_frame downstream was ready to dispatch PRIM_TEXT
        // via the sdf_core pipeline. Symptom: canvas_demo's
        // "Canvas Text" sample showed the background plate but
        // none of the headings.
        let mut scratch = GpuPaintContext::new(width as f32, height as f32);
        scratch.set_text_context(&mut self.text_ctx);
        let mut overlay = CanvasOverlay::default();
        for record in records {
            // Replay the ancestor clip stack BEFORE pushing the
            // canvas's affine. `push_clip` transforms the supplied
            // rect by the current affine, so pushing a screen-coord
            // rect on the fresh stack (current affine = identity)
            // keeps it in screen coords — exactly what we want for
            // a scissor that comes from a scroll-container ancestor.
            //
            // `ancestor_clip_aabb` already includes the canvas's
            // own clip (the snapshot was taken AFTER `clips_content`
            // had been pushed during the walker pass), so we skip
            // the per-canvas `clips_content` re-push below — the
            // intersection is already baked into the single rect we
            // push here.
            let mut pushed_ancestor_clip = false;
            if let Some([cx, cy, cw, ch]) = record.ancestor_clip_aabb {
                if cw > 0.0 && ch > 0.0 {
                    scratch.push_clip(blinc_core::layer::ClipShape::rect(Rect::new(
                        cx, cy, cw, ch,
                    )));
                    pushed_ancestor_clip = true;
                } else {
                    // Empty intersection — canvas is fully clipped
                    // out, emit no primitives this frame.
                    continue;
                }
            }

            scratch.push_transform(Transform::Affine2D(Affine2D {
                elements: record.affine,
            }));
            scratch.push_opacity(record.opacity);
            scratch.set_z_layer(record.z_layer);

            // Only push the canvas's own clip when we don't have an
            // ancestor snapshot (e.g. root-level canvas). When the
            // ancestor snapshot exists it already includes the
            // canvas's own clip.
            let push_local_clip = record.clips_content && record.ancestor_clip_aabb.is_none();
            if push_local_clip {
                scratch.push_clip(blinc_core::layer::ClipShape::rect(Rect::new(
                    0.0,
                    0.0,
                    record.bounds_wh.0,
                    record.bounds_wh.1,
                )));
            }
            let canvas_bounds = blinc_layout::canvas::CanvasBounds {
                x: 0.0,
                y: 0.0,
                width: record.bounds_wh.0,
                height: record.bounds_wh.1,
            };
            (record.render_fn)(&mut scratch as &mut dyn DrawContext, canvas_bounds);
            if push_local_clip {
                scratch.pop_clip();
            }
            // Pop the affine + opacity we pushed before invoking
            // the closure so the scratch context returns to its
            // initial state for the next canvas. `take_batch`
            // returns the primitives and resets the internal
            // batch, but transform/opacity stacks need explicit
            // pops to keep the reuse correct.
            scratch.pop_transform();
            scratch.pop_opacity();
            if pushed_ancestor_clip {
                scratch.pop_clip();
            }

            // Drain everything the closure emitted: normal SDF/text/mesh
            // primitives go to `overlay.primitives`, foreground
            // SDF/text/mesh primitives go to
            // `overlay.foreground_primitives`, raw-RGBA blits (video
            // frames, camera previews) go to
            // `overlay.dynamic_images`, and 3D mesh draws go to
            // `overlay.meshes`. Without draining all of these, the
            // compositor-mode overlay path drops foreground / video /
            // mesh / image content while the non-compositor path renders
            // them correctly — the canonical "minimap missing" /
            // "video doesn't render" / "3D helmet missing" bugs.
            let mut new_batch = scratch.take_batch();

            // Shift per-primitive aux_data offsets by the amount
            // already accumulated in `overlay.aux_data`. Each scratch
            // canvas builds offsets relative to its own (initially
            // empty) `PrimitiveBatch::aux_data`; we concatenate
            // every closure's aux_data into one buffer that
            // `composite_frame` re-uploads, so offsets recorded
            // against the first closure's [0..) range have to be
            // rebased to [prev_total..) after the second closure
            // appends.
            //
            // Three primitive types index into aux_data — keep all
            // three in sync or canvas-emitted variants render as
            // garbage / invisible:
            //
            //   - PRIM_MESH (PrimitiveType::Mesh = 9): triangle
            //     vertex positions in `border[2]` (per-triangle
            //     emit from `push_mesh_primitives_brush`). SVG
            //     icons rendered via `SvgDocument::render_fit` hit
            //     this path — pre-fix every icon's border[2] held a
            //     scratch-local offset that pointed into the
            //     dropped scratch aux_data, so the fragment shader
            //     sampled stale BG-batch triangles → icons silently
            //     invisible (despite text + rects working fine).
            //
            //   - 3D group SDF (any prim_type with `border[1]` =
            //     shape_count > 0): ShapeDesc records in
            //     `border[2]`. Rare in node-graph closures but
            //     symmetric with the mesh fix.
            //
            //   - Polygon clip (`type_info[2] = ClipType::Polygon`):
            //     polygon vertex pairs in `clip_radius[3]`. Hit by
            //     any canvas closure that pushes a polygon clip
            //     (e.g. cn::spinner arcs once those were canvas-
            //     hosted) — same garbage-read failure mode.
            let aux_shift = overlay.aux_data.len() as f32;
            Self::shift_aux_offsets(&mut new_batch.primitives, aux_shift);
            Self::shift_aux_offsets(&mut new_batch.foreground_primitives, aux_shift);

            overlay.aux_data.extend(new_batch.aux_data);
            overlay.primitives.extend(new_batch.primitives);
            overlay
                .foreground_primitives
                .extend(new_batch.foreground_primitives);
            overlay.dynamic_images.extend(new_batch.dynamic_images);
            overlay.meshes.extend(scratch.take_pending_meshes());
            overlay.gpu_passes.extend(scratch.take_pending_gpu_passes());
        }
        overlay
    }

    /// Re-walk every `DynamicRegion` in the tree onto a scratch
    /// `GpuPaintContext` and return the combined emitted batch.
    /// Used by the compositor fast path on CSS-only animation frames
    /// to refresh the per-region primitives without re-walking the
    /// whole tree (or re-painting the static cache).
    ///
    /// The scratch context is pushed into `motion_subtree` for the
    /// duration of each region's re-walk so primitive emissions go
    /// to `dynamic_batch`. Both batches (`batch` + `dynamic_batch`)
    /// are drained at the end and concatenated — content emitted
    /// outside a nested motion-subtree push still lands in `batch`
    /// for correctness (e.g. a CSS-animated parent whose primitive
    /// is emitted before push_motion_subtree fires for a
    /// motion-bound child).
    ///
    /// Aux-data offsets in the returned primitives index into the
    /// returned `aux_data`; callers forward both to `composite_frame`.
    fn collect_dynamic_region_primitives(
        &self,
        tree: &blinc_layout::RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
    ) -> blinc_gpu::PrimitiveBatch {
        use blinc_core::DrawContext;
        use blinc_gpu::{GpuPaintContext, PrimitiveBatch};

        let regions = tree.dynamic_regions();
        if regions.is_empty() {
            return PrimitiveBatch::new();
        }

        // One scratch context handles every region — its transform /
        // opacity / clip stacks return to identity between regions
        // because `render_dynamic_region` is balanced. `batch` and
        // `dynamic_batch` accumulate across the iteration; we collect
        // both at the end and merge into the returned batch.
        let mut scratch = GpuPaintContext::new(width as f32, height as f32);

        // Order by z_layer so the per-frame overlay paints
        // higher-layer regions on top of lower-layer ones (stable
        // sort on (z, root) keeps the walker's tree-order as the
        // tiebreaker — same convention `collect_canvas_overlay`
        // uses).
        //
        // CssAnimated AND Canvas regions are EXCLUDED. Same compounding-
        // DPI bug: re-walking them here re-enters `render_layer_with_motion`
        // from the scratch context, which then writes a fresh
        // `dynamic_regions` entry — but the scratch's cumulative
        // transform already has DPI pushed once (by the outer
        // `push_transform` below) AND again via the region's
        // `ambient.affine` (captured from the main walker, which had
        // DPI baked in). The result compounds DPI by an extra factor
        // every fast-path frame; the new `screen_aabb` / canvas
        // `saved_affine` is DPI² physical, then DPI³ next frame,
        // then DPI⁴…
        //
        // Symptom for the canvas case (verified by the 06-14 zoom-
        // flicker log capture): every fast-path frame after a
        // continuous_redraw-triggering event (focused text input,
        // motion animation) compounded the canvas record's
        // `saved_affine` (logged as 2 → 4 → 8 → 16 by the diagnostic
        // probe in motion.rs), making the node-editor canvas appear
        // to zoom out exponentially while the popover was open. The
        // canvas's per-frame render_fn replay in
        // `collect_canvas_overlay` reads the (now-corrupted)
        // `saved_affine` from the canvas paint record and emits the
        // node-graph primitives at that scale. The cache stays locked
        // at the broken affine until a fresh slow walker invalidation
        // (mouse-move → hover-state cache invalidation) resets it.
        //
        // Canvas content goes through `collect_canvas_overlay` —
        // which uses the slow walker's stored `canvas_paint_records`
        // affine (UNCORRUPTED) — so excluding canvas from this re-
        // walk loses nothing visually. Motion-bound subtrees that
        // ARE valid here (DynamicKind::MotionSubtree, not the texture
        // variant) keep going through.
        let mut ordered: Vec<_> = regions
            .values()
            .filter(|r| {
                !matches!(
                    r.kind,
                    blinc_layout::renderer::DynamicKind::CssAnimated { .. }
                        | blinc_layout::renderer::DynamicKind::Canvas { .. }
                )
            })
            .cloned()
            .collect();
        drop(regions);
        if ordered.is_empty() {
            return PrimitiveBatch::new();
        }
        ordered.sort_by(|a, b| {
            a.ambient
                .z_layer
                .cmp(&b.ambient.z_layer)
                .then_with(|| a.root.cmp(&b.root))
        });

        // The re-walk must not record regions: it runs with the DPI
        // scale pushed on top of an ambient affine that already has it,
        // so a region recorded here comes back at twice the scale, then
        // four times, then eight -- drawn at a position that doubles
        // with it. The main walk's regions are the correct ones.
        tree.set_region_capture_suppressed(true);
        for region in &ordered {
            // NO DPI push here. The ambient affine was captured under
            // the main walk's root DPI transform, so it already maps
            // the walker's logical coordinates to physical pixels —
            // measured: every region's ambient reads sx = scale_factor.
            // Wrapping the re-walk in DPI again drew each region at
            // scale_factor× its size, at scale_factor× its position:
            // on every fast-path frame the sliders lost their fill and
            // thumb to an oversized copy near whatever was hovered.
            //
            // The region's root is dynamic by definition — wrap the
            // re-walk in `push_motion_subtree` so emit routes into
            // `dynamic_batch` for consistency with the slow-path
            // walker's routing.
            scratch.push_motion_subtree();
            tree.render_dynamic_region(&mut scratch as &mut dyn DrawContext, region, render_state);
            scratch.pop_motion_subtree();
        }
        tree.set_region_capture_suppressed(false);

        // Drain the dynamic batch. The collect-time `push_motion_subtree`
        // around each region's re-walk routes every emit into
        // `dynamic_batch`, so the scratch's `batch` should always
        // come back empty here. If a future change breaks that
        // invariant we'd silently drop primitives, so debug-assert
        // it and ignore the static batch's contents (any primitive
        // there carries `aux_data` offsets pointing into a different
        // batch's aux array and can't be safely merged without
        // re-indexing them).
        let out = scratch.take_dynamic_batch();
        debug_assert!(
            scratch.take_batch().primitives.is_empty(),
            "collect_dynamic_region_primitives: scratch.batch should be empty — \
             region re-walk emitted outside its motion-subtree gate"
        );
        out
    }

    /// Hash a composite-layer scratch `PrimitiveBatch` for the
    /// rasterize-skip check in the slow-path texture step. Hashes
    /// only the channels that affect rendered pixels: the SDF
    /// primitives, foreground primitives, the `aux_data` buffer
    /// (polygon-clip vertices, 3D group descriptors), AND the
    /// `layer_commands` opacity bits. Glass / nested-glass primitives
    /// don't appear in composited subtrees today (the walker doesn't
    /// promote glass) so they're omitted; if a future change ever
    /// lets glass land in a composite scratch, promotability would
    /// have to be reconsidered anyway.
    ///
    /// `GpuPrimitive` and `aux_data` are `bytemuck::Pod`, so we hash
    /// their raw bytes. `LayerCommand` is a non-Pod enum (carries
    /// `LayerConfig` with `Vec<LayerEffect>` etc.), so we hash the
    /// discriminant + the bits that actually animate per frame
    /// (Push.config.opacity for CSS-promoted subtrees, Sample's
    /// source/dest rects, Pop is constant). Without the layer-command
    /// hash, an animating `push_layer { opacity: t }` around a
    /// composite-promoted subtree (e.g. cn::popover entry animation)
    /// produces byte-identical primitives every frame → the dirty-
    /// skip succeeds → the texture stays frozen at frame-1 (near-
    /// zero opacity) for the lifetime of the animation. Visible
    /// today as cn::input / cn::textarea opened inside a cn::popover
    /// rendering invisible until a mouse-move trips the staleness
    /// sweep that releases the layer texture.
    fn hash_composite_scratch(batch: &blinc_gpu::primitives::PrimitiveBatch) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytemuck::cast_slice::<_, u8>(&batch.primitives).hash(&mut hasher);
        bytemuck::cast_slice::<_, u8>(&batch.foreground_primitives).hash(&mut hasher);
        bytemuck::cast_slice::<_, u8>(&batch.aux_data).hash(&mut hasher);
        for entry in &batch.layer_commands {
            entry.primitive_index.hash(&mut hasher);
            entry.foreground_primitive_index.hash(&mut hasher);
            match &entry.command {
                blinc_gpu::primitives::LayerCommand::Push { config } => {
                    0u8.hash(&mut hasher);
                    config.opacity.to_bits().hash(&mut hasher);
                    std::mem::discriminant(&config.blend_mode).hash(&mut hasher);
                    config.depth.hash(&mut hasher);
                }
                blinc_gpu::primitives::LayerCommand::Pop => {
                    1u8.hash(&mut hasher);
                }
                blinc_gpu::primitives::LayerCommand::Sample { source, dest, .. } => {
                    2u8.hash(&mut hasher);
                    source.x().to_bits().hash(&mut hasher);
                    source.y().to_bits().hash(&mut hasher);
                    source.width().to_bits().hash(&mut hasher);
                    source.height().to_bits().hash(&mut hasher);
                    dest.x().to_bits().hash(&mut hasher);
                    dest.y().to_bits().hash(&mut hasher);
                    dest.width().to_bits().hash(&mut hasher);
                    dest.height().to_bits().hash(&mut hasher);
                }
            }
        }
        hasher.finish()
    }

    /// Composite each promoted CSS-animated layer's texture onto the
    /// surface with the active animation's current opacity / 2D
    /// transform applied at composite time.
    ///
    /// Called after `composite_frame` blits the static cache and
    /// dispatches the motion-binding overlay. The layer textures
    /// were rasterized at base state (the walker skipped applying
    /// the composite-promotable CSS properties), so the animated
    /// values read from the css-anim store here apply directly to
    /// `blit_tight_texture_to_target`'s `dest_pos` / `dest_size` /
    /// `opacity` — no delta math needed.
    ///
    /// Skips silently for any region whose texture isn't yet in
    /// `css_composited_textures` (first-frame race after promotion,
    /// or `LayerTexture` released on demotion).
    fn composite_css_layers_overlay(
        &mut self,
        tree: &blinc_layout::RenderTree,
        target_view: &wgpu::TextureView,
    ) {
        use slotmap::Key as _;

        // Collect (key, screen_aabb, natural_size, stable_id) up-front
        // so we don't hold the `dynamic_regions` borrow across the
        // `&mut self.renderer` blit calls.
        struct CompositeJob {
            key: u64,
            screen_aabb: [f32; 4],
            natural_size: (u32, u32),
            stable_id: Option<blinc_layout::tree::StableNodeId>,
            ambient_clip: Option<([f32; 4], [f32; 4])>,
            visit_seq: u32,
        }
        let mut jobs: Vec<CompositeJob> = {
            let regions = tree.dynamic_regions();
            regions
                .iter()
                .filter_map(|(node, region)| match region.kind {
                    blinc_layout::renderer::DynamicKind::CssAnimated {
                        natural_size,
                        ambient_clip,
                    } if natural_size.0 > 0 && natural_size.1 > 0 => Some(CompositeJob {
                        key: node.data().as_ffi(),
                        screen_aabb: region.screen_aabb,
                        natural_size,
                        stable_id: tree.stable_id(*node),
                        ambient_clip,
                        visit_seq: region.visit_seq,
                    }),
                    _ => None,
                })
                .collect()
        };
        // HashMap iteration is non-deterministic; sort by walker
        // visit order so overlapping subtrees composite back-to-front
        // (matches tree paint order). z_layer alone is insufficient
        // because flex siblings share the same z_layer.
        jobs.sort_by_key(|j| j.visit_seq);
        if jobs.is_empty() {
            return;
        }
        // Snapshot current animated properties per stable id under a
        // single lock acquisition. We need: opacity / translate / scale
        // to apply to the blit.
        let mut anim_props: std::collections::HashMap<
            blinc_layout::tree::StableNodeId,
            blinc_animation::KeyframeProperties,
        > = std::collections::HashMap::new();
        if let Ok(store) = tree.css_anim_store().lock() {
            for job in &jobs {
                let Some(sid) = job.stable_id else { continue };
                if let Some(anim) = store.animations.get(&sid) {
                    if anim.is_playing {
                        anim_props.insert(sid, anim.current_properties.clone());
                        continue;
                    }
                }
                if let Some(trans) = store.transitions.get(&sid) {
                    if trans.is_playing {
                        anim_props.insert(sid, trans.current_properties.clone());
                    }
                }
            }
        }
        // Issue the per-region blits. Texture was rendered at base
        // state, so animated values map straight through:
        //   dest_pos = screen_aabb.xy + translate (physical pixels)
        //   dest_size = natural_size * scale
        //   opacity = current opacity (default 1.0)
        // 2D rotation is in the predicate but skipped in the blit
        // until the shader gains a `rotate_z` path (next follow-up).
        for job in jobs {
            let Some((texture, content_size)) = self.css_composited_textures.get(&job.key) else {
                continue;
            };
            let allocated_size = *content_size;
            let props = job
                .stable_id
                .and_then(|sid| anim_props.get(&sid))
                .cloned()
                .unwrap_or_default();
            let translate = (
                props.translate_x.unwrap_or(0.0),
                props.translate_y.unwrap_or(0.0),
            );
            let scale = (props.scale_x.unwrap_or(1.0), props.scale_y.unwrap_or(1.0));
            let opacity = props.opacity.unwrap_or(1.0);

            let dpi = tree.scale_factor().max(1.0);
            let dest_pos = (
                job.screen_aabb[0] + translate.0 * dpi,
                job.screen_aabb[1] + translate.1 * dpi,
            );
            let dest_size = (
                job.natural_size.0 as f32 * scale.0,
                job.natural_size.1 as f32 * scale.1,
            );

            // P4.3 Option B: ancestor clip captured at promotion time
            // gets re-applied here as the blit scissor. The captured
            // tuple is `(aabb, radius)` — radius is `[0; 4]` when the
            // ancestor chain has no rounded-rect clip (the blit
            // shader treats that as a plain AABB scissor). Without
            // the rounded radius path, a CSS animation on a subtree
            // inside an `overflow_clip` rounded ancestor would get
            // its corners squared off at every blit.
            let blit_clip = job.ambient_clip;

            // `source_size` must be the ALLOCATED texture size (the
            // bucket-rounded content_size returned by
            // `render_subtree_to_layer_texture`), NOT the natural
            // content area — see `css_composited_textures` doc for
            // the squeezed-bar artefact this prevents.
            self.renderer.blit_tight_texture_to_target(
                &texture.view,
                allocated_size,
                target_view,
                dest_pos,
                dest_size,
                opacity,
                blinc_core::BlendMode::Normal,
                blit_clip,
                None,
                0.0,
            );
        }
    }

    /// Dispatch the motion-subtree text + SVG overlay pools with the
    /// LIVE composite-animation values patched in.
    ///
    /// Text glyphs / SVG icons under a composite-promoted CSS-animated
    /// subtree are collected at BASE alpha (the animation's opacity is
    /// excluded — see `TextElement::composite_ancestor`) because the
    /// pools are cached and fast-path frames dispatch them unchanged.
    /// This helper multiplies the current store opacity into the
    /// cloned prims each frame and re-anchors glyph centers with the
    /// same top-left math `blit_tight_texture_to_target` uses for the
    /// baked bg, so text fades AND tracks the panel in lockstep with
    /// the blit on every frame, fast or slow. The clone is cheap:
    /// `render_primitives_overlay` re-uploads CPU prims on every call
    /// anyway, and the cached pool stays at base values.
    fn dispatch_motion_subtree_overlay_pools(
        &mut self,
        tree: &blinc_layout::RenderTree,
        target_view: &wgpu::TextureView,
    ) {
        // TEXT — alpha + blit-anchored center remap.
        if let Some(mut prims) = self.cached_motion_subtree_text_prims.clone() {
            if !prims.is_empty() {
                if !self.motion_subtree_text_patch.is_empty() {
                    let dpi = tree.scale_factor().max(1.0);
                    if let Ok(store) = tree.css_anim_store().lock() {
                        for (range, sid) in &self.motion_subtree_text_patch {
                            let props = store
                                .animations
                                .get(sid)
                                .filter(|a| a.is_playing)
                                .map(|a| &a.current_properties)
                                .or_else(|| {
                                    store
                                        .transitions
                                        .get(sid)
                                        .filter(|t| t.is_playing)
                                        .map(|t| &t.current_properties)
                                });
                            let Some(props) = props else { continue };
                            let live_opacity = props.opacity.unwrap_or(1.0);
                            let t1 = (
                                props.translate_x.unwrap_or(0.0),
                                props.translate_y.unwrap_or(0.0),
                            );
                            let s1 = (props.scale_x.unwrap_or(1.0), props.scale_y.unwrap_or(1.0));
                            let baseline = self.composite_patch_baselines.get(sid).copied();
                            for prim in &mut prims[range.clone()] {
                                prim.color[3] *= live_opacity;
                                if let Some((aabb, t0, s0)) = baseline {
                                    // p_base = aabb + (p0 - aabb - t0*dpi)/s0
                                    // p1     = aabb + t1*dpi + p_base_rel*s1
                                    let cx = prim.bounds[0] + prim.bounds[2] * 0.5;
                                    let cy = prim.bounds[1] + prim.bounds[3] * 0.5;
                                    let rel_x = (cx - aabb[0] - t0.0 * dpi) / s0.0.max(1e-6);
                                    let rel_y = (cy - aabb[1] - t0.1 * dpi) / s0.1.max(1e-6);
                                    let nx = aabb[0] + t1.0 * dpi + rel_x * s1.0;
                                    let ny = aabb[1] + t1.1 * dpi + rel_y * s1.1;
                                    prim.bounds[0] = nx - prim.bounds[2] * 0.5;
                                    prim.bounds[1] = ny - prim.bounds[3] * 0.5;
                                }
                            }
                        }
                    }
                }
                self.rebind_glyph_atlas_for_overlay();
                self.renderer.render_primitives_overlay(target_view, &prims);
            }
        }
        // SVG — alpha only (icons are small; the settle repaint lands
        // the exact final position when the animation completes).
        if let Some(mut svgs) = self.cached_motion_subtree_svgs.clone() {
            if !svgs.is_empty() {
                if svgs.iter().any(|s| s.composite_ancestor.is_some()) {
                    if let Ok(store) = tree.css_anim_store().lock() {
                        for svg in &mut svgs {
                            let Some(sid) = svg.composite_ancestor else {
                                continue;
                            };
                            let props = store
                                .animations
                                .get(&sid)
                                .filter(|a| a.is_playing)
                                .map(|a| &a.current_properties)
                                .or_else(|| {
                                    store
                                        .transitions
                                        .get(&sid)
                                        .filter(|t| t.is_playing)
                                        .map(|t| &t.current_properties)
                                });
                            let Some(props) = props else { continue };
                            svg.motion_opacity *= props.opacity.unwrap_or(1.0);
                        }
                    }
                }
                let dpi = tree.scale_factor();
                self.render_rasterized_svgs(target_view, &svgs, dpi);
            }
        }
    }

    /// Phase 4.3 motion sibling of [`Self::composite_css_layers_overlay`].
    /// Blits each baked motion-subtree texture onto the surface with
    /// the current `MotionBindings` (translate / scale / opacity)
    /// applied at composite time — no walker re-entry, no SDF re-
    /// emission for the steady-state motion case.
    ///
    /// 2D rotation from `MotionBindings::get_rotation()` is read here
    /// for the dirty-skip predicate path (P4.4) but NOT applied to
    /// the blit until the shader gains a `rotate_z` path — same gap
    /// as the CSS overlay. Rotation-bound subtrees still bake; the
    /// rotation is just elided until the shader update lands.
    ///
    /// Skips silently for any region whose texture isn't yet in
    /// `motion_subtree_textures` (first-frame race: the bake hook
    /// ran at end of paint but the very first motion-driven frame
    /// after promotion may not have the texture yet — next frame
    /// catches up).
    fn composite_motion_layers_overlay(
        &mut self,
        tree: &blinc_layout::RenderTree,
        target_view: &wgpu::TextureView,
    ) {
        use slotmap::Key as _;

        struct MotionCompositeJob {
            key: u64,
            node: blinc_layout::tree::LayoutNodeId,
            screen_aabb: [f32; 4],
            natural_size: (u32, u32),
            ambient_clip: Option<([f32; 4], [f32; 4])>,
            visit_seq: u32,
        }
        let mut jobs: Vec<MotionCompositeJob> = {
            let regions = tree.dynamic_regions();
            regions
                .iter()
                .filter_map(|(node, region)| match region.kind {
                    blinc_layout::renderer::DynamicKind::MotionSubtreeTexture {
                        natural_size,
                        ambient_clip,
                    } if natural_size.0 > 0 && natural_size.1 > 0 => Some(MotionCompositeJob {
                        key: node.data().as_ffi(),
                        node: *node,
                        screen_aabb: region.screen_aabb,
                        natural_size,
                        ambient_clip,
                        visit_seq: region.visit_seq,
                    }),
                    _ => None,
                })
                .collect()
        };
        // `dynamic_regions` is a HashMap → iteration order is non-
        // deterministic. Sort by walker visit_seq so the blit
        // sequence matches tree paint order — children later in the
        // parent's child list have higher visit_seq and paint on top.
        // Symptom this fixes: cn::switch's on_layer (earlier child)
        // and thumb (later sibling) both promote; HashMap order may
        // blit on_layer AFTER the thumb, covering it. As on_layer's
        // opacity ramps 0→1 (OFF→ON), the thumb visibly "fades off"
        // — but only on OFF→ON; the inverse animation hides the bug
        // because the on_layer fades OUT, revealing the thumb. Flex
        // siblings share the same z_layer (no stack context), so
        // z_layer alone can't distinguish them — visit_seq must.
        jobs.sort_by_key(|j| j.visit_seq);
        if jobs.is_empty() {
            return;
        }
        let bindings_map = tree.motion_bindings_map().clone();
        for job in jobs {
            let Some((texture, content_size)) = self.motion_subtree_textures.get(&job.key) else {
                continue;
            };
            let allocated_size = *content_size;

            // Default to identity if the node lost its bindings
            // between paint and overlay (shouldn't happen — bindings
            // are stable across the frame — but stay defensive so a
            // missing entry produces a static blit rather than panic).
            let (translate, scale, opacity, rotate_deg) =
                if let Some(bindings) = bindings_map.get(&job.node) {
                    let translate = match bindings.get_transform() {
                        Some(blinc_core::Transform::Affine2D(a)) => (a.elements[4], a.elements[5]),
                        _ => (0.0, 0.0),
                    };
                    let scale = bindings.get_scale().unwrap_or((1.0, 1.0));
                    let opacity = bindings.get_opacity().unwrap_or(1.0);
                    // Rotation is in DEGREES from MotionBindings
                    // (covers both spring-based `rotation` and
                    // continuous `rotation_timeline`). Shader takes
                    // sin/cos radians, so convert at the blit site.
                    let rotate = bindings.get_rotation().unwrap_or(0.0);
                    (translate, scale, opacity, rotate)
                } else {
                    ((0.0, 0.0), (1.0, 1.0), 1.0, 0.0)
                };
            let rotate_z_rad = rotate_deg.to_radians();

            let dpi = tree.scale_factor().max(1.0);
            let dest_pos = (
                job.screen_aabb[0] + translate.0 * dpi,
                job.screen_aabb[1] + translate.1 * dpi,
            );
            let dest_size = (
                job.natural_size.0 as f32 * scale.0,
                job.natural_size.1 as f32 * scale.1,
            );

            // Ambient clip → blit scissor. The captured tuple is
            // `(aabb, radius)` — the blit shader applies the rounded-
            // rect SDF clip when radius is non-zero. This keeps a
            // motion-bound subtree inside a rounded ancestor (e.g.
            // progress_animated indicator inside an overflow_clip
            // rounded track) clipped along the parent's actual
            // outline instead of getting its corners squared off.
            let blit_clip = job.ambient_clip;

            // `source_size` is the ALLOCATED texture size (bucket-
            // rounded), not natural_size — see the analogous comment
            // in `composite_css_layers_overlay` for the squeeze
            // artefact this prevents.
            self.renderer.blit_tight_texture_to_target(
                &texture.view,
                allocated_size,
                target_view,
                dest_pos,
                dest_size,
                opacity,
                blinc_core::BlendMode::Normal,
                blit_clip,
                None,
                rotate_z_rad,
            );
        }
    }

    /// Re-walk a CSS-animated subtree and return the resulting bg
    /// `PrimitiveBatch`. Used by Phase 3 of the per-element slow path:
    /// when `apply_css_deltas` reports `PatchedWithReemits` for a
    /// node whose active animation touched an out-of-scope property
    /// (width / height / padding / etc.), the caller calls this to
    /// produce fresh primitives at the post-`compute_layout()` bounds,
    /// then splices them into `cached_bg_batch.primitives` at the
    /// record's old `primitive_range`.
    ///
    /// Returns `None` when:
    /// - The node has no entry in `dynamic_regions` (walker didn't
    ///   record one — should not happen for a node that produced a
    ///   `CssAnimPaintMeta`).
    /// - The region's kind isn't `CssAnimated` (motion / canvas
    ///   regions take different code paths).
    /// - The re-walk emitted complex batch state we can't safely
    ///   splice yet (layer_commands, paths, glass, foreground prims,
    ///   3D viewports, aux_data, glyphs). Caller falls back to the
    ///   full slow path. Documented limitation; future commits can
    ///   widen the splice surface.
    fn reemit_css_subtree_bg(
        &self,
        tree: &blinc_layout::RenderTree,
        render_state: &blinc_layout::RenderState,
        node_id: LayoutNodeId,
        width: u32,
        height: u32,
    ) -> Option<blinc_gpu::PrimitiveBatch> {
        use blinc_core::DrawContext;
        use blinc_gpu::GpuPaintContext;

        let regions = tree.dynamic_regions();
        let region = regions.get(&node_id)?.clone();
        drop(regions);

        if !matches!(
            region.kind,
            blinc_layout::renderer::DynamicKind::CssAnimated { .. }
        ) {
            return None;
        }

        let mut scratch = GpuPaintContext::new(width as f32, height as f32);

        // DPI scale mirrors what `collect_dynamic_region_primitives`
        // does — ambient affine is physical-pixel-space; logical
        // walker translates need wrapping by the scale.
        let scale = tree.scale_factor();
        let has_scale = scale != 1.0;
        if has_scale {
            scratch.push_transform(blinc_core::Transform::scale(scale, scale));
        }

        // Unlike `collect_dynamic_region_primitives`, do NOT
        // push_motion_subtree — CSS subtrees stay in the bg batch
        // (the static cache contract; routing them to the overlay
        // breaks text z-order, see motion.rs:464). The walker's emit
        // routes into `scratch.batch` (== bg batch).
        tree.render_dynamic_region(&mut scratch as &mut dyn DrawContext, &region, render_state);

        if has_scale {
            scratch.pop_transform();
        }

        let new_batch = scratch.take_batch();

        // Conservative: the splice is only safe today for the
        // primitive vector. Any other field carrying cross-primitive
        // indices (paths, layer_commands referencing aux_data
        // offsets) needs index re-numbering we don't yet do. Bail to
        // slow path when those are present.
        if !new_batch.layer_commands.is_empty()
            || !new_batch.foreground_primitives.is_empty()
            || !new_batch.glass_primitives.is_empty()
            || !new_batch.nested_glass_primitives.is_empty()
            || !new_batch.paths.vertices.is_empty()
            || !new_batch.foreground_paths.vertices.is_empty()
            || !new_batch.glyphs.is_empty()
            || !new_batch.viewports_3d.is_empty()
            || !new_batch.particle_viewports.is_empty()
            || !new_batch.aux_data.is_empty()
            || !new_batch.dynamic_images.is_empty()
        {
            tracing::trace!(
                target: "blinc_app::frame_timing",
                "reemit_css_subtree_complex_batch_bail",
            );
            return None;
        }

        Some(new_batch)
    }

    /// Phase 4 helper: scan the cached text glyph vectors for any
    /// glyph whose bounds intersect any of the supplied reemit
    /// subtree AABBs. Returns `true` if any glyph falls inside a
    /// reemit subtree, signalling the caller to bail to the slow
    /// path. The text-shaping pipeline isn't subtree-aware today
    /// — re-shaping just the affected subtree would require a
    /// significant refactor of `collect_elements_recursive` — so
    /// when the cache contains text inside a layout-animating
    /// subtree, the safe choice is the full slow path.
    fn glyphs_intersect_any_aabb(&self, snapshots: &[ReemitSnapshot]) -> bool {
        let aabbs: Vec<[f32; 4]> = snapshots.iter().filter_map(|(_, _, aabb)| *aabb).collect();
        if aabbs.is_empty() {
            return false;
        }
        let intersects = |g_bounds: [f32; 4], a: [f32; 4]| -> bool {
            let (gx, gy, gw, gh) = (g_bounds[0], g_bounds[1], g_bounds[2], g_bounds[3]);
            let (ax, ay, aw, ah) = (a[0], a[1], a[2], a[3]);
            !(gx + gw <= ax || gy + gh <= ay || gx >= ax + aw || gy >= ay + ah)
        };
        if let Some(by_layer) = self.cached_glyphs_by_layer.as_ref() {
            for glyphs in by_layer.values() {
                for g in glyphs {
                    for a in &aabbs {
                        if intersects(g.bounds, *a) {
                            return true;
                        }
                    }
                }
            }
        }
        if let Some(fg) = self.cached_fg_glyphs.as_ref() {
            for g in fg {
                for a in &aabbs {
                    if intersects(g.bounds, *a) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Phase 3 of the per-element slow path: re-emit each listed
    /// CSS-animated subtree via the walker and splice the resulting
    /// primitives back into `cached_bg_batch.primitives` at the
    /// record's original `primitive_range`. Returns `true` if the
    /// splice succeeded (caller proceeds with the damaged-cache
    /// repaint) or `false` if any subtree's re-emit produced complex
    /// batch state we can't safely splice yet (caller falls back to
    /// the full slow path).
    ///
    /// Splice details:
    /// - Snapshot every reemit node's original `primitive_range`
    ///   before any walker call; the walker mutates records during
    ///   the walk and we need the original indices to position the
    ///   splice into `cached_bg_batch`.
    /// - Sort splices by `old_range.start` descending. Processing
    ///   from the highest index first means each splice only shifts
    ///   elements *after* the splice point, so we don't have to
    ///   compose deltas across earlier splices.
    /// - For each splice, `primitives.splice(old_range, new)` does
    ///   the work. Other records whose `primitive_range.start >=
    ///   old_range.end` shift by `new_len - old_len`.
    fn try_reemit_and_splice_css_subtrees(
        &mut self,
        tree: &blinc_layout::RenderTree,
        render_state: &blinc_layout::RenderState,
        reemit_nodes: &[LayoutNodeId],
        width: u32,
        height: u32,
    ) -> bool {
        if reemit_nodes.is_empty() {
            return true;
        }

        // Step 1: snapshot old primitive_range AND screen AABB for
        // each reemit node BEFORE any walker call. The walker
        // overwrites `css_anim_paint_records[node].primitive_range`
        // with scratch batch indices; we need the cached_bg_batch
        // indices to position the splice. The AABB lets us check
        // for cached text inside the subtree (Phase 4 bail below).
        let snapshots: Vec<ReemitSnapshot> = {
            let records = tree.css_anim_paint_records();
            reemit_nodes
                .iter()
                .filter_map(|&n| {
                    records
                        .get(&n)
                        .map(|m| (n, m.primitive_range.clone(), m.last_screen_aabb))
                })
                .collect()
        };
        if snapshots.is_empty() {
            return true;
        }

        // Phase 4 conservative bail: if any cached glyph falls
        // inside a reemit subtree's old AABB, the cache for that
        // region contains text shaped at the previous layout's
        // bounds. We can't re-shape it from here (the text-shaping
        // pipeline isn't subtree-aware), so the post-splice
        // damaged-cache repaint would land new bg primitives next
        // to stale text glyphs — text would appear offset relative
        // to the moved background. Fall back to the full slow path
        // so taffy + the text collector run end-to-end and produce
        // a coherent frame.
        let any_text_in_reemit = self.glyphs_intersect_any_aabb(&snapshots);
        if any_text_in_reemit {
            tracing::trace!(
                target: "blinc_app::frame_timing",
                "reemit_text_intersect_bail",
            );
            return false;
        }

        // Step 2: re-walk each subtree into a fresh scratch context
        // and collect the produced primitives. Any subtree whose
        // re-walk emits complex batch state (layer_commands, paths,
        // glass, etc. — see `reemit_css_subtree_bg`) returns None
        // and we bail to the slow path.
        let mut new_prims: Vec<(
            LayoutNodeId,
            std::ops::Range<usize>,
            Vec<blinc_gpu::primitives::GpuPrimitive>,
        )> = Vec::with_capacity(snapshots.len());
        for (node, old_range, _aabb) in snapshots {
            match self.reemit_css_subtree_bg(tree, render_state, node, width, height) {
                Some(batch) => new_prims.push((node, old_range, batch.primitives)),
                None => return false,
            }
        }

        // Step 3: splice in reverse order of old_range.start. This
        // keeps each splice from invalidating earlier (smaller-start)
        // splices' indices.
        new_prims.sort_by_key(|entry| std::cmp::Reverse(entry.1.start));

        let Some(batch) = self.cached_bg_batch.as_mut() else {
            return false;
        };
        let mut records = tree.css_anim_paint_records_mut();
        for (node, old_range, prims) in new_prims {
            let old_len = old_range.end - old_range.start;
            let new_len = prims.len();
            let delta = new_len as isize - old_len as isize;
            batch.primitives.splice(old_range.clone(), prims);

            // Update THIS reemit's record to point into the patched
            // cached_bg_batch. The walker already wrote the NEW
            // screen AABB to `meta.last_screen_aabb` during the
            // re-walk; carry it into `last_css_damage_rects` so the
            // damaged-cache repaint scissor covers the new geometry.
            // Without this, layout animations that grow the element
            // past its previous AABB leave stale pixels in the
            // exposed area (the OLD aabb pushed by apply_css_deltas
            // doesn't extend that far).
            if let Some(meta) = records.get_mut(&node) {
                meta.primitive_range = old_range.start..(old_range.start + new_len);
                if let Some(rect) = meta.last_screen_aabb {
                    self.last_css_damage_rects.push(rect);
                }
            }

            // Shift every other record's range when its start is at
            // or past `old_range.end` — i.e., it lived in the
            // cache *after* the spliced region.
            if delta != 0 {
                for (other_node, other_meta) in records.iter_mut() {
                    if *other_node == node {
                        continue;
                    }
                    if other_meta.primitive_range.start >= old_range.end {
                        let new_start =
                            (other_meta.primitive_range.start as isize + delta) as usize;
                        let new_end = (other_meta.primitive_range.end as isize + delta) as usize;
                        other_meta.primitive_range = new_start..new_end;
                    }
                }
            }
        }

        true
    }

    /// frames in a row would double-apply).
    pub fn apply_binding_deltas(&mut self, tree: &blinc_layout::RenderTree, scale: f32) -> bool {
        self.last_binding_damage_rects.clear();
        // Compositor v2: motion-bound subtree primitives live in
        // `cached_dynamic_batch`, not `cached_bg_batch`. The walker
        // routes them there via `push_motion_subtree`, and
        // `composite_bindings[node].primitive_range` indexes into
        // the dynamic batch. Patching `cached_bg_batch` here would
        // corrupt unrelated static primitives at those indices.
        let batch = match self.cached_dynamic_batch.as_mut() {
            Some(b) => b,
            None => return false,
        };
        let mut bindings = tree.composite_bindings_mut();
        if bindings.is_empty() {
            // Cache is valid but nothing animated changed — fast
            // path can re-use the cache as-is. Caller still needs
            // to dispatch a render to present a fresh frame
            // (otherwise the surface shows the previous frame).
            return true;
        }

        // Walk every recorded binding. Read the current spring
        // values (post-tick), compute the delta vs the values baked
        // into the cached batch, and patch every primitive in the
        // binding's range.
        //
        // Track whether any binding's value moved this frame so we
        // can flag `visible_anim_active` on the tree below. The
        // walker is the other writer of that flag (called from
        // `render_with_motion`'s entry, then updated as it visits
        // bound nodes), but on the fast path the walker doesn't run
        // — without this manual mark the flag stays at whatever the
        // previous full paint left it at, and Phase 5's redraw
        // chain gates `needs_animation_redraw && visible_anim` →
        // false → no next frame → spring stops ticking → animation
        // freezes until something else (mouse move, scroll) wakes
        // the loop. That's the "animation doesn't play until I move
        // the mouse" symptom this addresses.
        let motion_bindings = tree.motion_bindings_map();

        // Pre-pass: compute each binding's translate delta this frame
        // and the SUM of ancestor bindings' translate deltas
        // ("inherited shift"). An "ancestor" here is any other binding
        // whose `primitive_range` strictly contains this one's — which
        // mirrors emission order, so it captures the motion-binding
        // parent chain without needing a tree walk.
        //
        // Why this matters: the walker records `meta.centre` from
        // `get_absolute_bounds(node)` — the LAYOUT centre, not the
        // post-translate world centre. If an ancestor motion binding
        // translates the subtree, this binding's primitives have
        // shifted but its stored centre hasn't. Applying scale around
        // the stale centre produces a positional error of
        // `ancestor_translate × (new_scale - 1)`. Symptom (seen in
        // cn::slider with halo): the inner scale binding paints the
        // halo at the right position only when the thumb sits at
        // translate=0; at any non-zero offset the halo drifts toward
        // the layout origin, leaving a ghost trail.
        //
        // Fix: pre-compute each binding's inherited shift, then add it
        // to the centre before applying scale (and persist the shifted
        // centre so subsequent frames see the new world position as
        // the baseline). Since we collect raw deltas BEFORE applying
        // any patches, iteration order doesn't matter.
        //
        // Cost: O(N²) over composite_bindings — N is the count of
        // motion-bound nodes in the rendered frame (~10-50 in cn_demo),
        // so ~2500 max iterations of cheap range comparisons. The
        // existing main loop is also O(N) with mutex locks per
        // binding; this pre-pass is dominated by it.
        let mut raw_translate_deltas: std::collections::HashMap<
            blinc_layout::tree::LayoutNodeId,
            (f32, f32),
        > = std::collections::HashMap::new();
        for (node, meta) in bindings.iter() {
            let Some(b) = motion_bindings.get(node) else {
                continue;
            };
            let tx = b
                .translate_x
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(0.0);
            let ty = b
                .translate_y
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(0.0);
            raw_translate_deltas.insert(
                *node,
                (tx - meta.last_translate.0, ty - meta.last_translate.1),
            );
        }
        let mut inherited_shifts: std::collections::HashMap<
            blinc_layout::tree::LayoutNodeId,
            (f32, f32),
        > = std::collections::HashMap::new();
        for (node, meta) in bindings.iter() {
            let mut shift = (0.0f32, 0.0f32);
            for (other_node, other_meta) in bindings.iter() {
                if other_node == node {
                    continue;
                }
                // "other strictly contains me" — handles the common
                // case (ancestor has its own primitives outside the
                // child's emission span). Equal-range pairs are
                // excluded; that case only arises when a parent emits
                // nothing of its own, and a translate on it composes
                // through the existing translate patch on the SAME
                // primitive set, so no centre adjustment is needed.
                let contains = other_meta.primitive_range.start <= meta.primitive_range.start
                    && other_meta.primitive_range.end >= meta.primitive_range.end
                    && (other_meta.primitive_range.start < meta.primitive_range.start
                        || other_meta.primitive_range.end > meta.primitive_range.end);
                if contains {
                    if let Some(&(dx, dy)) = raw_translate_deltas.get(other_node) {
                        shift.0 += dx;
                        shift.1 += dy;
                    }
                }
            }
            inherited_shifts.insert(*node, shift);
        }

        let mut any_binding_active = false;
        for (node, meta) in bindings.iter_mut() {
            let bindings_for_node = match motion_bindings.get(node) {
                Some(b) => b,
                None => continue,
            };
            // Snapshot the AABB before any patching this frame. The
            // damage rect for this binding = union of the AABB before
            // and after — whichever pixels the static cache might be
            // showing stale need re-painting.
            let aabb_before = meta.last_screen_aabb;
            let mut binding_moved = false;

            // Is this binding mid-flight? Mirrors `walker_motion_bindings_ref.
            // is_any_animating()`. A binding can be active even if no value
            // actually moved this frame — e.g. the very first tick after a
            // fresh `set_target` has `dt ≈ 0` (because `set_spring_target`
            // resets `last_frame = now`), so the spring sits at its old value
            // for one frame before integration starts producing real
            // displacement. Without flagging visible_anim_active on those
            // frames, Phase 5's redraw chain dies and the animation freezes
            // until something else wakes the loop.
            if bindings_for_node.is_any_animating() {
                any_binding_active = true;
            }

            // ----------------------------------------------------------------
            // Translate: read new (tx, ty), compute pixel-space delta,
            // shift every primitive's `bounds.xy` by it.
            // ----------------------------------------------------------------
            let (binding_tx, binding_ty) = {
                let tx = bindings_for_node
                    .translate_x
                    .as_ref()
                    .and_then(|v| v.lock().ok().map(|g| g.get()))
                    .unwrap_or(0.0);
                let ty = bindings_for_node
                    .translate_y
                    .as_ref()
                    .and_then(|v| v.lock().ok().map(|g| g.get()))
                    .unwrap_or(0.0);
                (tx, ty)
            };
            let new_translate = (binding_tx, binding_ty);
            let dx_logical = new_translate.0 - meta.last_translate.0;
            let dy_logical = new_translate.1 - meta.last_translate.1;
            if dx_logical.abs() > f32::EPSILON || dy_logical.abs() > f32::EPSILON {
                let dx_phys = dx_logical * scale;
                let dy_phys = dy_logical * scale;
                if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                    for p in prims.iter_mut() {
                        p.bounds[0] += dx_phys;
                        p.bounds[1] += dy_phys;
                    }
                }
                meta.last_translate = new_translate;
                any_binding_active = true;
                binding_moved = true;
            }

            // ----------------------------------------------------------------
            // Opacity: read new value, compute ratio against the
            // baked-in value, scale every alpha-bearing channel by it.
            //
            // Skip when last_opacity is ~0 (would divide by zero) —
            // any node painted with opacity 0 wasn't visible last
            // frame, so the delta-apply isn't meaningful and the
            // caller should fall back to the full path. Returning
            // `Err` is the conservative choice.
            // ----------------------------------------------------------------
            let new_opacity = bindings_for_node
                .opacity
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(meta.last_opacity);
            if (new_opacity - meta.last_opacity).abs() > f32::EPSILON {
                if let Some(push_idx) = meta.layer_push_index {
                    // Layered path (motion-binding opacity always
                    // takes this path post-Phase 4a): patch
                    // `LayerConfig.opacity` at the push command's
                    // index. The renderer's layer-composite blit
                    // reads `config.opacity` and the new spring
                    // value becomes visible the next composite.
                    //
                    // No divide-by-zero hazard here because we're
                    // writing an absolute value, not computing a
                    // ratio against `last_opacity`.
                    use blinc_gpu::primitives::LayerCommand;
                    match batch.layer_commands.get_mut(push_idx) {
                        Some(entry) => {
                            if let LayerCommand::Push { config } = &mut entry.command {
                                config.opacity = new_opacity;
                            } else {
                                return false;
                            }
                        }
                        None => return false,
                    }
                    meta.last_opacity = new_opacity;
                    any_binding_active = true;
                } else {
                    if meta.last_opacity.abs() < f32::EPSILON {
                        return false;
                    }
                    let ratio = new_opacity / meta.last_opacity;
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            p.color[3] *= ratio;
                            p.border_color[3] *= ratio;
                            p.shadow_color[3] *= ratio;
                        }
                    }
                    meta.last_opacity = new_opacity;
                    any_binding_active = true;
                }
                binding_moved = true;
            }

            // ----------------------------------------------------------------
            // Scale: scale around the binding's recorded centre. For each
            // primitive in the range, transform `bounds` from the
            // last_scale to the new scale, keeping the binding centre
            // fixed.
            //
            //   bounds_local = (old_bounds - centre) / last_scale
            //   new_bounds   = centre + new_scale * bounds_local
            //                = centre + (new_scale / last_scale) * (old_bounds - centre)
            //   delta        = (new_scale/last_scale - 1) * (old_bounds - centre)
            //
            // Size scales by the same ratio: bounds.w *= new_scale/last_scale.
            //
            // Bails (fast-path falls back to full walker) when
            // `last_scale` is degenerate (~0) — that ratio would
            // explode. The centre stored in `composite_bindings` is in
            // logical pixels (pre-DPI); bounds are in physical pixels,
            // so we scale the centre by `scale` before computing the
            // delta.
            // ----------------------------------------------------------------
            let binding_scale = bindings_for_node
                .scale
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(1.0);
            let binding_scale_x = bindings_for_node
                .scale_x
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(1.0);
            let binding_scale_y = bindings_for_node
                .scale_y
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(1.0);
            // Track the inherited translate shift on EVERY frame, even
            // when the local scale didn't change. Without this, an
            // ancestor binding can advance its translate while this
            // binding sits idle — and the next time scale moves, it
            // pivots around a stale centre. Persisting the shift each
            // frame keeps the centre tracking world space continuously.
            // Each frame's shift is the frame delta (ancestor.current
            // - ancestor.last), and the ancestor's `last_translate`
            // moves to `current` further down in this loop, so this
            // does NOT double-count across frames.
            if let Some(&(sx_shift, sy_shift)) = inherited_shifts.get(node) {
                if sx_shift.abs() > f32::EPSILON || sy_shift.abs() > f32::EPSILON {
                    meta.centre.0 += sx_shift;
                    meta.centre.1 += sy_shift;
                }
            }
            let new_sx = binding_scale_x * binding_scale;
            let new_sy = binding_scale_y * binding_scale;
            if (new_sx - meta.last_scale.0).abs() > f32::EPSILON
                || (new_sy - meta.last_scale.1).abs() > f32::EPSILON
            {
                if meta.last_scale.0.abs() < f32::EPSILON || meta.last_scale.1.abs() < f32::EPSILON
                {
                    return false;
                }
                let ratio_x = new_sx / meta.last_scale.0;
                let ratio_y = new_sy / meta.last_scale.1;
                let cx_phys = meta.centre.0 * scale;
                let cy_phys = meta.centre.1 * scale;
                // For corner_radius the GPU primitive carries a
                // single radius per corner that's used uniformly for
                // the SDF rounded-rect (the renderer's `scale_corner_radius`
                // pre-multiplies by the current affine's avg-scale at
                // bake time). Use the geometric mean of the two
                // ratios so a non-uniform scale still scales the
                // radius coherently — for the uniform-scale case (the
                // common one, e.g. cn::slider's halo) this is just
                // `ratio_x = ratio_y`. Without this step a halo that's
                // baked at scale=0.5 with corner_radius=halo_size/4
                // (because the walker multiplied by 0.5) but then
                // patched up to scale=1 by growing `bounds[2,3]`
                // leaves the radius unchanged — width is now
                // 2 × radius doubled, so the previously-circular halo
                // renders as a rounded SQUARE.
                let ratio_radius = (ratio_x * ratio_y).sqrt();
                if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                    for p in prims.iter_mut() {
                        p.bounds[0] = cx_phys + (p.bounds[0] - cx_phys) * ratio_x;
                        p.bounds[1] = cy_phys + (p.bounds[1] - cy_phys) * ratio_y;
                        p.bounds[2] *= ratio_x;
                        p.bounds[3] *= ratio_y;
                        p.corner_radius[0] *= ratio_radius;
                        p.corner_radius[1] *= ratio_radius;
                        p.corner_radius[2] *= ratio_radius;
                        p.corner_radius[3] *= ratio_radius;
                    }
                }
                meta.last_scale = (new_sx, new_sy);
                any_binding_active = true;
                binding_moved = true;
            }

            // ----------------------------------------------------------------
            // Rotation: spring `binding.rotation` (degrees) plus
            // timeline-driven `rotation_timeline` (also degrees) both
            // accumulate via `MotionBindings::get_rotation()`. Apply
            // the delta vs the rotation baked into the cached batch
            // by composing onto each primitive's existing rotation
            // sin/cos and local_affine, and by rotating every
            // primitive's centre around the binding centre.
            //
            // The walker applies binding rotation as
            // `T(c) * R(θ) * T(-c)` *after* any parent CSS transform
            // had already populated the transform stack, so the
            // baked-in rotation/affine values are
            //     parent_affine * R(old_θ)
            // and each primitive's centre is in physical pixels.
            // Composing a *delta* `δ = new_θ − old_θ` as a rotation
            // around the SAME binding centre updates both correctly:
            //
            //   new_total = parent_affine * R(new_θ)
            //             = parent_affine * R(old_θ) * R(δ)
            //
            // and the centres pivot the same way:
            //
            //   p' − c = R(δ) · (p − c)
            //
            // which is what the loop below does in two passes (one
            // for the rotation/local_affine fields, one for the
            // centre / `bounds.xy`). Independent of any parent CSS
            // rotation that was on the stack at paint time — the
            // delta application doesn't depend on knowing it.
            // ----------------------------------------------------------------
            let new_rotation_rad = bindings_for_node
                .get_rotation()
                .map(|deg| deg.to_radians())
                .unwrap_or(0.0);
            let drot = new_rotation_rad - meta.last_rotation_rad;
            if drot.abs() > f32::EPSILON {
                let (sin_d, cos_d) = drot.sin_cos();
                let cx_phys = meta.centre.0 * scale;
                let cy_phys = meta.centre.1 * scale;
                if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                    for p in prims.iter_mut() {
                        // Rotate the primitive's centre around the
                        // binding centre by `δ`. `p.bounds` is the
                        // axis-aligned AABB at the (DPI-scaled)
                        // centre; the GPU vertex shader rebuilds the
                        // post-rotation AABB at draw time off the
                        // rotation sin/cos fields, so we only need
                        // the centre to track the pivot.
                        let cx = p.bounds[0] + p.bounds[2] * 0.5;
                        let cy = p.bounds[1] + p.bounds[3] * 0.5;
                        let dx = cx - cx_phys;
                        let dy = cy - cy_phys;
                        let new_dx = dx * cos_d - dy * sin_d;
                        let new_dy = dx * sin_d + dy * cos_d;
                        let new_cx = cx_phys + new_dx;
                        let new_cy = cy_phys + new_dy;
                        p.bounds[0] = new_cx - p.bounds[2] * 0.5;
                        p.bounds[1] = new_cy - p.bounds[3] * 0.5;

                        // Compose δ onto the stored rotation
                        // `[sin α, cos α, sin_ry, cos_ry]`. Working
                        // in (sin, cos) avoids any atan2 round-trip.
                        // `α + δ` keeps the Y-rotation slots intact.
                        let sin_a = p.rotation[0];
                        let cos_a = p.rotation[1];
                        p.rotation[0] = sin_a * cos_d + cos_a * sin_d;
                        p.rotation[1] = cos_a * cos_d - sin_a * sin_d;

                        // Compose δ onto local_affine. The walker
                        // builds local_affine as the parent affine
                        // post-multiplied by every transform on the
                        // stack, so the binding rotation that's
                        // baked in is in the same position (rightmost
                        // factor). To advance the rotation by `δ`,
                        // post-multiply by R(δ):
                        //
                        //   new = old · R(δ)
                        //
                        // Column 0 of new = R(δ)ᵀ applied to col 0
                        // of old, etc., which in flat [a, b, c, d]
                        // layout is the formula below. Works for
                        // any old that's `parent · R(α)` — the parent
                        // factor passes through unchanged.
                        let a = p.local_affine[0];
                        let b = p.local_affine[1];
                        let c = p.local_affine[2];
                        let d = p.local_affine[3];
                        p.local_affine[0] = a * cos_d + c * sin_d;
                        p.local_affine[1] = b * cos_d + d * sin_d;
                        p.local_affine[2] = -a * sin_d + c * cos_d;
                        p.local_affine[3] = -b * sin_d + d * cos_d;
                    }
                }
                meta.last_rotation_rad = new_rotation_rad;
                any_binding_active = true;
                binding_moved = true;
            }

            // Update the cached screen AABB even when we skip the
            // damage-rect push below — `meta.last_screen_aabb` is
            // also consumed by hit-test / scroll bookkeeping, not
            // just damage rects.
            if binding_moved {
                let aabb_after = bounds_union_of_range(&batch.primitives, &meta.primitive_range);
                meta.last_screen_aabb = aabb_after;
            }
            // No damage-rect push intentionally. Motion-bound
            // primitives live in `cached_dynamic_batch` (see the
            // function-header comment), not `cached_bg_batch`. The
            // static-cache pixels under a rotating spinner / a
            // sliding switch thumb / a spring-y progress bar don't
            // change between frames — those primitives never landed
            // in the static cache to begin with. The dispatch step
            // after this function (composite_frame) repaints the
            // dynamic batch on top of the unchanged static cache.
            //
            // Earlier (pre-PR-#42) motion-bound primitives lived in
            // the bg batch and the damage-rect path here was
            // necessary to scrub their stale pixels off the static
            // cache. After PR #42 routed motion subtrees through
            // `push_motion_subtree`, the damage rect repaint became
            // ~500 µs of wasted work per frame on cn_demo's three
            // visible spinners — clearing card-background pixels and
            // re-rendering identical ones in their place, plus the
            // text/SVG re-dispatch chain. Dropping it is the single
            // biggest steady-state win for motion-binding-only
            // frames.
            //
            // `aabb_before` is intentionally unused now; kept
            // computed above for readability + so a future
            // re-introduction (if motion bindings ever route back
            // through the bg batch) doesn't need extra plumbing.
            let _ = aabb_before;
        }
        self.last_binding_damage_rects.clear();
        // Authoritatively write `visible_anim_active` from what we
        // observed this frame. The walker resets it to `false` at the
        // top of every full paint and sets `true` only if it visits a
        // node that's actually animating — but on the fast path the
        // walker doesn't run. If we only set `true` (the previous
        // posture), a `true` set by the last full paint stays latched
        // forever once the spring settles: `is_any_animating()`
        // returns false, no value moved, `any_binding_active` stays
        // false, we leave the flag alone — but the flag is still
        // `true` from before, so Phase 5's `needs_animation_redraw =
        // raw && visible_anim` stays `true`, the chain keeps firing
        // request_redraw, and CPU pins at vsync forever.
        //
        // Writing authoritatively works on the fast path because the
        // gate (`try_fast_paint`) already requires no rebuild / no
        // relayout / no CSS activity / no scroll — the only remaining
        // dynamic source is motion bindings, which we track. Motion
        // FSM (enter/exit) drives its own redraw signal
        // (`needs_motion_redraw`) and Statefuls drive
        // `has_visible_animating_statefuls`, so neither relies on
        // `visible_anim_active` to stay alive.
        tree.set_visible_anim_active(any_binding_active);

        // Motion-subtree text / SVG primitives live in their own
        // post-overlay caches (`cached_motion_subtree_text_prims`,
        // `cached_motion_subtree_svgs`) — `apply_binding_deltas`
        // patches the dynamic batch in place but doesn't touch those
        // caches, so a spring snap-back after release would translate
        // the bg correctly while the text / SVG stayed at the old
        // position. Force the slow path when bindings are mid-flight
        // *and* we have motion-subtree text or SVG cached: the slow
        // path's walker + collect_elements_recursive re-shapes them
        // at the current binding values.
        //
        // Returning false here flows back to the caller, which
        // invalidates the static layer and falls through to the slow
        // path. The cost is one full re-walk per binding-animating
        // frame — same cost CSS / FLIP / FSM animations already pay
        // — but only when motion-subtree text / SVG is in scope. Pure
        // SDF motion bindings (cn_demo's switch / slider / progress
        // bar without text inside the motion subtree) still get the
        // fast in-place patch.
        let has_motion_overlay_glyphs = self
            .cached_motion_subtree_text_prims
            .as_ref()
            .is_some_and(|v| !v.is_empty())
            || self
                .cached_motion_subtree_svgs
                .as_ref()
                .is_some_and(|v| !v.is_empty());
        !(any_binding_active && has_motion_overlay_glyphs)
    }

    /// Re-bind the latest glyph atlas views onto the SDF pipeline.
    /// Called right before `composite_frame` (and before any other
    /// `PRIM_TEXT`-dispatching pass) on the compositor path so
    /// canvas-emitted `draw_text` primitives — whose glyphs are
    /// added to `text_ctx`'s atlas *after* the slow-path's
    /// `set_glyph_atlas` call inside `render_tree_with_motion_opt` —
    /// reach the GPU through a bind group that points at the
    /// post-growth atlas texture rather than the stale pre-growth
    /// view.
    ///
    /// Pre-fix the SDF bind group was set once per slow-path frame
    /// (line ~7110), then `collect_canvas_overlay` ran canvas
    /// closures whose `draw_text` calls could grow the atlas
    /// texture (capacity exceeded). When the atlas re-allocated,
    /// the view pointer changed but the bind group still
    /// referenced the old view. Canvas-text PRIM_TEXT primitives
    /// then sampled blank UVs and rendered invisibly — symptom:
    /// canvas_demo's "Canvas Text" sample shows the background
    /// plate but none of the headings.
    ///
    /// `set_glyph_atlas` no-ops on pointer equality so calling
    /// before every `composite_frame` is cheap when the atlas
    /// didn't grow.
    fn rebind_glyph_atlas_for_overlay(&mut self) {
        if let (Some(atlas), Some(color_atlas)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer.set_glyph_atlas(atlas, color_atlas);
        }
    }

    /// Re-dispatch foreground primitives on top of the canvas overlay.
    /// Host foreground chrome (zoom HUD, toolbar, etc.) stays visible
    /// above normal canvas-drawn content, then canvas-emitted foreground
    /// primitives (minimap chrome and other explicit foreground canvas
    /// output) land above the cached foreground.
    ///
    /// Global overlay surfaces (OverlayStack / ToastTray) are dispatched
    /// after this helper at the compositor call sites so popovers and
    /// toasts still sit above canvas foreground chrome.
    ///
    /// Background:
    /// * The slow paint walker's Pass 4 bakes foreground primitives
    ///   into the static cache alongside bg + glass content.
    /// * On compositor frames, `composite_frame` blits that cache then
    ///   dispatches canvas-emitted primitives as an overlay with
    ///   `LoadOp::Load`. The canvas overlay paints over the fg pixels
    ///   that were captured in the cache.
    /// * Net result without this hook: a host div marked
    ///   `.layer(RenderLayer::Foreground)` that sits above a canvas
    ///   appears underneath canvas content because the overlay pass
    ///   re-paints canvas on top of the cached fg pixels.
    ///
    /// Re-dispatching the cached foreground primitives after the
    /// canvas overlay restores the intended z-order, and dispatching
    /// canvas foreground after that preserves explicit
    /// `set_foreground_layer(true)` output from canvas closures:
    /// `cache (bg + glass + fg) → canvas overlay → fg overlay →
    /// canvas fg overlay → global overlays`.
    /// The double-paint of fg pixels (once from cache, once from
    /// this dispatch) is identical for opaque content.
    /// Semi-transparent fg over a canvas will blend twice
    /// (acceptable for most overlay use; revisit if a
    /// pixel-perfect single-blend variant is needed).
    ///
    fn render_foreground_overlay(
        &mut self,
        target_view: &wgpu::TextureView,
        canvas_overlays: &[&CanvasOverlay],
    ) {
        let fg_prims = self
            .cached_bg_batch
            .as_ref()
            .map(|b| b.foreground_primitives.clone())
            .filter(|v| !v.is_empty());
        let fg_glyphs = self
            .cached_fg_glyphs
            .as_ref()
            .filter(|v| !v.is_empty())
            .cloned();
        let has_canvas_fg = canvas_overlays
            .iter()
            .any(|overlay| !overlay.foreground_primitives.is_empty());
        if fg_prims.is_none() && fg_glyphs.is_none() && !has_canvas_fg {
            return;
        }
        self.rebind_glyph_atlas_for_overlay();
        if let Some(prims) = fg_prims {
            // `composite_frame` ran immediately before this dispatch
            // and overwrote the GPU's `aux_data` buffer with the
            // canvas-overlay's `merged_aux` (canvas-overlay aux +
            // dynamic aux). The foreground primitives we're about to
            // dispatch carry aux offsets that were computed against
            // the STATIC batch's `aux_data` — the buffer state from
            // before composite_frame. Without re-uploading the
            // static aux_data here, any foreground primitive that
            // references aux entries (polygon clip-paths, 3D-group
            // SDF, mesh-triangle offsets — anything routed through
            // `shift_aux_offsets`) samples garbage indices. Symptom
            // matches the documented `gotcha_dynamic_batch_aux_data`
            // bug (cn_demo spinner arcs lost their tail) but on the
            // foreground-layer dispatch path. Re-upload restores
            // the index space these primitives were built against.
            if let Some(batch) = self.cached_bg_batch.as_ref() {
                if !batch.aux_data.is_empty() {
                    self.renderer.update_aux_data_for_batch(batch);
                }
            }
            self.renderer.render_primitives_overlay(target_view, &prims);
        }
        // Foreground text glyphs go through the dedicated text shader
        // pipeline (not the SDF PRIM_TEXT path), so they need their
        // own overlay dispatch on top of the canvas. Without this,
        // text inside a `.layer(RenderLayer::Foreground)` element
        // appears muffled or hidden under canvas-drawn content.
        if let Some(glyphs) = fg_glyphs {
            self.render_text(target_view, &glyphs);
        }
        for canvas_overlay in canvas_overlays {
            if canvas_overlay.foreground_primitives.is_empty() {
                continue;
            }
            // Prior foreground/text dispatches may have restored static
            // aux_data. Canvas foreground primitives were emitted against
            // the overlay's merged aux buffer, so re-upload it before this
            // final foreground pass.
            if !canvas_overlay.aux_data.is_empty() {
                self.renderer
                    .update_aux_data_slice(&canvas_overlay.aux_data);
            }
            self.rebind_glyph_atlas_for_overlay();
            self.renderer
                .render_primitives_overlay(target_view, &canvas_overlay.foreground_primitives);
        }
    }

    /// Dispatch the OverlayStack / motion-subtree dynamic batch in its own
    /// SDF pass on top of the previously-composited surface.
    ///
    /// Pairs with `composite_frame` to split what was previously one
    /// merged SDF dispatch into two. Canvas-emitted primitives stay in
    /// `composite_frame`'s pass, carrying the canvas wrapper's
    /// `ancestor_clip_aabb` baked at emit time. Dynamic-batch primitives
    /// (OverlayStack popovers, toolbars, dialog content, motion
    /// subtrees) ride this pass with their own intrinsic clips (no-clip
    /// sentinel by default; per-prim `clip_bounds` from any inner
    /// `overflow_clip` they emit themselves).
    ///
    /// Why split? The two batches' primitives were previously
    /// concatenated into one `Vec` and dispatched in a single sorted SDF
    /// call. Z-ordering depended on `.stack_layer()` reliably bumping the
    /// OverlayStack subtree's z above every canvas-emitted primitive —
    /// any path that injects z >= overlay-stack-z into the canvas batch
    /// (an inner stack_layer push from canvas-kit, a future change to
    /// canvas record ordering) silently regresses to canvas overdraw on
    /// top of the popover, visually a horizontal split where the popover
    /// crosses the canvas's content edge. Splitting the dispatch makes
    /// the layering structural rather than z-policy-dependent.
    ///
    /// `composite_frame` overwrites the GPU's `aux_data` buffer with
    /// the canvas overlay's aux array. The dynamic batch carries its
    /// own aux (polygon-clip vertices, 3D-group ShapeDescs, mesh
    /// triangles) that primitives reference via `border[2]` /
    /// `clip_radius[3]` offsets baked relative to its own buffer. Re-
    /// upload before dispatch so those indices land on the right data;
    /// mirrors the pattern `render_foreground_overlay` uses.
    fn dispatch_dynamic_batch_overlay(&mut self, target_view: &wgpu::TextureView) {
        let Some(dyn_batch) = self.cached_dynamic_batch.as_ref() else {
            return;
        };
        if dyn_batch.primitives.is_empty() {
            return;
        }
        if !dyn_batch.aux_data.is_empty() {
            self.renderer.update_aux_data_for_batch(dyn_batch);
        }
        // Clone the primitive slice out of `cached_dynamic_batch` so the
        // immutable borrow on `self` ends before the `&mut self` call
        // on the renderer. Borrow checker only; primitives are POD.
        let prims = dyn_batch.primitives.clone();
        self.rebind_glyph_atlas_for_overlay();
        self.renderer.render_primitives_overlay(target_view, &prims);
    }

    /// Dispatch the overlay-subtree canvas pool — caret canvases and
    /// other canvases nested inside an `is_overlay_root` ancestor —
    /// AFTER `dispatch_dynamic_batch_overlay` so they paint above
    /// the popover bg/border their host overlay's dyn-batch SDF pass
    /// just laid down. Mirrors `dispatch_dynamic_batch_overlay`'s
    /// aux-rebind / glyph-atlas-rebind pattern: the previous dyn-batch
    /// dispatch left its own aux_data on the GPU, so we must re-upload
    /// this pool's aux_data before submitting its draw or every
    /// mesh / 3D-group / polygon-clip primitive in here would sample
    /// stale floats. Foreground primitives in this pool are dispatched
    /// here too, rather than in `render_foreground_overlay`, because
    /// overlay-subtree canvases need to sit above their own overlay
    /// panel while main canvas foreground must stay below global
    /// OverlayStack / ToastTray surfaces. The dynamic_images / meshes /
    /// gpu_passes channels stay with the main canvas overlay (their
    /// dispatch paths live just after this in the call sites).
    fn dispatch_overlay_canvas_overlay(
        &mut self,
        target_view: &wgpu::TextureView,
        overlay: &CanvasOverlay,
    ) {
        if overlay.primitives.is_empty() && overlay.foreground_primitives.is_empty() {
            return;
        }
        if !overlay.aux_data.is_empty() {
            self.renderer.update_aux_data_slice(&overlay.aux_data);
        }
        self.rebind_glyph_atlas_for_overlay();
        if !overlay.primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target_view, &overlay.primitives);
        }
        if !overlay.foreground_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target_view, &overlay.foreground_primitives);
        }
    }

    /// Shift every `aux_data` index on `prims` by `shift`.
    ///
    /// Used when concatenating one source's primitives into a
    /// merged `aux_data` buffer that already contains entries from
    /// another source. Three index sites need adjusting in lockstep:
    ///
    /// - **Mesh primitives** (`type_info[0] == PrimitiveType::Mesh`)
    ///   carry the triangle's aux offset in `border[2]`.
    /// - **3D-group SDF** primitives (any `prim_type` with
    ///   `border[1]` = `shape_count > 0`) also use `border[2]` for
    ///   the ShapeDesc-records offset.
    /// - **Polygon clip** is host-primitive-agnostic
    ///   (`type_info[2] == ClipType::Polygon`); the offset lives on
    ///   `clip_radius[3]`.
    fn shift_aux_offsets(prims: &mut [blinc_gpu::primitives::GpuPrimitive], shift: f32) {
        if shift == 0.0 {
            return;
        }
        for prim in prims {
            let prim_type = prim.type_info[0];
            if prim_type == blinc_gpu::primitives::PrimitiveType::Mesh as u32
                || prim.border[1] > 0.0
            {
                prim.border[2] += shift;
            }
            if prim.type_info[2] == blinc_gpu::primitives::ClipType::Polygon as u32 {
                prim.clip_radius[3] += shift;
            }
        }
    }

    /// Patch the cached background batch's CSS-animated regions in
    /// place from current `css_anim_store` values, without
    /// re-walking the tree. Mirror of [`Self::apply_binding_deltas`]
    /// for the CSS keyframe / transition path.
    ///
    /// Returns a `CssDeltaResult` indicating what the caller must
    /// do next:
    ///
    /// - `Patched`: all records handled in place; caller dispatches
    ///   the damaged-cache repaint and the text / SVG / image
    ///   re-dispatch step. No walker work needed.
    /// - `PatchedWithReemits { reemit_nodes }`: in-scope records were
    ///   patched, but one or more records carried out-of-scope
    ///   properties (width / height / padding / margin / gap /
    ///   clip-path geometry / filter blur / backdrop blur). Caller
    ///   must re-emit the listed subtrees via a scratch walker pass
    ///   into `cached_bg_batch` before the cache repaint, then run
    ///   the normal damaged-cache repaint. The corresponding damage
    ///   rects are already in `last_css_damage_rects`.
    /// - `Bail`: the patch path couldn't reach a coherent state
    ///   (cache missing, lock poisoned, opacity-zero divide-by-zero,
    ///   layer push index out of bounds, etc.). Caller must take
    ///   the full slow path.
    ///
    /// Returns `false` (caller falls back to slow path) when:
    /// - No cached batch exists yet (cold start; slow path will
    ///   populate it).
    /// - The animation targets an out-of-scope property (clip-path
    ///   geometry, filter blur, layout dimensions) — Phase 4 first
    ///   cut doesn't patch those.
    /// - The cache shape changed since recording (layer push index
    ///   out of bounds, etc.).
    /// - A property went through a divide-by-zero (opacity zero on
    ///   the previous frame).
    ///
    /// Returns `true` even when no actual patches happened — that's
    /// the steady-state case where the animation tick produced the
    /// same value as last frame (settled, paused, or at a flat
    /// portion of the curve). The caller still needs to dispatch a
    /// render to present a fresh frame.
    ///
    /// Property coverage (first cut):
    /// - opacity (LayerConfig.opacity when push index present, OR
    ///   primitive alpha channels via ratio multiplication)
    /// - background_color (matching-based: only patches primitives
    ///   whose current color equals `meta.last_background_color`,
    ///   leaving children with their own backgrounds intact)
    /// - border_color, border_width (matching-based)
    /// - corner_radius (matching-based)
    /// - shadow params + color (matching-based)
    /// - rotate_x, rotate_y (absolute write across the range —
    ///   3D rotation applies to the whole subtree per CSS spec)
    /// - filter values (absolute write across the range)
    ///
    /// Properties that trigger a bail-to-slow-path:
    /// - clip_inset / clip_circle_radius / clip_ellipse_radii
    ///   (polygon vertices live in aux_data with cross-primitive
    ///   offset bookkeeping)
    /// - filter_blur (lives in LayerConfig.effects Vec)
    /// - width / height / min_* / max_* / padding / margin / gap
    ///   (layout — needs `compute_layout`)
    /// - backdrop_* (glass material; separate dispatch path)
    pub fn apply_css_deltas(
        &mut self,
        tree: &blinc_layout::RenderTree,
        _scale: f32,
    ) -> CssDeltaResult {
        // CSS-animated primitives live on `cached_bg_batch` (the
        // walker doesn't push a `motion_subtree` for CSS animations
        // — that routing is motion-binding only). Patch through the
        // bg batch.
        let batch = match self.cached_bg_batch.as_mut() {
            Some(b) => b,
            None => {
                self.last_css_damage_rects.clear();
                return CssDeltaResult::Bail;
            }
        };
        let mut records = tree.css_anim_paint_records_mut();
        if records.is_empty() {
            // No CSS-animated nodes recorded at last paint. Nothing
            // to patch — caller re-uses the cache as-is.
            self.last_css_damage_rects.clear();
            return CssDeltaResult::Patched;
        }
        let store_arc = tree.css_anim_store();
        let store = match store_arc.lock() {
            Ok(g) => g,
            Err(_) => {
                self.last_css_damage_rects.clear();
                return CssDeltaResult::Bail;
            }
        };

        // Tracks whether anything changed this frame. Always true
        // for now (we always return `true` on no-op patches), but
        // kept for future damage-rect collection (Phase 4d).
        let mut any_changed = false;
        // Phase 4d Opt 2: collect per-record damage rects. The
        // caller's `last_css_damage_rects` slot is overwritten at
        // the end of the function; if `apply_css_deltas` bails the
        // caller falls back to the slow path and ignores rects.
        // One entry per record whose patch actually moved a pixel.
        let mut damage_rects: Vec<[f32; 4]> = Vec::new();
        // Bail sentinel — set inside the per-record loop when a
        // record can't be patched coherently (opacity-zero ratio,
        // layer push index out of bounds, layer command shape
        // mismatch). We can't `return CssDeltaResult::Bail` directly
        // from inside the loop because the live `&mut self` borrow
        // (through `batch`) plus the `records` / `store` borrows on
        // `tree` block the post-return `self.last_css_damage_rects`
        // write. Setting the flag + breaking out lets the borrows
        // drop naturally before we touch `self`.
        let mut bail = false;
        // Phase 2 of the per-element slow path: records whose active
        // animation / transition touches an out-of-scope property
        // (layout, clip-path geometry, blur) get marked for walker
        // re-emission instead of bailing the whole frame. The
        // record's screen AABB still goes into `damage_rects` so the
        // cache repaint clears that area before Phase 3 splices in
        // the fresh primitives.
        let mut reemit_nodes: Vec<LayoutNodeId> = Vec::new();

        for (&node, meta) in records.iter_mut() {
            // Per-record patch tracking — distinct from `any_changed`
            // which spans the whole loop. Only push a damage rect
            // when at least one property on THIS record moved.
            let mut record_changed = false;
            // Look up the active animation / transition for this
            // node. Prefer animations (CSS `animation: ...`); fall
            // back to transitions (CSS `transition: ...`). Both can
            // target the same node in theory, but
            // `apply_all_css_animation_props` runs before
            // `apply_all_css_transition_props` so animations win in
            // the slow-path render too.
            let active_anim = store
                .animations
                .get(&meta.stable_id)
                .filter(|a| a.is_playing);
            let active_trans = store
                .transitions
                .get(&meta.stable_id)
                .filter(|t| t.is_playing);

            let props_anim = active_anim.map(|a| &a.current_properties);
            let props_trans = active_trans.map(|t| &t.current_properties);

            // Bail if either source targets a property we can't
            // patch in place. The slow path handles those correctly.
            let touches_out_of_scope = |p: &blinc_animation::KeyframeProperties| {
                p.clip_inset.is_some()
                    || p.clip_circle_radius.is_some()
                    || p.clip_ellipse_radii.is_some()
                    || p.filter_blur.is_some()
                    || p.width.is_some()
                    || p.height.is_some()
                    || p.min_width.is_some()
                    || p.max_width.is_some()
                    || p.min_height.is_some()
                    || p.max_height.is_some()
                    || p.padding.is_some()
                    || p.margin.is_some()
                    || p.gap.is_some()
                    || p.backdrop_blur.is_some()
                    || p.backdrop_saturation.is_some()
                    || p.backdrop_brightness.is_some()
            };
            if props_anim.is_some_and(touches_out_of_scope)
                || props_trans.is_some_and(touches_out_of_scope)
            {
                // Mark this node for walker re-emission. Push its
                // AABB so the cache repaint clears that area before
                // Phase 3 splices fresh primitives. Skip the rest of
                // this record's property branches — the walker will
                // re-emit with all current animated values applied
                // together (including in-scope properties like
                // opacity that we'd otherwise patch here), so
                // patching now would just be overwritten.
                //
                // Replaces the previous `return false` +
                // `apply_css_deltas_bail_out_of_scope` trace from
                // PR #46 — the bail-on-first-out-of-scope behaviour
                // is gone; the equivalent diagnostic is
                // `apply_css_deltas_reemit_pending` emitted once
                // after the loop with the total count.
                reemit_nodes.push(node);
                if let Some(rect) = meta.last_screen_aabb {
                    damage_rects.push(rect);
                }
                continue;
            }

            // Helper: read a property from animation first, else
            // transition, else default to `None` (no change).
            let read_f32 = |get: fn(&blinc_animation::KeyframeProperties) -> Option<f32>| {
                props_anim
                    .and_then(get)
                    .or_else(|| props_trans.and_then(get))
            };
            let read_arr = |get: fn(&blinc_animation::KeyframeProperties) -> Option<[f32; 4]>| {
                props_anim
                    .and_then(get)
                    .or_else(|| props_trans.and_then(get))
            };

            // ---- opacity ----
            if let Some(new_opacity) = read_f32(|p| p.opacity) {
                if (new_opacity - meta.last_opacity).abs() > f32::EPSILON {
                    if meta.last_opacity.abs() < f32::EPSILON {
                        // Divide-by-zero — primitives were emitted
                        // at zero alpha last paint; can't recover
                        // the original color via ratio. Slow path.
                        bail = true;
                        break;
                    }
                    if let Some(push_idx) = meta.layer_push_index {
                        use blinc_gpu::primitives::LayerCommand;
                        match batch.layer_commands.get_mut(push_idx) {
                            Some(entry) => {
                                if let LayerCommand::Push { config } = &mut entry.command {
                                    config.opacity = new_opacity;
                                } else {
                                    bail = true;
                                    break;
                                }
                            }
                            None => {
                                bail = true;
                                break;
                            }
                        }
                    } else if let Some(prims) =
                        batch.primitives.get_mut(meta.primitive_range.clone())
                    {
                        let ratio = new_opacity / meta.last_opacity;
                        for p in prims.iter_mut() {
                            p.color[3] *= ratio;
                            p.color2[3] *= ratio;
                            p.border_color[3] *= ratio;
                            p.shadow_color[3] *= ratio;
                        }
                    }
                    meta.last_opacity = new_opacity;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- background_color ----
            // Matching-based: only patches primitives whose `color`
            // matches `meta.last_background_color` exactly, so a
            // child with its own bg stays untouched.
            if let Some(new_bg) = read_arr(|p| p.background_color) {
                if let Some(last_bg) = meta.last_background_color {
                    if new_bg != last_bg {
                        if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone())
                        {
                            for p in prims.iter_mut() {
                                if p.color == last_bg {
                                    p.color = new_bg;
                                }
                            }
                        }
                        meta.last_background_color = Some(new_bg);
                        any_changed = true;
                        record_changed = true;
                    }
                }
            }

            // ---- border_color ----
            if let Some(new_bc) = read_arr(|p| p.border_color) {
                if let Some(last_bc) = meta.last_border_color {
                    if new_bc != last_bc {
                        if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone())
                        {
                            for p in prims.iter_mut() {
                                if p.border_color == last_bc {
                                    p.border_color = new_bc;
                                }
                            }
                        }
                        meta.last_border_color = Some(new_bc);
                        any_changed = true;
                        record_changed = true;
                    }
                }
            }

            // ---- border_width ----
            if let Some(new_bw) = read_f32(|p| p.border_width) {
                if (new_bw - meta.last_border_width).abs() > f32::EPSILON {
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            if (p.border[0] - meta.last_border_width).abs() < f32::EPSILON {
                                p.border[0] = new_bw;
                            }
                        }
                    }
                    meta.last_border_width = new_bw;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- corner_radius ----
            if let Some(new_cr) = read_arr(|p| p.corner_radius) {
                if new_cr != meta.last_corner_radius {
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            if p.corner_radius == meta.last_corner_radius {
                                p.corner_radius = new_cr;
                            }
                        }
                    }
                    meta.last_corner_radius = new_cr;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- shadow_params ----
            if let Some(new_sp) = read_arr(|p| p.shadow_params) {
                if new_sp != meta.last_shadow_params {
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            if p.shadow == meta.last_shadow_params {
                                p.shadow = new_sp;
                            }
                        }
                    }
                    meta.last_shadow_params = new_sp;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- shadow_color ----
            if let Some(new_sc) = read_arr(|p| p.shadow_color) {
                if new_sc != meta.last_shadow_color {
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            if p.shadow_color == meta.last_shadow_color {
                                p.shadow_color = new_sc;
                            }
                        }
                    }
                    meta.last_shadow_color = new_sc;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- rotate_x (3D tilt) ----
            // perspective[0] = sin(rx), perspective[1] = cos(rx)
            if let Some(new_rx_deg) = read_f32(|p| p.rotate_x) {
                let new_rx_rad = new_rx_deg.to_radians();
                if (new_rx_rad - meta.last_rotate_x_rad).abs() > f32::EPSILON {
                    let sin_rx = new_rx_rad.sin();
                    let cos_rx = new_rx_rad.cos();
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            p.perspective[0] = sin_rx;
                            p.perspective[1] = cos_rx;
                        }
                    }
                    meta.last_rotate_x_rad = new_rx_rad;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- rotate_y (3D turn) ----
            // rotation[2] = sin(ry), rotation[3] = cos(ry)
            if let Some(new_ry_deg) = read_f32(|p| p.rotate_y) {
                let new_ry_rad = new_ry_deg.to_radians();
                if (new_ry_rad - meta.last_rotate_y_rad).abs() > f32::EPSILON {
                    let sin_ry = new_ry_rad.sin();
                    let cos_ry = new_ry_rad.cos();
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            p.rotation[2] = sin_ry;
                            p.rotation[3] = cos_ry;
                        }
                    }
                    meta.last_rotate_y_rad = new_ry_rad;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- filter values ----
            // filter_a = (grayscale, invert, sepia, hue_rotate_rad)
            // filter_b = (brightness, contrast, saturate, 0)
            let new_fa = [
                read_f32(|p| p.filter_grayscale).unwrap_or(meta.last_filter_a[0]),
                read_f32(|p| p.filter_invert).unwrap_or(meta.last_filter_a[1]),
                read_f32(|p| p.filter_sepia).unwrap_or(meta.last_filter_a[2]),
                read_f32(|p| p.filter_hue_rotate)
                    .map(|d| d.to_radians())
                    .unwrap_or(meta.last_filter_a[3]),
            ];
            let new_fb = [
                read_f32(|p| p.filter_brightness).unwrap_or(meta.last_filter_b[0]),
                read_f32(|p| p.filter_contrast).unwrap_or(meta.last_filter_b[1]),
                read_f32(|p| p.filter_saturate).unwrap_or(meta.last_filter_b[2]),
                0.0,
            ];
            if new_fa != meta.last_filter_a || new_fb != meta.last_filter_b {
                if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                    for p in prims.iter_mut() {
                        p.filter_a = new_fa;
                        p.filter_b = new_fb;
                    }
                }
                meta.last_filter_a = new_fa;
                meta.last_filter_b = new_fb;
                any_changed = true;
                record_changed = true;
            }

            // Phase 4d Opt 2: if any property on this record moved,
            // its `last_screen_aabb` is the damage rect for the
            // cache repaint. Visual-only properties (opacity / colour
            // / corner / shadow / filter / rotate_x / rotate_y) keep
            // the AABB stable, so the walker's captured rect covers
            // both pre- and post-patch pixel footprints. Records
            // with no AABB (empty primitive range — text-only
            // subtrees, off-screen) contribute nothing.
            if record_changed {
                if let Some(rect) = meta.last_screen_aabb {
                    damage_rects.push(rect);
                }
            }
        }

        // Mirror `apply_binding_deltas`: drop the lock + the
        // record borrow before the caller composes the frame.
        let _ = any_changed;
        drop(store);
        drop(records);
        if bail {
            // Partial-patch state may have landed in cached_bg_batch
            // — the loop ran some property branches before hitting
            // the bail condition. Damage rects are unreliable, the
            // slow path will repaint the whole cache anyway, so
            // clear and signal Bail.
            self.last_css_damage_rects.clear();
            return CssDeltaResult::Bail;
        }
        self.last_css_damage_rects = damage_rects;
        if reemit_nodes.is_empty() {
            CssDeltaResult::Patched
        } else {
            tracing::trace!(
                target: "blinc_app::frame_timing",
                count = reemit_nodes.len(),
                "apply_css_deltas_reemit_pending",
            );
            CssDeltaResult::PatchedWithReemits { reemit_nodes }
        }
    }

    /// Update the current cursor position in physical pixels (for @flow pointer input)
    /// Register a custom render pass with the GPU renderer.
    ///
    /// Scene3D-stage passes run inside the mesh HDR pipeline with
    /// camera context (view_proj, inv_view_proj, camera_pos) populated
    /// on `RenderPassContext`. PreRender/PostProcess stages run at
    /// their existing points in the frame.
    pub fn register_custom_pass(
        &mut self,
        pass: Box<dyn blinc_gpu::custom_pass::CustomRenderPass>,
    ) {
        self.renderer.register_custom_pass(pass);
    }

    pub fn set_cursor_position(&mut self, x: f32, y: f32) {
        self.cursor_pos = [x, y];
    }

    /// Whether the last render frame contained @flow shader elements.
    /// Used to trigger continuous redraws for animated flow shaders.
    pub fn has_active_flows(&self) -> bool {
        self.has_active_flows
    }

    /// Whether any image emitted during the last render is mid load-time
    /// fade-in. Read by the windowed runner's redraw-gate; without
    /// this signal, the fade ticks invisibly and the user has to
    /// jiggle the cursor for the image to finish fading in.
    pub fn has_pending_image_fade(&self) -> bool {
        self.has_pending_image_fade
    }

    /// Set the current render target texture for blend mode two-pass compositing.
    /// Must be called before rendering when the batch may use non-Normal blend modes.
    pub fn set_blend_target(&mut self, texture: &wgpu::Texture) {
        self.renderer.set_blend_target(texture);
    }

    /// Clear the blend target texture reference after rendering.
    pub fn clear_blend_target(&mut self) {
        self.renderer.clear_blend_target();
    }

    /// Load font data into the text rendering registry
    ///
    /// This adds fonts that will be available for text rendering.
    /// Returns the number of font faces loaded.
    pub fn load_font_data_to_registry(&mut self, data: Vec<u8>) -> usize {
        self.text_ctx.load_font_data_to_registry(data)
    }

    /// Render a layout tree to a texture view
    ///
    /// Handles everything automatically - glass, text, SVG, MSAA.
    pub fn render_tree(
        &mut self,
        tree: &RenderTree,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        // Get scale factor for HiDPI rendering
        let scale_factor = tree.scale_factor();

        // Create paint contexts for each layer with text rendering support
        let mut bg_ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);

        // Render layout layers (background and glass go to bg_ctx)
        tree.render_to_layer(&mut bg_ctx, RenderLayer::Background);
        tree.render_to_layer(&mut bg_ctx, RenderLayer::Glass);

        // Take the batch from bg_ctx before we can reuse text_ctx for fg_ctx
        let mut bg_batch = bg_ctx.take_batch();

        // Create foreground context with text rendering support
        let mut fg_ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);
        tree.render_to_layer(&mut fg_ctx, RenderLayer::Foreground);

        // Take the batch from fg_ctx before reusing text_ctx for text elements
        let mut fg_batch = fg_ctx.take_batch();

        // Collect text, SVG, image, and flow elements
        let (texts, svgs, images, _flows) = self.collect_render_elements(tree);

        // Pre-load all images into cache before rendering
        self.preload_images(&images, width as f32, height as f32);

        // Prepare text glyphs
        let mut all_glyphs = Vec::new();
        let mut css_transformed_text_prims: Vec<GpuPrimitive> = Vec::new();
        for text in &texts {
            // Convert layout TextAlign to GPU TextAlignment
            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            // Vertical alignment:
            // - Center: Use TextAnchor::Center with y at vertical center of bounds.
            //   This ensures text appears visually centered (by cap-height) rather than
            //   mathematically centered by the full bounding box (which includes descenders).
            // - Top: Text is centered within its layout box (items_center works).
            // - Baseline: Position text so baseline aligns at the font's actual baseline.
            //   Using the actual ascender from font metrics ensures all fonts align by
            //   their true baseline regardless of font family.
            let (anchor, y_pos, use_layout_height) = match text.v_align {
                TextVerticalAlign::Center => {
                    (TextAnchor::Center, text.y + text.height / 2.0, false)
                }
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    // Use the actual font ascender for baseline positioning.
                    // This ensures each font aligns by its true baseline.
                    let baseline_y = text.y + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };

            // Determine wrap width: use clip bounds if available (parent constraint),
            // otherwise use the text element's own layout width
            let wrap_width = if text.wrap {
                if let Some(clip) = text.clip_bounds {
                    // clip[2] is the clip width - use it if smaller than text width
                    clip[2].min(text.width)
                } else {
                    text.width
                }
            } else {
                text.width
            };

            // Convert font family to GPU types
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            // Only pass layout_height when we want centering within the box
            let layout_height = if use_layout_height {
                Some(text.height)
            } else {
                None
            };

            match self.text_ctx.prepare_text_with_style(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                text.color,
                anchor,
                alignment,
                Some(wrap_width),
                text.wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height,
                text.letter_spacing,
            ) {
                Ok(mut glyphs) => {
                    tracing::trace!(
                        "Prepared {} glyphs for text '{}' (font={:?}, generic={:?})",
                        glyphs.len(),
                        text.content,
                        font_name,
                        generic
                    );
                    // Apply clip bounds to all glyphs if the text element has clip bounds
                    if let Some(clip) = text.clip_bounds {
                        for glyph in &mut glyphs {
                            glyph.clip_bounds = clip;
                        }
                    }

                    if let Some(affine) = text.css_affine {
                        // CSS-transformed text: convert to SDF primitives with local_affine
                        let [a, b, c, d, tx, ty] = affine;
                        let tx_scaled = tx * scale_factor;
                        let ty_scaled = ty * scale_factor;
                        for glyph in &glyphs {
                            let gc_x = glyph.bounds[0] + glyph.bounds[2] / 2.0;
                            let gc_y = glyph.bounds[1] + glyph.bounds[3] / 2.0;
                            let new_gc_x = a * gc_x + c * gc_y + tx_scaled;
                            let new_gc_y = b * gc_x + d * gc_y + ty_scaled;
                            let mut prim = GpuPrimitive::from_glyph(glyph);
                            prim.bounds = [
                                new_gc_x - glyph.bounds[2] / 2.0,
                                new_gc_y - glyph.bounds[3] / 2.0,
                                glyph.bounds[2],
                                glyph.bounds[3],
                            ];
                            prim.local_affine = [a, b, c, d];
                            css_transformed_text_prims.push(prim);
                        }
                    } else {
                        // Pixel-snap the glyph origin to the physical grid.
                        // Quad w/h and per-glyph advance/bearing are already
                        // integers, so the only fractional part is the text
                        // origin (fractional layout position + the
                        // (line_height - cap_height)/2 vertical centering).
                        // Left unsnapped, the Linear glyph-atlas sampler
                        // smears each glyph across texels — invisible at 2x
                        // (the error halves and usually lands on integers) but
                        // visibly soft at 1x on standard-DPI displays. Rounding
                        // the origin makes every quad span an exact integer
                        // pixel range so the SDF shader hits texel centers.
                        for glyph in &mut glyphs {
                            glyph.bounds[0] = glyph.bounds[0].round();
                            glyph.bounds[1] = glyph.bounds[1].round();
                        }
                        all_glyphs.extend(glyphs);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to prepare text '{}': {:?}", text.content, e);
                }
            }
        }

        tracing::trace!(
            "Text rendering: {} texts collected, {} total glyphs prepared",
            texts.len(),
            all_glyphs.len()
        );

        // SVGs are rendered as rasterized images (not tessellated paths) for better anti-aliasing
        // They will be rendered later via render_rasterized_svgs

        self.renderer.resize(width, height);

        // Bind the real glyph atlas to the SDF pipeline whenever
        // it's available — `set_glyph_atlas` no-ops on pointer
        // equality so calling each frame is free. CSS-transformed
        // text + canvas `draw_text` calls both land glyph-sourced
        // primitives in `bg_batch.primitives`, and the SDF pipeline
        // needs the real atlas bound to sample them; the old
        // "only when CSS text exists" guard silently swallowed
        // canvas text that reached this path any other way.
        if let (Some(atlas), Some(color_atlas)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            if !css_transformed_text_prims.is_empty() {
                bg_batch.primitives.append(&mut css_transformed_text_prims);
            }
            self.renderer.set_glyph_atlas(atlas, color_atlas);
        }

        let has_glass = bg_batch.glass_count() > 0;

        // Only allocate glass textures when glass is actually used
        if has_glass {
            self.ensure_glass_textures(width, height);
        }
        let use_msaa_overlay = self.sample_count > 1;

        // Background layer uses SDF rendering (shader-based AA, no MSAA needed)
        // Foreground layer (SVGs as tessellated paths) uses MSAA for smooth edges

        if has_glass {
            // Split images by layer: background images go behind glass (get blurred),
            // glass/foreground images render on top of glass (not blurred)
            let (bg_images, fg_images): (Vec<_>, Vec<_>) = images
                .iter()
                .partition(|img| img.layer == RenderLayer::Background);

            // Pre-render background images to both backdrop and target so glass can blur them
            let has_bg_images = !bg_images.is_empty();
            if has_bg_images {
                // Take backdrop temporarily to avoid borrow conflict with render_images_ref(&mut self)
                let backdrop_tex = self.backdrop_texture.take().unwrap();
                self.renderer
                    .clear_target(&backdrop_tex.view, wgpu::Color::TRANSPARENT);
                self.renderer.clear_target(
                    target,
                    wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: self.clear_alpha as f64,
                    },
                );
                self.render_images_ref(&backdrop_tex.view, &bg_images);
                self.render_images_ref(target, &bg_images);
                self.backdrop_texture = Some(backdrop_tex);
            }

            // Glass path - batched rendering for reduced command buffer overhead:
            // Steps 1-3 are batched into a single encoder submission
            {
                let backdrop = self.backdrop_texture.as_ref().unwrap();
                self.renderer.render_glass_frame(
                    target,
                    &backdrop.view,
                    (backdrop.width, backdrop.height),
                    &bg_batch,
                    has_bg_images,
                );
            }

            // Render background paths. MSAA overlay smooths curves
            // (notch, custom shapes) when sample_count > 1; on
            // 1×-sample targets (web's webgpu/webgl backend, headless
            // tests) we still have to dispatch the paths, just without
            // the multisample resolve — otherwise SVG icons / subgraph
            // diamonds / any fill_path content stays invisible.
            if bg_batch.has_paths() {
                if use_msaa_overlay {
                    self.renderer
                        .render_paths_overlay_msaa(target, &bg_batch, self.sample_count);
                } else {
                    self.renderer.render_paths_overlay(target, &bg_batch);
                }
            }

            // Render remaining bg images to target (only if not already pre-rendered)
            if !has_bg_images {
                self.render_images_ref(target, &bg_images);
            }

            // Step 5: Render glass/foreground-layer images (on top of glass, NOT blurred)
            self.render_images_ref(target, &fg_images);

            // Step 5b: Render dynamic RGBA images (video frames, camera preview)
            if !bg_batch.dynamic_images.is_empty() {
                self.renderer
                    .render_dynamic_images(target, &bg_batch.dynamic_images);
            }
            if !fg_batch.dynamic_images.is_empty() {
                self.renderer
                    .render_dynamic_images(target, &fg_batch.dynamic_images);
            }

            // Step 6: Render foreground and text
            // Use batch-based rendering when layer effects are present
            let has_layer_effects = fg_batch.has_layer_effects();
            if has_layer_effects {
                // Layer effects require batch-based rendering to process layer commands
                fg_batch.convert_glyphs_to_primitives();
                if !fg_batch.is_empty() {
                    // Pre-load any mask images referenced by layer effects
                    self.preload_mask_images(&fg_batch);
                    self.renderer.render_overlay(target, &fg_batch);
                }
                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else if self.renderer.unified_text_rendering() {
                // Unified rendering: combine text glyphs with foreground primitives.
                // See the simple-path branch below for the rationale on
                // extending `unified_primitives` with the local
                // `all_glyphs` — `get_unified_foreground_primitives()`
                // reads from `fg_batch.glyphs`, which is empty here.
                let mut unified_primitives = fg_batch.get_unified_foreground_primitives();
                for glyph in &all_glyphs {
                    unified_primitives.push(GpuPrimitive::from_glyph(glyph));
                }
                if !unified_primitives.is_empty() {
                    self.render_unified(target, &unified_primitives);
                }

                // Render paths (not included in unified primitives).
                // MSAA when available; non-MSAA dispatch when not so
                // web (sample_count = 1) still gets fill_path content.
                if fg_batch.has_paths() {
                    if use_msaa_overlay {
                        self.renderer.render_paths_overlay_msaa(
                            target,
                            &fg_batch,
                            self.sample_count,
                        );
                    } else {
                        self.renderer.render_paths_overlay(target, &fg_batch);
                    }
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else {
                // Legacy rendering: separate foreground and text passes
                if !fg_batch.is_empty() {
                    if use_msaa_overlay {
                        self.renderer
                            .render_overlay_msaa(target, &fg_batch, self.sample_count);
                    } else {
                        self.renderer.render_overlay(target, &fg_batch);
                    }
                }

                // Step 7: Render text
                if !all_glyphs.is_empty() {
                    self.render_text(target, &all_glyphs);
                }

                // Paths in the legacy branch also need a 1×-sample
                // fallback dispatch — same reason as the unified
                // branch above.
                if fg_batch.has_paths() {
                    if use_msaa_overlay {
                        self.renderer.render_paths_overlay_msaa(
                            target,
                            &fg_batch,
                            self.sample_count,
                        );
                    } else {
                        self.renderer.render_paths_overlay(target, &fg_batch);
                    }
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            }

            // Step 8: Render text decorations (strikethrough, underline)
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            for primitives in decorations_by_layer.values() {
                if !primitives.is_empty() {
                    self.render_unified(target, primitives);
                }
            }
        } else {
            // Simple path (no glass):
            // Background uses SDF rendering (no MSAA needed)
            // Foreground uses MSAA for smooth SVG edges

            // Render background directly to target. Alpha comes from
            // `clear_alpha` so transparent windows get a fully clear
            // surface (0.0) while opaque windows keep the historical
            // opaque black (1.0) clear.
            self.renderer.render_with_clear(
                target,
                &bg_batch,
                [0.0, 0.0, 0.0, self.clear_alpha as f64],
            );

            // Render background paths. Same MSAA/non-MSAA split as the
            // glass branch — 1×-sample targets still have to dispatch.
            if bg_batch.has_paths() {
                if use_msaa_overlay {
                    self.renderer
                        .render_paths_overlay_msaa(target, &bg_batch, self.sample_count);
                } else {
                    self.renderer.render_paths_overlay(target, &bg_batch);
                }
            }

            // Render images after background primitives
            self.render_images(target, &images, width as f32, height as f32, scale_factor);

            // Render foreground and text
            // Use batch-based rendering when layer effects are present to preserve
            // layer commands for effect processing
            let has_layer_effects = fg_batch.has_layer_effects();
            if has_layer_effects {
                // Layer effects require batch-based rendering to process layer commands
                // First convert glyphs to primitives so they're included in the batch
                fg_batch.convert_glyphs_to_primitives();

                // Use render_overlay which supports layer effect processing
                if !fg_batch.is_empty() {
                    self.renderer.render_overlay(target, &fg_batch);
                }
                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else if self.renderer.unified_text_rendering() {
                // Unified rendering: combine text glyphs with foreground primitives
                // This ensures text and shapes transform together during animations.
                //
                // `get_unified_foreground_primitives()` reads from
                // `fg_batch.glyphs`, which is empty here — the glyph
                // preparation loop above writes into the local
                // `all_glyphs` vec, not into the batch. We have to
                // extend the unified primitive list with our local
                // glyphs ourselves, otherwise the unified path silently
                // drops every text element. (The `render_tree_with_motion`
                // variant doesn't hit this because it pushes glyphs
                // through a different intermediate buffer.)
                let mut unified_primitives = fg_batch.get_unified_foreground_primitives();
                for glyph in &all_glyphs {
                    unified_primitives.push(GpuPrimitive::from_glyph(glyph));
                }
                if !unified_primitives.is_empty() {
                    self.render_unified(target, &unified_primitives);
                }

                // Render paths (not included in unified primitives).
                // 1×-sample fallback dispatch so web still emits.
                if fg_batch.has_paths() {
                    if use_msaa_overlay {
                        self.renderer.render_paths_overlay_msaa(
                            target,
                            &fg_batch,
                            self.sample_count,
                        );
                    } else {
                        self.renderer.render_paths_overlay(target, &fg_batch);
                    }
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else {
                // Legacy rendering: separate foreground and text passes
                if !fg_batch.is_empty() {
                    if use_msaa_overlay {
                        self.renderer
                            .render_overlay_msaa(target, &fg_batch, self.sample_count);
                    } else {
                        self.renderer.render_overlay(target, &fg_batch);
                    }
                }

                // Render text
                if !all_glyphs.is_empty() {
                    self.render_text(target, &all_glyphs);
                }

                // Paths in the legacy branch — 1×-sample fallback so
                // web emits fill_path / stroke_path content.
                if fg_batch.has_paths() {
                    if use_msaa_overlay {
                        self.renderer.render_paths_overlay_msaa(
                            target,
                            &fg_batch,
                            self.sample_count,
                        );
                    } else {
                        self.renderer.render_paths_overlay(target, &fg_batch);
                    }
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            }

            // Render text decorations (strikethrough, underline)
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            for primitives in decorations_by_layer.values() {
                if !primitives.is_empty() {
                    self.render_unified(target, primitives);
                }
            }
        }

        // Return scratch buffers for reuse on next frame
        self.return_scratch_elements(texts, svgs, images);

        // Poll the device to free completed command buffers and prevent memory accumulation
        self.renderer.poll();

        Ok(())
    }

    /// Return element vectors to scratch pool for reuse
    #[inline]
    fn return_scratch_elements(
        &mut self,
        mut texts: Vec<TextElement>,
        mut svgs: Vec<SvgElement>,
        mut images: Vec<ImageElement>,
    ) {
        // Clear and keep capacity for reuse
        texts.clear();
        svgs.clear();
        images.clear();
        self.scratch_texts = texts;
        self.scratch_svgs = svgs;
        self.scratch_images = images;
    }

    /// Log cache statistics (called every 300 frames, ~5 seconds at 60fps).
    /// Visible at RUST_LOG=blinc_app=debug level.
    fn log_cache_stats(&mut self) {
        self.frame_count += 1;
        if self.frame_count % 300 != 1 {
            return;
        }
        let (aw, ah) = self.text_ctx.atlas_dimensions();
        let (caw, cah) = self.text_ctx.color_atlas_dimensions();
        let atlas_glyphs = self.text_ctx.atlas().glyph_count();
        let atlas_util = self.text_ctx.atlas().utilization();
        let color_glyphs = self.text_ctx.color_atlas().glyph_count();
        let color_util = self.text_ctx.color_atlas().utilization();
        let glyph_cache = self.text_ctx.glyph_cache_len();
        let glyph_cap = self.text_ctx.glyph_cache_capacity();
        let color_cache = self.text_ctx.color_glyph_cache_len();
        let color_cap = self.text_ctx.color_glyph_cache_capacity();
        let img_cache = self.image_cache.len();
        let svg_cache = self.svg_cache.len();
        let svg_atlas_entries = self.svg_atlas.entry_count();
        let svg_atlas_util = self.svg_atlas.utilization();
        let (svg_aw, svg_ah) = (self.svg_atlas.width(), self.svg_atlas.height());

        tracing::info!(
            "Cache stats [frame {}]: \
             atlas={}x{} ({} glyphs, {:.1}% used), \
             color_atlas={}x{} ({} glyphs, {:.1}% used), \
             glyph_lru={}/{}, color_glyph_lru={}/{}, \
             image={}/{}, svg_doc={}/{}, svg_atlas={}x{} ({} entries, {:.1}% used)",
            self.frame_count,
            aw,
            ah,
            atlas_glyphs,
            atlas_util * 100.0,
            caw,
            cah,
            color_glyphs,
            color_util * 100.0,
            glyph_cache,
            glyph_cap,
            color_cache,
            color_cap,
            img_cache,
            IMAGE_CACHE_CAPACITY,
            svg_cache,
            SVG_CACHE_CAPACITY,
            svg_aw,
            svg_ah,
            svg_atlas_entries,
            svg_atlas_util * 100.0,
        );
    }

    /// Drop cached entries that may have been backed by `abs_path` on
    /// disk so the next frame re-reads from disk. Used by the
    /// `hot-reload` runtime when `dx serve --hot-patch` ships an
    /// asset rebuild.
    ///
    /// Image cache is keyed by the URI string the user wrote in
    /// `image("...")`. dx hands us absolute paths, but cache keys are
    /// usually relative (e.g. `"assets/logo.png"`). We match by
    /// suffix — if the absolute path ends with the cache key the
    /// entry is dropped. This stays precise for relative paths,
    /// exact for absolute paths, and correctly skips URLs since
    /// they're not local files.
    ///
    /// SVG cache + atlas are keyed by hash of the SVG source bytes
    /// with no reverse lookup, so we clear them wholesale. Both caps
    /// are small (128 docs, ~8 MB atlas) and re-parsing happens
    /// lazily on next render — cheap relative to a full app
    /// restart.
    ///
    /// Glyph caches and font faces sit in `blinc_text` and are
    /// invalidated by the hot-reload module separately so this
    /// method stays scoped to GPU-resident render-context state.
    pub fn invalidate_asset_path(&mut self, abs_path: &std::path::Path) {
        let abs_str = abs_path.to_string_lossy();
        // Image cache: drop entries where the URI key is a path-suffix
        // of the absolute path dx sent. Matches relative ("assets/x.png"
        // ends-with → match against "/abs/.../assets/x.png") and exact
        // absolute keys, while leaving URLs (no file path component)
        // untouched.
        let dropped: Vec<String> = self
            .image_cache
            .iter()
            .filter_map(|(k, _)| {
                if !k.is_empty() && abs_str.ends_with(k.as_str()) {
                    Some(k.clone())
                } else {
                    None
                }
            })
            .collect();
        for k in &dropped {
            self.image_cache.pop(k);
            self.image_load_times.remove(k);
            self.image_fade_deadlines.remove(k);
        }
        if !dropped.is_empty() {
            tracing::info!(
                "hot-reload: dropped {} image cache entries for {}",
                dropped.len(),
                abs_path.display()
            );
        }

        // SVG cache + atlas keyed by source-bytes hash — no path
        // reverse lookup, so blanket-clear when any asset changes.
        // Sub-millisecond on the cache, frame-time-bounded on the
        // atlas (it just zeros the shadow buffer + drops shelf
        // metadata; the GPU texture stays allocated).
        let svg_n = self.svg_cache.len();
        if svg_n > 0 {
            self.svg_cache.clear();
            self.svg_atlas.clear();
            tracing::info!(
                "hot-reload: cleared {} SVG doc cache entries + atlas",
                svg_n
            );
        }
    }

    /// Ensure glass-related textures exist and are the right size.
    /// Only called when glass elements are present in the scene.
    ///
    /// We use a single texture for both rendering and sampling (backdrop_texture).
    /// The texture is rendered at half resolution to save memory (blur doesn't need full res).
    fn ensure_glass_textures(&mut self, width: u32, height: u32) {
        // Use the same texture format as the renderer's pipelines
        let format = self.renderer.texture_format();

        // Use half resolution for glass backdrop - blur effect doesn't need full resolution
        // This saves 75% of texture memory (e.g., 2.5MB -> 0.6MB for 900x700 window)
        let backdrop_width = (width / 2).max(1);
        let backdrop_height = (height / 2).max(1);

        let needs_backdrop = self
            .backdrop_texture
            .as_ref()
            .map(|t| t.width != backdrop_width || t.height != backdrop_height)
            .unwrap_or(true);

        if needs_backdrop {
            // Single texture that can be both rendered to AND sampled from
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Glass Backdrop"),
                size: wgpu::Extent3d {
                    width: backdrop_width,
                    height: backdrop_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.backdrop_texture = Some(CachedTexture {
                texture,
                view,
                width: backdrop_width,
                height: backdrop_height,
            });
        }
    }

    /// Render text glyphs
    fn render_text(&mut self, target: &wgpu::TextureView, glyphs: &[GpuGlyph]) {
        if let (Some(atlas_view), Some(color_atlas_view)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer.render_text(
                target,
                glyphs,
                atlas_view,
                color_atlas_view,
                self.text_ctx.sampler(),
            );
        }
    }

    /// Render SDF primitives and text glyphs in a unified pass
    ///
    /// This ensures text and shapes transform together during animations,
    /// preventing visual lag when parent containers have motion transforms.
    ///
    /// **Glyph atlas binding is required.** The unified rendering path
    /// converts text glyphs into `GpuPrimitive`s with `prim_type =
    /// PRIM_TEXT`, which the SDF shader's `case PRIM_TEXT:` arm
    /// samples from `glyph_atlas` / `color_glyph_atlas`. The default
    /// SDF bind group has 1×1 placeholder textures bound to those
    /// slots — without explicitly binding the real atlases via
    /// `render_primitives_overlay_with_glyphs`, every text quad
    /// samples a transparent placeholder pixel and the text renders
    /// invisibly.
    ///
    /// `render_tree_with_motion` (the desktop / mobile path) handles
    /// this via the same call. Skipping it was a `render_tree`
    /// (headless / web) path bug — text was being correctly converted
    /// to primitives but the placeholder atlas was producing zero
    /// output for every glyph quad.
    fn render_unified(&mut self, target: &wgpu::TextureView, primitives: &[GpuPrimitive]) {
        if primitives.is_empty() {
            return;
        }

        // MSAA route: draw the SDF primitive stream (including the
        // mesh triangles tessellated from solid path fills) through the
        // MSAA-enabled split pipelines so path fills get the same
        // hardware-resolved smoothing that gradient and stroke paths
        // already receive via `render_paths_overlay_msaa`. Without
        // this, unified-text mode bypassed MSAA for every `PRIM_MESH`
        // primitive, leaving tessellated vector output visibly rougher
        // than the rasterized-SVG path.
        //
        // The glyph atlas bind group is already attached to
        // `self.bind_groups.sdf` via `set_glyph_atlas()` at the start
        // of the frame (the same way
        // `render_primitives_overlay_with_glyphs` uses it), so
        // `PRIM_TEXT` primitives in the same stream still sample the
        // real atlas here.
        if self.sample_count > 1 {
            self.renderer
                .render_primitives_overlay_msaa(target, primitives, self.sample_count);
            return;
        }

        if let (Some(atlas_view), Some(color_atlas_view)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer.render_primitives_overlay_with_glyphs(
                target,
                primitives,
                atlas_view,
                color_atlas_view,
            );
        } else {
            // No atlas available — fall back to plain primitive
            // rendering. Text quads will sample placeholder pixels
            // and render invisibly, but at least non-text primitives
            // still render.
            self.renderer.render_primitives_overlay(target, primitives);
        }
    }

    /// Render text decorations for a specific z-layer
    fn render_text_decorations_for_layer(
        &mut self,
        target: &wgpu::TextureView,
        decorations_by_layer: &std::collections::HashMap<u32, Vec<GpuPrimitive>>,
        z_layer: u32,
    ) {
        if let Some(primitives) = decorations_by_layer.get(&z_layer) {
            if !primitives.is_empty() {
                self.renderer.render_primitives_overlay(target, primitives);
            }
        }
    }

    /// Render debug visualization overlays for text elements
    ///
    /// When `BLINC_DEBUG=text` (or `1`, `all`, `true`) is set, this renders:
    /// - Cyan: Text bounding box outline
    /// - Magenta: Baseline position
    /// - Green: Top of bounding box (ascender reference)
    /// - Yellow: Bottom of bounding box (descender reference)
    fn render_text_debug(&mut self, target: &wgpu::TextureView, texts: &[TextElement]) {
        let debug_primitives = generate_text_debug_primitives(texts);
        if !debug_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &debug_primitives);
        }
    }

    /// Render debug visualization overlays for all layout elements
    ///
    /// When `BLINC_DEBUG=layout` (or `all`) is set, this renders:
    /// - Semi-transparent colored rectangles for each element's bounding box
    /// - Colors cycle based on tree depth to distinguish nested elements
    fn render_layout_debug(&mut self, target: &wgpu::TextureView, tree: &RenderTree, scale: f32) {
        let debug_bounds = collect_debug_bounds(tree, scale);
        let debug_primitives = generate_layout_debug_primitives(&debug_bounds);
        if !debug_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &debug_primitives);
        }
    }

    /// Render debug visualization for motion/animations
    ///
    /// When `BLINC_DEBUG=motion` (or `all`) is set, this renders:
    /// - Top-right corner overlay showing animation stats
    /// - Number of active visual animations, layout animations, etc.
    fn render_motion_debug(
        &mut self,
        target: &wgpu::TextureView,
        tree: &RenderTree,
        width: u32,
        _height: u32,
    ) {
        let stats = tree.debug_stats();
        let mut debug_primitives = Vec::new();

        // Background for the debug panel
        let panel_width = 200.0;
        let panel_height = 100.0;
        let panel_x = width as f32 - panel_width - 10.0;
        let panel_y = 10.0;

        // Semi-transparent dark background
        debug_primitives.push(
            GpuPrimitive::rect(panel_x, panel_y, panel_width, panel_height)
                .with_color(0.1, 0.1, 0.15, 0.85)
                .with_corner_radius(6.0),
        );

        // Status indicator - green if any animations active
        let has_active = stats.visual_animation_count > 0
            || stats.layout_animation_count > 0
            || stats.animated_bounds_count > 0;

        let (r, g, b, a) = if has_active {
            (0.2, 0.9, 0.3, 1.0) // Green when animating
        } else {
            (0.4, 0.4, 0.5, 1.0) // Gray when idle
        };

        debug_primitives.push(
            GpuPrimitive::rect(panel_x + 10.0, panel_y + 12.0, 10.0, 10.0)
                .with_color(r, g, b, a)
                .with_corner_radius(5.0),
        );

        // Visual bars showing animation counts
        let bar_x = panel_x + 12.0;
        let bar_width = panel_width - 24.0;
        let bar_height = 6.0;

        // Visual animations bar (cyan)
        let visual_ratio = (stats.visual_animation_count as f32).min(10.0) / 10.0;
        if visual_ratio > 0.0 {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x, panel_y + 35.0, bar_width * visual_ratio, bar_height)
                    .with_color(0.0, 0.8, 0.9, 0.9)
                    .with_corner_radius(3.0),
            );
        }

        // Layout animations bar (magenta)
        let layout_ratio = (stats.layout_animation_count as f32).min(10.0) / 10.0;
        if layout_ratio > 0.0 {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x, panel_y + 50.0, bar_width * layout_ratio, bar_height)
                    .with_color(0.9, 0.2, 0.8, 0.9)
                    .with_corner_radius(3.0),
            );
        }

        // Animated bounds bar (yellow)
        let bounds_ratio = (stats.animated_bounds_count as f32).min(50.0) / 50.0;
        if bounds_ratio > 0.0 {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x, panel_y + 65.0, bar_width * bounds_ratio, bar_height)
                    .with_color(0.95, 0.85, 0.2, 0.9)
                    .with_corner_radius(3.0),
            );
        }

        // Scroll physics indicator (orange dots)
        let scroll_count = stats.scroll_physics_count.min(8);
        for i in 0..scroll_count {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x + (i as f32 * 14.0), panel_y + 80.0, 8.0, 8.0)
                    .with_color(1.0, 0.6, 0.2, 0.9)
                    .with_corner_radius(4.0),
            );
        }

        if !debug_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &debug_primitives);
        }
    }

    /// Render images to the backdrop texture (for images that should be blurred by glass)
    fn render_images_to_backdrop(&mut self, images: &[&ImageElement]) {
        let Some(ref backdrop) = self.backdrop_texture else {
            return;
        };
        // Create a new view to avoid borrow conflicts
        let target = backdrop
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.render_images_ref(&target, images);
    }

    /// Pre-load images into cache (call before rendering)
    ///
    /// Images with lazy loading strategy are only loaded when visible in the viewport.
    /// A buffer zone extends the viewport to preload images that are about to become visible.
    fn preload_images(
        &mut self,
        images: &[ImageElement],
        viewport_width: f32,
        viewport_height: f32,
    ) {
        // Buffer zone: load images that are within 100px of becoming visible
        const VISIBILITY_BUFFER: f32 = 100.0;

        // Eagerly load placeholder images for any lazy element with placeholder_type == 2
        // (so the placeholder is already in cache when we go to render it).
        // Use get() instead of contains() so cached placeholders are promoted to
        // MRU and survive eviction pressure from the main image puts below.
        for image in images {
            if image.placeholder_type == 2 {
                if let Some(ref placeholder_src) = image.placeholder_image {
                    if self.image_cache.get(placeholder_src).is_none() {
                        let source = blinc_image::ImageSource::from_uri(placeholder_src);
                        if let Ok(data) = blinc_image::ImageData::load(source) {
                            // `is_srgb = false` mirrors
                            // `create_image_labeled`'s existing
                            // behavior so we don't accidentally
                            // change sampling here — the 2D image
                            // pipeline treats bytes as linear.
                            let has_bc = self.renderer.has_texture_compression_bc()
                                && bc_eligible(placeholder_src, data.width(), data.height());
                            let gpu_image = self.image_ctx.create_image_maybe_compressed(
                                data.pixels(),
                                data.width(),
                                data.height(),
                                false,
                                has_bc,
                                placeholder_src,
                            );
                            self.image_cache.put(placeholder_src.clone(), gpu_image);
                        }
                    }
                }
            }
        }

        for image in images {
            // Use get() (not contains()) so the cache hit promotes the entry
            // to MRU. Without this, the LRU order is set entirely by insertion
            // order, and any new put() during scroll evicts the oldest visible
            // image first — which is exactly the row at the top of the viewport.
            // Promoting on hit during preload guarantees the eviction victims
            // are non-visible entries at the back of the cache.
            if self.image_cache.get(&image.source).is_some() {
                continue;
            }

            // Check if lazy loading is enabled (loading_strategy == 1)
            if image.loading_strategy == 1 {
                // If image has clip bounds from a scroll container, use those for visibility check
                // The clip bounds represent the visible area of the parent scroll container
                let is_visible = if let Some([clip_x, clip_y, clip_w, clip_h]) = image.clip_bounds {
                    // Check if image intersects with its clip region (+ buffer for prefetching)
                    let clip_left = clip_x - VISIBILITY_BUFFER;
                    let clip_top = clip_y - VISIBILITY_BUFFER;
                    let clip_right = clip_x + clip_w + VISIBILITY_BUFFER;
                    let clip_bottom = clip_y + clip_h + VISIBILITY_BUFFER;

                    let image_right = image.x + image.width;
                    let image_bottom = image.y + image.height;

                    image.x < clip_right
                        && image_right > clip_left
                        && image.y < clip_bottom
                        && image_bottom > clip_top
                } else {
                    // No clip bounds - check against viewport
                    let viewport_left = -VISIBILITY_BUFFER;
                    let viewport_top = -VISIBILITY_BUFFER;
                    let viewport_right = viewport_width + VISIBILITY_BUFFER;
                    let viewport_bottom = viewport_height + VISIBILITY_BUFFER;

                    let image_right = image.x + image.width;
                    let image_bottom = image.y + image.height;

                    image.x < viewport_right
                        && image_right > viewport_left
                        && image.y < viewport_bottom
                        && image_bottom > viewport_top
                };

                if !is_visible {
                    // Skip loading - image is not yet visible
                    continue;
                }
            }

            // Try to load the image - use from_uri to handle emoji://, data:, and file paths
            let source = blinc_image::ImageSource::from_uri(&image.source);
            let image_data = match blinc_image::ImageData::load(source) {
                Ok(data) => data,
                Err(e) => {
                    tracing::trace!("Failed to load image '{}': {:?}", image.source, e);
                    continue; // Skip images that fail to load
                }
            };

            // Create GPU texture — compress to BC1/BC3 when the
            // device supports it so the image cache's VRAM
            // footprint scales with asset count instead of blowing
            // past the LRU budget on 4K dashboards. `bc_eligible`
            // filters out cases where BC's 4×4-block quantization
            // would be visually unacceptable (emoji sprites, small
            // icons with smooth gradients).
            let has_bc = self.renderer.has_texture_compression_bc()
                && bc_eligible(&image.source, image_data.width(), image_data.height());
            let gpu_image = self.image_ctx.create_image_maybe_compressed(
                image_data.pixels(),
                image_data.width(),
                image_data.height(),
                false,
                has_bc,
                &image.source,
            );

            // LruCache::put evicts oldest entry if at capacity
            self.image_cache.put(image.source.clone(), gpu_image);
            // Record load time for fade-in animation
            let now = web_time::Instant::now();
            self.image_load_times.insert(image.source.clone(), now);
            // Record the exact fade-end deadline so the frame-boundary
            // redraw gate can stop firing as soon as the fade settles,
            // instead of a conservative 2 s upper bound that pinned
            // CPU for ~1.5 s of doing-nothing redraws after the fade
            // was visually done. Skip when no fade requested.
            if image.fade_duration_ms > 0 {
                if let Some(deadline) = now.checked_add(std::time::Duration::from_millis(
                    image.fade_duration_ms as u64,
                )) {
                    self.image_fade_deadlines
                        .insert(image.source.clone(), deadline);
                }
            }
        }
    }

    /// Pre-load mask images referenced in a primitive batch's layer effects
    fn preload_mask_images(&mut self, batch: &PrimitiveBatch) {
        use blinc_core::LayerEffect;
        for entry in &batch.layer_commands {
            if let blinc_gpu::primitives::LayerCommand::Push { config } = &entry.command {
                for effect in &config.effects {
                    if let LayerEffect::MaskImage { image_url, .. } = effect {
                        if self.renderer.has_mask_image(image_url) {
                            continue;
                        }
                        let source = blinc_image::ImageSource::from_uri(image_url);
                        if let Ok(data) = blinc_image::ImageData::load(source) {
                            self.renderer.load_mask_image_rgba(
                                image_url,
                                data.pixels(),
                                data.width(),
                                data.height(),
                            );
                        }
                    }
                }
            }
        }
    }

    /// Convert a CssFilter into filter_a/filter_b arrays for the image shader.
    /// Returns (filter_a, filter_b) where identity = (`[0,0,0,0]`, `[1,1,1,0]`).
    /// Extract mask gradient params and info from a MaskImage gradient.
    /// Returns (`mask_params`, `mask_info`) or zero arrays if not a gradient.
    fn mask_image_to_arrays(mask: Option<&blinc_core::MaskImage>) -> ([f32; 4], [f32; 4]) {
        match mask {
            Some(blinc_core::MaskImage::Gradient(gradient)) => match gradient {
                blinc_core::Gradient::Linear {
                    start, end, stops, ..
                } => {
                    let (sa, ea) = Self::extract_mask_alphas_from_stops(stops);
                    ([start.x, start.y, end.x, end.y], [1.0, sa, ea, 0.0])
                }
                blinc_core::Gradient::Radial {
                    center,
                    radius,
                    stops,
                    ..
                } => {
                    let (sa, ea) = Self::extract_mask_alphas_from_stops(stops);
                    ([center.x, center.y, *radius, 0.0], [2.0, sa, ea, 0.0])
                }
                blinc_core::Gradient::Conic { center, stops, .. } => {
                    let (sa, ea) = Self::extract_mask_alphas_from_stops(stops);
                    ([center.x, center.y, 0.5, 0.0], [2.0, sa, ea, 0.0])
                }
            },
            _ => ([0.0; 4], [0.0; 4]),
        }
    }

    fn extract_mask_alphas_from_stops(stops: &[blinc_core::GradientStop]) -> (f32, f32) {
        if stops.is_empty() {
            return (1.0, 0.0);
        }
        (stops[0].color.a, stops[stops.len() - 1].color.a)
    }

    fn css_filter_to_arrays(
        filter: &blinc_layout::element_style::CssFilter,
    ) -> ([f32; 4], [f32; 4]) {
        (
            [
                filter.grayscale,
                filter.invert,
                filter.sepia,
                filter.hue_rotate.to_radians(),
            ],
            [filter.brightness, filter.contrast, filter.saturate, 0.0],
        )
    }

    /// Transform clip bounds and radii by a CSS affine.
    /// When a parent div has a CSS transform (e.g. `scale(1.08)` on hover), the image
    /// clip must follow the same transform so the image fills the visually-scaled parent.
    fn transform_clip_by_affine(
        clip: [f32; 4],
        clip_radius: [f32; 4],
        affine: [f32; 6],
        scale_factor: f32,
    ) -> ([f32; 4], [f32; 4]) {
        let [a, b, c, d, tx, ty] = affine;
        let tx_s = tx * scale_factor;
        let ty_s = ty * scale_factor;
        // Transform clip center through the affine
        let ccx = clip[0] + clip[2] * 0.5;
        let ccy = clip[1] + clip[3] * 0.5;
        let new_cx = a * ccx + c * ccy + tx_s;
        let new_cy = b * ccx + d * ccy + ty_s;
        // Uniform scale for dimensions
        let s = (a * d - b * c).abs().sqrt().max(1e-6);
        let new_clip = [
            new_cx - clip[2] * s * 0.5,
            new_cy - clip[3] * s * 0.5,
            clip[2] * s,
            clip[3] * s,
        ];
        let new_radius = [
            clip_radius[0] * s,
            clip_radius[1] * s,
            clip_radius[2] * s,
            clip_radius[3] * s,
        ];
        (new_clip, new_radius)
    }

    /// Decompose a CSS affine `[a,b,c,d,tx,ty]` into position and 2x2 transform for image rendering.
    /// Input: original rect (already DPI-scaled), affine (layout coords), scale_factor.
    /// Returns: (draw_x, draw_y, draw_w, draw_h, transform_a, transform_b, transform_c, transform_d)
    /// The 2x2 matrix `[a, b, c, d]` is passed to the shader for full affine support (rotation, scale, skew).
    fn decompose_image_affine(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        affine: [f32; 6],
        scale_factor: f32,
    ) -> (f32, f32, f32, f32, f32, f32, f32, f32) {
        let [a, b, c, d, tx, ty] = affine;
        // DPI-scale the translation components
        let tx_s = tx * scale_factor;
        let ty_s = ty * scale_factor;
        // Transform center through the affine (positions are already in screen space)
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let new_cx = a * cx + c * cy + tx_s;
        let new_cy = b * cx + d * cy + ty_s;
        // Pass original bounds — the 2x2 transform is applied in the shader around the center
        (new_cx - w * 0.5, new_cy - h * 0.5, w, h, a, b, c, d)
    }

    /// Render images to target (images must be preloaded first)
    fn render_images(
        &mut self,
        target: &wgpu::TextureView,
        images: &[ImageElement],
        viewport_width: f32,
        viewport_height: f32,
        scale_factor: f32,
    ) {
        use blinc_image::{ObjectFit, ObjectPosition, calculate_fit_rects, src_rect_to_uv};

        for image in images {
            // Get cached GPU image
            let gpu_image = self.image_cache.get(&image.source);

            // Compute fade-in opacity multiplier from load time + duration
            // Returns 1.0 if no fade configured or fade complete; <1.0 during fade
            let fade_factor = if image.fade_duration_ms > 0 && gpu_image.is_some() {
                if let Some(loaded_at) = self.image_load_times.get(&image.source) {
                    let elapsed_ms = loaded_at.elapsed().as_secs_f32() * 1000.0;
                    (elapsed_ms / image.fade_duration_ms as f32).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            } else {
                1.0
            };
            // `has_pending_image_fade` is now set at the top of
            // `try_render_with_compositor` from `image_load_times`
            // — it has to be known BEFORE the cache decision so
            // the slow path is forced; the per-image dispatch
            // here doesn't need to set anything.

            // If image is not loaded and has a placeholder, render placeholder
            if gpu_image.is_none() && image.placeholder_type != 0 {
                match image.placeholder_type {
                    // Type 1: Solid color
                    1 => {
                        let color = blinc_core::Color::rgba(
                            image.placeholder_color[0],
                            image.placeholder_color[1],
                            image.placeholder_color[2],
                            image.placeholder_color[3],
                        );
                        let mut ctx = GpuPaintContext::new(viewport_width, viewport_height);
                        let rect =
                            blinc_core::Rect::new(image.x, image.y, image.width, image.height);
                        ctx.fill_rounded_rect(
                            rect,
                            blinc_core::CornerRadius::uniform(image.border_radius),
                            color,
                        );
                        let batch = ctx.take_batch();
                        self.renderer.render_overlay(target, &batch);
                    }
                    // Type 2: Image placeholder (e.g., low-res thumbnail or blur hash)
                    2 => {
                        if let Some(ref placeholder_src) = image.placeholder_image {
                            if let Some(placeholder_gpu) = self.image_cache.get(placeholder_src) {
                                let (src_rect, dst_rect) = calculate_fit_rects(
                                    placeholder_gpu.width(),
                                    placeholder_gpu.height(),
                                    image.width,
                                    image.height,
                                    ObjectFit::Cover,
                                    ObjectPosition::new(0.5, 0.5),
                                );
                                let src_uv = src_rect_to_uv(
                                    src_rect,
                                    placeholder_gpu.width(),
                                    placeholder_gpu.height(),
                                );
                                let instance = GpuImageInstance::new(
                                    image.x + dst_rect[0],
                                    image.y + dst_rect[1],
                                    dst_rect[2],
                                    dst_rect[3],
                                )
                                .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
                                .with_border_radius(image.border_radius)
                                .with_opacity(image.opacity);
                                self.renderer.render_images(
                                    target,
                                    placeholder_gpu.view(),
                                    &[instance],
                                );
                            }
                        }
                    }
                    // Type 3: Skeleton shimmer (animated gradient sweep)
                    3 => {
                        let t =
                            self.frame_count.saturating_mul(16).rem_euclid(2400) as f32 / 2400.0;
                        let base_a = image.placeholder_color[3].max(0.4);
                        let base = blinc_core::Color::rgba(
                            image.placeholder_color[0],
                            image.placeholder_color[1],
                            image.placeholder_color[2],
                            base_a,
                        );
                        let highlight_a = (base_a + 0.25).min(1.0);
                        let highlight = blinc_core::Color::rgba(
                            (image.placeholder_color[0] + 0.15).min(1.0),
                            (image.placeholder_color[1] + 0.15).min(1.0),
                            (image.placeholder_color[2] + 0.15).min(1.0),
                            highlight_a,
                        );
                        let mut ctx = GpuPaintContext::new(viewport_width, viewport_height);
                        let rect =
                            blinc_core::Rect::new(image.x, image.y, image.width, image.height);
                        // Base background
                        ctx.fill_rounded_rect(
                            rect,
                            blinc_core::CornerRadius::uniform(image.border_radius),
                            base,
                        );
                        // Shimmer band — narrow vertical strip swept horizontally
                        let band_w = (image.width * 0.25).max(40.0);
                        let band_x = image.x + (image.width + band_w) * t - band_w;
                        let band_rect =
                            blinc_core::Rect::new(band_x, image.y, band_w, image.height);
                        ctx.fill_rounded_rect(
                            band_rect,
                            blinc_core::CornerRadius::uniform(image.border_radius),
                            highlight,
                        );
                        let batch = ctx.take_batch();
                        self.renderer.render_overlay(target, &batch);
                        // Mark frame as needing redraw for animation
                        self.has_active_flows = true;
                    }
                    _ => {}
                }
                continue;
            }

            let Some(gpu_image) = gpu_image else {
                continue; // Skip images that failed to load
            };

            // Convert object_fit byte to ObjectFit enum
            let object_fit = match image.object_fit {
                0 => ObjectFit::Cover,
                1 => ObjectFit::Contain,
                2 => ObjectFit::Fill,
                3 => ObjectFit::ScaleDown,
                4 => ObjectFit::None,
                _ => ObjectFit::Cover,
            };

            // Create ObjectPosition from array
            let object_position =
                ObjectPosition::new(image.object_position[0], image.object_position[1]);

            // Calculate fit rectangles
            let (src_rect, dst_rect) = calculate_fit_rects(
                gpu_image.width(),
                gpu_image.height(),
                image.width,
                image.height,
                object_fit,
                object_position,
            );

            // Convert src_rect to UV coordinates
            let src_uv = src_rect_to_uv(src_rect, gpu_image.width(), gpu_image.height());

            // Apply CSS affine transform if present
            let base_x = image.x + dst_rect[0];
            let base_y = image.y + dst_rect[1];
            let base_w = dst_rect[2];
            let base_h = dst_rect[3];

            let (draw_x, draw_y, draw_w, draw_h, ta, tb, tc, td) = if let Some(affine) =
                image.css_affine
            {
                Self::decompose_image_affine(base_x, base_y, base_w, base_h, affine, scale_factor)
            } else {
                (base_x, base_y, base_w, base_h, 1.0, 0.0, 0.0, 1.0)
            };

            // Pre-compute effective clip (transformed by CSS affine if present)
            let effective_clip = image.clip_bounds.map(|clip| {
                if let Some(affine) = image.css_affine {
                    Self::transform_clip_by_affine(clip, image.clip_radius, affine, scale_factor)
                } else {
                    (clip, image.clip_radius)
                }
            });

            // Render shadow before image if present
            if let Some(ref shadow) = image.shadow {
                let mut shadow_ctx = GpuPaintContext::new(viewport_width, viewport_height);
                // Push scroll/parent clip so shadow doesn't escape the container
                if let Some(clip) = image.clip_bounds {
                    shadow_ctx.push_clip(blinc_core::ClipShape::RoundedRect {
                        rect: blinc_core::Rect::new(clip[0], clip[1], clip[2], clip[3]),
                        corner_radius: blinc_core::CornerRadius {
                            top_left: image.clip_radius[0],
                            top_right: image.clip_radius[1],
                            bottom_right: image.clip_radius[2],
                            bottom_left: image.clip_radius[3],
                        },
                        corner_shape: blinc_core::CornerShape::ROUND,
                    });
                }
                let shadow_rect =
                    blinc_core::Rect::new(image.x, image.y, image.width, image.height);
                let shadow_radius = blinc_core::CornerRadius::uniform(image.border_radius);
                shadow_ctx.draw_shadow(shadow_rect, shadow_radius, *shadow);
                let shadow_batch = shadow_ctx.take_batch();
                self.renderer.render_overlay(target, &shadow_batch);
            }

            // Create GPU instance with proper positioning
            let mut instance = GpuImageInstance::new(draw_x, draw_y, draw_w, draw_h)
                .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
                .with_tint(image.tint[0], image.tint[1], image.tint[2], image.tint[3])
                .with_border_radius(image.border_radius)
                .with_opacity(image.opacity * fade_factor)
                .with_transform(ta, tb, tc, td)
                .with_filter(image.filter_a, image.filter_b);

            // Render border inside the image shader (same SDF, perfect transform alignment)
            if image.border_width > 0.0 {
                instance = instance.with_image_border(
                    image.border_width,
                    image.border_color.r,
                    image.border_color.g,
                    image.border_color.b,
                    image.border_color.a,
                );
            }

            // Apply mask gradient
            if image.mask_info[0] > 0.5 {
                instance.mask_params = image.mask_params;
                instance.mask_info = image.mask_info;
            }

            // Apply clip bounds (primary rounded clip)
            if let Some((clip, clip_r)) = effective_clip {
                instance = instance.with_clip_rounded_rect_corners(
                    clip[0], clip[1], clip[2], clip[3], clip_r[0], clip_r[1], clip_r[2], clip_r[3],
                );
            }
            // Apply secondary scroll clip (sharp rect)
            if let Some(sc) = image.scroll_clip {
                instance = instance.with_clip2_rect(sc[0], sc[1], sc[2], sc[3]);
            }

            // Render the image
            self.renderer
                .render_images(target, gpu_image.view(), &[instance]);
        }
    }

    /// Render images to target from references (images must be preloaded first)
    fn render_images_ref(&mut self, target: &wgpu::TextureView, images: &[&ImageElement]) {
        use blinc_image::{ObjectFit, ObjectPosition, calculate_fit_rects, src_rect_to_uv};

        for image in images {
            // Get cached GPU image
            let Some(gpu_image) = self.image_cache.get(&image.source) else {
                continue; // Skip images that failed to load
            };

            // Compute fade-in opacity multiplier
            let fade_factor = if image.fade_duration_ms > 0 {
                if let Some(loaded_at) = self.image_load_times.get(&image.source) {
                    let elapsed_ms = loaded_at.elapsed().as_secs_f32() * 1000.0;
                    (elapsed_ms / image.fade_duration_ms as f32).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            } else {
                1.0
            };
            // Fade signal lives on `has_pending_image_fade`, set
            // at the top of `try_render_with_compositor` from
            // `image_load_times`. No per-dispatch set needed here.

            // Convert object_fit byte to ObjectFit enum
            let object_fit = match image.object_fit {
                0 => ObjectFit::Cover,
                1 => ObjectFit::Contain,
                2 => ObjectFit::Fill,
                3 => ObjectFit::ScaleDown,
                4 => ObjectFit::None,
                _ => ObjectFit::Cover,
            };

            // Create ObjectPosition from array
            let object_position =
                ObjectPosition::new(image.object_position[0], image.object_position[1]);

            // Calculate fit rectangles
            let (src_rect, dst_rect) = calculate_fit_rects(
                gpu_image.width(),
                gpu_image.height(),
                image.width,
                image.height,
                object_fit,
                object_position,
            );

            // Convert src_rect to UV coordinates
            let src_uv = src_rect_to_uv(src_rect, gpu_image.width(), gpu_image.height());

            // Apply CSS affine transform if present
            let base_x = image.x + dst_rect[0];
            let base_y = image.y + dst_rect[1];
            let base_w = dst_rect[2];
            let base_h = dst_rect[3];

            // render_images_ref is called for backdrop images; no scale_factor available,
            // but affine translation is already in screen coords for backdrop path
            let (draw_x, draw_y, draw_w, draw_h, ta, tb, tc, td) =
                if let Some(affine) = image.css_affine {
                    Self::decompose_image_affine(base_x, base_y, base_w, base_h, affine, 1.0)
                } else {
                    (base_x, base_y, base_w, base_h, 1.0, 0.0, 0.0, 1.0)
                };

            // Pre-compute effective clip (transformed by CSS affine if present)
            let effective_clip = image.clip_bounds.map(|clip| {
                if let Some(affine) = image.css_affine {
                    Self::transform_clip_by_affine(clip, image.clip_radius, affine, 1.0)
                } else {
                    (clip, image.clip_radius)
                }
            });

            // Create GPU instance with proper positioning
            let mut instance = GpuImageInstance::new(draw_x, draw_y, draw_w, draw_h)
                .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
                .with_tint(image.tint[0], image.tint[1], image.tint[2], image.tint[3])
                .with_border_radius(image.border_radius)
                .with_opacity(image.opacity * fade_factor)
                .with_transform(ta, tb, tc, td)
                .with_filter(image.filter_a, image.filter_b);

            // Render border inside the image shader (same SDF, perfect transform alignment)
            if image.border_width > 0.0 {
                instance = instance.with_image_border(
                    image.border_width,
                    image.border_color.r,
                    image.border_color.g,
                    image.border_color.b,
                    image.border_color.a,
                );
            }

            // Apply mask gradient
            if image.mask_info[0] > 0.5 {
                instance.mask_params = image.mask_params;
                instance.mask_info = image.mask_info;
            }

            // Apply clip bounds (primary rounded clip)
            if let Some((clip, clip_r)) = effective_clip {
                instance = instance.with_clip_rounded_rect_corners(
                    clip[0], clip[1], clip[2], clip[3], clip_r[0], clip_r[1], clip_r[2], clip_r[3],
                );
            }
            // Apply secondary scroll clip (sharp rect)
            if let Some(sc) = image.scroll_clip {
                instance = instance.with_clip2_rect(sc[0], sc[1], sc[2], sc[3]);
            }

            // Render the image
            self.renderer
                .render_images(target, gpu_image.view(), &[instance]);
        }
    }

    /// Render an SVG element with clipping and opacity support
    fn render_svg_element(&mut self, ctx: &mut GpuPaintContext, svg: &SvgElement) {
        // Skip completely transparent SVGs
        if svg.motion_opacity <= 0.001 {
            return;
        }

        // Skip SVGs completely outside their clip bounds
        if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
            let svg_right = svg.x + svg.width;
            let svg_bottom = svg.y + svg.height;
            let clip_right = clip_x + clip_w;
            let clip_bottom = clip_y + clip_h;

            // Check if SVG is completely outside clip bounds
            if svg.x >= clip_right
                || svg_right <= clip_x
                || svg.y >= clip_bottom
                || svg_bottom <= clip_y
            {
                return;
            }
        }

        // Hash the SVG source for cache lookup (faster than using string as key)
        let svg_hash = {
            let mut hasher = DefaultHasher::new();
            svg.source.hash(&mut hasher);
            hasher.finish()
        };

        // Try cache lookup first, parse only on miss
        let doc = if let Some(cached) = self.svg_cache.get(&svg_hash) {
            cached.clone()
        } else {
            let Ok(parsed) = SvgDocument::from_str(&svg.source) else {
                return;
            };
            self.svg_cache.put(svg_hash, parsed.clone());
            parsed
        };

        // Apply clipping if present
        if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
            ctx.push_clip(blinc_core::ClipShape::rect(Rect::new(
                clip_x, clip_y, clip_w, clip_h,
            )));
        }

        // Apply opacity if not fully opaque
        if svg.motion_opacity < 1.0 {
            ctx.push_opacity(svg.motion_opacity);
        }

        // Render the SVG with optional CSS overrides
        let has_css_overrides = svg.tint.is_some()
            || svg.fill.is_some()
            || svg.stroke.is_some()
            || svg.stroke_width.is_some();
        if has_css_overrides {
            self.render_svg_with_overrides(
                ctx,
                &doc,
                svg.x,
                svg.y,
                svg.width,
                svg.height,
                svg.tint,
                svg.fill,
                svg.stroke,
                svg.stroke_width,
            );
        } else {
            doc.render_fit(ctx, Rect::new(svg.x, svg.y, svg.width, svg.height));
        }

        // Pop opacity if applied
        if svg.motion_opacity < 1.0 {
            ctx.pop_opacity();
        }

        // Pop clip if applied
        if svg.clip_bounds.is_some() {
            ctx.pop_clip();
        }
    }

    /// Render an SVG with CSS overrides for fill, stroke, stroke-width, and tint
    #[allow(clippy::too_many_arguments)]
    fn render_svg_with_overrides(
        &self,
        ctx: &mut GpuPaintContext,
        doc: &SvgDocument,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        tint: Option<blinc_core::Color>,
        fill: Option<blinc_core::Color>,
        stroke: Option<blinc_core::Color>,
        stroke_width: Option<f32>,
    ) {
        use blinc_svg::SvgDrawCommand;

        // Calculate scale to fit within bounds while maintaining aspect ratio
        let scale_x = width / doc.width;
        let scale_y = height / doc.height;
        let scale = scale_x.min(scale_y);

        // Center within bounds
        let scaled_width = doc.width * scale;
        let scaled_height = doc.height * scale;
        let offset_x = x + (width - scaled_width) / 2.0;
        let offset_y = y + (height - scaled_height) / 2.0;

        let commands = doc.commands();

        for cmd in commands {
            match cmd {
                SvgDrawCommand::FillPath { path, brush } => {
                    let scaled = scale_and_translate_path(&path, offset_x, offset_y, scale);
                    // Priority: fill > tint > original brush
                    let fill_brush = if let Some(f) = fill {
                        Brush::Solid(f)
                    } else if let Some(t) = tint {
                        Brush::Solid(t)
                    } else {
                        brush.clone()
                    };
                    ctx.fill_path(&scaled, fill_brush);
                }
                SvgDrawCommand::StrokePath {
                    path,
                    stroke: orig_stroke,
                    brush,
                } => {
                    let scaled = scale_and_translate_path(&path, offset_x, offset_y, scale);
                    // Apply stroke-width override or scale original
                    let sw = stroke_width.unwrap_or(orig_stroke.width) * scale;
                    let scaled_stroke = Stroke::new(sw)
                        .with_cap(orig_stroke.cap)
                        .with_join(orig_stroke.join);
                    // Priority: stroke > tint > original brush
                    let stroke_brush = if let Some(s) = stroke {
                        Brush::Solid(s)
                    } else if let Some(t) = tint {
                        Brush::Solid(t)
                    } else {
                        brush.clone()
                    };
                    ctx.stroke_path(&scaled, &scaled_stroke, stroke_brush);
                }
            }
        }
    }

    /// Render SVG elements using CPU rasterization for high-quality anti-aliased output
    ///
    /// This method rasterizes SVGs using resvg/tiny-skia and renders them as textures,
    /// providing much better anti-aliasing than tessellation-based path rendering.
    ///
    /// The `scale_factor` parameter is the display's DPI scale (e.g., 2.0 for Retina).
    /// SVGs are rasterized at physical pixel resolution for crisp rendering on HiDPI displays.
    fn render_rasterized_svgs(
        &mut self,
        target: &wgpu::TextureView,
        svgs: &[SvgElement],
        scale_factor: f32,
    ) {
        // Evict stale atlas entries from the previous frame BEFORE
        // the loop so every UV coordinate computed below stays valid
        // for the entire render pass. Doing this mid-loop (inside
        // `insert`) would repack surviving entries to new shelf
        // positions, invalidating UVs already pushed into the
        // instance buffer → visible blink on animated SVGs.
        self.svg_atlas.begin_frame(&self.device);

        // Collect all instances for a single batched draw call
        let mut instances: Vec<GpuImageInstance> = Vec::with_capacity(svgs.len());

        for svg in svgs {
            // Skip completely transparent SVGs
            if svg.motion_opacity <= 0.001 {
                continue;
            }

            // Skip SVGs completely outside their clip bounds
            if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
                let svg_right = svg.x + svg.width;
                let svg_bottom = svg.y + svg.height;
                let clip_right = clip_x + clip_w;
                let clip_bottom = clip_y + clip_h;

                if svg.x >= clip_right
                    || svg_right <= clip_x
                    || svg.y >= clip_bottom
                    || svg_bottom <= clip_y
                {
                    continue;
                }
            }

            // Rasterize at physical pixel resolution.
            //
            // `svg.width` / `svg.height` come out of `collect_elements_recursive`
            // already multiplied by `tree.scale_factor()` (see the SVG branch
            // around line 3066: `base_width = bounds.width * scale`), so they
            // are in *physical* pixels, not logical pixels — the same units
            // the GPU draw quad will be sized in. Multiplying by `scale_factor`
            // a second time here used to rasterize each icon at 4× its drawn
            // area on Retina (9× on 3× DPR), bloating the SVG atlas
            // (`cn_demo` was hitting the 4096×4096 ceiling on the first frame
            // and burning ~134 MB of CPU+GPU memory between the two mirror
            // buffers in `svg_atlas.rs`). resvg already does sub-pixel AA at
            // the target resolution, so 1:1 physical-pixel rasterization is
            // sharp enough; if a future workload turns up edge cases that
            // need supersampling, gate it behind an explicit knob rather than
            // a silent multiply.
            let raster_width = (svg.width.ceil() as u32).max(1);
            let raster_height = (svg.height.ceil() as u32).max(1);

            // Detect tintable SVGs: simple currentColor icons that can use shader tinting
            // instead of CPU re-rasterization per color variant.
            // Tintable = has tint, no other overrides, source uses currentColor.
            let is_tintable = svg.tint.is_some()
                && svg.fill.is_none()
                && svg.stroke.is_none()
                && svg.stroke_width.is_none()
                && svg.stroke_dasharray.is_none()
                && svg.stroke_dashoffset.is_none()
                && svg.svg_path_data.is_none()
                && svg.tag_overrides.is_empty()
                && svg.source.contains("currentColor");

            // Compute cache key: hash of (svg_source, width, height, scale, tint, fill, stroke, stroke_width)
            // For tintable SVGs, exclude tint from hash so all color variants share one texture.
            let cache_key = {
                let mut hasher = DefaultHasher::new();
                svg.source.hash(&mut hasher);
                raster_width.hash(&mut hasher);
                raster_height.hash(&mut hasher);
                if is_tintable {
                    // Sentinel byte to distinguish from non-tintable hashes
                    255u8.hash(&mut hasher);
                } else if let Some(tint) = &svg.tint {
                    tint.r.to_bits().hash(&mut hasher);
                    tint.g.to_bits().hash(&mut hasher);
                    tint.b.to_bits().hash(&mut hasher);
                    tint.a.to_bits().hash(&mut hasher);
                }
                if let Some(fill) = &svg.fill {
                    1u8.hash(&mut hasher);
                    fill.r.to_bits().hash(&mut hasher);
                    fill.g.to_bits().hash(&mut hasher);
                    fill.b.to_bits().hash(&mut hasher);
                    fill.a.to_bits().hash(&mut hasher);
                }
                if let Some(stroke) = &svg.stroke {
                    2u8.hash(&mut hasher);
                    stroke.r.to_bits().hash(&mut hasher);
                    stroke.g.to_bits().hash(&mut hasher);
                    stroke.b.to_bits().hash(&mut hasher);
                    stroke.a.to_bits().hash(&mut hasher);
                }
                if let Some(sw) = &svg.stroke_width {
                    3u8.hash(&mut hasher);
                    sw.to_bits().hash(&mut hasher);
                }
                if let Some(ref da) = svg.stroke_dasharray {
                    4u8.hash(&mut hasher);
                    for v in da {
                        v.to_bits().hash(&mut hasher);
                    }
                }
                if let Some(offset) = &svg.stroke_dashoffset {
                    5u8.hash(&mut hasher);
                    offset.to_bits().hash(&mut hasher);
                }
                if let Some(ref path_data) = svg.svg_path_data {
                    6u8.hash(&mut hasher);
                    path_data.hash(&mut hasher);
                }
                // Hash per-tag style overrides
                if !svg.tag_overrides.is_empty() {
                    7u8.hash(&mut hasher);
                    // Sort keys for deterministic hashing
                    let mut keys: Vec<&String> = svg.tag_overrides.keys().collect();
                    keys.sort();
                    for key in keys {
                        key.hash(&mut hasher);
                        if let Some(ts) = svg.tag_overrides.get(key) {
                            if let Some(f) = &ts.fill {
                                for v in f {
                                    v.to_bits().hash(&mut hasher);
                                }
                            }
                            if let Some(s) = &ts.stroke {
                                for v in s {
                                    v.to_bits().hash(&mut hasher);
                                }
                            }
                            if let Some(sw) = &ts.stroke_width {
                                sw.to_bits().hash(&mut hasher);
                            }
                            if let Some(op) = &ts.opacity {
                                op.to_bits().hash(&mut hasher);
                            }
                        }
                    }
                }
                hasher.finish()
            };

            // Check atlas first — skip string manipulation entirely on cache hit
            if self.svg_atlas.get(cache_key).is_none() {
                // Cache miss: build SVG source with inline attribute overrides
                let has_overrides = svg.tint.is_some()
                    || svg.fill.is_some()
                    || svg.stroke.is_some()
                    || svg.stroke_width.is_some()
                    || svg.stroke_dasharray.is_some()
                    || svg.stroke_dashoffset.is_some()
                    || svg.svg_path_data.is_some()
                    || !svg.tag_overrides.is_empty();

                fn color_val(c: blinc_core::Color) -> String {
                    if c.a < 1.0 {
                        format!(
                            "rgba({},{},{},{})",
                            (c.r * 255.0) as u8,
                            (c.g * 255.0) as u8,
                            (c.b * 255.0) as u8,
                            c.a
                        )
                    } else {
                        format!(
                            "#{:02x}{:02x}{:02x}",
                            (c.r * 255.0) as u8,
                            (c.g * 255.0) as u8,
                            (c.b * 255.0) as u8
                        )
                    }
                }

                let effective_source = if has_overrides {
                    // Build attribute string to inject into the root <svg> tag
                    let mut svg_attrs = String::new();
                    if let Some(fill) = svg.fill {
                        svg_attrs.push_str(&format!(r#" fill="{}""#, color_val(fill)));
                    }
                    if let Some(stroke) = svg.stroke {
                        svg_attrs.push_str(&format!(r#" stroke="{}""#, color_val(stroke)));
                    }
                    if let Some(sw) = svg.stroke_width {
                        svg_attrs.push_str(&format!(r#" stroke-width="{}""#, sw));
                    }
                    if let Some(ref da) = svg.stroke_dasharray {
                        let da_str = da
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        svg_attrs.push_str(&format!(r#" stroke-dasharray="{}""#, da_str));
                    }
                    if let Some(offset) = svg.stroke_dashoffset {
                        svg_attrs.push_str(&format!(r#" stroke-dashoffset="{}""#, offset));
                    }

                    // Strip existing attribute from a tag region in the SVG string.
                    fn strip_attr(s: &mut String, tag_start: usize, tag_end: usize, attr: &str) {
                        let region = &s[tag_start..tag_end];
                        let attr_eq = format!("{}=", attr);
                        if let Some(attr_offset) = region.find(&attr_eq) {
                            let abs_attr = tag_start + attr_offset;
                            let after_eq = abs_attr + attr.len() + 1;
                            if after_eq < s.len() {
                                let quote = s.as_bytes()[after_eq];
                                if quote == b'"' || quote == b'\'' {
                                    if let Some(end_quote) = s[after_eq + 1..].find(quote as char) {
                                        let remove_end = after_eq + 1 + end_quote + 1;
                                        let remove_start =
                                            if abs_attr > 0 && s.as_bytes()[abs_attr - 1] == b' ' {
                                                abs_attr - 1
                                            } else {
                                                abs_attr
                                            };
                                        s.replace_range(remove_start..remove_end, "");
                                    }
                                }
                            }
                        }
                    }

                    let mut modified = String::from(&*svg.source);

                    // Strip existing attributes from the <svg> tag
                    if let Some(svg_close) = modified.find('>') {
                        if svg.stroke.is_some() {
                            strip_attr(&mut modified, 0, svg_close, "stroke");
                        }
                        if svg.fill.is_some() {
                            let svg_close = modified.find('>').unwrap_or(0);
                            strip_attr(&mut modified, 0, svg_close, "fill");
                        }
                        if svg.stroke_width.is_some() {
                            let svg_close = modified.find('>').unwrap_or(0);
                            strip_attr(&mut modified, 0, svg_close, "stroke-width");
                        }
                        if svg.stroke_dasharray.is_some() {
                            let svg_close = modified.find('>').unwrap_or(0);
                            strip_attr(&mut modified, 0, svg_close, "stroke-dasharray");
                        }
                        if svg.stroke_dashoffset.is_some() {
                            let svg_close = modified.find('>').unwrap_or(0);
                            strip_attr(&mut modified, 0, svg_close, "stroke-dashoffset");
                        }
                    }

                    // Insert new attributes into the opening <svg tag
                    if !svg_attrs.is_empty() {
                        if let Some(pos) = modified.find('>') {
                            let insert_pos = if pos > 0 && modified.as_bytes()[pos - 1] == b'/' {
                                pos - 1
                            } else {
                                pos
                            };
                            modified.insert_str(insert_pos, &svg_attrs);
                        }
                    }

                    // Override fill/stroke on individual shape elements
                    let shape_tags = [
                        "<path",
                        "<circle",
                        "<rect",
                        "<polygon",
                        "<line",
                        "<ellipse",
                        "<polyline",
                    ];
                    for tag in &shape_tags {
                        let tag_name = tag.trim_start_matches('<');
                        let tag_style = svg.tag_overrides.get(tag_name);

                        // Per-tag overrides take priority over global element-level overrides
                        let effective_fill: Option<blinc_core::Color> = tag_style
                            .and_then(|ts| ts.fill)
                            .map(|c| blinc_core::Color::rgba(c[0], c[1], c[2], c[3]))
                            .or(svg.fill);
                        let effective_stroke: Option<blinc_core::Color> = tag_style
                            .and_then(|ts| ts.stroke)
                            .map(|c| blinc_core::Color::rgba(c[0], c[1], c[2], c[3]))
                            .or(svg.stroke);
                        let effective_stroke_width: Option<f32> = tag_style
                            .and_then(|ts| ts.stroke_width)
                            .or(svg.stroke_width);
                        let effective_dasharray: Option<Vec<f32>> = tag_style
                            .and_then(|ts| ts.stroke_dasharray.clone())
                            .or_else(|| svg.stroke_dasharray.clone());
                        let effective_dashoffset: Option<f32> = tag_style
                            .and_then(|ts| ts.stroke_dashoffset)
                            .or(svg.stroke_dashoffset);
                        let effective_opacity: Option<f32> = tag_style.and_then(|ts| ts.opacity);

                        let mut search_from = 0;
                        while let Some(tag_start) = modified[search_from..].find(tag) {
                            let abs_tag = search_from + tag_start;
                            let abs_start = abs_tag + tag.len();
                            if let Some(close) = modified[abs_start..].find('>') {
                                let abs_close = abs_start + close;

                                if effective_stroke.is_some() {
                                    strip_attr(&mut modified, abs_tag, abs_close, "stroke-width");
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(&mut modified, abs_tag, new_close, "stroke");
                                }
                                if effective_fill.is_some() {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(&mut modified, abs_tag, new_close, "fill");
                                }
                                if effective_stroke_width.is_some() {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(&mut modified, abs_tag, new_close, "stroke-width");
                                }
                                if effective_dasharray.is_some() {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(
                                        &mut modified,
                                        abs_tag,
                                        new_close,
                                        "stroke-dasharray",
                                    );
                                }
                                if effective_dashoffset.is_some() {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(
                                        &mut modified,
                                        abs_tag,
                                        new_close,
                                        "stroke-dashoffset",
                                    );
                                }
                                if effective_opacity.is_some() {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(&mut modified, abs_tag, new_close, "opacity");
                                }
                                if svg.svg_path_data.is_some() && *tag == "<path" {
                                    let new_close = abs_start
                                        + modified[abs_start..].find('>').unwrap_or(close);
                                    strip_attr(&mut modified, abs_tag, new_close, "d");
                                }

                                // Recompute close position after stripping
                                let abs_close =
                                    abs_start + modified[abs_start..].find('>').unwrap_or(0);
                                let is_self_close =
                                    abs_close > 0 && modified.as_bytes()[abs_close - 1] == b'/';
                                let insert_at = if is_self_close {
                                    abs_close - 1
                                } else {
                                    abs_close
                                };
                                let mut elem_attrs = String::new();
                                if let Some(fill) = effective_fill {
                                    elem_attrs.push_str(&format!(r#" fill="{}""#, color_val(fill)));
                                }
                                if let Some(stroke) = effective_stroke {
                                    elem_attrs
                                        .push_str(&format!(r#" stroke="{}""#, color_val(stroke)));
                                }
                                if let Some(sw) = effective_stroke_width {
                                    elem_attrs.push_str(&format!(r#" stroke-width="{}""#, sw));
                                }
                                if let Some(ref da) = effective_dasharray {
                                    let da_str = da
                                        .iter()
                                        .map(|v| v.to_string())
                                        .collect::<Vec<_>>()
                                        .join(",");
                                    elem_attrs
                                        .push_str(&format!(r#" stroke-dasharray="{}""#, da_str));
                                }
                                if let Some(offset) = effective_dashoffset {
                                    elem_attrs
                                        .push_str(&format!(r#" stroke-dashoffset="{}""#, offset));
                                }
                                if let Some(opacity) = effective_opacity {
                                    elem_attrs.push_str(&format!(r#" opacity="{}""#, opacity));
                                }
                                if let Some(ref path_data) = svg.svg_path_data {
                                    if *tag == "<path" {
                                        elem_attrs.push_str(&format!(r#" d="{}""#, path_data));
                                    }
                                }
                                modified.insert_str(insert_at, &elem_attrs);
                                search_from = insert_at + elem_attrs.len() + 1;
                            } else {
                                break;
                            }
                        }
                    }

                    std::borrow::Cow::Owned(modified)
                } else {
                    std::borrow::Cow::Borrowed(&*svg.source)
                };

                // Resolve currentColor references in SVG source.
                // For tintable SVGs: rasterize as white — color applied via shader tint.
                // For non-tintable: replace with actual tint color for CPU rasterization.
                // For SVGs that have a tint but no currentColor at all (e.g.
                // hard-coded `fill="white"`), the post-rasterize `apply_tint`
                // path below handles it instead.
                let has_current_color = effective_source.contains("currentColor");
                let needs_post_raster_tint =
                    !is_tintable && svg.tint.is_some() && !has_current_color;
                let final_source = if is_tintable {
                    std::borrow::Cow::Owned(effective_source.replace("currentColor", "#ffffff"))
                } else if let Some(tint) = svg.tint {
                    if has_current_color {
                        std::borrow::Cow::Owned(
                            effective_source.replace("currentColor", &color_val(tint)),
                        )
                    } else {
                        effective_source
                    }
                } else {
                    effective_source
                };

                let rasterized =
                    RasterizedSvg::from_str(&final_source, raster_width, raster_height);

                let mut rasterized = match rasterized {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("Failed to rasterize SVG: {}", e);
                        continue;
                    }
                };

                // When a tint color is set but the SVG source doesn't
                // use `currentColor` (e.g. hard-coded `fill="white"`),
                // the currentColor replacement above was a no-op and
                // the rasterized pixels still carry the original fill.
                // Apply the tint as a post-rasterization color replace:
                // every non-transparent pixel gets its RGB replaced
                // with the tint color while preserving the original
                // alpha. This makes `.color(Color::RED)` work on any
                // SVG regardless of how its fills are authored.
                if needs_post_raster_tint {
                    rasterized.apply_tint(svg.tint.unwrap());
                }

                // Insert into atlas (handles grow/clear internally)
                if self
                    .svg_atlas
                    .insert(
                        cache_key,
                        rasterized.width,
                        rasterized.height,
                        rasterized.data(),
                        &self.device,
                    )
                    .is_none()
                {
                    tracing::warn!(
                        "SVG atlas full, could not allocate {}x{}",
                        raster_width,
                        raster_height
                    );
                    continue;
                }
            }

            // Get the atlas region for this SVG
            let Some(region) = self.svg_atlas.get(cache_key) else {
                continue;
            };
            let src_uv = region.uv_bounds(self.svg_atlas.width(), self.svg_atlas.height());
            self.svg_atlas.mark_used(cache_key);

            // Apply CSS affine transform to SVG bounds if present.
            // Pass full 2x2 affine to shader for rotation, scale, and skew support.
            let (draw_x, draw_y, draw_w, draw_h, ta, tb, tc, td) =
                if let Some([a, b, c, d, tx, ty]) = svg.css_affine {
                    // DPI-scale the translation components
                    let tx_s = tx * scale_factor;
                    let ty_s = ty * scale_factor;

                    // Transform center through the affine (in screen space)
                    let cx = svg.x + svg.width * 0.5;
                    let cy = svg.y + svg.height * 0.5;
                    let new_cx = a * cx + c * cy + tx_s;
                    let new_cy = b * cx + d * cy + ty_s;

                    // Pass original bounds — the 2x2 transform is applied in the shader
                    (
                        new_cx - svg.width * 0.5,
                        new_cy - svg.height * 0.5,
                        svg.width,
                        svg.height,
                        a,
                        b,
                        c,
                        d,
                    )
                } else {
                    // Pixel-snap the icon origin to the physical grid, same
                    // crispness fix as text glyph origins: a fractional dest
                    // position (from fractional layout coords) blurs a 1:1
                    // rasterized icon through the Linear image sampler at 1x.
                    // Icons are typically integer-sized, so snapping the
                    // origin sharpens the whole quad. Only the untransformed
                    // case — the `css_affine` branch keeps sub-pixel motion.
                    (
                        svg.x.round(),
                        svg.y.round(),
                        svg.width,
                        svg.height,
                        1.0,
                        0.0,
                        0.0,
                        1.0,
                    )
                };

            // Create instance with atlas UV coordinates
            let mut instance = GpuImageInstance::new(draw_x, draw_y, draw_w, draw_h)
                .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
                .with_opacity(svg.motion_opacity)
                .with_transform(ta, tb, tc, td);

            // For tintable SVGs, apply color via shader tint multiplication
            // (white texture * tint = correctly colored output)
            if is_tintable {
                if let Some(tint) = svg.tint {
                    instance = instance.with_tint(tint.r, tint.g, tint.b, tint.a);
                }
            }

            // Apply clip bounds if specified
            if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
                instance = instance.with_clip_rect(clip_x, clip_y, clip_w, clip_h);
            }

            instances.push(instance);
        }

        // Upload atlas to GPU if dirty, then batch-render all SVG instances.
        // The atlas is lazily allocated on first insert; if we have
        // instances we must have inserted at least one this frame, so the
        // view is guaranteed to exist by the time we read it.
        if !instances.is_empty() {
            self.svg_atlas.upload(&self.queue);
            if let Some(view) = self.svg_atlas.view() {
                self.renderer.render_images(target, view, &instances);
            }
        }
    }

    /// Collect text, SVG, and image elements from the render tree
    fn collect_render_elements(
        &mut self,
        tree: &RenderTree,
    ) -> (
        Vec<TextElement>,
        Vec<SvgElement>,
        Vec<ImageElement>,
        Vec<FlowElement>,
    ) {
        self.collect_render_elements_with_state(tree, None)
    }

    /// Collect text, SVG, and image elements with motion state
    fn collect_render_elements_with_state(
        &mut self,
        tree: &RenderTree,
        render_state: Option<&blinc_layout::RenderState>,
    ) -> (
        Vec<TextElement>,
        Vec<SvgElement>,
        Vec<ImageElement>,
        Vec<FlowElement>,
    ) {
        // Reuse scratch buffers - take them, clear, populate, and return
        // On next call they'll be reallocated if not returned
        let mut texts = std::mem::take(&mut self.scratch_texts);
        let mut svgs = std::mem::take(&mut self.scratch_svgs);
        let mut images = std::mem::take(&mut self.scratch_images);
        let mut flows = Vec::new();
        texts.clear();
        svgs.clear();
        images.clear();

        // Get the scale factor from the tree for DPI scaling
        let scale = tree.scale_factor();

        // Build a snapshot of `(composite-promoted node → current
        // animated opacity)` so descendant text / SVG / image elements
        // can multiply that opacity into their per-glyph color alpha.
        // Composite-promoted SDF primitives (the popover bg / border /
        // shadow) already pick up the animated opacity via the per-frame
        // blit in `composite_css_layers_overlay`, but text glyphs route
        // through a separate `cached_motion_subtree_text_prims` pool
        // that doesn't see the composite blit's opacity — without this
        // snapshot, cn::context_menu / cn::popover / cn::dropdown BG
        // would fade in 0→1 while text snapped in at full alpha.
        //
        // The snapshot is a map (not a flag) so we can also handle
        // nested composite-promoted ancestors correctly — descendants
        // multiply their effective opacity from the outermost composite
        // ancestor through to the innermost.
        self.composite_patch_baselines.clear();
        let composite_layer_opacities: std::collections::HashMap<blinc_layout::LayoutNodeId, f32> = {
            let promo = tree.composite_promotion();
            let store = tree.css_anim_store();
            if let Ok(guard) = store.lock() {
                promo
                    .iter()
                    .filter_map(|&layout| {
                        let stable = tree.stable_id(layout)?;
                        let anim = guard
                            .animations
                            .get(&stable)
                            .or_else(|| guard.transitions.get(&stable))?;
                        if !anim.is_playing {
                            return None;
                        }
                        let opacity = anim.current_properties.opacity?;
                        // Baseline for the dispatch-time text patch: the
                        // promoted region's screen AABB plus the
                        // translate/scale this collect bakes into the
                        // glyph geometry. The patch re-anchors glyph
                        // centers with the same top-left math the
                        // composite blit uses, so text tracks the
                        // blitted bg exactly.
                        if let Some(region) = tree.dynamic_regions().get(&layout) {
                            let p = &anim.current_properties;
                            self.composite_patch_baselines.insert(
                                stable,
                                (
                                    region.screen_aabb,
                                    (p.translate_x.unwrap_or(0.0), p.translate_y.unwrap_or(0.0)),
                                    (p.scale_x.unwrap_or(1.0), p.scale_y.unwrap_or(1.0)),
                                ),
                            );
                        }
                        Some((layout, opacity))
                    })
                    .collect()
            } else {
                std::collections::HashMap::new()
            }
        };

        if let Some(root) = tree.root() {
            let mut z_layer = 0u32;
            self.collect_elements_recursive(
                tree,
                root,
                (0.0, 0.0),
                false,      // inside_glass
                false,      // inside_foreground
                None,       // No initial clip bounds
                None,       // No initial clip radius
                1.0,        // Initial motion opacity
                (0.0, 0.0), // Initial motion translate offset
                (1.0, 1.0), // Initial motion scale
                None,       // No initial motion scale center
                render_state,
                scale,
                &mut z_layer,
                &mut texts,
                &mut svgs,
                &mut images,
                &mut flows,
                None,  // No initial CSS transform
                1.0,   // Initial inherited CSS opacity
                None,  // No parent node
                None,  // No initial scroll clip
                None,  // No 3D layer ancestor
                false, // No ancestor pending motion at root
                false, // No ancestor motion container at root
                &composite_layer_opacities,
                1.0,  // Initial composite-layer opacity (no ancestor promoted)
                None, // No composite-promoted ancestor at root
            );
        }

        // Sort texts by z_index (z_layer) to ensure correct rendering order with primitives
        texts.sort_by_key(|t| t.z_index);

        (texts, svgs, images, flows)
    }

    #[allow(clippy::too_many_arguments, clippy::only_used_in_recursion)]
    fn collect_elements_recursive(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        inside_glass: bool,
        inside_foreground: bool,
        current_clip: Option<[f32; 4]>,
        current_clip_radius: Option<[f32; 4]>,
        inherited_motion_opacity: f32,
        inherited_motion_translate: (f32, f32),
        inherited_motion_scale: (f32, f32),
        // Center point for motion scale (in layout coordinates, before DPI scaling)
        // When a parent has motion scale, children should scale around the parent's center
        inherited_motion_scale_center: Option<(f32, f32)>,
        render_state: Option<&blinc_layout::RenderState>,
        scale: f32,
        z_layer: &mut u32,
        texts: &mut Vec<TextElement>,
        svgs: &mut Vec<SvgElement>,
        images: &mut Vec<ImageElement>,
        flows: &mut Vec<FlowElement>,
        // Accumulated CSS transform from ancestors as a 6-element affine [a,b,c,d,tx,ty]
        // in layout coordinates. Maps pre-transform coords to post-transform visual coords.
        inherited_css_affine: Option<[f32; 6]>,
        // Accumulated CSS opacity from ancestors (compounds multiplicatively).
        // CSS `opacity` applies to the element and its entire visual subtree.
        inherited_css_opacity: f32,
        // Parent node ID for inheriting non-cascading CSS props (border, shadow, filter)
        // to child images that render separately from the SDF pipeline.
        parent_node: Option<LayoutNodeId>,
        // Scroll container clip — sharp rect kept separate from the primary rounded clip.
        // This prevents corner radius morphing when a rounded element (card) is partially
        // scrolled past a sharp scroll boundary.
        current_scroll_clip: Option<[f32; 4]>,
        // 3D layer info if inside a perspective-transformed ancestor.
        // Text/SVGs/images inside 3D layers are rendered to offscreen textures
        // and blitted with the same perspective transform.
        inside_3d_layer: Option<Transform3DLayerInfo>,
        // True if any ancestor on the recursion stack has a pending
        // motion binding / FSM animation. When set, the opacity ≤ 0.001
        // early-out below is suppressed so a transient motion's child
        // text doesn't get filtered out on the opacity=0 first frame
        // (the descendant has no motion of its own, so its local
        // `has_pending_motion` would otherwise be false, dropping the
        // whole subtree).
        ancestor_pending_motion: bool,
        // True if any ancestor is a motion-bound container (has
        // `motion_bindings` or a motion FSM config). Used to flag
        // collected text / SVG / image elements so they're deferred
        // from the static-cache pass and re-dispatched on top of the
        // motion overlay — the static cache otherwise sits *under* the
        // motion-bound bg primitives and text gets covered by the
        // overlay's bg paint.
        inside_motion_subtree: bool,
        // Snapshot of `composite-promoted node → current animated
        // opacity` built once at the top of
        // `collect_render_elements_with_state`. Walker emits SDF for
        // composite-promoted subtrees into a per-node scratch batch
        // and `composite_css_layers_overlay` blits that texture with
        // the animated opacity — but text glyphs / SVGs / images
        // collected here route through separate post-overlay pools
        // that don't see the composite blit's opacity. Lookups against
        // this map multiply the composite-layer opacity into each
        // collected element's per-channel alpha so glyphs / SVGs /
        // images fade in / out in sync with the BG bake.
        composite_layer_opacities: &std::collections::HashMap<blinc_layout::LayoutNodeId, f32>,
        // Effective composite-layer opacity inherited from the nearest
        // composite-promoted ancestor (multiplied through nested
        // promotions). Multiplied into descendant IMAGE opacity at
        // construction. Text / SVG exclude it instead — they carry
        // `composite_ancestor` and get the LIVE factor applied at
        // dispatch (see `dispatch_motion_subtree_text_overlay`).
        inherited_composite_layer_opacity: f32,
        // Nearest composite-promoted ancestor with a playing CSS
        // animation, threaded down so text / SVG elements can record
        // it for the dispatch-time live patch.
        nearest_composite_ancestor: Option<blinc_layout::tree::StableNodeId>,
    ) {
        use blinc_layout::Material;

        // Use animated bounds if this node has layout animation, otherwise use layout bounds
        // This ensures children are positioned correctly during layout animation transitions
        let Some(bounds) = tree.get_render_bounds(node, parent_offset) else {
            return;
        };

        let abs_x = bounds.x;
        let abs_y = bounds.y;

        // Get motion values for this node from RenderState (entry/exit animations)
        let motion_values = render_state.and_then(|rs| {
            // Try stable motion first, then node-based
            if let Some(render_node) = tree.get_render_node(node) {
                if let Some(ref stable_key) = render_node.props.motion_stable_id {
                    return rs.get_stable_motion_values(stable_key);
                }
            }
            rs.get_motion_values(node)
        });

        // Get motion bindings from RenderTree (continuous AnimatedValue animations)
        // NOTE: binding_transform (translate) is NOT added to effective_motion_translate
        // because it's already included in new_offset for child positioning (see line ~1250).
        // Only RenderState motion values need to be inherited through effective_motion_translate.
        let binding_scale = tree.get_motion_scale(node);
        let binding_opacity = tree.get_motion_opacity(node);

        // Is this node itself a motion container? Either it owns
        // continuous AnimatedValue bindings (motion bindings — used by
        // springs / scrubs) or it carries a motion FSM config (enter /
        // exit animations from the router or transient `motion()`). The
        // walker pushes such nodes onto `motion_subtree_depth` and
        // routes their primitives to the dynamic batch. We mirror that
        // here so descendants' text / SVG / image flags can be marked
        // for the post-overlay re-dispatch path.
        let is_motion_container_node = tree.motion_bindings_map().contains_key(&node)
            || tree
                .get_render_node(node)
                .map(|n| n.props.motion.is_some() || n.props.is_overlay_root)
                .unwrap_or(false);
        let child_inside_motion_subtree = inside_motion_subtree || is_motion_container_node;

        // Calculate motion opacity for this node (combine both sources)
        let node_motion_opacity = motion_values
            .and_then(|m| m.opacity)
            .unwrap_or_else(|| binding_opacity.unwrap_or(1.0));

        // Get motion translate for this node from RenderState only
        // (binding translate is handled via new_offset in recursive calls)
        let node_motion_translate = motion_values
            .map(|m| m.resolved_translate())
            .unwrap_or((0.0, 0.0));

        // Get motion scale for this node from RenderState
        let node_motion_scale = motion_values
            .map(|m| m.resolved_scale())
            .unwrap_or((1.0, 1.0));

        // Combine with binding scale
        let binding_scale_values = binding_scale.unwrap_or((1.0, 1.0));

        // Combine with inherited values
        // NOTE: effective_motion_translate only includes RenderState motion values,
        // NOT binding transforms (which are already in the position via new_offset)
        let effective_motion_opacity = inherited_motion_opacity * node_motion_opacity;
        // If this node is a composite-promotion root (CSS animation
        // running on opacity/transform), multiply its current animated
        // opacity into the running composite-layer-opacity factor so
        // descendants' glyphs / SVGs / images fade in lockstep with
        // the composite blit. Identity (1.0) for nodes that are not
        // themselves composite-promoted.
        let node_composite_layer_opacity =
            composite_layer_opacities.get(&node).copied().unwrap_or(1.0);
        let effective_composite_layer_opacity =
            inherited_composite_layer_opacity * node_composite_layer_opacity;
        // Track the NEAREST promoted ancestor for the dispatch-time
        // text/SVG live patch. Nested playing promotions are rare
        // (overlays don't nest inside other animating promoted
        // subtrees); the patch applies the nearest ancestor's live
        // opacity, which is exact for the single-ancestor case.
        let nearest_composite_ancestor = if composite_layer_opacities.contains_key(&node) {
            tree.stable_id(node).or(nearest_composite_ancestor)
        } else {
            nearest_composite_ancestor
        };
        let effective_motion_translate = (
            inherited_motion_translate.0 + node_motion_translate.0,
            inherited_motion_translate.1 + node_motion_translate.1,
        );
        // Scale compounds multiplicatively (including binding scale)
        let effective_motion_scale = (
            inherited_motion_scale.0 * node_motion_scale.0 * binding_scale_values.0,
            inherited_motion_scale.1 * node_motion_scale.1 * binding_scale_values.1,
        );

        // Determine the motion scale center for children
        // If this node has motion scale (from RenderState or binding), use its center as the scale center
        // Otherwise, inherit the parent's scale center
        let this_node_has_scale = (node_motion_scale.0 - 1.0).abs() > 0.001
            || (node_motion_scale.1 - 1.0).abs() > 0.001
            || (binding_scale_values.0 - 1.0).abs() > 0.001
            || (binding_scale_values.1 - 1.0).abs() > 0.001;

        let effective_motion_scale_center = if this_node_has_scale {
            // This node has motion scale - compute its center in absolute layout coordinates
            let center_x = abs_x + bounds.width / 2.0;
            let center_y = abs_y + bounds.height / 2.0;
            Some((center_x, center_y))
        } else {
            // No scale on this node - inherit the parent's scale center
            inherited_motion_scale_center
        };

        // Skip if completely transparent — UNLESS this node has an
        // active motion binding or motion FSM that could ramp the
        // opacity back up. Mirrors the same bypass the walker has at
        // `paint/motion.rs::render_layer_with_motion` so SDF primitives
        // (emitted by the walker) and text / SVG / image (collected
        // here) stay in lockstep across animation frames.
        //
        // Without this bypass, a transient motion's first paint after
        // a rebuild — when `current` still equals `enter_from` and
        // opacity is exactly 0.0 — would emit bg / border primitives
        // into the cache but no text. Subsequent slow-path frames
        // re-collect with opacity > 0.001 and would normally repopulate
        // the cache, but the cache invalidation gate (`other_animations
        // _active`) reads `has_active_motions()` which goes false on
        // the very frame the FSM hits `Visible` — so the final cached
        // text vectors come from whatever the last *animating* frame
        // produced. If that final animating frame happened to early-out
        // here, the cache ships without text and the only way back is
        // a mouse move to force another full paint. Symptom: router
        // page transitions show buttons with invisible labels.
        let has_pending_motion = tree
            .motion_bindings_map()
            .get(&node)
            .is_some_and(|b| b.is_any_animating())
            || render_state.is_some_and(|rs| {
                if let Some(render_node) = tree.get_render_node(node) {
                    if let Some(ref stable_key) = render_node.props.motion_stable_id {
                        return rs.is_stable_motion_active(stable_key);
                    }
                }
                rs.is_motion_active(node)
            });
        // Propagate pending-motion state to descendants so a transient
        // motion's children don't get filtered out at opacity=0. The
        // immediate child has its own `has_pending_motion = false`
        // (no motion bindings / FSM of its own), but its inherited
        // opacity is 0 because the ancestor is mid-enter — without
        // this propagation the child early-returns and its text /
        // SVG / images never make it into the cache for the rest of
        // the animation.
        let subtree_pending_motion = ancestor_pending_motion || has_pending_motion;
        if effective_motion_opacity <= 0.001 && !subtree_pending_motion {
            return;
        }

        // CSS visibility: hidden — skip rendering but preserve layout space
        if let Some(render_node) = tree.get_render_node(node) {
            if !render_node.props.visible {
                return;
            }
        }

        // Determine if this node is a glass element
        let is_glass = tree
            .get_render_node(node)
            .map(|n| matches!(n.props.material, Some(Material::Glass(_))))
            .unwrap_or(false);

        // Track if children should be considered inside glass
        let children_inside_glass = inside_glass || is_glass;

        // Track if we're inside a foreground-layer element
        let is_foreground_node = tree
            .get_render_node(node)
            .map(|n| n.props.layer == RenderLayer::Foreground)
            .unwrap_or(false);
        let children_inside_foreground = inside_foreground || is_foreground_node;

        // Check if this node clips its children (e.g., scroll containers)
        let clips_content = tree
            .get_render_node(node)
            .map(|n| n.props.clips_content)
            .unwrap_or(false);

        // Check if this node has an active layout animation (also needs clipping)
        // Layout animations need to clip children to animated bounds
        let has_layout_animation = tree.is_layout_animating(node);

        // Check if this is a Stack layer - if so, increment z_layer for proper z-ordering
        let is_stack_layer = tree
            .get_render_node(node)
            .map(|n| n.props.is_stack_layer)
            .unwrap_or(false);
        if is_stack_layer {
            *z_layer += 1;
        }

        // Apply CSS z-index to z_layer for stacking order
        let saved_z_layer = *z_layer;
        let node_z_index = tree
            .get_render_node(node)
            .map(|n| n.props.z_index)
            .unwrap_or(0);
        if node_z_index > 0 {
            *z_layer = node_z_index as u32;
        }

        // Update clip bounds for children if this node clips (either via clips_content or layout animation)
        // When a node clips, we INTERSECT its bounds with any existing clip
        // This ensures nested clipping works correctly (inner clips can't expand outer clips)
        let should_clip = clips_content || has_layout_animation;
        let (child_clip, child_clip_radius, child_scroll_clip) = if should_clip {
            // For layout animation, use animated bounds for clipping
            // This ensures content is clipped to the animating size during transition
            let clip_bounds = if has_layout_animation {
                // Get animated bounds - these are the interpolated bounds during animation
                tree.get_render_bounds(node, parent_offset)
                    .map(|b| [b.x, b.y, b.width, b.height])
                    .unwrap_or([abs_x, abs_y, bounds.width, bounds.height])
            } else {
                [abs_x, abs_y, bounds.width, bounds.height]
            };
            // Inset clip by border-width only.  Per CSS spec, overflow clips
            // at the padding box (inside border, but padding area is visible).
            // Padding affects layout positioning, not clipping.
            let bw = tree
                .get_render_node(node)
                .map(|n| n.props.border_width)
                .unwrap_or(0.0);
            let this_clip = [
                clip_bounds[0] + bw,
                clip_bounds[1] + bw,
                (clip_bounds[2] - bw * 2.0).max(0.0),
                (clip_bounds[3] - bw * 2.0).max(0.0),
            ];

            // Extract border radius from this node for rounded clipping.
            // Inner corner radius = max(outer_radius − border_width, 0)
            let this_clip_radius = tree.get_render_node(node).map(|n| {
                let r = &n.props.border_radius;
                [
                    (r.top_left - bw).max(0.0),
                    (r.top_right - bw).max(0.0),
                    (r.bottom_right - bw).max(0.0),
                    (r.bottom_left - bw).max(0.0),
                ]
            });

            let this_has_radius = this_clip_radius
                .map(|r| r.iter().any(|&v| v > 0.5))
                .unwrap_or(false);
            let parent_has_radius = current_clip_radius
                .map(|r| r.iter().any(|&v| v > 0.5))
                .unwrap_or(false);

            if let Some(parent_clip) = current_clip {
                if this_has_radius && !parent_has_radius {
                    // This node is rounded (card), parent is sharp (scroll container).
                    // Keep them separate to avoid SDF radius clamping/morphing.
                    // Primary clip = this node's rounded clip (full card bounds).
                    // Scroll clip = parent's sharp clip intersected with any existing scroll clip.
                    (
                        Some(this_clip),
                        this_clip_radius,
                        merge_scroll_clip(parent_clip, current_scroll_clip),
                    )
                } else if !this_has_radius && parent_has_radius {
                    // This node is sharp (scroll), parent is rounded (card).
                    // Keep parent as primary rounded clip, this as scroll clip
                    // intersected with any existing scroll clip.
                    (
                        current_clip,
                        current_clip_radius,
                        merge_scroll_clip(this_clip, current_scroll_clip),
                    )
                } else {
                    // Both have same kind of radius — intersect normally.
                    let x1 = parent_clip[0].max(this_clip[0]);
                    let y1 = parent_clip[1].max(this_clip[1]);
                    let parent_right = parent_clip[0] + parent_clip[2];
                    let parent_bottom = parent_clip[1] + parent_clip[3];
                    let this_right = this_clip[0] + this_clip[2];
                    let this_bottom = this_clip[1] + this_clip[3];
                    let x2 = parent_right.min(this_right);
                    let y2 = parent_bottom.min(this_bottom);
                    let w = (x2 - x1).max(0.0);
                    let h = (y2 - y1).max(0.0);
                    let clip = Some([x1, y1, w, h]);

                    let child_r = this_clip_radius.unwrap_or([0.0; 4]);
                    let parent_r = current_clip_radius.unwrap_or([0.0; 4]);
                    let radius = Some([
                        child_r[0].max(parent_r[0]),
                        child_r[1].max(parent_r[1]),
                        child_r[2].max(parent_r[2]),
                        child_r[3].max(parent_r[3]),
                    ]);

                    (clip, radius, current_scroll_clip)
                }
            } else {
                // No parent clip — this is the first clip level.
                if this_has_radius {
                    // Rounded clip becomes primary, scroll clip passes through.
                    (Some(this_clip), this_clip_radius, current_scroll_clip)
                } else {
                    // Sharp clip becomes scroll clip; intersect with existing scroll clip
                    // so nested sharp clips (scroll + stack wrapper) don't lose the outer boundary.
                    let new_scroll_clip = if let Some(existing) = current_scroll_clip {
                        let x1 = existing[0].max(this_clip[0]);
                        let y1 = existing[1].max(this_clip[1]);
                        let x2 = (existing[0] + existing[2]).min(this_clip[0] + this_clip[2]);
                        let y2 = (existing[1] + existing[3]).min(this_clip[1] + this_clip[3]);
                        [x1, y1, (x2 - x1).max(0.0), (y2 - y1).max(0.0)]
                    } else {
                        this_clip
                    };
                    (None, None, Some(new_scroll_clip))
                }
            }
        } else {
            (current_clip, current_clip_radius, current_scroll_clip)
        };

        // Compute this node's CSS affine: compose its own CSS transform with inherited.
        // This must happen BEFORE the element-type match block so that SVGs, text, and images
        // get their own transform applied (not just the parent's inherited transform).
        // NOTE: 3D rotations (rotate-x/rotate-y/perspective) are NOT included here — they
        // can't be accurately represented as a 2D affine (perspective is projective, not linear).
        // Proper 3D text compositing requires layer-based rendering (render to texture, then
        // apply 3D transform to the composite). For now, text stays flat under 3D parents.
        let node_css_affine = if let Some(render_node) = tree.get_render_node(node) {
            let has_non_identity = if let Some(blinc_core::Transform::Affine2D(affine)) =
                &render_node.props.transform
            {
                let [a, b, c, d, tx, ty] = affine.elements;
                !((a - 1.0).abs() < 0.0001
                    && b.abs() < 0.0001
                    && c.abs() < 0.0001
                    && (d - 1.0).abs() < 0.0001
                    && tx.abs() < 0.0001
                    && ty.abs() < 0.0001)
            } else {
                false
            };

            if has_non_identity {
                let affine = match &render_node.props.transform {
                    Some(blinc_core::Transform::Affine2D(a)) => a.elements,
                    _ => unreachable!(),
                };
                let [a, b, c, d, tx, ty] = affine;
                // Compute transform center in absolute layout coords
                let (cx, cy) = if let Some([ox_pct, oy_pct]) = render_node.props.transform_origin {
                    (
                        abs_x + bounds.width * ox_pct / 100.0,
                        abs_y + bounds.height * oy_pct / 100.0,
                    )
                } else {
                    (abs_x + bounds.width / 2.0, abs_y + bounds.height / 2.0)
                };
                // Build full 6-element affine: T(center) * [a,b,c,d,tx,ty] * T(-center)
                // = [a, b, c, d, cx*(1-a) - cy*c + tx, cy*(1-d) - cx*b + ty]
                let this_affine = [
                    a,
                    b,
                    c,
                    d,
                    cx * (1.0 - a) - cy * c + tx,
                    cy * (1.0 - d) - cx * b + ty,
                ];
                match inherited_css_affine {
                    Some(parent) => {
                        let [pa, pb, pc, pd, ptx, pty] = parent;
                        Some([
                            a * pa + c * pb,
                            b * pa + d * pb,
                            a * pc + c * pd,
                            b * pc + d * pd,
                            a * ptx + c * pty + this_affine[4],
                            b * ptx + d * pty + this_affine[5],
                        ])
                    }
                    None => Some(this_affine),
                }
            } else {
                inherited_css_affine
            }
        } else {
            inherited_css_affine
        };

        if let Some(render_node) = tree.get_render_node(node) {
            // Determine effective layer: children inside glass render in Foreground
            let effective_layer = if inside_glass && !is_glass {
                RenderLayer::Foreground
            } else if is_glass {
                RenderLayer::Glass
            } else {
                render_node.props.layer
            };

            match &render_node.element_type {
                ElementType::Text(text_data) => {
                    // Apply DPI scale factor FIRST to match shape rendering order
                    // In render_with_motion, DPI scale is pushed at root level before any other transforms
                    // So we must: scale base positions first, then apply motion transforms
                    let base_x = abs_x * scale;
                    let base_y = abs_y * scale;
                    let base_width = bounds.width * scale;
                    let base_height = bounds.height * scale;

                    // Motion (scale-around-center + translate) is composed into the glyph
                    // affine instead of being baked into font_size/position. This keeps
                    // glyph rasterization at base size — without this, every animation
                    // frame stamps a fresh `(font_id, glyph_id, font_size)` cache key,
                    // overflowing the glyph LRU and triggering atlas growth, which then
                    // distorts text after repeated transitions.
                    let has_motion_scale = (effective_motion_scale.0 - 1.0).abs() > 1e-6
                        || (effective_motion_scale.1 - 1.0).abs() > 1e-6;
                    let has_motion_translate = effective_motion_translate.0.abs() > 1e-6
                        || effective_motion_translate.1.abs() > 1e-6;
                    let motion_affine = if has_motion_scale || has_motion_translate {
                        let (sx, sy) = effective_motion_scale;
                        let (tx, ty) = effective_motion_translate;
                        let (cx, cy) = effective_motion_scale_center.unwrap_or((0.0, 0.0));
                        // Scale around (cx, cy) plus translate (tx, ty) — all in layout coords.
                        Some([sx, 0.0, 0.0, sy, cx * (1.0 - sx) + tx, cy * (1.0 - sy) + ty])
                    } else {
                        None
                    };

                    // Compose motion_affine ∘ node_css_affine (motion is the outer transform).
                    let text_affine = match (motion_affine, node_css_affine) {
                        (Some(m), Some(c)) => {
                            let [ma, mb, mc, md, mtx, mty] = m;
                            let [ca, cb, cc, cd, ctx, cty] = c;
                            Some([
                                ma * ca + mc * cb,
                                mb * ca + md * cb,
                                ma * cc + mc * cd,
                                mb * cc + md * cd,
                                ma * ctx + mc * cty + mtx,
                                mb * ctx + md * cty + mty,
                            ])
                        }
                        (Some(m), None) => Some(m),
                        (None, c) => c,
                    };

                    // Glyph layout uses the BASE box — motion is applied via text_affine
                    // at glyph emission, so position/size here intentionally exclude motion.
                    let scaled_x = base_x;
                    let scaled_y = base_y;
                    let scaled_width = base_width;
                    let scaled_height = base_height;

                    // Use CSS-overridden font size if available (from stylesheet/animation/transition)
                    let base_font_size = render_node.props.font_size.unwrap_or(text_data.font_size);
                    // Rasterize at base size only — motion scale is in text_affine.
                    let scaled_font_size = base_font_size * scale;
                    let scaled_measured_width = text_data.measured_width * scale;

                    // Intersect primary clip with scroll clip — text only supports
                    // a single clip rect so we must merge both boundaries.
                    let effective_clip = effective_single_clip(current_clip, current_scroll_clip);
                    let scaled_clip = effective_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Log motion values if non-trivial (for debugging text/shape sync issues)
                    if effective_motion_translate.0.abs() > 0.1
                        || effective_motion_translate.1.abs() > 0.1
                        || (effective_motion_scale.0 - 1.0).abs() > 0.01
                        || (effective_motion_scale.1 - 1.0).abs() > 0.01
                    {
                        tracing::trace!(
                            "Text '{}': motion_translate=({:.1}, {:.1}), motion_scale=({:.2}, {:.2}), base=({:.1}, {:.1}), final=({:.1}, {:.1})",
                            text_data.content,
                            effective_motion_translate.0,
                            effective_motion_translate.1,
                            effective_motion_scale.0,
                            effective_motion_scale.1,
                            base_x,
                            base_y,
                            scaled_x,
                            scaled_y,
                        );
                    }
                    tracing::trace!(
                        "Text '{}': abs=({:.1}, {:.1}), size=({:.1}x{:.1}), font={:.1}, align={:?}, v_align={:?}, z_layer={}",
                        text_data.content,
                        scaled_x,
                        scaled_y,
                        scaled_width,
                        scaled_height,
                        scaled_font_size,
                        text_data.align,
                        text_data.v_align,
                        *z_layer
                    );

                    // Apply text-overflow: ellipsis truncation if needed.
                    // Check both text_data.wrap (set at build time) and render_node.props.white_space
                    // (set by CSS after build). CSS white-space: nowrap overrides the builder wrap setting.
                    let is_nowrap = !text_data.wrap
                        || matches!(
                            render_node.props.white_space,
                            Some(blinc_layout::element_style::WhiteSpace::Nowrap)
                                | Some(blinc_layout::element_style::WhiteSpace::Pre)
                        );
                    let content = if is_nowrap
                        && matches!(
                            render_node.props.text_overflow,
                            Some(blinc_layout::element_style::TextOverflow::Ellipsis)
                        )
                        && scaled_measured_width > scaled_width
                        && scaled_width > 0.0
                    {
                        // Measure with the same options used for layout
                        let mut options = blinc_layout::text_measure::TextLayoutOptions::new();
                        options.font_name = text_data.font_family.name.clone();
                        options.generic_font = text_data.font_family.generic;
                        options.font_weight =
                            match render_node.props.font_weight.unwrap_or(text_data.weight) {
                                FontWeight::Bold => 700,
                                FontWeight::Normal => 400,
                                FontWeight::Light => 300,
                                _ => 400,
                            };
                        options.letter_spacing = render_node
                            .props
                            .letter_spacing
                            .unwrap_or(text_data.letter_spacing);

                        // Measure "..." to know reserved width
                        let ellipsis = "\u{2026}";
                        let ellipsis_w = blinc_layout::text_measure::measure_text_with_options(
                            ellipsis,
                            scaled_font_size / scale,
                            &options,
                        )
                        .width
                            * scale;
                        let target_width = scaled_width - ellipsis_w;

                        if target_width > 0.0 {
                            // Binary search for the right truncation point
                            let chars: Vec<char> = text_data.content.chars().collect();
                            let mut lo = 0usize;
                            let mut hi = chars.len();
                            while lo < hi {
                                #[allow(clippy::manual_div_ceil)]
                                let mid = (lo + hi + 1) / 2;
                                let sub: String = chars[..mid].iter().collect();
                                let w = blinc_layout::text_measure::measure_text_with_options(
                                    &sub,
                                    scaled_font_size / scale,
                                    &options,
                                )
                                .width
                                    * scale;
                                if w <= target_width {
                                    lo = mid;
                                } else {
                                    hi = mid - 1;
                                }
                            }
                            let truncated: String = chars[..lo].iter().collect();
                            format!("{}{}", truncated.trim_end(), ellipsis)
                        } else {
                            ellipsis.to_string()
                        }
                    } else {
                        text_data.content.clone()
                    };

                    texts.push(TextElement {
                        content,
                        x: scaled_x,
                        y: scaled_y,
                        width: scaled_width,
                        height: scaled_height,
                        font_size: scaled_font_size,
                        color: render_node.props.text_color.unwrap_or(text_data.color),
                        align: text_data.align,
                        weight: render_node.props.font_weight.unwrap_or(text_data.weight),
                        italic: render_node
                            .props
                            .font_style
                            .map_or(text_data.italic, |fs| fs.is_italic()),
                        v_align: text_data.v_align,
                        clip_bounds: scaled_clip,
                        // Composite-promoted subtrees: bake at BASE alpha
                        // (factor excluded); the dispatch-time patch
                        // applies the LIVE store opacity every frame.
                        motion_opacity: effective_motion_opacity
                            * render_node.props.opacity
                            * inherited_css_opacity
                            * if nearest_composite_ancestor.is_some() {
                                1.0
                            } else {
                                effective_composite_layer_opacity
                            },
                        wrap: !is_nowrap && text_data.wrap,
                        line_height: text_data.line_height,
                        measured_width: scaled_measured_width,
                        font_family: text_data.font_family.clone(),
                        word_spacing: text_data.word_spacing,
                        letter_spacing: render_node
                            .props
                            .letter_spacing
                            .unwrap_or(text_data.letter_spacing),
                        z_index: *z_layer,
                        ascender: text_data.ascender * scale,
                        strikethrough: render_node.props.text_decoration.map_or(
                            text_data.strikethrough,
                            |td| {
                                matches!(
                                    td,
                                    blinc_layout::element_style::TextDecoration::LineThrough
                                )
                            },
                        ),
                        underline: render_node.props.text_decoration.map_or(
                            text_data.underline,
                            |td| {
                                matches!(td, blinc_layout::element_style::TextDecoration::Underline)
                            },
                        ),
                        decoration_color: render_node.props.text_decoration_color,
                        decoration_thickness: render_node.props.text_decoration_thickness,
                        css_affine: text_affine,
                        text_shadow: render_node.props.text_shadow,
                        transform_3d_layer: inside_3d_layer.clone(),
                        is_foreground: children_inside_foreground,
                        in_motion_subtree: inside_motion_subtree,
                        composite_ancestor: nearest_composite_ancestor,
                    });
                }
                ElementType::Svg(svg_data) => {
                    // Apply DPI scale factor FIRST to match shape rendering order
                    let base_x = abs_x * scale;
                    let base_y = abs_y * scale;
                    let base_width = bounds.width * scale;
                    let base_height = bounds.height * scale;

                    // Scale motion translate by DPI factor
                    let scaled_motion_tx = effective_motion_translate.0 * scale;
                    let scaled_motion_ty = effective_motion_translate.1 * scale;

                    // Apply motion scale and translation (same logic as Text)
                    let (scaled_x, scaled_y, scaled_width, scaled_height) =
                        if let Some((motion_center_x, motion_center_y)) =
                            effective_motion_scale_center
                        {
                            let motion_center_x_scaled = motion_center_x * scale;
                            let motion_center_y_scaled = motion_center_y * scale;

                            let rel_x = base_x - motion_center_x_scaled;
                            let rel_y = base_y - motion_center_y_scaled;

                            let scaled_rel_x = rel_x * effective_motion_scale.0;
                            let scaled_rel_y = rel_y * effective_motion_scale.1;
                            let scaled_w = base_width * effective_motion_scale.0;
                            let scaled_h = base_height * effective_motion_scale.1;

                            let final_x = motion_center_x_scaled + scaled_rel_x + scaled_motion_tx;
                            let final_y = motion_center_y_scaled + scaled_rel_y + scaled_motion_ty;

                            (final_x, final_y, scaled_w, scaled_h)
                        } else {
                            let final_x = base_x + scaled_motion_tx;
                            let final_y = base_y + scaled_motion_ty;
                            (final_x, final_y, base_width, base_height)
                        };

                    // Intersect primary clip with scroll clip — text/SVG only support
                    // a single clip rect so we must merge both boundaries.
                    let effective_clip = effective_single_clip(current_clip, current_scroll_clip);
                    let scaled_clip = effective_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Tint resolves `currentColor` references in SVG source.
                    // CSS fill/stroke are explicit overrides injected as SVG attributes.
                    // Both can coexist: tint handles currentColor, CSS handles specifics.
                    svgs.push(SvgElement {
                        source: svg_data.source.clone(),
                        x: scaled_x,
                        y: scaled_y,
                        width: scaled_width,
                        height: scaled_height,
                        tint: svg_data.tint.or_else(|| {
                            render_node
                                .props
                                .text_color
                                .map(|c| blinc_core::Color::rgba(c[0], c[1], c[2], c[3]))
                        }),
                        fill: render_node
                            .props
                            .fill
                            .map(|c| blinc_core::Color::rgba(c[0], c[1], c[2], c[3]))
                            .or(svg_data.fill),
                        stroke: render_node
                            .props
                            .stroke
                            .map(|c| blinc_core::Color::rgba(c[0], c[1], c[2], c[3]))
                            .or(svg_data.stroke),
                        stroke_width: render_node.props.stroke_width.or(svg_data.stroke_width),
                        stroke_dasharray: render_node.props.stroke_dasharray.clone(),
                        stroke_dashoffset: render_node.props.stroke_dashoffset,
                        svg_path_data: render_node.props.svg_path_data.clone(),
                        clip_bounds: scaled_clip,
                        // Same base-alpha contract as TextElement:
                        // composite factor applied live at dispatch.
                        motion_opacity: effective_motion_opacity
                            * render_node.props.opacity
                            * inherited_css_opacity
                            * if nearest_composite_ancestor.is_some() {
                                1.0
                            } else {
                                effective_composite_layer_opacity
                            },
                        css_affine: node_css_affine,
                        tag_overrides: render_node.props.svg_tag_styles.clone(),
                        transform_3d_layer: inside_3d_layer.clone(),
                        in_motion_subtree: inside_motion_subtree,
                        z_layer: *z_layer,
                        composite_ancestor: nearest_composite_ancestor,
                    });
                }
                ElementType::Image(image_data) => {
                    // Apply DPI scale factor to image positions and sizes
                    let scaled_clip = current_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Scale clip radius by DPI factor (radius values are in layout coordinates)
                    let scaled_clip_radius = current_clip_radius
                        .map(|[tl, tr, br, bl]| [tl * scale, tr * scale, br * scale, bl * scale])
                        .unwrap_or([0.0; 4]);

                    // Scale scroll clip by DPI factor
                    let scaled_scroll_clip = current_scroll_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Look up parent render props for CSS property inheritance.
                    // Images render via a separate pipeline and don't inherit parent CSS
                    // properties automatically — we must propagate them explicitly.
                    let parent_props = parent_node
                        .and_then(|pid| tree.get_render_node(pid))
                        .map(|pn| &pn.props);

                    // Opacity: own CSS opacity * inherited CSS opacity chain * builder * motion
                    // * composite-layer opacity (so images inside a composite-promoted
                    // subtree fade in lockstep with the bake's blit).
                    let own_css_opacity = render_node.props.opacity;
                    let final_opacity = image_data.opacity
                        * own_css_opacity
                        * inherited_css_opacity
                        * effective_motion_opacity
                        * effective_composite_layer_opacity;

                    // Border-radius: prefer own CSS, then builder.
                    // Parent clip (now at content-box) handles corner rounding.
                    let own_br = render_node.props.border_radius.top_left;
                    let final_border_radius = if own_br > 0.0 {
                        own_br * scale
                    } else {
                        image_data.border_radius * scale
                    };

                    // Border: use image's own CSS border (parent border renders via SDF,
                    // visible because clip now insets by border-width)
                    let border_width = render_node.props.border_width * scale;
                    let border_color = render_node
                        .props
                        .border_color
                        .unwrap_or(blinc_core::Color::TRANSPARENT);

                    // Shadow: use image's own (parent shadow renders via SDF).
                    // ImageElement holds a single shadow — pick the topmost
                    // layer of any compound stack.
                    let shadow = render_node.props.shadow.first().copied();

                    // Filter: prefer own, fall back to parent
                    let own_filter = &render_node.props.filter;
                    let parent_filter = parent_props.and_then(|p| p.filter.as_ref());
                    let effective_filter = own_filter.as_ref().or(parent_filter);
                    let filter_a = effective_filter
                        .map(|f| Self::css_filter_to_arrays(f).0)
                        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                    let filter_b = effective_filter
                        .map(|f| Self::css_filter_to_arrays(f).1)
                        .unwrap_or([1.0, 1.0, 1.0, 0.0]);

                    // object-fit / object-position: CSS overrides builder values
                    let final_object_fit = render_node
                        .props
                        .object_fit
                        .unwrap_or(image_data.object_fit);
                    let final_object_position = render_node
                        .props
                        .object_position
                        .unwrap_or(image_data.object_position);

                    // CSS overrides for lazy loading properties
                    let final_loading_strategy = render_node
                        .props
                        .loading_strategy
                        .unwrap_or(image_data.loading_strategy);
                    let final_placeholder_type = render_node
                        .props
                        .placeholder_type
                        .unwrap_or(image_data.placeholder_type);
                    let final_placeholder_color = render_node
                        .props
                        .placeholder_color
                        .unwrap_or(image_data.placeholder_color);
                    let final_placeholder_image = render_node
                        .props
                        .placeholder_image
                        .clone()
                        .or_else(|| image_data.placeholder_image.clone());
                    let final_fade_duration = render_node
                        .props
                        .fade_duration_ms
                        .unwrap_or(image_data.fade_duration_ms);

                    // Mask: prefer own, fall back to parent
                    let own_mask = render_node.props.mask_image.as_ref();
                    let parent_mask = parent_props.and_then(|p| p.mask_image.as_ref());
                    let effective_mask = own_mask.or(parent_mask);
                    let (mask_params, mask_info) = Self::mask_image_to_arrays(effective_mask);

                    images.push(ImageElement {
                        source: image_data.source.clone(),
                        x: abs_x * scale,
                        y: abs_y * scale,
                        width: bounds.width * scale,
                        height: bounds.height * scale,
                        object_fit: final_object_fit,
                        object_position: final_object_position,
                        opacity: final_opacity,
                        border_radius: final_border_radius,
                        tint: image_data.tint,
                        clip_bounds: scaled_clip,
                        clip_radius: scaled_clip_radius,
                        layer: effective_layer,
                        loading_strategy: final_loading_strategy,
                        placeholder_type: final_placeholder_type,
                        placeholder_color: final_placeholder_color,
                        placeholder_image: final_placeholder_image,
                        fade_duration_ms: final_fade_duration,
                        z_index: *z_layer,
                        border_width,
                        border_color,
                        css_affine: node_css_affine,
                        shadow,
                        filter_a,
                        filter_b,
                        scroll_clip: scaled_scroll_clip,
                        mask_params,
                        mask_info,
                        transform_3d_layer: inside_3d_layer.clone(),
                    });
                }
                // Canvas elements are rendered inline during tree traversal (in render_layer)
                ElementType::Canvas(_) => {}
                ElementType::Div => {
                    // Check if this div has a background image brush
                    if let Some(blinc_core::Brush::Image(ref img_brush)) =
                        render_node.props.background
                    {
                        let scaled_clip = current_clip.map(|[cx, cy, cw, ch]| {
                            [cx * scale, cy * scale, cw * scale, ch * scale]
                        });
                        let scaled_clip_radius = current_clip_radius
                            .map(|[tl, tr, br, bl]| {
                                [tl * scale, tr * scale, br * scale, bl * scale]
                            })
                            .unwrap_or([0.0; 4]);
                        let scaled_scroll_clip_bg = current_scroll_clip.map(|[cx, cy, cw, ch]| {
                            [cx * scale, cy * scale, cw * scale, ch * scale]
                        });

                        images.push(ImageElement {
                            source: img_brush.source.clone(),
                            x: abs_x * scale,
                            y: abs_y * scale,
                            width: bounds.width * scale,
                            height: bounds.height * scale,
                            object_fit: match img_brush.fit {
                                blinc_core::ImageFit::Cover => 0,
                                blinc_core::ImageFit::Contain => 1,
                                blinc_core::ImageFit::Fill => 2,
                                blinc_core::ImageFit::Tile => 0,
                            },
                            object_position: [img_brush.position.x, img_brush.position.y],
                            opacity: img_brush.opacity
                                * render_node.props.opacity
                                * inherited_css_opacity
                                * effective_motion_opacity
                                * effective_composite_layer_opacity,
                            border_radius: render_node.props.border_radius.top_left * scale,
                            tint: [
                                img_brush.tint.r,
                                img_brush.tint.g,
                                img_brush.tint.b,
                                img_brush.tint.a,
                            ],
                            clip_bounds: scaled_clip,
                            clip_radius: scaled_clip_radius,
                            layer: effective_layer,
                            loading_strategy: 0, // Eager
                            placeholder_type: 0, // None
                            placeholder_color: [0.0; 4],
                            placeholder_image: None,
                            fade_duration_ms: 0,
                            z_index: *z_layer,
                            border_width: 0.0,
                            border_color: blinc_core::Color::TRANSPARENT,
                            css_affine: node_css_affine,
                            shadow: render_node.props.shadow.first().copied(),
                            filter_a: render_node
                                .props
                                .filter
                                .as_ref()
                                .map(|f| Self::css_filter_to_arrays(f).0)
                                .unwrap_or([0.0, 0.0, 0.0, 0.0]),
                            filter_b: render_node
                                .props
                                .filter
                                .as_ref()
                                .map(|f| Self::css_filter_to_arrays(f).1)
                                .unwrap_or([1.0, 1.0, 1.0, 0.0]),
                            scroll_clip: scaled_scroll_clip_bg,
                            mask_params: {
                                let (mp, _) = Self::mask_image_to_arrays(
                                    render_node.props.mask_image.as_ref(),
                                );
                                mp
                            },
                            mask_info: {
                                let (_, mi) = Self::mask_image_to_arrays(
                                    render_node.props.mask_image.as_ref(),
                                );
                                mi
                            },
                            transform_3d_layer: inside_3d_layer.clone(),
                        });
                    }
                }
                // StyledText: render text with inline styling using multiple TextElements
                ElementType::StyledText(styled_data) => {
                    // Apply DPI scale factor first
                    let base_x = abs_x * scale;
                    let base_y = abs_y * scale;
                    let base_width = bounds.width * scale;
                    let base_height = bounds.height * scale;

                    // Scale motion translate by DPI factor
                    let scaled_motion_tx = effective_motion_translate.0 * scale;
                    let scaled_motion_ty = effective_motion_translate.1 * scale;

                    // Apply motion scale and translation (same logic as Text)
                    let (scaled_x, scaled_y, scaled_width, scaled_height) =
                        if let Some((motion_center_x, motion_center_y)) =
                            effective_motion_scale_center
                        {
                            let motion_center_x_scaled = motion_center_x * scale;
                            let motion_center_y_scaled = motion_center_y * scale;

                            let rel_x = base_x - motion_center_x_scaled;
                            let rel_y = base_y - motion_center_y_scaled;

                            let scaled_rel_x = rel_x * effective_motion_scale.0;
                            let scaled_rel_y = rel_y * effective_motion_scale.1;
                            let scaled_w = base_width * effective_motion_scale.0;
                            let scaled_h = base_height * effective_motion_scale.1;

                            let final_x = motion_center_x_scaled + scaled_rel_x + scaled_motion_tx;
                            let final_y = motion_center_y_scaled + scaled_rel_y + scaled_motion_ty;

                            (final_x, final_y, scaled_w, scaled_h)
                        } else {
                            let final_x = base_x + scaled_motion_tx;
                            let final_y = base_y + scaled_motion_ty;
                            (final_x, final_y, base_width, base_height)
                        };

                    // Use CSS-overridden font size if available (from stylesheet/animation/transition)
                    let base_styled_font_size =
                        render_node.props.font_size.unwrap_or(styled_data.font_size);
                    let scaled_font_size = base_styled_font_size * effective_motion_scale.1 * scale;
                    // Intersect primary clip with scroll clip for styled text
                    let effective_clip = effective_single_clip(current_clip, current_scroll_clip);
                    let scaled_clip = effective_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Build non-overlapping segments from potentially overlapping spans
                    // This handles nested tags like <span color="red"><b>text</b></span>
                    let content = &styled_data.content;
                    let content_len = content.len();

                    // Get default styles from element config
                    let default_bold = styled_data.weight == FontWeight::Bold;
                    let default_italic = styled_data.italic;

                    // Collect all boundary positions where style might change
                    let mut boundaries: Vec<usize> = vec![0, content_len];
                    for span in &styled_data.spans {
                        if span.start < content_len {
                            boundaries.push(span.start);
                        }
                        if span.end <= content_len {
                            boundaries.push(span.end);
                        }
                    }
                    boundaries.sort();
                    boundaries.dedup();

                    // Build segments between boundaries
                    #[allow(clippy::type_complexity)]
                    let mut segments: Vec<(
                        usize,
                        usize,
                        [f32; 4],
                        bool,
                        bool,
                        bool,
                        bool,
                    )> = Vec::new();

                    for window in boundaries.windows(2) {
                        let seg_start = window[0];
                        let seg_end = window[1];
                        if seg_start >= seg_end {
                            continue;
                        }

                        // Determine style for this segment by merging all overlapping spans
                        let mut color: Option<[f32; 4]> = None;
                        let mut bold = default_bold;
                        let mut italic = default_italic;
                        let mut underline = false;
                        let mut strikethrough = false;

                        for span in &styled_data.spans {
                            // Check if span overlaps this segment
                            if span.start <= seg_start && span.end >= seg_end {
                                // This span covers this segment - merge styles
                                if span.bold {
                                    bold = true;
                                }
                                if span.italic {
                                    italic = true;
                                }
                                if span.underline {
                                    underline = true;
                                }
                                if span.strikethrough {
                                    strikethrough = true;
                                }
                                // Use color if span has explicit color (not transparent)
                                if span.color[3] > 0.0 {
                                    color = Some(span.color);
                                }
                            }
                        }

                        // CSS text_color override takes precedence over span colors
                        let default_color = render_node
                            .props
                            .text_color
                            .unwrap_or(styled_data.default_color);
                        let final_color = color.unwrap_or(default_color);
                        segments.push((
                            seg_start,
                            seg_end,
                            final_color,
                            bold,
                            italic,
                            underline,
                            strikethrough,
                        ));
                    }

                    // Use consistent ascender from element for baseline alignment
                    let scaled_ascender = styled_data.ascender * scale;

                    // Where the text wraps at this node's laid-out width.
                    // Segments are placed within a line, so a segment
                    // straddling a wrap point is drawn as two runs rather
                    // than one that overflows.
                    let mut wrap_options = blinc_layout::text_measure::TextLayoutOptions::new();
                    wrap_options.font_name = styled_data.font_family.name.clone();
                    wrap_options.generic_font = styled_data.font_family.generic;
                    wrap_options.font_weight = styled_data.weight.weight();
                    wrap_options.italic = styled_data.italic;
                    wrap_options.line_height = styled_data.line_height;
                    wrap_options.max_width = (bounds.width > 0.0).then_some(bounds.width);
                    let lines = blinc_layout::text_measure::text_line_spans(
                        content,
                        styled_data.font_size,
                        &wrap_options,
                    );
                    // Measured, not `font_size * line_height`: the layout
                    // sized this node from font metrics, and a run placed
                    // on a different step would drift further off with
                    // every line.
                    let scaled_line_height = blinc_layout::text_measure::line_height_px(
                        styled_data.font_size,
                        &wrap_options,
                    ) * scale
                        * effective_motion_scale.1;

                    // Calculate x offsets for each segment and push as TextElements
                    for (row, line) in lines.iter().enumerate() {
                        let line_y = scaled_y + row as f32 * scaled_line_height;
                        let mut x_offset = 0.0f32;

                        for &(start, end, color, bold, italic, underline, strikethrough) in
                            &segments
                        {
                            // Clip the segment to the line it is being drawn on.
                            let start = start.max(line.start);
                            let end = end.min(line.end).min(content.len());
                            if start >= end
                                || !content.is_char_boundary(start)
                                || !content.is_char_boundary(end)
                            {
                                continue;
                            }
                            let segment_text = &content[start..end];
                            if segment_text.is_empty() {
                                continue;
                            }

                            // Measure segment width for positioning
                            let mut options = blinc_layout::text_measure::TextLayoutOptions::new();
                            options.font_name = styled_data.font_family.name.clone();
                            options.generic_font = styled_data.font_family.generic;
                            options.font_weight = if bold { 700 } else { 400 };
                            options.italic = italic;
                            let metrics = blinc_layout::text_measure::measure_text_with_options(
                                segment_text,
                                styled_data.font_size,
                                &options,
                            );
                            // Apply both DPI scale and motion scale to segment width
                            let segment_width = metrics.width * scale * effective_motion_scale.0;

                            texts.push(TextElement {
                                content: segment_text.to_string(),
                                x: scaled_x + x_offset,
                                y: line_y,
                                width: segment_width,
                                // One line keeps the node's full height so
                                // existing vertical centring is untouched;
                                // wrapped text bands each run to its own line.
                                height: if lines.len() > 1 {
                                    scaled_line_height
                                } else {
                                    scaled_height
                                },
                                font_size: scaled_font_size,
                                color,
                                align: TextAlign::Left, // Always left-align segments
                                weight: if bold {
                                    FontWeight::Bold
                                } else {
                                    FontWeight::Normal
                                },
                                italic,
                                v_align: styled_data.v_align,
                                clip_bounds: scaled_clip,
                                // Base-alpha contract: composite factor
                                // applied live at dispatch.
                                motion_opacity: effective_motion_opacity
                                    * render_node.props.opacity
                                    * inherited_css_opacity
                                    * if nearest_composite_ancestor.is_some() {
                                        1.0
                                    } else {
                                        effective_composite_layer_opacity
                                    },
                                wrap: false, // Don't wrap individual segments
                                line_height: styled_data.line_height,
                                measured_width: segment_width,
                                font_family: styled_data.font_family.clone(),
                                word_spacing: 0.0,
                                letter_spacing: render_node.props.letter_spacing.unwrap_or(0.0),
                                z_index: *z_layer,
                                ascender: scaled_ascender * effective_motion_scale.1, // Scale ascender with motion
                                strikethrough,
                                underline,
                                decoration_color: render_node.props.text_decoration_color,
                                decoration_thickness: render_node.props.text_decoration_thickness,
                                css_affine: node_css_affine,
                                text_shadow: render_node.props.text_shadow,
                                transform_3d_layer: inside_3d_layer.clone(),
                                is_foreground: children_inside_foreground,
                                in_motion_subtree: inside_motion_subtree,
                                composite_ancestor: nearest_composite_ancestor,
                            });

                            x_offset += segment_width;
                        }
                    }
                }
            }

            // Collect flow element if this node has a @flow shader reference.
            // Flow elements render via custom GPU pipelines instead of (or on top of) the SDF path.
            if let Some(ref flow_name) = render_node.props.flow {
                flows.push(FlowElement {
                    flow_name: flow_name.clone(),
                    flow_graph: render_node.props.flow_graph.clone(),
                    x: abs_x * scale,
                    y: abs_y * scale,
                    width: bounds.width * scale,
                    height: bounds.height * scale,
                    z_index: *z_layer,
                    corner_radius: render_node.props.border_radius.top_left * scale,
                });
            }
        }

        // Include scroll offset and motion offset when calculating child positions
        let scroll_offset = tree.get_scroll_offset(node);
        let static_motion_offset = tree
            .get_motion_transform(node)
            .map(|t| match t {
                blinc_core::Transform::Affine2D(a) => (a.elements[4], a.elements[5]),
                _ => (0.0, 0.0),
            })
            .unwrap_or((0.0, 0.0));

        let new_offset = (
            abs_x + scroll_offset.0 + static_motion_offset.0,
            abs_y + scroll_offset.1 + static_motion_offset.1,
        );

        // Compute inherited CSS opacity for children: compound this node's CSS opacity
        // CSS `opacity` applies to the element AND its visual subtree
        let child_css_opacity = if let Some(rn) = tree.get_render_node(node) {
            inherited_css_opacity * rn.props.opacity
        } else {
            inherited_css_opacity
        };

        // Detect 3D layer: if this node has rotate-x/rotate-y/perspective,
        // create a Transform3DLayerInfo for children to inherit.
        let child_3d_layer = if let Some(rn) = tree.get_render_node(node) {
            let has_3d = rn.props.rotate_x.is_some()
                || rn.props.rotate_y.is_some()
                || rn.props.perspective.is_some();
            if has_3d {
                let rx = rn.props.rotate_x.unwrap_or(0.0).to_radians();
                let ry = rn.props.rotate_y.unwrap_or(0.0).to_radians();
                let d = rn.props.perspective.unwrap_or(800.0) * scale;
                Some(Transform3DLayerInfo {
                    node_id: node,
                    layer_bounds: [
                        abs_x * scale,
                        abs_y * scale,
                        bounds.width * scale,
                        bounds.height * scale,
                    ],
                    transform_3d: blinc_core::Transform3DParams {
                        sin_rx: rx.sin(),
                        cos_rx: rx.cos(),
                        sin_ry: ry.sin(),
                        cos_ry: ry.cos(),
                        perspective_d: d,
                    },
                    opacity: rn.props.opacity,
                })
            } else {
                inside_3d_layer.clone()
            }
        } else {
            inside_3d_layer.clone()
        };

        for child_id in tree.layout().children(node) {
            self.collect_elements_recursive(
                tree,
                child_id,
                new_offset,
                children_inside_glass,
                children_inside_foreground,
                child_clip,
                child_clip_radius,
                effective_motion_opacity,
                effective_motion_translate,
                effective_motion_scale,
                effective_motion_scale_center,
                render_state,
                scale,
                z_layer,
                texts,
                svgs,
                images,
                flows,
                node_css_affine,
                child_css_opacity,
                Some(node), // pass current node as parent for children
                child_scroll_clip,
                child_3d_layer.clone(),
                subtree_pending_motion,
                child_inside_motion_subtree,
                composite_layer_opacities,
                effective_composite_layer_opacity,
                nearest_composite_ancestor,
            );
        }

        // Restore z_layer after this subtree
        if node_z_index > 0 {
            *z_layer = saved_z_layer;
        }
    }

    /// Get device arc
    pub fn device(&self) -> &Arc<wgpu::Device> {
        &self.device
    }

    /// Get queue arc
    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        &self.queue
    }

    /// Whether the GPU adapter supports storage buffers.
    /// False on WebGL2 (GL adapter) — the renderer uses data textures instead.
    pub fn has_storage_buffers(&self) -> bool {
        self.renderer.has_storage_buffers()
    }

    /// Get the shared font registry
    ///
    /// This can be used to share fonts between text measurement and rendering,
    /// ensuring consistent font loading and metrics.
    pub fn font_registry(&self) -> Arc<Mutex<FontRegistry>> {
        self.text_ctx.font_registry()
    }

    /// Get the texture format used by the renderer
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.renderer.texture_format()
    }

    /// The adapter the renderer was initialized against.
    pub fn adapter(&self) -> &wgpu::Adapter {
        self.renderer.adapter()
    }

    /// Create a new wgpu surface for an additional window (multi-window support)
    pub fn create_surface<W>(
        &self,
        window: Arc<W>,
    ) -> std::result::Result<wgpu::Surface<'static>, blinc_gpu::RendererError>
    where
        W: raw_window_handle::HasWindowHandle
            + raw_window_handle::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        self.renderer.create_surface(window)
    }

    /// Create a new wgpu surface from platform raw handles.
    ///
    /// Android uses this when `ANativeWindow` is recreated while the
    /// renderer/device should be preserved.
    ///
    /// # Safety
    ///
    /// The caller must uphold [`wgpu::Instance::create_surface_unsafe`]'s
    /// handle validity and lifetime requirements.
    pub unsafe fn create_surface_unsafe(
        &self,
        target: wgpu::SurfaceTargetUnsafe,
    ) -> std::result::Result<wgpu::Surface<'static>, blinc_gpu::RendererError> {
        unsafe { self.renderer.create_surface_unsafe(target) }
    }

    /// Render a layout tree with dynamic render state overlays
    ///
    /// This method renders:
    /// 1. The stable RenderTree (element hierarchy and layout)
    /// 2. RenderState overlays (cursors, selections, focus rings)
    ///
    /// The RenderState overlays are drawn on top of the tree without requiring
    /// tree rebuilds. This enables smooth cursor blinking and animations.
    pub fn render_tree_with_state(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        // First render the tree as normal
        self.render_tree(tree, width, height, target)?;

        // Then render overlays from RenderState
        self.render_overlays(render_state, width, height, target);

        Ok(())
    }

    /// Render a layout tree with motion animations from RenderState
    ///
    /// This method renders:
    /// 1. The RenderTree with motion animations applied (opacity, scale, translate)
    /// 2. RenderState overlays (cursors, selections, focus rings)
    ///
    /// Use this method when you have elements wrapped in motion() containers
    /// for enter/exit animations.
    pub fn render_tree_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        self.render_tree_with_motion_opt(tree, render_state, width, height, target, None, false)
    }

    /// Render with motion. `try_fast_paint = true` lets the renderer
    /// skip the paint walker when the cached `PrimitiveBatch` from a
    /// previous full paint is valid and `apply_binding_deltas` can
    /// patch it in place (i.e. only translate / opacity changed
    /// this frame, no scale / rotation moved). On a successful
    /// fast path: skip `tree.render_with_motion(&mut ctx, ...)`,
    /// take the patched cached batch instead of `ctx.take_batch()`,
    /// continue with the existing GPU pipeline.
    ///
    /// On any "fast path not applicable" signal (no cache,
    /// `composite_bindings` requires scale/rotation that the helper
    /// can't handle yet, fast path explicitly disabled) the function
    /// falls back to the full walker path and repopulates the
    /// cache.
    /// Layer-compositor render path. When the renderer can use it,
    /// this is dramatically cheaper per frame than
    /// [`Self::render_tree_with_motion_opt`]:
    ///
    /// - The walker output is rendered once into an offscreen
    ///   "static layer" texture, with canvas drawing skipped (so
    ///   the canvas regions stay transparent in the cache).
    /// - Every subsequent frame, the cache is blitted onto the
    ///   surface (one `copy_texture_to_texture`) and the fresh
    ///   canvas primitives are dispatched on top with
    ///   `LoadOp::Load`.
    ///
    /// Falls back to the existing `render_tree_with_motion_opt`
    /// path when `target_texture` is `None` (no surface texture
    /// reference available — e.g. offscreen render-to-view callers)
    /// or any compositor invariant is violated.
    #[allow(clippy::too_many_arguments)]
    fn try_render_with_compositor(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target_view: &wgpu::TextureView,
        target_texture: &wgpu::Texture,
        try_fast_paint: bool,
    ) -> Result<()> {
        self.renderer.ensure_static_layer(width, height);

        // Image fade-in detection at frame boundary — independent
        // of whether the fast or slow path runs this frame. The
        // image dispatch only runs on the slow path; once the
        // cache is warm we'd never re-set this flag mid-fade, so
        // we read the deadline map directly here.
        //
        // Deadline = `loaded_at + fade_duration_ms` captured at
        // image-load time. Once `now >= deadline`, the fade has
        // visually settled and we drop the entry so the redraw
        // gate stops firing. Previously a blanket 2 s upper bound
        // pinned the chain for ~1.5 s of nothing-changing frames
        // (the actual fade was 250-500 ms).
        let now = web_time::Instant::now();
        self.image_fade_deadlines
            .retain(|_, deadline| now < *deadline);
        self.has_pending_image_fade = !self.image_fade_deadlines.is_empty();

        if self.has_pending_image_fade {
            // Force the slow path so the image dispatch actually
            // runs with the latest fade_factor — fast path would
            // just blit the previous frame's static cache and the
            // image stays frozen at mid-fade.
            self.renderer.invalidate_static_layer();
        }

        // ----- Compositor v2 Phase 1 verification trace -----
        // Compute the per-node animation status from the new model
        // (motion bindings + canvas presence + CSS-anim store, with
        // hysteresis) and log it. Behaviour-neutral — the actual
        // composite path below still uses the legacy
        // `bindings_animating` boolean. Phase 2 wires the status
        // map into the walker; Phase 3 replaces the legacy path.
        //
        // Enable with `RUST_LOG=blinc_app::v2_status=trace` to see
        // the classification each frame:
        //   v2_status frame anim_count=1 static_count=0 \
        //     entries=[(LayoutNodeId(108v1), Animating(Motion))]
        let v2_statuses = tree.compute_animation_status();
        if tracing::enabled!(target: "blinc_app::v2_status", tracing::Level::TRACE) {
            let anim_count = v2_statuses
                .iter()
                .filter(|(_, s)| matches!(s, blinc_layout::renderer::AnimationStatus::Animating(_)))
                .count();
            let static_count = v2_statuses.len() - anim_count;
            tracing::trace!(
                target: "blinc_app::v2_status",
                anim_count,
                static_count,
                entries = ?v2_statuses,
                "v2 animation status",
            );
        }
        tree.commit_animation_status(&v2_statuses);

        // If any motion binding is mid-flight the static-layer cache
        // is stale this frame — its primitives encode the binding's
        // position at the last full paint, not the spring's current
        // value. Invalidate the cache so the compositor takes the
        // full-paint branch and rebuilds it.
        //
        // Canvases don't trigger this because their content is
        // overlaid each frame; only non-canvas binding-bound subtrees
        // (the cn_demo `progress_animated` indicator's translate_x
        // is the canonical case) need to invalidate. The simplest
        // correct rule is "any binding animating → invalidate";
        // smarter mappings (motion-bound subtrees move to the
        // overlay batch) can land later without changing call sites.
        // Detect bindings whose current value would visibly diverge
        // from what the static-layer cache was painted with.
        //
        // Important: a binding being animating does NOT mean we
        // should re-walk the whole tree. The cached batch
        // (`cached_bg_batch`) can be patched in place via
        // `apply_binding_deltas` (translates/scales/rotations) and
        // re-dispatched to the static-layer texture — no walker, no
        // re-emission of surrounding elements. Surrounding elements
        // have nothing to do with the active binding; only the
        // binding-bound primitives' values change.
        //
        // What we DO need to invalidate when bindings move: the
        // cached static-layer TEXTURE pixels, because they were
        // rasterized from the pre-patch batch. The next render must
        // dispatch the patched batch into the texture before blitting
        // to the surface. We rely on the inner
        // `render_tree_with_motion_opt(try_fast_paint=true)` to take
        // the apply_binding_deltas-then-dispatch path; the full
        // walker only runs when the cache is actually structurally
        // invalid.
        //
        // `is_any_animating()` is too sensitive for this purpose:
        // under-damped springs (e.g. `SpringConfig::gentle()`)
        // asymptotically oscillate around the target at sub-pixel
        // amplitude for many seconds before
        // `(value - target).abs() < 0.01` clears the gate. Those
        // sub-pixel wobbles round to the same pixel after
        // rasterization, so the cache stays visually correct — but
        // the "is animating" reading was forcing a full repaint per
        // frame anyway, pinning CPU at vsync forever.
        //
        // Treat a binding as "visibly animating" only if its current
        // value differs from its target by more than half a logical
        // pixel. Same posture for rotations (degrees) — half a
        // degree at typical spinner sizes (16-32 px) is less than a
        // pixel of arc-length travel. Timeline-driven bindings (used
        // by spinners — but those are canvases, not motion-bound,
        // so this branch wouldn't hit) always count as animating
        // because they have no notion of "target settled".
        const VISIBLE_PIXEL_EPS: f32 = 0.5;
        const VISIBLE_DEG_EPS: f32 = 0.5;
        let value_far_from_target =
            |v: &Option<blinc_animation::SharedAnimatedValue>, eps: f32| -> bool {
                v.as_ref()
                    .and_then(|s| s.lock().ok())
                    .map(|g| (g.get() - g.target()).abs() > eps)
                    .unwrap_or(false)
            };
        // Detect direct-write motion (`set_immediate`): the binding
        // is at its target (so `is_any_animating` returns false and
        // value_far_from_target won't fire) but its CURRENT value
        // has drifted from the cache's `last_translate` /
        // `last_opacity` snapshot. Compare against composite_bindings
        // — the walker's record of what got baked into the cached
        // primitive batch — for the bindings where mismatch
        // visibly matters: translate and opacity. Scale / rotation
        // also drift, but their resting value is already detected
        // by `value_far_from_target` (any `set_immediate` to a
        // value that differs from the binding's target would also
        // re-target it, tripping the mid-flight gate). Keeping the
        // drift check tight (just translate + opacity) avoids
        // false positives that would pin CPU forever.
        let direct_write_drift = {
            let bindings_table = tree.motion_bindings_map();
            tree.composite_bindings().iter().any(|(node, meta)| {
                let Some(b) = bindings_table.get(node) else {
                    return false;
                };
                let cx = b
                    .translate_x
                    .as_ref()
                    .and_then(|v| v.lock().ok())
                    .map(|g| g.get())
                    .unwrap_or(0.0);
                let cy = b
                    .translate_y
                    .as_ref()
                    .and_then(|v| v.lock().ok())
                    .map(|g| g.get())
                    .unwrap_or(0.0);
                (cx - meta.last_translate.0).abs() > VISIBLE_PIXEL_EPS
                    || (cy - meta.last_translate.1).abs() > VISIBLE_PIXEL_EPS
            })
        };
        let bindings_animating = direct_write_drift
            || tree.motion_bindings_map().values().any(|b| {
                value_far_from_target(&b.translate_x, VISIBLE_PIXEL_EPS)
                    || value_far_from_target(&b.translate_y, VISIBLE_PIXEL_EPS)
                    || value_far_from_target(&b.scale, 0.01)
                    || value_far_from_target(&b.scale_x, 0.01)
                    || value_far_from_target(&b.scale_y, 0.01)
                    || value_far_from_target(&b.rotation, VISIBLE_DEG_EPS)
                    || value_far_from_target(&b.opacity, 0.01)
                    || b.rotation_timeline
                        .as_ref()
                        .and_then(|t| t.timeline.lock().ok())
                        .is_some_and(|g| g.is_playing())
            });
        // Cache-invalidation gate — Site B in the animation-driver
        // map. The single source of truth is
        // `RenderTree::has_any_active_animation`, which ORs in
        // every system that mutates state per frame:
        //
        // - MotionBindings (springs, set_immediate-driven drag,
        //   rotation timelines)
        // - motion() FSM enter / exit (PageTransition, fade_in,
        //   etc.)
        // - animate_bounds / animate_layout (accordion height,
        //   FLIP-style position transitions)
        // - FLIP transitions on string IDs
        // - CSS keyframe animations
        // - CSS property transitions (the image placeholder
        //   fade-in, hover styles, etc.)
        //
        // Without this union, individual systems went unaccounted
        // for and produced the "transition only plays when I
        // scroll" symptom — the scheduler ticks the animation, the
        // windowed redraw chain fires `request_redraw`, but the
        // compositor fast path here saw cache-valid and returned
        // the frozen surface. Any input event coincidentally
        // invalidated the cache and the animation appeared to
        // "catch up".
        //
        // `bindings_animating` here also catches `set_immediate`
        // drift (binding's current value diverges from
        // composite_bindings.last_translate) — the
        // `has_any_active_animation` path covers spring-mid-flight
        // and rotation_timeline but not set_immediate, so we keep
        // the drift signal alongside.
        // Non-motion-binding animation systems. We intentionally
        // exclude `motion_bindings.is_any_animating()` here because
        // `bindings_animating` above already covers motion bindings
        // with the `VISIBLE_PIXEL_EPS` (0.5 px) threshold — the same
        // threshold our composite path uses to decide whether to
        // patch a binding's primitive range. `is_any_animating` on
        // the spring uses a much tighter 0.01 px threshold, so a
        // settled-but-not-officially-settled spring (between 0.01 px
        // and 0.5 px) was triggering full cache invalidation every
        // frame for sub-pixel oscillation that wasn't visible. The
        // captured `cache_invalidation` trace showed this accounted
        // for ~44 % of all invalidations during cn_demo — a
        // 1:1-replaceable drop in slow-path frames.
        let css_anim_active = {
            let store = tree.css_anim_store();

            match store.lock() {
                Ok(g) => g.has_active_animations() || g.has_active_transitions(),
                Err(_) => false,
            }
        };
        // CSS animations / transitions no longer force a static-cache
        // invalidation on their own — Phase 3a routed the CSS-animated
        // subtrees out of the static batch and into `dynamic_batch`
        // (via `push_motion_subtree`), and the compositor's fast path
        // refreshes those regions by per-region re-walking
        // (`tree.render_dynamic_region` for each entry in
        // `dynamic_regions`). The static cache content is unchanged
        // across CSS-active frames, so re-painting it is wasted work.
        //
        // Other animation systems still go through the slow path
        // because they mutate properties on nodes the walker emits
        // into the static batch (visual / layout animations resize
        // and reposition elements; FLIP / motion FSM enter-exit can
        // restructure things; spring values that drive both motion
        // bindings and CSS keyframes on the same node need a
        // re-walk).
        let other_animations_active = render_state.has_active_motions()
            || tree.has_active_visual_animations()
            || tree.has_active_layout_animations()
            || tree.has_active_flip_animations();
        // The CSS-only frame predicate gates the per-region re-walk
        // step below: only fire when CSS is the ONLY animation source
        // touching primitives this frame, so we don't double-paint
        // motion-bound subtrees that `apply_binding_deltas` is
        // already updating in place.
        let css_only_active = css_anim_active && !other_animations_active;
        // Diagnostic: name the predicate that's invalidating the
        // static cache. Pinpoints which animation source keeps the
        // compositor slow path active in steady state. Off unless
        // `RUST_LOG=blinc_app::cache_invalidation=trace` is set.
        if tracing::enabled!(target: "blinc_app::cache_invalidation", tracing::Level::TRACE)
            && (bindings_animating || other_animations_active)
        {
            let mb_anim = tree
                .motion_bindings_map()
                .values()
                .any(|b| b.is_any_animating());
            let fsm = render_state.has_active_motions();
            let visual = tree.has_active_visual_animations();
            let layout = tree.has_active_layout_animations();
            let flip = tree.has_active_flip_animations();
            let (css_anim, css_xition) = {
                let store = tree.css_anim_store();
                let g = store.lock();
                match g {
                    Ok(s) => (s.has_active_animations(), s.has_active_transitions()),
                    Err(_) => (false, false),
                }
            };
            tracing::trace!(
                target: "blinc_app::cache_invalidation",
                bindings = bindings_animating,
                motion_binding_anim = mb_anim,
                fsm,
                visual,
                layout,
                flip,
                css_anim,
                css_xition,
                "cache_invalidated"
            );
        }
        // Compositor v2 damage-rect path: scaffolding in place but
        // OFF by default. The scissored re-render correctly handles
        // SDF primitives, but the static cache also contains TEXT
        // GLYPHS (dispatched by `render_text` into the same texture)
        // and SVG / image content. The damage-rect path clears the
        // scissor region but only re-draws SDF primitives — wiping
        // any text/SVG in the damage rect, which then re-appears
        // when the next slow-path frame fires.
        //
        // For motion-bound animations next to text (cn_demo's
        // progress bar, switch, slider), this presents as the
        // animated element "vibrating" — text near the moving
        // element flashes in and out as paths alternate.
        //
        // To finish Phase 3 cleanly we need to either:
        //   (a) Re-dispatch text/SVG/image inside the damage rect
        //       too (requires caching their per-frame collected
        //       state on fast-path frames so it's available without
        //       re-running the collect pipeline).
        //   (b) Extract motion-bound primitives from the cache
        //       entirely — the "compositor v2 proper" approach
        //       where the cache holds static content only and
        //       motion-bound elements get a separate per-frame
        //       overlay.
        //
        // Opt-in via `BLINC_DAMAGE_RECT=1` to try the partial
        // implementation. Useful for experimentation / when only
        // pure-SDF subtrees are animating. Default off so visuals
        // stay correct.
        // Compositor v2 Phase 4: motion-binding-only animation frames
        // never invalidate the static cache. Motion-bound subtree
        // primitives live in `cached_dynamic_batch` (the walker
        // routed them out of the static batch via
        // `push_motion_subtree`), so the cache contains zero
        // motion content. `apply_binding_deltas` patches the
        // dynamic batch in place each frame; the per-frame overlay
        // dispatch (after `composite_frame`) shows the new
        // positions. The cache only needs invalidation when a
        // NON-motion-binding animation is active (CSS, FLIP,
        // layout, motion() FSM, visual) — those mutate static-
        // batch primitives the delta patcher can't represent.
        //
        // Drops the cn_demo switch / progress / slider / accordion
        // animations from per-frame slow path to one full paint
        // (entry frame) + per-frame fast path. Cache reused for
        // the entire spring duration.
        let damage_rect_enabled = std::env::var("BLINC_DAMAGE_RECT").as_deref() == Ok("1");
        let damage_rect_eligible =
            damage_rect_enabled && bindings_animating && !other_animations_active;
        if !damage_rect_eligible && other_animations_active {
            self.renderer.invalidate_static_layer();
            // Also invalidate the cached text / SVG / image vectors
            // up-front. Without this clear, the downstream
            // `cache_hit` check in render_tree_with_motion_opt
            // runs AFTER apply_binding_deltas has just updated
            // composite_bindings.last_translate to the current
            // value — so its `direct_write_drift` re-check returns
            // false on the same frame that we already KNEW the
            // cache was stale, and the stale text gets re-used.
            // Symptom: only the first drag updates text correctly,
            // subsequent drags shift primitives but leave the text
            // labels at the previous translate.
            self.cached_texts = None;
            self.cached_svgs = None;
            self.cached_images = None;
            self.cached_flows = None;
            self.cached_glyphs_by_layer = None;
            self.cached_fg_glyphs = None;
            self.cached_motion_subtree_text_prims = None;
            self.motion_subtree_text_patch.clear();
            self.cached_motion_subtree_svgs = None;
            self.cached_css_transformed_text_prims = None;
        }

        // Phase 4d/f: CSS-only patch fast path. When CSS animations
        // / transitions are the *only* animation signal (no motion
        // bindings, no visual / layout / FLIP), patch the cached
        // batch in place via `apply_css_deltas` and re-render the
        // static cache from the patched batch. Skips the walker and
        // `collect_elements_recursive` — the two biggest CPU costs
        // on a CSS-only frame.
        //
        // Phase 4f relaxed the previous `BLINC_CSS_PATCH=1` opt-in:
        // the path now engages by default. The per-record eligibility
        // checks below (cached batch present + static layer valid +
        // walker has populated records) keep the path correct for
        // cold-start frames — first CSS frame after a structural
        // change falls through to slow path, which repopulates the
        // records; subsequent frames take the fast path.
        //
        // Bails (falls through to slow path) on:
        //   - Motion bindings or other animations active (motion
        //     damage path takes precedence; mixing the two would
        //     race their respective last_* bookkeeping).
        //   - Empty `css_anim_paint_records` (walker hasn't seen
        //     the CSS-animated node yet — first frame of a newly-
        //     started animation; slow path populates the records).
        //   - `apply_css_deltas` reports an out-of-scope property
        //     (clip-path geometry, blur, layout) or cache shape
        //     mismatch.
        //
        // Cost vs slow path on a CSS-only frame:
        //   - Skipped: paint walker (~0.5 ms), text shaping +
        //     SVG/image collect (~0.5 ms), per-frame State updates.
        //   - Paid: apply_css_deltas (~50 µs), batch clone for
        //     re-render (~50 µs), full SDF dispatch (same as slow
        //     path), text / SVG / image dispatch from cached
        //     vectors (same as slow path).
        //   - Net ≈ 1 ms saved per CSS-only frame, ~6 % at 60 fps.
        let css_patch_eligible = css_only_active
            && !bindings_animating
            && self.cached_bg_batch.is_some()
            && self.renderer.static_layer_valid()
            && !tree.css_anim_paint_records().is_empty();
        if !css_patch_eligible
            && tracing::enabled!(target: "blinc_app::frame_timing", tracing::Level::TRACE)
        {
            tracing::trace!(
                target: "blinc_app::frame_timing",
                css_anim_active,
                css_only_active,
                bindings_animating,
                try_fast_paint,
                has_cached_batch = self.cached_bg_batch.is_some(),
                static_layer_valid = self.renderer.static_layer_valid(),
                css_records_present = !tree.css_anim_paint_records().is_empty(),
                "css_patch_gate_failed",
            );
        }
        if css_patch_eligible {
            let scale_factor = tree.scale_factor();
            let css_path_start = web_time::Instant::now();
            // CssDeltaResult drives the next step:
            //   - Patched: in-scope patches landed, run the damaged
            //     repaint as before.
            //   - PatchedWithReemits: in-scope patches landed AND
            //     out-of-scope records (layout / clip-geom / blur)
            //     need walker re-emission into `cached_bg_batch`
            //     before the cache repaint. Try
            //     `try_reemit_and_splice_css_subtrees`; on success
            //     we have a fully-up-to-date cache and proceed like
            //     Patched. On failure (complex subtree batch state
            //     we can't splice yet) fall through to the slow path.
            //   - Bail: fall through to the slow path entirely.
            let delta_result = self.apply_css_deltas(tree, scale_factor);
            let took_fast_path = match delta_result {
                CssDeltaResult::Patched => true,
                CssDeltaResult::PatchedWithReemits { ref reemit_nodes } => self
                    .try_reemit_and_splice_css_subtrees(
                        tree,
                        render_state,
                        reemit_nodes,
                        width,
                        height,
                    ),
                CssDeltaResult::Bail => false,
            };
            if took_fast_path {
                let batch_clone = self
                    .cached_bg_batch
                    .as_ref()
                    .expect("css_patch_eligible implies cached_bg_batch is Some")
                    .clone();

                // Phase 4d Opt 2: try the scissored cache repaint
                // first. `render_static_layer_damaged` re-renders
                // only the union of `last_css_damage_rects`,
                // preserving the rest of the cache via LoadOp::Load.
                // Falls back to false today on any batch with
                // `layer_commands` / paths / 3D viewports / particles
                // — Task 3 extends it to handle layer_commands so
                // CSS animations whose walker emitted opacity / blur
                // / 3D layers stay on the damaged path. Batches
                // without layer commands engage already.
                let damaged = self.last_css_damage_rects.clone();
                let damaged_ok = !damaged.is_empty()
                    && self
                        .renderer
                        .render_static_layer_damaged(&damaged, &batch_clone);

                if !damaged_ok {
                    // Full-cache re-render fallback (Phase 4d Opt 1
                    // behaviour). Used when the damaged path bailed
                    // — e.g., batch has layer_commands the
                    // damage-rect code can't replay yet, or no
                    // records moved this frame so there's nothing
                    // to scissor.
                    self.renderer.render_static_layer(
                        &batch_clone,
                        [0.0, 0.0, 0.0, self.clear_alpha as f64],
                    );
                }

                // Re-dispatch text / SVG / image. When the damaged
                // path engaged, only the cache region inside the
                // damage union was cleared + repainted; we filter
                // cached vectors to those intersecting the damage
                // and dispatch through `pending_scissor` so the
                // writes stay inside the cleared region. When the
                // full-cache fallback ran, dispatch every cached
                // vector unfiltered — the whole cache needs
                // re-stamping.
                //
                // Cached vectors are from the last slow-path frame.
                // For pure visual / 3D-transform / colour CSS
                // animations they stay valid (positions don't
                // shift). Layout animations would invalidate them,
                // but layout properties trigger an
                // `apply_css_deltas` bail above so we never reach
                // here with stale positions.
                let static_view_opt = self.renderer.static_layer_view().cloned();
                if let Some(static_view) = static_view_opt {
                    if damaged_ok {
                        let union = damage_union(&damaged);
                        if let Some(scissor) = damage_scissor_from_union(union, &self.renderer) {
                            self.renderer.set_pending_scissor(scissor);
                            if let Some(glyphs_by_layer) = self.cached_glyphs_by_layer.clone() {
                                for glyphs in glyphs_by_layer.values() {
                                    let filtered: Vec<_> = glyphs
                                        .iter()
                                        .filter(|g| aabb_intersects_any(g.bounds, &damaged))
                                        .copied()
                                        .collect();
                                    if !filtered.is_empty() {
                                        self.render_text(&static_view, &filtered);
                                    }
                                }
                            }
                            if let Some(fg_glyphs) = self.cached_fg_glyphs.clone() {
                                let filtered: Vec<_> = fg_glyphs
                                    .iter()
                                    .filter(|g| aabb_intersects_any(g.bounds, &damaged))
                                    .copied()
                                    .collect();
                                if !filtered.is_empty() {
                                    self.render_text(&static_view, &filtered);
                                }
                            }
                            if let Some(svgs) = self.cached_svgs.clone() {
                                let filtered: Vec<_> = svgs
                                    .into_iter()
                                    .filter(|s| {
                                        aabb_intersects_any([s.x, s.y, s.width, s.height], &damaged)
                                    })
                                    .collect();
                                if !filtered.is_empty() {
                                    self.render_rasterized_svgs(
                                        &static_view,
                                        &filtered,
                                        scale_factor,
                                    );
                                }
                            }
                            if let Some(images) = self.cached_images.clone() {
                                let filtered_refs: Vec<&ImageElement> = images
                                    .iter()
                                    .filter(|i| {
                                        aabb_intersects_any([i.x, i.y, i.width, i.height], &damaged)
                                    })
                                    .collect();
                                if !filtered_refs.is_empty() {
                                    self.render_images_ref(&static_view, &filtered_refs);
                                }
                            }
                            self.renderer.clear_pending_scissor();
                        }
                    } else {
                        // Full re-dispatch (no scissor) — cache was
                        // fully cleared by `render_static_layer`.
                        if let Some(glyphs_by_layer) = self.cached_glyphs_by_layer.clone() {
                            for glyphs in glyphs_by_layer.values() {
                                if !glyphs.is_empty() {
                                    self.render_text(&static_view, glyphs);
                                }
                            }
                        }
                        if let Some(fg_glyphs) = self.cached_fg_glyphs.clone() {
                            if !fg_glyphs.is_empty() {
                                self.render_text(&static_view, &fg_glyphs);
                            }
                        }
                        if let Some(svgs) = self.cached_svgs.clone() {
                            if !svgs.is_empty() {
                                self.render_rasterized_svgs(&static_view, &svgs, scale_factor);
                            }
                        }
                        if let Some(images) = self.cached_images.clone() {
                            if !images.is_empty() {
                                let refs: Vec<&ImageElement> = images.iter().collect();
                                self.render_images_ref(&static_view, &refs);
                            }
                        }
                    }
                }
                // Composite cache + canvas overlay onto the surface
                // (canvas primitives carry their wrapper's
                // ancestor_clip baked at emit time). The OverlayStack
                // dynamic batch then rides on top via its own SDF
                // pass — see dispatch_dynamic_batch_overlay for why
                // the two are no longer merged into one dispatch.
                let overlay = self.collect_canvas_overlay(tree, width, height);
                let overlay_subtree_canvases =
                    self.collect_overlay_canvas_overlay(tree, width, height);
                if !tree.dynamic_regions().is_empty() {
                    let walked =
                        self.collect_dynamic_region_primitives(tree, render_state, width, height);
                    // Only overwrite when the re-walk produced
                    // primitives. `collect_dynamic_region_primitives`
                    // filters out `DynamicKind::CssAnimated` and
                    // `DynamicKind::Canvas` — both paint through their
                    // own paths (composite_css_layers_overlay /
                    // canvas overlay). When the only dynamic regions
                    // present are those two kinds (e.g. a settled
                    // cn::context_menu parent + an actively-entering
                    // submenu + the node-editor canvas), the filtered
                    // ordered set is empty and `walked` is
                    // `PrimitiveBatch::new()`. Overwriting with an
                    // empty batch wipes the prior slow-walker-emitted
                    // overlay-subtree primitives (panel bg, border,
                    // item :hover bg) routed through dynamic_batch
                    // via the `is_overlay_root` motion-subtree gate
                    // in paint/motion.rs — the parent menu would
                    // disappear from the surface until the submenu's
                    // CSS enter animation settles and the next slow
                    // walker runs.
                    if !walked.primitives.is_empty() {
                        self.cached_dynamic_batch = Some(walked);
                    }
                }
                self.rebind_glyph_atlas_for_overlay();
                self.renderer.composite_frame(
                    target_view,
                    target_texture,
                    &overlay.primitives,
                    &overlay.aux_data,
                );
                // Canvas-emitted dynamic images (portal_ui noise /
                // texture nodes via draw_rgba_pixels). Dispatched in the
                // canvas band — BEFORE the foreground overlay —
                // so a node's RGBA content sits with the rest of the
                // canvas content and UNDER foreground chrome (minimap,
                // search bar, host `RenderLayer::Foreground` overlays).
                // Each image is scissored to its node body by the clip
                // captured at emit time (see `render_dynamic_images`).
                if !overlay.dynamic_images.is_empty() {
                    self.renderer
                        .render_dynamic_images(target_view, &overlay.dynamic_images);
                }
                // Foreground overlay — re-dispatch fg primitives on
                // top of the canvas overlay so host overlays marked
                // `RenderLayer::Foreground` win z-order over canvas
                // content, while still staying below global overlay
                // surfaces. See `render_foreground_overlay` doc.
                self.render_foreground_overlay(target_view, &[&overlay]);
                self.dispatch_dynamic_batch_overlay(target_view);
                // Overlay-subtree canvases (e.g. caret blink inside a
                // focused cn::input) dispatch HERE so they land above
                // the dyn-batch SDF the popover just painted.
                self.dispatch_overlay_canvas_overlay(target_view, &overlay_subtree_canvases);
                // Composited CSS layers — blit each promoted subtree's
                // texture with the current animation transform. MUST
                // run BEFORE the motion-subtree text / SVG overlays
                // below: composite-promoted subtrees (cn::context_menu,
                // cn::popover, cn::dropdown) bake their BG SDF into a
                // `LayerTexture`, but their text glyphs route through
                // a separate `cached_motion_subtree_text_prims` pool
                // because text is segregated from the SDF batch up in
                // `render_tree_with_motion`. Dispatching text FIRST
                // and the composite BG SECOND inverts tree-order
                // z-ordering — the opaque BG texture lands on top of
                // its own descendants' glyphs at the end of the entry
                // animation and the menu reads as a blank rectangle.
                // Symptom (2026-06-15): cn::context_menu BG faded in
                // correctly post is_composite_promotable fix but the
                // menu item text was invisible until a mouse-move
                // path eventually re-dispatched it.
                self.composite_css_layers_overlay(tree, target_view);
                // P4.3 sibling: motion-subtree textures.
                self.composite_motion_layers_overlay(tree, target_view);
                // Motion-subtree text + SVG overlay — rendered on the
                // surface after the motion-bound bg primitives AND
                // after the composite-layer blits so they land on top
                // of both. PRIM_TEXT primitives carry `local_affine`
                // so the motion container's scale / translate / rotate
                // applies to each glyph. SVGs go through the same
                // rasterized-image dispatch the static-cache path uses.
                self.dispatch_motion_subtree_overlay_pools(tree, target_view);
                if !overlay.meshes.is_empty() {
                    dispatch_pending_meshes(
                        &mut self.renderer,
                        target_view,
                        width,
                        height,
                        &overlay.meshes,
                    );
                }
                if !overlay.gpu_passes.is_empty() {
                    dispatch_pending_gpu_passes(
                        &mut self.renderer,
                        target_view,
                        width,
                        height,
                        tree.scale_factor(),
                        &overlay.gpu_passes,
                    );
                }
                // Authoritative `visible_anim_active` update —
                // walker didn't run this frame, so reset the flag
                // from current animation state. Mirrors the motion
                // damage path's end-of-frame restate.
                let has_visible_canvas = !tree.canvas_paint_records().is_empty();
                let any_visible_anim = {
                    let painted_set = tree.painted_node_ids().clone();
                    let painted_stable = tree.painted_stable_ids();
                    tree.has_any_active_animation_visible(
                        render_state,
                        &painted_set,
                        &painted_stable,
                    )
                };
                tree.set_visible_anim_active(any_visible_anim || has_visible_canvas);
                tracing::trace!(
                    target: "blinc_app::frame_timing",
                    path = "css_patch",
                    damaged = damaged_ok,
                    damage_rect_count = damaged.len(),
                    gpu_us = css_path_start.elapsed().as_micros() as u64,
                    "fast_path",
                );
                return Ok(());
            }
            // apply_css_deltas returned false — out-of-scope
            // property, cache shape mismatch, or divide-by-zero.
            // Fall through to slow path.
        }

        // Fast path: cache valid AND caller is fine with reusing it.
        // Skip the entire walker / dispatch chain — just composite.
        let use_fast =
            try_fast_paint && self.renderer.static_layer_valid() && self.cached_bg_batch.is_some();

        if use_fast {
            let fast_path_start = web_time::Instant::now();
            // Compositor v2 damage-rect step: when motion bindings
            // are animating, patch the cached batch in-place and
            // repaint the affected regions of the static cache.
            // `apply_binding_deltas` populates
            // `self.last_binding_damage_rects` with one union-AABB per
            // moved binding. We then call `render_static_layer_damaged`
            // to clear-and-redraw those rects inside the cache (with
            // scissor; `LoadOp::Load` preserves the surrounding
            // pixels). Falls back to invalidation + slow path on any
            // condition the damage-rect path can't handle.
            // Compositor v2 Phase 4: when any motion binding moved
            // this frame, patch its primitives in `cached_dynamic_batch`
            // (where the walker routed them via `push_motion_subtree`)
            // so the per-frame overlay dispatch below paints them at
            // the current spring values. The static cache stays
            // untouched. Used by all the cn motion widgets — switch
            // thumb, progress indicator, slider thumb, sortable drag
            // preview, etc.
            // Single `apply_binding_deltas` per frame. Previously the
            // damage-rect path called it again after the unconditional
            // motion-binding update — the second call saw zero deltas
            // (because the first had already advanced `last_translate`
            // / `last_rotation_rad` / etc.) so `last_binding_damage_rects`
            // came back empty and the scissored repaint step never
            // ran. One call collects damage rects AND patches the
            // cached dynamic batch in the same pass; the rect set is
            // then consumed by the damage-rect branch below.
            let mut damage_rect_failed = false;
            let patched = if bindings_animating && self.cached_dynamic_batch.is_some() {
                let scale_factor = tree.scale_factor();
                self.apply_binding_deltas(tree, scale_factor)
            } else {
                true
            };
            // Bail handling MUST run regardless of
            // `damage_rect_eligible`. `apply_binding_deltas` returns
            // false on the last-opacity-is-zero guard (opacity
            // binding going `0 → 1` can't be ratio-scaled) and on
            // degenerate scale (last_scale ~ 0). In either case the
            // cache is stale this frame; `composite_frame` would
            // blit the previous frame's pixels and the animation
            // visibly freezes until something else (mouse move,
            // scroll) forced a slow-path paint. Symptom: switch
            // toggle's color fade (`motion().opacity(color_anim)`
            // 0 → 1) starts the spring but the colored track stays
            // at off-state alpha until you wiggle the mouse.
            //
            // Pre-fix this branch was gated on `damage_rect_eligible`
            // which itself required `BLINC_DAMAGE_RECT=1`, so the
            // bail handling never fired in the default build. Move
            // it out of that gate — invalidate + `damage_rect_failed`
            // marker so the slow path below picks up and re-walks
            // with current binding values.
            if !patched {
                self.renderer.invalidate_static_layer();
                self.cached_texts = None;
                self.cached_svgs = None;
                self.cached_images = None;
                self.cached_flows = None;
                self.cached_glyphs_by_layer = None;
                self.cached_fg_glyphs = None;
                self.cached_motion_subtree_text_prims = None;
                self.motion_subtree_text_patch.clear();
                self.cached_motion_subtree_svgs = None;
                self.cached_css_transformed_text_prims = None;
                damage_rect_failed = true;
            }
            if damage_rect_eligible
                && self.cached_bg_batch.is_some()
                && patched
                && !self.last_binding_damage_rects.is_empty()
            {
                {
                    let damaged = self.last_binding_damage_rects.clone();
                    let batch_ref = self
                        .cached_bg_batch
                        .as_ref()
                        .expect("use_fast implies cached_bg_batch is Some");
                    let ok = self
                        .renderer
                        .render_static_layer_damaged(&damaged, batch_ref);
                    if !ok {
                        // Batch had content the damage-rect path
                        // can't handle (layer effects, paths, 3D
                        // viewports). Bail to full invalidation and
                        // fall through — next frame will re-walk.
                        self.renderer.invalidate_static_layer();
                        self.cached_texts = None;
                        self.cached_svgs = None;
                        self.cached_images = None;
                        self.cached_flows = None;
                        self.cached_glyphs_by_layer = None;
                        self.cached_fg_glyphs = None;
                        self.cached_motion_subtree_text_prims = None;
                        self.motion_subtree_text_patch.clear();
                        self.cached_motion_subtree_svgs = None;
                        self.cached_css_transformed_text_prims = None;
                    } else {
                        // Re-dispatch any glyph / SVG / image that
                        // intersects the damage rect, with
                        // `pending_scissor` set so the writes are
                        // confined to the just-cleared region.
                        // `render_static_layer_damaged` only re-paints
                        // SDF primitives; without this re-dispatch the
                        // scissored clear wipes everything else that
                        // was previously painted in the same pixels
                        // — text, SVG icons, raster images — and
                        // they wouldn't reappear until the next full
                        // slow-path paint. Phase 4 of the compositor
                        // plan, finally making `BLINC_DAMAGE_RECT=1`
                        // safe.
                        //
                        // Each render method (`render_text`,
                        // `render_rasterized_svgs`, `render_images_ref`)
                        // honours `pending_scissor` internally — it
                        // gets applied to the underlying render pass
                        // via `wgpu::RenderPass::set_scissor_rect`.
                        let static_view_opt = self.renderer.static_layer_view().cloned();
                        if let Some(static_view) = static_view_opt {
                            let union = damage_union(&damaged);
                            if let Some(scissor) = damage_scissor_from_union(union, &self.renderer)
                            {
                                let scale_factor = tree.scale_factor();
                                self.renderer.set_pending_scissor(scissor);
                                if let Some(glyphs_by_layer) = self.cached_glyphs_by_layer.clone() {
                                    for glyphs in glyphs_by_layer.values() {
                                        let filtered: Vec<_> = glyphs
                                            .iter()
                                            .filter(|g| aabb_intersects_any(g.bounds, &damaged))
                                            .copied()
                                            .collect();
                                        if !filtered.is_empty() {
                                            self.render_text(&static_view, &filtered);
                                        }
                                    }
                                }
                                if let Some(fg_glyphs) = self.cached_fg_glyphs.clone() {
                                    let filtered: Vec<_> = fg_glyphs
                                        .iter()
                                        .filter(|g| aabb_intersects_any(g.bounds, &damaged))
                                        .copied()
                                        .collect();
                                    if !filtered.is_empty() {
                                        self.render_text(&static_view, &filtered);
                                    }
                                }
                                // SVG re-dispatch. `SvgElement` x/y/w/h
                                // are stored in physical pixels (the
                                // collect path multiplies by
                                // `scale_factor`), so they share the
                                // damage rects' coordinate space.
                                if let Some(svgs) = self.cached_svgs.clone() {
                                    let filtered: Vec<_> = svgs
                                        .into_iter()
                                        .filter(|s| {
                                            aabb_intersects_any(
                                                [s.x, s.y, s.width, s.height],
                                                &damaged,
                                            )
                                        })
                                        .collect();
                                    if !filtered.is_empty() {
                                        self.render_rasterized_svgs(
                                            &static_view,
                                            &filtered,
                                            scale_factor,
                                        );
                                    }
                                }
                                // Image re-dispatch — same coordinate
                                // convention. Filter cached images by
                                // damage intersection then dispatch
                                // through the standard image path,
                                // which routes through `render_images`
                                // whose render pass honours
                                // `pending_scissor`.
                                if let Some(images) = self.cached_images.clone() {
                                    let filtered_refs: Vec<&ImageElement> = images
                                        .iter()
                                        .filter(|i| {
                                            aabb_intersects_any(
                                                [i.x, i.y, i.width, i.height],
                                                &damaged,
                                            )
                                        })
                                        .collect();
                                    if !filtered_refs.is_empty() {
                                        self.render_images_ref(&static_view, &filtered_refs);
                                    }
                                }
                                self.renderer.clear_pending_scissor();
                            }
                        }
                    }
                }
            }

            // Damage-rect path bailed (apply_binding_deltas couldn't
            // patch — e.g. opacity-from-zero divide-by-zero guard).
            // Cache was invalidated above; falling through to the
            // slow path below will re-walk and emit fresh primitives
            // at the current binding values, so the animation
            // progresses on this same frame instead of freezing
            // until a mouse move forces a paint.
            if damage_rect_failed {
                // Note: do NOT `return Ok(())` — proceed past the
                // `if use_fast` block to the slow path.
            } else {
                // Full canvas overlay — drains SDF primitives + raw-
                // RGBA images (video frames) + 3D meshes from every
                // canvas closure. composite_frame blits the static
                // cache + dispatches the canvas SDF prims; we then
                // layer the dynamic images and meshes onto the same
                // target.
                let overlay = self.collect_canvas_overlay(tree, width, height);
                let overlay_subtree_canvases =
                    self.collect_overlay_canvas_overlay(tree, width, height);
                // Compositor v2: motion-bound subtree primitives go
                // into a separate `cached_dynamic_batch` (walker
                // pushed motion subtree depth around them so they
                // bypassed the static cache).
                //
                // The dynamic batch carries its own `aux_data`
                // (polygon-clip vertices for the spinner arc, 3D
                // group descriptors, etc.). Dispatched in its own
                // pass via dispatch_dynamic_batch_overlay AFTER the
                // canvas overlay so OverlayStack content can never
                // get z-interleaved with canvas content in a single
                // sorted SDF dispatch — the prior single-merged-
                // dispatch shape produced a horizontal "split" on
                // popovers / toolbars / focused inputs straddling the
                // canvas's content edge in node_editor_demo.
                //
                // CSS-only animation frames: per-region re-walk
                // refreshes every `DynamicRegion`'s primitives at the
                // current animation state. We write the walked batch
                // back into `cached_dynamic_batch` so the next frame
                // — even if it leaves the CSS-only path (because the
                // animation just settled, or a motion binding started
                // alongside it) — picks up the settled-value
                // primitives instead of falling back to the pre-CSS
                // slow-path snapshot. Motion-only frames keep using
                // the cached batch as patched by
                // `apply_binding_deltas` above.
                if css_only_active && !tree.dynamic_regions().is_empty() {
                    let walked =
                        self.collect_dynamic_region_primitives(tree, render_state, width, height);
                    // See sibling gate in the css_patch_eligible
                    // branch above for the rationale: a walked batch
                    // that's empty after filtering CssAnimated /
                    // Canvas regions must NOT overwrite the prior
                    // slow-walker overlay-subtree primitives. Without
                    // this guard a settled parent cn::context_menu
                    // (or popover / dropdown) sitting beside an
                    // actively-animating CSS-promoted sibling loses
                    // its panel bg + border for the duration of the
                    // sibling's enter animation.
                    if !walked.primitives.is_empty() {
                        self.cached_dynamic_batch = Some(walked);
                    }
                }
                self.rebind_glyph_atlas_for_overlay();
                self.renderer.composite_frame(
                    target_view,
                    target_texture,
                    &overlay.primitives,
                    &overlay.aux_data,
                );
                // Canvas-emitted dynamic images (noise / texture nodes)
                // dispatched in the canvas band — before the foreground
                // overlay — and scissored to their node body. See the
                // longer note at the first fast-path site.
                if !overlay.dynamic_images.is_empty() {
                    self.renderer
                        .render_dynamic_images(target_view, &overlay.dynamic_images);
                }
                // Foreground overlay — re-dispatch fg primitives on
                // top of the canvas overlay so host overlays marked
                // `RenderLayer::Foreground` win z-order over canvas
                // content, while still staying below global overlay
                // surfaces. See `render_foreground_overlay` doc.
                self.render_foreground_overlay(target_view, &[&overlay]);
                self.dispatch_dynamic_batch_overlay(target_view);
                self.dispatch_overlay_canvas_overlay(target_view, &overlay_subtree_canvases);
                // Composited CSS layers — blit each promoted subtree's
                // texture with the current animation transform. MUST
                // run BEFORE the motion-subtree text / SVG overlays
                // below — see the longer comment on the slow-path
                // counterpart for the z-order rationale.
                self.composite_css_layers_overlay(tree, target_view);
                // P4.3 sibling: motion-subtree textures.
                self.composite_motion_layers_overlay(tree, target_view);
                // Motion-subtree text + SVG overlay — rendered after
                // composite-layer blits so glyphs / SVGs land on top.
                // PRIM_TEXT primitives carry `local_affine` so the
                // motion container's scale / translate / rotate applies
                // to each glyph. SVGs go through the same rasterized-
                // image dispatch the static-cache path uses.
                self.dispatch_motion_subtree_overlay_pools(tree, target_view);
                if !overlay.meshes.is_empty() {
                    dispatch_pending_meshes(
                        &mut self.renderer,
                        target_view,
                        width,
                        height,
                        &overlay.meshes,
                    );
                }
                if !overlay.gpu_passes.is_empty() {
                    dispatch_pending_gpu_passes(
                        &mut self.renderer,
                        target_view,
                        width,
                        height,
                        tree.scale_factor(),
                        &overlay.gpu_passes,
                    );
                }

                // The walker normally writes `visible_anim_active` from
                // its per-frame observations (canvas paints, active
                // motion bindings, active motion FSM). On the compositor
                // fast path the walker doesn't run, so the flag would
                // stay latched at whatever the last full paint left it
                // at — typically `true` after any binding animation,
                // which then pins Phase 5's redraw chain at vsync
                // forever even when the bar's spring has long since
                // settled. (Symptom: "after the animation plays, CPU
                // never drops.") Restate the flag authoritatively from
                // the two signals we can observe here:
                //
                //   - Any canvas is on screen → its draw closure may
                //     produce new output every frame, keep the chain
                //     alive.
                //   - Any motion binding is mid-flight → its spring is
                //     advancing, keep the chain alive.
                //
                // Motion-FSM-driven elements (enter/exit) live in a
                // separate signal (`render_state.has_active_motions()`)
                // that the windowed runner ORs in alongside this flag,
                // so we don't need to mirror them here.
                // Site C in the animation-driver map. Mirror the
                // unified predicate the cache-invalidation gate uses,
                // visibility-filtered so a CSS keyframe on a node
                // scrolled out of view doesn't pin the chain forever.
                // The fast path doesn't have a fresh painted set
                // (walker didn't run this frame); use the last walker
                // pass's set, which the windowed runner's redraw gate
                // also relies on.
                let has_visible_canvas = !tree.canvas_paint_records().is_empty();
                let any_visible_anim = {
                    let painted_set = tree.painted_node_ids().clone();
                    let painted_stable = tree.painted_stable_ids();
                    tree.has_any_active_animation_visible(
                        render_state,
                        &painted_set,
                        &painted_stable,
                    )
                };
                tree.set_visible_anim_active(any_visible_anim || has_visible_canvas);

                let path_label = if bindings_animating {
                    "binding_damage"
                } else {
                    "cache_blit"
                };
                tracing::trace!(
                    target: "blinc_app::frame_timing",
                    path = path_label,
                    damage_rect_count = self.last_binding_damage_rects.len(),
                    gpu_us = fast_path_start.elapsed().as_micros() as u64,
                    "fast_path",
                );
                return Ok(());
            } // close else { ... } for !damage_rect_failed
        }

        // Full paint into the static layer. The existing
        // `render_tree_with_motion_opt` is reused with the
        // static-layer view as its target. `skip_canvas_drawing` is
        // set so the walker doesn't emit canvas primitives — those
        // are re-emitted into the overlay batch below.
        let static_view = self.renderer.static_layer_view().cloned();
        let static_view = match static_view {
            Some(v) => v,
            None => {
                // Static layer not allocated (zero-size viewport?) —
                // fall through to the non-compositor path so the
                // frame still presents.
                return self.render_tree_with_motion_opt(
                    tree,
                    render_state,
                    width,
                    height,
                    target_view,
                    None,
                    try_fast_paint,
                );
            }
        };

        tree.set_skip_canvas_drawing(true);
        // Tell the inner `render_tree_with_motion_opt` call NOT to
        // dispatch `cached_dynamic_batch` onto its `target` (the
        // static layer view) — composite_frame below handles that
        // dispatch onto the surface. Without this gate, the motion-
        // bound primitives end up in BOTH the static cache (at the
        // slow-paint rotation) and the per-frame overlay (at the
        // current rotation), which is the cn::spinner double-arc
        // symptom.
        self.in_compositor_inner_call = true;
        // Pass `try_fast_paint=true` so the inner call can take its
        // `apply_binding_deltas`-then-dispatch path when the cache
        // is structurally valid but pixel-stale (the
        // bindings-animating case). That skips the walker for the
        // surrounding tree — only the patched binding values
        // re-flow through dispatch, no traversal of unrelated
        // nodes. Falls back to the full walker only when the cache
        // is genuinely invalid (rebuild, layout change).
        //
        // `apply_binding_deltas` can ONLY patch motion_binding
        // primitive ranges (translate / scale / rotation /
        // opacity). It has no representation for any other
        // animation system — motion() FSM, animate_bounds,
        // animate_layout, FLIP, CSS keyframes, CSS transitions.
        // When ANY of those systems is live, the inner fast path
        // must fall through to the walker so primitives + text
        // emit at the current animation state in lockstep.
        //
        // `bindings_animating` (which includes the
        // set_immediate-drift case AND always-playing rotation
        // timelines — the cn::spinner case) doesn't require the
        // walker because apply_binding_deltas handles all four
        // patchable channels. Splitting it out here is what keeps
        // a rotating spinner at ~1 % CPU instead of forcing a
        // full-tree re-walk every frame.
        let needs_walker = render_state.has_active_motions()
            || tree.has_active_visual_animations()
            || tree.has_active_layout_animations()
            || tree.has_active_flip_animations()
            // Scroll-physics-driven offset changes (programmatic
            // `scroll_to_animated`, edge bounce spring, post-flick
            // momentum) advance the scroll offset off any input.
            // The cached batch was emitted at the pre-tick offset, so
            // skipping the walker reuses stale scroll content. The
            // symptom is that smooth `scroll_to` looks like an instant
            // jump that only "catches up" when the next mouse move
            // forces a full re-walk through some other path. Wheel /
            // touch drag scrolls don't need this — their input event
            // already invalidates the static layer via `had_scroll`.
            || tree.has_animating_scroll_physics()
            || {
                let store = tree.css_anim_store();
                let guard = store.lock();
                match guard {
                    Ok(g) => g.has_active_animations() || g.has_active_transitions(),
                    Err(_) => false,
                }
            };
        let inner_try_fast = self.cached_bg_batch.is_some() && !needs_walker;
        let result = self.render_tree_with_motion_opt(
            tree,
            render_state,
            width,
            height,
            &static_view,
            None, // suppress compositor mode inside the inner call
            inner_try_fast,
        );
        self.in_compositor_inner_call = false;
        tree.set_skip_canvas_drawing(false);
        result?;

        self.renderer.mark_static_layer_valid();

        // ----- Compositor v2 Phase 2 verification trace -----
        // Compare the new `dynamic_regions` map populated by the
        // walker against the legacy `canvas_paint_records` +
        // `composite_bindings` records the existing composite path
        // still consumes. The expected invariant: every canvas in
        // `canvas_paint_records` produces a `DynamicKind::Canvas`
        // entry; every motion-bound node whose status was
        // `Animating(Motion)` produces a `DynamicKind::MotionSubtree`
        // entry. Run with `RUST_LOG=blinc_app::v2_regions=trace`.
        if tracing::enabled!(target: "blinc_app::v2_regions", tracing::Level::TRACE) {
            let regions = tree.dynamic_regions();
            let canvas_count = regions
                .values()
                .filter(|r| matches!(r.kind, blinc_layout::renderer::DynamicKind::Canvas { .. }))
                .count();
            let motion_count = regions
                .values()
                .filter(|r| matches!(r.kind, blinc_layout::renderer::DynamicKind::MotionSubtree))
                .count();
            let css_count = regions
                .values()
                .filter(|r| {
                    matches!(
                        r.kind,
                        blinc_layout::renderer::DynamicKind::CssAnimated { .. }
                    )
                })
                .count();
            let legacy_canvas = tree.canvas_paint_records().len();
            let legacy_motion = tree.composite_bindings().len();
            tracing::trace!(
                target: "blinc_app::v2_regions",
                regions = regions.len(),
                canvas = canvas_count,
                motion = motion_count,
                css = css_count,
                legacy_canvas,
                legacy_motion,
                "v2 dynamic regions",
            );
        }

        // Composite static cache + full canvas overlay onto the
        // surface. The overlay drain captures SDF primitives, raw
        // dynamic images (video frames), and 3D meshes — each
        // dispatched in z-order via its own pipeline so video and
        // mesh content actually reaches the surface in compositor
        // mode (it was dropped on the floor by the
        // primitives-only path).
        let overlay = self.collect_canvas_overlay(tree, width, height);
        let overlay_subtree_canvases = self.collect_overlay_canvas_overlay(tree, width, height);
        // Compositor v2: motion-bound subtree primitives ride in a
        // SEPARATE SDF pass via dispatch_dynamic_batch_overlay,
        // mirroring the fast paths above. Splitting the dispatch
        // keeps canvas-emitted primitives (which carry their wrapper
        // ancestor_clip baked at emit time) z-isolated from
        // OverlayStack / motion-subtree primitives — the prior single
        // merged dispatch let canvas content z-interleave with
        // popover / toolbar / focused-input bg primitives in
        // node_editor_demo, producing a horizontal "split" where the
        // popover crossed the canvas content edge.
        self.rebind_glyph_atlas_for_overlay();
        self.renderer.composite_frame(
            target_view,
            target_texture,
            &overlay.primitives,
            &overlay.aux_data,
        );
        // Canvas-emitted dynamic images (noise / texture nodes)
        // dispatched in the canvas band — before the foreground overlay
        // — and scissored to their node body. See the longer note at
        // the first fast-path site.
        if !overlay.dynamic_images.is_empty() {
            self.renderer
                .render_dynamic_images(target_view, &overlay.dynamic_images);
        }
        // Foreground overlay — re-dispatch fg primitives on top of
        // the canvas overlay so host overlays marked
        // `RenderLayer::Foreground` win z-order over canvas content.
        // Global overlay surfaces still dispatch after this pass so
        // popovers and toasts sit above canvas foreground chrome. See
        // `render_foreground_overlay` doc.
        self.render_foreground_overlay(target_view, &[&overlay]);
        self.dispatch_dynamic_batch_overlay(target_view);
        self.dispatch_overlay_canvas_overlay(target_view, &overlay_subtree_canvases);
        // Motion-subtree text overlay — same rationale as the fast
        // paths: motion-bound bg primitives paint on top of the static
        // cache via `composite_frame`'s overlay, so the text inside
        // those subtrees has to render afterwards to land on top of
        // the bg paint. PRIM_TEXT primitives carry the motion affine
        // so the glyphs scale / translate with the bg.
        self.dispatch_motion_subtree_overlay_pools(tree, target_view);
        // Composited CSS layers (slow path's composite site) — same
        // overlay step the fast paths do, so the texture for a
        // freshly-promoted region gets visible content on the very
        // first frame after promotion.
        self.composite_css_layers_overlay(tree, target_view);
        // P4.3 sibling: motion-subtree textures.
        self.composite_motion_layers_overlay(tree, target_view);
        if !overlay.meshes.is_empty() {
            dispatch_pending_meshes(
                &mut self.renderer,
                target_view,
                width,
                height,
                &overlay.meshes,
            );
        }
        if !overlay.gpu_passes.is_empty() {
            dispatch_pending_gpu_passes(
                &mut self.renderer,
                target_view,
                width,
                height,
                tree.scale_factor(),
                &overlay.gpu_passes,
            );
        }

        // Site C bookkeeping (slow-path counterpart). The walker
        // ran (or got bypassed by apply_binding_deltas) — either
        // way, we know the painted set this frame matches what
        // the inner render emitted. Mirror the fast-path
        // visibility-gated set_visible_anim_active so the windowed
        // runner's redraw chain gets a consistent signal whether
        // we landed on the fast or slow path.
        let has_visible_canvas = !tree.canvas_paint_records().is_empty();
        let any_visible_anim = {
            let painted_set = tree.painted_node_ids().clone();
            let painted_stable = tree.painted_stable_ids();
            tree.has_any_active_animation_visible(render_state, &painted_set, &painted_stable)
        };
        tree.set_visible_anim_active(any_visible_anim || has_visible_canvas);

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_tree_with_motion_opt(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
        target_texture: Option<&wgpu::Texture>,
        try_fast_paint: bool,
    ) -> Result<()> {
        // NOTE: glyph atlas growth invalidates baked `PRIM_TEXT`
        // UVs in `cached_bg_batch`. A naive `invalidate_render_cache`
        // here (gated on `text_ctx.take_atlas_grew()`) STOPPED the
        // permanent corruption but caused per-frame blink during
        // continuous-zoom interactions — every smooth-zoom step
        // rasterised a new glyph size which fired growth, which
        // fired invalidation, which dropped the cache, which forced
        // a full re-walk, which (mid-walk) grew the atlas again.
        // The right fix is in the upstream paint path: rewrite
        // `gradient_params` on PRIM_TEXT primitives at emit time
        // against the FINAL atlas dimensions (not whatever interim
        // dimension existed when `prepare_text` was first called).
        // Tracked as a follow-up.
        let _ = self.text_ctx.take_atlas_grew();

        // Layer-compositor mode: a surface texture is available
        // (`target_texture` is `Some`). Route through the cached
        // static-layer path, which paints the non-canvas tree once
        // and overlays only canvas primitives per frame.
        //
        // The inner `render_tree_with_motion_opt` call still drives
        // the walker for the full-paint case — it just renders into
        // the static layer view instead of the surface, with
        // `target_texture = None` so this dispatch branch doesn't
        // recurse.
        if let Some(texture) = target_texture {
            return self.try_render_with_compositor(
                tree,
                render_state,
                width,
                height,
                target,
                texture,
                try_fast_paint,
            );
        }
        // Sub-Phase 4 timing — disabled by default; gated by
        // `RUST_LOG=blinc_app::frame_timing=trace` (same target the
        // outer per-phase instrumentation uses). Lets us see whether
        // the 18 ms Phase 4 cost is the paint walker, the
        // text/SVG/image collector, or the GPU pipeline — three very
        // different optimization targets.
        let p4_start = web_time::Instant::now();

        // Get scale factor for HiDPI rendering
        let scale_factor = tree.scale_factor();

        // Compute & commit animation-status / composite-promotion
        // before the walker runs. The compositor path does this
        // inside `try_render_with_compositor`; the non-compositor
        // path (wasm / Android / iOS) was skipping it entirely, so
        // `composite_promotion` stayed empty and the walker never
        // routed any composite-promotable CSS subtree into the
        // per-node scratch batch. Net effect on those targets:
        // `styling_demo`'s `anim-pulse` / `anim-glow` (and any
        // other composite-promotable CSS animation) emitted
        // primitives straight into the bg batch with their *resting*
        // values, then never animated — the composite_css_layers
        // overlay had nothing to blit.
        let v2_statuses = tree.compute_animation_status();
        tree.commit_animation_status(&v2_statuses);

        // Try the compositor fast path. Two steps in sequence:
        //
        //   1. `redraw_canvases` — re-invokes every recorded
        //      canvas's `render_fn` and splices the fresh primitives
        //      into the cached batch. Bails (returns false) when a
        //      canvas's primitive count has changed since the last
        //      full paint or no cached batch exists.
        //
        //   2. `apply_binding_deltas` — patches motion-binding
        //      transform / opacity changes onto the (possibly
        //      canvas-refreshed) cached batch.
        //
        // Either step bailing falls all the way through to the full
        // walker path and repopulates the cache.
        let used_fast_paint = try_fast_paint
            && self.redraw_canvases(tree, width, height)
            && self.apply_binding_deltas(tree, scale_factor);

        // Create a single paint context for all layers with text rendering support
        let mut ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);

        // Skip the walker on the fast path — the cached batch already
        // contains the post-walker primitives, and `apply_binding_deltas`
        // just shifted them to match this frame's spring values. On the
        // full path, run the walker normally and use whatever it
        // emitted into `ctx`.
        if !used_fast_paint {
            tree.render_with_motion(&mut ctx, render_state);
        }
        let t_paint_walker = p4_start.elapsed();

        // Drain the per-node composite-layer scratch batches the
        // walker peeled out of the bg batch (one per promoted CSS-
        // animated subtree). Rasterize each into its own
        // `LayerTexture` and stash on `css_composited_textures`
        // keyed by slotmap key, so the per-frame composite (Phase 4)
        // can blit each texture with the active animation
        // transform applied — no walker re-entry on animation
        // ticks. Fast path: no walker ran, no scratch batches; the
        // map keeps the previous-paint textures alive.
        if !used_fast_paint {
            let scratch_batches = ctx.take_composite_layer_batches();
            if !scratch_batches.is_empty() {
                // Look up screen_aabb + natural_size from the
                // matching `DynamicRegion`. We index by slotmap key
                // (`LayoutNodeId::data().as_ffi()`) since
                // `composite_layer_batches` keyed on that and
                // blinc_layout doesn't share a slotmap with
                // blinc_app.
                use slotmap::Key as _;
                let regions = tree.dynamic_regions();
                for (region_node, region) in regions.iter() {
                    let key = region_node.data().as_ffi();
                    let Some(scratch_batch) = scratch_batches.get(&key) else {
                        continue;
                    };
                    let natural_size = match region.kind {
                        blinc_layout::renderer::DynamicKind::CssAnimated {
                            natural_size, ..
                        } => natural_size,
                        _ => continue,
                    };
                    if natural_size.0 == 0 || natural_size.1 == 0 {
                        continue;
                    }
                    // Dirty check: if the scratch batch we just walked
                    // hashes the same as the one that produced the
                    // currently-cached texture, the subtree's intrinsic
                    // content didn't change this frame and we can keep
                    // the existing texture. The promoted animation's
                    // transform / opacity is applied at composite time
                    // (`composite_css_layers_overlay`), so steady-state
                    // pulse / glow frames hit this path and skip the
                    // GPU rasterize entirely.
                    //
                    // Re-raster fires automatically whenever the walker
                    // emits different primitives — state-style flip
                    // (hover / active), layout recompute, theme swap,
                    // promotion-property change all flow through the
                    // walker and produce a different scratch. No
                    // separate dirty flag plumbing needed.
                    let scratch_hash = Self::hash_composite_scratch(scratch_batch);
                    if self.css_composited_textures.contains_key(&key)
                        && self.css_composited_scratch_hash.get(&key).copied() == Some(scratch_hash)
                    {
                        continue;
                    }
                    let layer_pos = (region.screen_aabb[0], region.screen_aabb[1]);
                    let layer_size = (natural_size.0 as f32, natural_size.1 as f32);
                    let (layer_texture, content_size) = self
                        .renderer
                        .render_subtree_to_layer_texture(scratch_batch, layer_pos, layer_size);
                    if let Some((old, _)) = self.css_composited_textures.remove(&key) {
                        self.renderer.layer_texture_cache_mut().release(old);
                    }
                    self.css_composited_textures
                        .insert(key, (layer_texture, content_size));
                    self.css_composited_scratch_hash.insert(key, scratch_hash);
                }
            }
            // Demotion cleanup: any cached composited texture whose
            // node is no longer in `dynamic_regions` as a CssAnimated
            // region got demoted this paint (animation settled, went
            // out-of-scope, or the node disappeared). Release its
            // texture back to the pool so the bucket eviction can
            // reclaim it. Without this step the map grows unbounded
            // across re-promotions.
            use slotmap::Key as _;
            let live_keys: std::collections::HashSet<u64> = {
                let regions = tree.dynamic_regions();
                regions
                    .iter()
                    .filter_map(|(node, region)| {
                        if matches!(
                            region.kind,
                            blinc_layout::renderer::DynamicKind::CssAnimated { .. }
                        ) {
                            Some(node.data().as_ffi())
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            let stale_keys: Vec<u64> = self
                .css_composited_textures
                .keys()
                .copied()
                .filter(|k| !live_keys.contains(k))
                .collect();
            for key in stale_keys {
                if let Some((old, _)) = self.css_composited_textures.remove(&key) {
                    self.renderer.layer_texture_cache_mut().release(old);
                }
                self.css_composited_scratch_hash.remove(&key);
            }

            // P4.3 — motion-subtree texture bake. Same draining +
            // dirty-skip + release-on-demote pattern as the CssAnimated
            // block above, but routed by `DynamicKind::MotionSubtreeTexture`
            // and stashed on `motion_subtree_textures`. The walker
            // gated its motion-binding transform pushes on
            // `pushed_for_motion_subtree` (`bake_skip`), so the scratch
            // batch captures the subtree at base state — the per-frame
            // motion overlay applies the current `MotionBindings`
            // (translate / scale / rotation / opacity) at blit time.
            //
            // After a successful raster we flip the bake record to
            // `Baked` so subsequent paints know the texture is ready
            // (consumed by P4.4 invalidation triggers).
            if !scratch_batches.is_empty() {
                let regions = tree.dynamic_regions();
                for (region_node, region) in regions.iter() {
                    let key = region_node.data().as_ffi();
                    let Some(scratch_batch) = scratch_batches.get(&key) else {
                        continue;
                    };
                    let natural_size = match region.kind {
                        blinc_layout::renderer::DynamicKind::MotionSubtreeTexture {
                            natural_size,
                            ..
                        } => natural_size,
                        _ => continue,
                    };
                    if natural_size.0 == 0 || natural_size.1 == 0 {
                        continue;
                    }
                    let scratch_hash = Self::hash_composite_scratch(scratch_batch);
                    if self.motion_subtree_textures.contains_key(&key)
                        && self.motion_subtree_scratch_hash.get(&key).copied() == Some(scratch_hash)
                    {
                        continue;
                    }
                    let layer_pos = (region.screen_aabb[0], region.screen_aabb[1]);
                    let layer_size = (natural_size.0 as f32, natural_size.1 as f32);
                    let (layer_texture, content_size) = self
                        .renderer
                        .render_subtree_to_layer_texture(scratch_batch, layer_pos, layer_size);
                    if let Some((old, _)) = self.motion_subtree_textures.remove(&key) {
                        self.renderer.layer_texture_cache_mut().release(old);
                    }
                    self.motion_subtree_textures
                        .insert(key, (layer_texture, content_size));
                    self.motion_subtree_scratch_hash.insert(key, scratch_hash);
                    tree.mark_motion_subtree_baked(*region_node);
                }
            }
            // Demotion cleanup for the motion-subtree path. Demote
            // lapsed bake records (returns the node ids that fell out
            // of `subtree_texture_candidates`) and release any pooled
            // textures for nodes that are no longer in
            // `dynamic_regions` as a `MotionSubtreeTexture`.
            let _demoted = tree.demote_lapsed_motion_bake_records();
            let live_motion_keys: std::collections::HashSet<u64> = {
                let regions = tree.dynamic_regions();
                regions
                    .iter()
                    .filter_map(|(node, region)| {
                        if matches!(
                            region.kind,
                            blinc_layout::renderer::DynamicKind::MotionSubtreeTexture { .. }
                        ) {
                            Some(node.data().as_ffi())
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            let stale_motion_keys: Vec<u64> = self
                .motion_subtree_textures
                .keys()
                .copied()
                .filter(|k| !live_motion_keys.contains(k))
                .collect();
            for key in stale_motion_keys {
                if let Some((old, _)) = self.motion_subtree_textures.remove(&key) {
                    self.renderer.layer_texture_cache_mut().release(old);
                }
                self.motion_subtree_scratch_hash.remove(&key);
            }
        }

        // Take the batch (mutable so CSS-transformed text primitives can be added).
        // Fast path: clone the patched cache; full path: take from ctx.
        let mut batch = if used_fast_paint {
            self.cached_bg_batch
                .as_ref()
                .cloned()
                .unwrap_or_else(|| ctx.take_batch())
        } else {
            ctx.take_batch()
        };

        // Compositor v2: drain the motion-bound subtree primitives.
        // On the fast path nothing was emitted (walker skipped), so
        // `take_dynamic_batch` returns an empty batch and we keep
        // the previous full paint's cached one — which
        // `apply_binding_deltas` has been patching in place each
        // motion-binding frame. On the full path we replace it with
        // the freshly-emitted batch.
        if !used_fast_paint {
            self.cached_dynamic_batch = Some(ctx.take_dynamic_batch());
        }

        // Take any 3D mesh draws captured via `ctx.draw_mesh_data(...)`
        // inside canvas callbacks. These are dispatched after all 2D
        // content lands so the mesh composites on top of the UI — see
        // the `render_mesh_data` dispatch loop near the end of this
        // function. Drained here (not at the dispatch site) so
        // `ctx` can drop right after `take_batch`/`take_pending_meshes`
        // and the rest of the frame runs without holding onto it.
        let pending_meshes = ctx.take_pending_meshes();
        // Custom GPU passes scheduled via `ctx.run_gpu_pass(...)` inside
        // canvas callbacks. Same lifecycle as `pending_meshes` —
        // dispatched alongside meshes on the legacy (non-overlay) path
        // a few hundred lines down.
        let pending_gpu_passes = ctx.take_pending_gpu_passes();

        // Collect text, SVG, image, and flow elements WITH motion state.
        //
        // Fast path: reuse the cached vecs from the last full paint
        // (saves the 0.8–1.0 ms collect pass on cn_demo). Flow
        // elements aren't cached today — they're cheap to re-collect
        // and re-collecting them gives time-driven flow shaders
        // current `time` / `pointer` uniforms anyway. Cache hit only
        // when all three element vecs were captured on the previous
        // full paint (set together, invalidated together).
        //
        // Note: texts/SVGs inside a motion-bound subtree carry their
        // x/y positions baked in at collect time. If a binding's
        // translate changes between full paints, the cached positions
        // are stale by that delta. For cn_demo's animated_progress
        // (which has no text inside the bound subtree) this isn't
        // user-visible. Properly delta-patching text positions
        // requires per-element binding ownership tracking — left as
        // a follow-up; today the fast path bails any frame that
        // would visibly mis-render via the existing
        // `apply_binding_deltas` checks (which also guard against
        // scale/rotation).
        let collect_start = web_time::Instant::now();
        // Mid-flight motion (either a `MotionBindings` spring or a
        // `motion()` wrapper's enter / exit FSM) shifts the
        // text/SVG/image positions of every element underneath it.
        // The cached text/SVG/image vectors carry positions baked
        // in at last-collect time, so reusing them on the fast
        // path freezes the text at frame-1 positions while
        // `apply_binding_deltas` keeps shifting the underlying SDF
        // primitives — visual effect: text falls off motion-bound
        // cards as they slide / fade in.
        //
        // Cheap fix: invalidate the text/SVG/image cache whenever
        // any motion source is live. Same cost posture as the
        // walker re-collect on a full paint, but cheap relative to
        // a walker re-run.
        //
        // Also catch direct mutations via `set_immediate` — those
        // remove the underlying spring so `is_any_animating()`
        // returns false, but the binding's current value has
        // diverged from the cached primitive snapshot. Comparing
        // current vs `composite_bindings`' `last_translate` /
        // `last_scale` / `last_rotation` / `last_opacity` catches
        // pull-to-refresh-style drag flows that wouldn't otherwise
        // trip `is_any_animating`.
        // Only consider motion sources that can move text / SVG / image
        // positions. Pure timeline-driven rotation (the cn::spinner
        // case) is excluded — its subtree is a single motion-bound
        // SDF primitive with no text/SVG/image children, and surrounding
        // tree elements aren't affected by an in-flight rotation
        // timeline. Keeping the spinner in this predicate pinned 60 %
        // of cn_demo's frame budget on re-collecting text positions
        // for hundreds of unrelated elements 60×/s; the rotation only
        // touches a single primitive in `cached_dynamic_batch` which
        // `apply_binding_deltas` patches in place.
        let any_motion_active = render_state.has_active_motions()
            || tree.has_active_visual_animations()
            || tree.has_active_layout_animations()
            || tree
                .motion_bindings_map()
                .values()
                .any(|b| b.is_any_position_animating())
            || {
                let bindings_table = tree.motion_bindings_map();
                tree.composite_bindings().iter().any(|(node, meta)| {
                    let Some(b) = bindings_table.get(node) else {
                        return false;
                    };
                    let cx = b
                        .translate_x
                        .as_ref()
                        .and_then(|v| v.lock().ok())
                        .map(|g| g.get())
                        .unwrap_or(0.0);
                    let cy = b
                        .translate_y
                        .as_ref()
                        .and_then(|v| v.lock().ok())
                        .map(|g| g.get())
                        .unwrap_or(0.0);
                    if (cx - meta.last_translate.0).abs() > 0.5
                        || (cy - meta.last_translate.1).abs() > 0.5
                    {
                        return true;
                    }
                    if let Some(ref v) = b.opacity {
                        if let Ok(g) = v.lock() {
                            if (g.get() - meta.last_opacity).abs() > 0.01 {
                                return true;
                            }
                        }
                    }
                    false
                })
            };
        let cache_hit = used_fast_paint
            && !any_motion_active
            && self.cached_texts.is_some()
            && self.cached_svgs.is_some()
            && self.cached_images.is_some()
            && self.cached_flows.is_some();
        let (all_texts, all_svgs, all_images, flow_elements) = if cache_hit {
            (
                self.cached_texts.as_ref().unwrap().clone(),
                self.cached_svgs.as_ref().unwrap().clone(),
                self.cached_images.as_ref().unwrap().clone(),
                self.cached_flows.as_ref().unwrap().clone(),
            )
        } else {
            self.collect_render_elements_with_state(tree, Some(render_state))
        };
        let t_collect_elements = collect_start.elapsed();
        // Cache the collected elements alongside the batch on full-path
        // frames so the next fast-path frame can skip collect.
        if !used_fast_paint {
            self.cached_texts = Some(all_texts.clone());
            self.cached_svgs = Some(all_svgs.clone());
            self.cached_images = Some(all_images.clone());
            self.cached_flows = Some(flow_elements.clone());
        }

        // Partition elements into normal (no 3D ancestor) and 3D-layer groups.
        // Elements inside a 3D-transformed parent need to be rendered to an offscreen
        // texture and blitted with the same perspective transform.
        // Motion-subtree text is split out from `texts` so it can be
        // rendered on the surface *after* the overlay pass, on top of
        // the motion-bound bg primitives (which paint over the static
        // cache the regular `texts` path renders into).
        let mut texts = Vec::new();
        let mut fg_texts = Vec::new();
        let mut motion_subtree_texts: Vec<TextElement> = Vec::new();
        let mut layer_3d_texts: std::collections::HashMap<
            LayoutNodeId,
            (Transform3DLayerInfo, Vec<TextElement>),
        > = std::collections::HashMap::new();
        for text in all_texts {
            if let Some(ref info) = text.transform_3d_layer {
                layer_3d_texts
                    .entry(info.node_id)
                    .or_insert_with(|| (info.clone(), Vec::new()))
                    .1
                    .push(text);
            } else if text.composite_ancestor.is_some() {
                // Text under a composite-promoted CSS-animated subtree
                // (e.g. a `.foreground()` select / combobox panel mid
                // enter-fade) is collected at BASE alpha and repaid the
                // live opacity at dispatch via `motion_subtree_text_patch`.
                // That patch is built ONLY while iterating
                // `motion_subtree_texts`, so this must win over
                // `is_foreground` — otherwise the fg pool draws the text
                // at full alpha on top of the still-fading baked panel
                // (the "text before the container" + "double text"
                // symptom). Once the animation settles the falling-edge
                // repaint demotes the subtree, `composite_ancestor`
                // clears, and the text falls back to the fg pool below.
                motion_subtree_texts.push(text);
            } else if text.is_foreground {
                fg_texts.push(text);
            } else if text.in_motion_subtree {
                motion_subtree_texts.push(text);
            } else {
                texts.push(text);
            }
        }

        let mut svgs = Vec::new();
        let mut motion_subtree_svgs: Vec<SvgElement> = Vec::new();
        let mut layer_3d_svgs: std::collections::HashMap<LayoutNodeId, Vec<SvgElement>> =
            std::collections::HashMap::new();
        for svg in all_svgs {
            if let Some(ref info) = svg.transform_3d_layer {
                layer_3d_svgs.entry(info.node_id).or_default().push(svg);
            } else if svg.in_motion_subtree || svg.composite_ancestor.is_some() {
                // Same contract as the text partition above: a promoted
                // `.foreground()` panel's icons must ride the motion pool
                // so `dispatch_motion_subtree_overlay_pools` applies the
                // live composite opacity, instead of painting at full
                // alpha over the fading panel.
                motion_subtree_svgs.push(svg);
            } else {
                svgs.push(svg);
            }
        }
        // Stable-sort by z_layer so icons inside `stack_layer()` subtrees
        // (overlays, toast tray, foreground content) dispatch after lower-z
        // siblings. `render_rasterized_svgs` issues a single batched draw
        // call per slice, so painting order = vec order. Tree order is the
        // tiebreaker — `sort_by_key` is stable so siblings on the same
        // z_layer keep their walker-emitted order.
        svgs.sort_by_key(|s| s.z_layer);
        motion_subtree_svgs.sort_by_key(|s| s.z_layer);

        let mut images = Vec::new();
        let mut layer_3d_images: std::collections::HashMap<LayoutNodeId, Vec<ImageElement>> =
            std::collections::HashMap::new();
        for image in all_images {
            if let Some(ref info) = image.transform_3d_layer {
                layer_3d_images.entry(info.node_id).or_default().push(image);
            } else {
                images.push(image);
            }
        }

        // Collect unique 3D layer IDs for rendering
        let layer_3d_ids: Vec<LayoutNodeId> = layer_3d_texts.keys().cloned().collect();

        // Pre-load all images into cache before rendering (both normal and 3D-layer)
        self.preload_images(&images, width as f32, height as f32);
        for layer_imgs in layer_3d_images.values() {
            self.preload_images(layer_imgs, width as f32, height as f32);
        }

        // Prepare text glyphs with z_layer information
        // Store (z_layer, glyphs) to enable interleaved rendering
        let mut glyphs_by_layer: std::collections::BTreeMap<u32, Vec<GpuGlyph>> =
            std::collections::BTreeMap::new();
        let mut css_transformed_text_prims: Vec<GpuPrimitive> = Vec::new();
        for text in &texts {
            // Skip text that's completely outside its clip bounds (visibility culling)
            // This prevents loading emoji fonts for off-screen text in scroll containers
            if let Some([clip_x, clip_y, clip_w, clip_h]) = text.clip_bounds {
                let text_right = text.x + text.width;
                let text_bottom = text.y + text.height;
                let clip_right = clip_x + clip_w;
                let clip_bottom = clip_y + clip_h;

                // Check if text is completely outside clip bounds
                if text.x >= clip_right
                    || text_right <= clip_x
                    || text.y >= clip_bottom
                    || text_bottom <= clip_y
                {
                    // Text is not visible, skip rendering entirely
                    continue;
                }
            }

            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            // Apply motion opacity to text color
            let color = if text.motion_opacity < 1.0 {
                [
                    text.color[0],
                    text.color[1],
                    text.color[2],
                    text.color[3] * text.motion_opacity,
                ]
            } else {
                text.color
            };

            // Determine wrap width:
            // 1. If clip bounds exist and are smaller than measured width, use clip width
            //    (this handles scroll containers where layout width isn't constrained)
            // 2. Otherwise, if layout width is smaller than measured, use layout width
            // 3. Otherwise, don't wrap (text fits naturally)
            let effective_width = if let Some(clip) = text.clip_bounds {
                // Use clip width if it constrains the text
                clip[2].min(text.width)
            } else {
                text.width
            };

            // Wrap if effective width is significantly smaller than measured width
            let needs_wrap = text.wrap && effective_width < text.measured_width - 2.0;

            // Always pass width for alignment - the layout engine needs max_width
            // to calculate center/right alignment offsets
            let wrap_width = Some(text.width);

            // Convert font family to GPU types
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            // Map vertical alignment to text anchor
            let (anchor, y_pos, use_layout_height) = match text.v_align {
                TextVerticalAlign::Center => {
                    (TextAnchor::Center, text.y + text.height / 2.0, false)
                }
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    let baseline_y = text.y + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };
            let layout_height = if use_layout_height {
                Some(text.height)
            } else {
                None
            };

            // Render text shadow first (behind text) if present
            if let Some(shadow) = &text.text_shadow {
                let shadow_color = [
                    shadow.color.r,
                    shadow.color.g,
                    shadow.color.b,
                    shadow.color.a * text.motion_opacity,
                ];
                let shadow_x = text.x + shadow.offset_x * scale_factor;
                let shadow_y = y_pos + shadow.offset_y * scale_factor;
                if let Ok(mut shadow_glyphs) = self.text_ctx.prepare_text_with_style(
                    &text.content,
                    shadow_x,
                    shadow_y,
                    text.font_size,
                    shadow_color,
                    anchor,
                    alignment,
                    wrap_width,
                    needs_wrap,
                    font_name,
                    generic,
                    font_weight,
                    text.italic,
                    layout_height,
                    text.letter_spacing,
                ) {
                    if let Some(clip) = text.clip_bounds {
                        for glyph in &mut shadow_glyphs {
                            glyph.clip_bounds = clip;
                        }
                    }
                    if let Some(affine) = text.css_affine {
                        let [a, b, c, d, tx, ty] = affine;
                        let tx_scaled = tx * scale_factor;
                        let ty_scaled = ty * scale_factor;
                        for glyph in &shadow_glyphs {
                            let gc_x = glyph.bounds[0] + glyph.bounds[2] / 2.0;
                            let gc_y = glyph.bounds[1] + glyph.bounds[3] / 2.0;
                            let new_gc_x = a * gc_x + c * gc_y + tx_scaled;
                            let new_gc_y = b * gc_x + d * gc_y + ty_scaled;
                            let mut prim = GpuPrimitive::from_glyph(glyph);
                            prim.bounds = [
                                new_gc_x - glyph.bounds[2] / 2.0,
                                new_gc_y - glyph.bounds[3] / 2.0,
                                glyph.bounds[2],
                                glyph.bounds[3],
                            ];
                            prim.local_affine = [a, b, c, d];
                            prim.set_z_layer(text.z_index);
                            css_transformed_text_prims.push(prim);
                        }
                    } else {
                        glyphs_by_layer
                            .entry(text.z_index)
                            .or_default()
                            .extend(shadow_glyphs);
                    }
                }
            }

            match self.text_ctx.prepare_text_with_style(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                color,
                anchor,
                alignment,
                wrap_width,
                needs_wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height,
                text.letter_spacing,
            ) {
                Ok(mut glyphs) => {
                    tracing::trace!(
                        "render_tree_with_motion: prepared {} glyphs for '{}' (font={:?})",
                        glyphs.len(),
                        text.content,
                        font_name
                    );
                    // Apply clip bounds if present
                    if let Some(clip) = text.clip_bounds {
                        for glyph in &mut glyphs {
                            glyph.clip_bounds = clip;
                        }
                    }

                    if let Some(affine) = text.css_affine {
                        // CSS-transformed text: convert glyphs to SDF primitives with local_affine
                        let [a, b, c, d, tx, ty] = affine;
                        let tx_scaled = tx * scale_factor;
                        let ty_scaled = ty * scale_factor;
                        for glyph in &glyphs {
                            // Transform glyph center through the affine
                            let gc_x = glyph.bounds[0] + glyph.bounds[2] / 2.0;
                            let gc_y = glyph.bounds[1] + glyph.bounds[3] / 2.0;
                            let new_gc_x = a * gc_x + c * gc_y + tx_scaled;
                            let new_gc_y = b * gc_x + d * gc_y + ty_scaled;
                            let mut prim = GpuPrimitive::from_glyph(glyph);
                            prim.bounds = [
                                new_gc_x - glyph.bounds[2] / 2.0,
                                new_gc_y - glyph.bounds[3] / 2.0,
                                glyph.bounds[2],
                                glyph.bounds[3],
                            ];
                            prim.local_affine = [a, b, c, d];
                            prim.set_z_layer(text.z_index);
                            css_transformed_text_prims.push(prim);
                        }
                    } else {
                        // Normal text: pixel-snap the glyph origin to the
                        // physical grid before the glyph pipeline. See the
                        // matching snap on the DrawContext path — without it
                        // the Linear atlas sampler softens every glyph at 1x
                        // (standard-DPI); crisp and unchanged at 2x.
                        for glyph in &mut glyphs {
                            glyph.bounds[0] = glyph.bounds[0].round();
                            glyph.bounds[1] = glyph.bounds[1].round();
                        }
                        glyphs_by_layer
                            .entry(text.z_index)
                            .or_default()
                            .extend(glyphs);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "render_tree_with_motion: failed to prepare text '{}': {:?}",
                        text.content,
                        e
                    );
                }
            }
        }

        // Prepare foreground text glyphs (rendered after foreground primitives)
        let mut fg_glyphs: Vec<GpuGlyph> = Vec::new();
        for text in &fg_texts {
            if let Some([clip_x, clip_y, clip_w, clip_h]) = text.clip_bounds {
                let text_right = text.x + text.width;
                let text_bottom = text.y + text.height;
                let clip_right = clip_x + clip_w;
                let clip_bottom = clip_y + clip_h;
                if text.x >= clip_right
                    || text_right <= clip_x
                    || text.y >= clip_bottom
                    || text_bottom <= clip_y
                {
                    continue;
                }
            }

            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            let color = if text.motion_opacity < 1.0 {
                [
                    text.color[0],
                    text.color[1],
                    text.color[2],
                    text.color[3] * text.motion_opacity,
                ]
            } else {
                text.color
            };

            let effective_width = if let Some(clip) = text.clip_bounds {
                clip[2].min(text.width)
            } else {
                text.width
            };
            let needs_wrap = text.wrap && effective_width < text.measured_width - 2.0;
            let wrap_width = Some(text.width);
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            let (anchor, y_pos, use_layout_height) = match text.v_align {
                TextVerticalAlign::Center => {
                    (TextAnchor::Center, text.y + text.height / 2.0, false)
                }
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    let baseline_y = text.y + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };
            let layout_height = if use_layout_height {
                Some(text.height)
            } else {
                None
            };

            if let Ok(mut glyphs) = self.text_ctx.prepare_text_with_style(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                color,
                anchor,
                alignment,
                wrap_width,
                needs_wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height,
                text.letter_spacing,
            ) {
                if let Some(clip) = text.clip_bounds {
                    for glyph in &mut glyphs {
                        glyph.clip_bounds = clip;
                    }
                }
                fg_glyphs.extend(glyphs);
            }
        }

        // Prepare motion-subtree text *primitives*. Shape the glyphs
        // first, then convert each to a `GpuPrimitive` (PRIM_TEXT type)
        // with the motion / css affine baked into `local_affine` — same
        // path the in-cache `css_transformed_text_prims` loop uses, but
        // diverted into a separate batch we dispatch after
        // `composite_frame`'s overlay. Going through the SDF pipeline
        // (instead of the simpler glyph pipeline) means the motion
        // container's scale / translate / rotate is applied per-glyph,
        // so the text follows the bg as the animation progresses.
        let mut motion_subtree_text_prims: Vec<GpuPrimitive> = Vec::new();
        let mut motion_subtree_text_patch: Vec<(
            std::ops::Range<usize>,
            blinc_layout::tree::StableNodeId,
        )> = Vec::new();
        for text in &motion_subtree_texts {
            let patch_start = motion_subtree_text_prims.len();
            if let Some([clip_x, clip_y, clip_w, clip_h]) = text.clip_bounds {
                let text_right = text.x + text.width;
                let text_bottom = text.y + text.height;
                let clip_right = clip_x + clip_w;
                let clip_bottom = clip_y + clip_h;
                if text.x >= clip_right
                    || text_right <= clip_x
                    || text.y >= clip_bottom
                    || text_bottom <= clip_y
                {
                    continue;
                }
            }

            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            let color = if text.motion_opacity < 1.0 {
                [
                    text.color[0],
                    text.color[1],
                    text.color[2],
                    text.color[3] * text.motion_opacity,
                ]
            } else {
                text.color
            };

            let effective_width = if let Some(clip) = text.clip_bounds {
                clip[2].min(text.width)
            } else {
                text.width
            };
            let needs_wrap = text.wrap && effective_width < text.measured_width - 2.0;
            let wrap_width = Some(text.width);
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            let (anchor, y_pos, use_layout_height) = match text.v_align {
                TextVerticalAlign::Center => {
                    (TextAnchor::Center, text.y + text.height / 2.0, false)
                }
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    let baseline_y = text.y + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };
            let layout_height = if use_layout_height {
                Some(text.height)
            } else {
                None
            };

            if let Ok(mut glyphs) = self.text_ctx.prepare_text_with_style(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                color,
                anchor,
                alignment,
                wrap_width,
                needs_wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height,
                text.letter_spacing,
            ) {
                if let Some(clip) = text.clip_bounds {
                    for glyph in &mut glyphs {
                        glyph.clip_bounds = clip;
                    }
                }
                // Convert glyphs to PRIM_TEXT primitives with the
                // motion / css affine baked in. Identity affine when
                // the text has none — the SDF pipeline still works
                // and the glyphs render at base position, matching
                // the behaviour of `render_text` for unaffined text.
                let (a, b, c, d, tx_aff, ty_aff) = match text.css_affine {
                    Some([a, b, c, d, tx, ty]) => (a, b, c, d, tx, ty),
                    None => (1.0, 0.0, 0.0, 1.0, 0.0, 0.0),
                };
                let tx_scaled = tx_aff * scale_factor;
                let ty_scaled = ty_aff * scale_factor;
                for glyph in &glyphs {
                    let gc_x = glyph.bounds[0] + glyph.bounds[2] / 2.0;
                    let gc_y = glyph.bounds[1] + glyph.bounds[3] / 2.0;
                    let new_gc_x = a * gc_x + c * gc_y + tx_scaled;
                    let new_gc_y = b * gc_x + d * gc_y + ty_scaled;
                    let mut prim = GpuPrimitive::from_glyph(glyph);
                    prim.bounds = [
                        new_gc_x - glyph.bounds[2] / 2.0,
                        new_gc_y - glyph.bounds[3] / 2.0,
                        glyph.bounds[2],
                        glyph.bounds[3],
                    ];
                    prim.local_affine = [a, b, c, d];
                    prim.set_z_layer(text.z_index);
                    motion_subtree_text_prims.push(prim);
                }
            }
            // Record the emitted range for the dispatch-time live
            // patch when this text sits under a composite-promoted
            // ancestor (its alpha was baked at BASE, factor excluded).
            let patch_end = motion_subtree_text_prims.len();
            if patch_end > patch_start {
                if let Some(ancestor) = text.composite_ancestor {
                    motion_subtree_text_patch.push((patch_start..patch_end, ancestor));
                }
            }
        }

        // Cache the prepared glyph + CSS-transformed-prim vecs for
        // the compositor v2 damage-rect path. When motion bindings
        // patch the cached batch on a subsequent frame, the damage-
        // rect re-render needs to scissor-redraw any text glyphs
        // that fall inside the cleared region — without these
        // caches we'd have to re-run the entire text-shaping
        // pipeline on the fast path.
        //
        // Cloned because the dispatch loop below consumes the
        // originals; on cn_demo's typical text density (~30-100
        // glyphs total) the clone is well under 100 µs and pays
        // off the first time a damage-rect frame would otherwise
        // wipe text.
        self.cached_glyphs_by_layer = Some(glyphs_by_layer.clone());
        self.cached_fg_glyphs = Some(fg_glyphs.clone());
        self.cached_motion_subtree_text_prims = Some(motion_subtree_text_prims.clone());
        self.motion_subtree_text_patch = motion_subtree_text_patch;
        self.cached_motion_subtree_svgs = Some(motion_subtree_svgs.clone());
        self.cached_css_transformed_text_prims = Some(css_transformed_text_prims.clone());

        // Generate decoration primitives for foreground text once so the
        // three render paths below can each render them after their
        // `render_text(target, &fg_glyphs)` call. Without this, any
        // strikethrough / underline on a `.foreground()` element is
        // silently dropped.
        let fg_decorations_by_layer = generate_text_decoration_primitives_by_layer(&fg_texts);

        tracing::trace!(
            "render_tree_with_motion: {} texts, {} fg texts, {} z-layers with glyphs, {} css-transformed",
            texts.len(),
            fg_texts.len(),
            glyphs_by_layer.len(),
            css_transformed_text_prims.len()
        );

        // SVGs are rendered as rasterized images (not tessellated paths) for better anti-aliasing
        // They will be rendered later via render_rasterized_svgs

        self.renderer.resize(width, height);

        // Bind the real glyph atlas to the SDF pipeline whenever
        // it's available — `set_glyph_atlas` no-ops on pointer
        // equality so calling each frame is free. CSS-transformed
        // text + canvas `draw_text` calls both land glyph-sourced
        // primitives in `batch.primitives`, and the SDF pipeline
        // needs the real atlas bound to sample them; the old
        // "only when CSS text exists" guard silently swallowed
        // canvas text that reached this path any other way.
        if let (Some(atlas), Some(color_atlas)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            if !css_transformed_text_prims.is_empty() {
                batch.primitives.append(&mut css_transformed_text_prims);
            }
            self.renderer.set_glyph_atlas(atlas, color_atlas);
        }

        // Snapshot the post-walker, post-CSS-text batch for the
        // compositor fast path (Phase 4 follow-up frame). At this
        // point `batch.primitives` matches what the walker emitted
        // and what the bindings in `RenderTree::composite_bindings`
        // reference by index. Subsequent rendering only reads from
        // `batch` (the `render_with_clear` calls take `&`), so a
        // clone here is safe and the original keeps flowing through
        // the rest of the function.
        //
        // Cost: one `Vec::clone()` of `GpuPrimitive` (each ~288 B).
        // For cn_demo with ~400 primitives that's ~110 KB / frame
        // memcpy — ~30 µs on M-series silicon — in exchange for
        // skipping the full paint walker on follow-up frames.
        tracing::trace!(
            target: "blinc_app::frame_timing",
            primitives = batch.primitives.len(),
            "cached_bg_batch_set",
        );
        self.cached_bg_batch = Some(batch.clone());

        let has_glass = batch.glass_count() > 0;
        let has_layer_effects_in_batch = batch.has_layer_effects();

        // Only allocate glass textures when glass is actually used
        if has_glass {
            self.ensure_glass_textures(width, height);
        }
        let use_msaa_overlay = self.sample_count > 1;

        if has_glass {
            // Glass path with layer effects support
            let (bg_images, fg_images): (Vec<_>, Vec<_>) = images
                .iter()
                .partition(|img| img.layer == RenderLayer::Background);

            // Pre-render background images to both backdrop and target so glass can blur them
            let has_bg_images = !bg_images.is_empty();
            if has_bg_images {
                let backdrop_tex = self.backdrop_texture.take().unwrap();
                self.renderer
                    .clear_target(&backdrop_tex.view, wgpu::Color::TRANSPARENT);
                self.renderer.clear_target(
                    target,
                    wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: self.clear_alpha as f64,
                    },
                );
                self.render_images_ref(&backdrop_tex.view, &bg_images);
                self.render_images_ref(target, &bg_images);
                self.backdrop_texture = Some(backdrop_tex);
            }

            if has_layer_effects_in_batch {
                // When we have layer effects, we need a more complex render path:
                // 1. Render backdrop for glass blur sampling (with pre-rendered images if any)
                // 2. Use render_with_clear which handles layer effects
                // 3. Render background images to target (after clear, before glass)
                // 4. Render glass primitives on top
                {
                    let backdrop = self.backdrop_texture.as_ref().unwrap();
                    self.renderer.render_to_backdrop(
                        &backdrop.view,
                        (backdrop.width, backdrop.height),
                        &batch,
                        has_bg_images,
                    );
                }

                // Then use render_with_clear which handles layer effects
                self.renderer.render_with_clear(
                    target,
                    &batch,
                    [0.0, 0.0, 0.0, self.clear_alpha as f64],
                );

                // Render dynamic images (video frames from draw_rgba_pixels)
                if !batch.dynamic_images.is_empty() {
                    self.renderer
                        .render_dynamic_images(target, &batch.dynamic_images);
                }

                // Render background images to target after clear (so they're visible behind glass)
                if has_bg_images {
                    self.render_images_ref(target, &bg_images);
                }

                // Finally render glass primitives on top
                if batch.glass_count() > 0 {
                    let backdrop = self.backdrop_texture.as_ref().unwrap();
                    self.renderer.render_glass(target, &backdrop.view, &batch);
                }
            } else {
                // No layer effects, use optimized glass frame rendering
                let backdrop = self.backdrop_texture.as_ref().unwrap();
                self.renderer.render_glass_frame(
                    target,
                    &backdrop.view,
                    (backdrop.width, backdrop.height),
                    &batch,
                    has_bg_images,
                );
            }

            // Render paths. MSAA when sample_count > 1; non-MSAA
            // dispatch when not so 1×-sample targets (web) still emit
            // fill_path / stroke_path content.
            if batch.has_paths() {
                if use_msaa_overlay {
                    self.renderer
                        .render_paths_overlay_msaa(target, &batch, self.sample_count);
                } else {
                    self.renderer.render_paths_overlay(target, &batch);
                }
            }

            // Render remaining bg images (only if not already pre-rendered for glass)
            if !has_bg_images {
                self.render_images_ref(target, &bg_images);
            }
            self.render_images_ref(target, &fg_images);

            // Interleaved z-layer rendering for proper text z-ordering in glass path
            let max_z = batch.max_z_layer();
            let max_text_z = glyphs_by_layer.keys().cloned().max().unwrap_or(0);
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            let max_decoration_z = decorations_by_layer.keys().cloned().max().unwrap_or(0);
            let max_glass_layer = max_z.max(max_text_z).max(max_decoration_z);

            // Render z=0 text first (before any z>0 primitives)
            {
                let mut scratch = std::mem::take(&mut self.scratch_glyphs);
                scratch.clear();
                if let Some(glyphs) = glyphs_by_layer.get(&0) {
                    scratch.extend_from_slice(glyphs);
                }
                if !scratch.is_empty() {
                    self.render_text(target, &scratch);
                }
                self.scratch_glyphs = scratch;
            }
            self.render_text_decorations_for_layer(target, &decorations_by_layer, 0);

            if max_glass_layer > 0 {
                let effect_indices = batch.effect_layer_indices();
                for z in 1..=max_glass_layer {
                    // Render primitives for this layer
                    let layer_primitives = if effect_indices.is_empty() {
                        batch.primitives_for_layer(z)
                    } else {
                        batch.primitives_for_layer_excluding_effects(z, &effect_indices)
                    };
                    if !layer_primitives.is_empty() {
                        self.renderer
                            .render_primitives_overlay(target, &layer_primitives);
                    }

                    // Render text for this layer (interleaved for proper z-order)
                    {
                        let mut scratch = std::mem::take(&mut self.scratch_glyphs);
                        scratch.clear();
                        if let Some(glyphs) = glyphs_by_layer.get(&z) {
                            scratch.extend_from_slice(glyphs);
                        }
                        if !scratch.is_empty() {
                            self.render_text(target, &scratch);
                        }
                        self.scratch_glyphs = scratch;
                    }
                    self.render_text_decorations_for_layer(target, &decorations_by_layer, z);
                }
            }

            // Render SVGs as rasterized images for high-quality anti-aliasing
            if !svgs.is_empty() {
                self.render_rasterized_svgs(target, &svgs, scale_factor);
            }

            // Render foreground text (inside foreground-layer elements, after everything else)
            if !fg_glyphs.is_empty() {
                self.render_text(target, &fg_glyphs);
            }
            // Render foreground text decorations (strikethrough / underline)
            // for every z-layer present in the foreground decoration index.
            for &z in fg_decorations_by_layer.keys() {
                self.render_text_decorations_for_layer(target, &fg_decorations_by_layer, z);
            }
        } else {
            // Simple path (no glass)
            // Pre-generate text decorations grouped by layer for interleaved rendering
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);

            let max_z = batch.max_z_layer();
            let max_text_z = glyphs_by_layer.keys().cloned().max().unwrap_or(0);
            let max_decoration_z = decorations_by_layer.keys().cloned().max().unwrap_or(0);
            let max_layer = max_z.max(max_text_z).max(max_decoration_z);
            let has_layer_effects = batch.has_layer_effects();

            if max_layer > 0 && !has_layer_effects {
                // Interleaved z-layer rendering for proper Stack z-ordering
                // Group images by z_index for interleaved rendering
                let mut images_by_layer: std::collections::BTreeMap<u32, Vec<&ImageElement>> =
                    std::collections::BTreeMap::new();
                for img in &images {
                    images_by_layer.entry(img.z_index).or_default().push(img);
                }
                let max_image_z = images_by_layer.keys().cloned().max().unwrap_or(0);
                let max_layer = max_layer.max(max_image_z);

                // First pass: render z_layer=0 primitives with clear
                let z0_primitives = batch.primitives_for_layer(0);
                // Create a temporary batch for z=0 (include paths - they don't have z-layer support)
                let mut z0_batch = PrimitiveBatch::new();
                z0_batch.primitives = z0_primitives;
                z0_batch.paths = batch.paths.clone();
                // Clone the aux_data too. MESH primitives reference
                // their triangle vertices via `border.z = aux_offset`
                // into this buffer; the subsequent
                // `render_primitives_overlay` calls for z>0 layers
                // don't re-upload aux_data, so the z=0 pass's upload
                // is the only chance for the GPU to see fresh mesh
                // triangles. Without this clone, the buffer uploads
                // empty (`PrimitiveBatch::new()` has `aux_data =
                // vec![]`) and every MESH primitive (SVG icons,
                // tessellated paths with solid brush) reads garbage
                // at its offset — visible on web (any 1×-sample
                // target with `stack_layer()` in the tree) as icons
                // stuck at content-space origin (0, 0) while node
                // bodies (PRIM_RECT, using `bounds` directly) track
                // viewport changes correctly.
                z0_batch.aux_data = batch.aux_data.clone();
                self.renderer.render_with_clear(
                    target,
                    &z0_batch,
                    [0.0, 0.0, 0.0, self.clear_alpha as f64],
                );

                // Render dynamic images (video frames)
                if !batch.dynamic_images.is_empty() {
                    self.renderer
                        .render_dynamic_images(target, &batch.dynamic_images);
                }

                // Render paths. MSAA/non-MSAA split so 1×-sample
                // targets still dispatch fill_path / stroke_path.
                if z0_batch.has_paths() {
                    if use_msaa_overlay {
                        self.renderer.render_paths_overlay_msaa(
                            target,
                            &z0_batch,
                            self.sample_count,
                        );
                    } else {
                        self.renderer.render_paths_overlay(target, &z0_batch);
                    }
                }

                // Render z=0 images
                if let Some(z0_images) = images_by_layer.get(&0) {
                    self.render_images_ref(target, z0_images);
                }

                // Render z=0 text (must render before z=1 primitives for proper z-ordering)
                if let Some(glyphs) = glyphs_by_layer.get(&0) {
                    if !glyphs.is_empty() {
                        self.render_text(target, glyphs);
                    }
                }
                self.render_text_decorations_for_layer(target, &decorations_by_layer, 0);

                // Render subsequent layers interleaved (primitives, images, text per layer)
                for z in 1..=max_layer {
                    // Render primitives for this layer. MSAA route for
                    // silhouette smoothing on stacked vector content
                    // (scroll overlays, stacked components, etc.). The
                    // z=0 batch still flows through `render_with_clear`
                    // below — wiring that one up takes a different
                    // helper because it fuses clearing with drawing.
                    let layer_primitives = batch.primitives_for_layer(z);
                    if !layer_primitives.is_empty() {
                        if use_msaa_overlay {
                            self.renderer.render_primitives_overlay_msaa(
                                target,
                                &layer_primitives,
                                self.sample_count,
                            );
                        } else {
                            self.renderer
                                .render_primitives_overlay(target, &layer_primitives);
                        }
                    }

                    // Render images for this layer
                    if let Some(layer_images) = images_by_layer.get(&z) {
                        self.render_images_ref(target, layer_images);
                    }

                    // Render text for this layer (interleaved with primitives for proper z-order)
                    if let Some(glyphs) = glyphs_by_layer.get(&z) {
                        if !glyphs.is_empty() {
                            self.render_text(target, glyphs);
                        }
                    }
                    self.render_text_decorations_for_layer(target, &decorations_by_layer, z);
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }

                // Render foreground primitives (e.g. borders on top)
                if !batch.foreground_primitives.is_empty() {
                    self.renderer
                        .render_primitives_overlay(target, &batch.foreground_primitives);
                }

                // Render foreground text (inside foreground-layer elements, after foreground primitives)
                if !fg_glyphs.is_empty() {
                    self.render_text(target, &fg_glyphs);
                }
                for &z in fg_decorations_by_layer.keys() {
                    self.render_text_decorations_for_layer(target, &fg_decorations_by_layer, z);
                }
            } else {
                // Fast path: render the full batch through
                // `render_with_clear`, which dispatches into
                // `render_with_layer_effects` when the batch carries
                // any `LayerCommand::Push { effects: !empty }` and
                // falls through to the simple SDF path otherwise.
                //
                // This branch used to flip between
                // `render_overlay_msaa` (when no layer effects) and
                // `render_with_clear` (when effects were present),
                // which gave mesh primitives hardware-coverage AA on
                // the no-effect frames. The flip ran per frame, so an
                // animated effect — e.g. ruffle's `breathe-blur`
                // sweeping radius 0 → 32 → 0 — toggled the rendering
                // path twice per cycle. Mesh primitives' silhouette AA
                // shifts subtly between hardware-MSAA and the SDF
                // shader's barycentric ramp, and the toggle reads as
                // a flash on the affected node. Pinning the path to
                // `render_with_clear` keeps the visual stable, and
                // the path overlay below still applies hardware MSAA
                // to actual `Path` geometry. Mesh primitives lose the
                // hardware-coverage refinement on no-effect frames,
                // but the shader-side edge AA they fall back to is
                // already 1 px wide and is the same path that runs
                // when effects DO exist — no per-frame discontinuity.
                self.renderer.render_with_clear(
                    target,
                    &batch,
                    [0.0, 0.0, 0.0, self.clear_alpha as f64],
                );

                // Render dynamic images (video frames)
                if !batch.dynamic_images.is_empty() {
                    self.renderer
                        .render_dynamic_images(target, &batch.dynamic_images);
                }

                // Path overlay. MSAA when configured; non-MSAA
                // dispatch otherwise so 1×-sample targets (web) still
                // emit fill_path / stroke_path. The main pass above is
                // always single-sampled so paths always need their own
                // dispatch here.
                if batch.has_paths() {
                    if self.sample_count > 1 {
                        self.renderer
                            .render_paths_overlay_msaa(target, &batch, self.sample_count);
                    } else {
                        self.renderer.render_paths_overlay(target, &batch);
                    }
                }

                self.render_images(target, &images, width as f32, height as f32, scale_factor);

                // Render foreground primitives (e.g. borders on top)
                if !batch.foreground_primitives.is_empty() {
                    self.renderer
                        .render_primitives_overlay(target, &batch.foreground_primitives);
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }

                // Interleaved z-layer rendering for proper text z-ordering
                // Render z=0 text before any z>0 primitive overlays
                if let Some(glyphs) = glyphs_by_layer.get(&0) {
                    if !glyphs.is_empty() {
                        self.render_text(target, glyphs);
                    }
                }
                self.render_text_decorations_for_layer(target, &decorations_by_layer, 0);

                if max_layer > 0 {
                    let effect_indices = batch.effect_layer_indices();
                    for z in 1..=max_layer {
                        // Render primitives for this z-layer
                        let layer_primitives = if effect_indices.is_empty() {
                            batch.primitives_for_layer(z)
                        } else {
                            batch.primitives_for_layer_excluding_effects(z, &effect_indices)
                        };
                        if !layer_primitives.is_empty() {
                            self.renderer
                                .render_primitives_overlay(target, &layer_primitives);
                        }

                        // Render text for this z-layer (interleaved for proper z-order)
                        if let Some(glyphs) = glyphs_by_layer.get(&z) {
                            if !glyphs.is_empty() {
                                self.render_text(target, glyphs);
                            }
                        }
                        self.render_text_decorations_for_layer(target, &decorations_by_layer, z);
                    }
                }

                // Render foreground text (inside foreground-layer elements, after all z-layers)
                if !fg_glyphs.is_empty() {
                    self.render_text(target, &fg_glyphs);
                }
                for &z in fg_decorations_by_layer.keys() {
                    self.render_text_decorations_for_layer(target, &fg_decorations_by_layer, z);
                }
            }
        }

        // Render 3D-layer text/SVGs/images: for each 3D layer group, render to an
        // offscreen texture and blit with the same perspective transform as the parent.
        for layer_id in &layer_3d_ids {
            if let Some((info, layer_texts)) = layer_3d_texts.get(layer_id) {
                let layer_svgs_vec = layer_3d_svgs.get(layer_id);
                let layer_images_vec = layer_3d_images.get(layer_id);
                self.render_3d_layer_elements(
                    target,
                    info,
                    layer_texts,
                    layer_svgs_vec.map(|v| v.as_slice()).unwrap_or(&[]),
                    layer_images_vec.map(|v| v.as_slice()).unwrap_or(&[]),
                    scale_factor,
                );
            }
        }

        // Render @flow shader elements on top of their SDF base
        self.has_active_flows = !flow_elements.is_empty();
        if !flow_elements.is_empty() {
            let stylesheet = tree.stylesheet();

            // Use monotonic time for smooth animation
            static START_TIME: std::sync::OnceLock<web_time::Instant> = std::sync::OnceLock::new();
            let start = START_TIME.get_or_init(web_time::Instant::now);
            let elapsed_secs = start.elapsed().as_secs_f32();

            for flow_el in &flow_elements {
                // Resolve FlowGraph: direct graph first, then stylesheet lookup
                let graph = flow_el
                    .flow_graph
                    .as_deref()
                    .or_else(|| stylesheet.and_then(|s| s.get_flow(&flow_el.flow_name)));

                if let Some(graph) = graph {
                    // Compile on first use (no-op if already cached)
                    if let Err(e) = self.renderer.flow_pipeline_cache().compile(graph) {
                        tracing::warn!("@flow '{}' compile error: {}", flow_el.flow_name, e);
                        continue;
                    }

                    let uniforms = blinc_gpu::FlowUniformData {
                        viewport_size: [width as f32, height as f32],
                        time: elapsed_secs,
                        frame_index: 0.0, // TODO: track frame counter
                        element_bounds: [flow_el.x, flow_el.y, flow_el.width, flow_el.height],
                        pointer: [
                            (self.cursor_pos[0] - flow_el.x) / flow_el.width.max(1.0),
                            (self.cursor_pos[1] - flow_el.y) / flow_el.height.max(1.0),
                        ],
                        corner_radius: flow_el.corner_radius,
                        _padding: 0.0,
                    };

                    let viewport = [flow_el.x, flow_el.y, flow_el.width, flow_el.height];
                    if !self.renderer.render_flow(
                        target,
                        &flow_el.flow_name,
                        &uniforms,
                        Some(viewport),
                    ) {
                        tracing::warn!("@flow '{}' render failed", flow_el.flow_name);
                    }
                }
            }
        }

        // Poll the device to free completed command buffers
        self.renderer.poll();

        // Motion-bound bg primitives (the dynamic batch) — slider
        // thumbs, progress fills, spinner arcs, anything inside a
        // `motion()` container. The walker routes these into a
        // separate `cached_dynamic_batch` so the compositor's
        // motion-binding fast path can patch transform / opacity
        // in place without re-walking.
        //
        // Gate this dispatch on `!in_compositor_inner_call`: when
        // we're called by `try_render_with_compositor` as its inner
        // static-layer paint step, `composite_frame` will dispatch
        // `cached_dynamic_batch` onto the surface separately —
        // dispatching it here too lands the motion primitives in
        // the static cache at the slow-paint rotation, then
        // `composite_frame` paints them again at the current
        // rotation, producing visibly duplicated motion content
        // (cn::spinner double-arc). The non-compositor path
        // (wasm / iOS / fuchsia) still needs this dispatch since it
        // never reaches `composite_frame`.
        //
        // `render_overlay` re-uploads the dynamic batch's aux_data
        // (polygon clip vertices, etc.) before dispatch.
        if !self.in_compositor_inner_call {
            if let Some(ref dynamic_batch) = self.cached_dynamic_batch {
                if !dynamic_batch.primitives.is_empty() {
                    self.renderer.render_overlay(target, dynamic_batch);
                    // The dynamic batch upload clobbered the static
                    // aux_data the next text/SVG dispatch might rely
                    // on (polygon clip-paths on regular elements).
                    // Re-upload the main batch's aux_data so anything
                    // downstream sees consistent state. (The compositor
                    // path doesn't need this because composite_frame
                    // handles aux_data sequencing internally.)
                    self.renderer.update_aux_data_for_batch(&batch);
                }
            }
        }

        // Motion-subtree text + SVG overlay — text/SVG inside a
        // motion container (or with an inherited CSS affine) were
        // filtered out of the main `texts` / `svgs` lists earlier
        // and cached on `cached_motion_subtree_*`. The compositor
        // dispatches them after `composite_frame`; we mirror that
        // dispatch here for the non-compositor path so wasm /
        // Android / iOS show motion-container text + SVG. Without
        // this, motion_demo's inner labels, cn::tabs content,
        // context-menu dropdowns, and any motion-animated SVG icon
        // render invisible.
        self.dispatch_motion_subtree_overlay_pools(tree, target);

        // Composited CSS layers — blit each promoted subtree's
        // texture with the current animation transform. Same as
        // the compositor flow's post-`composite_frame` call.
        // Skips silently when nothing is promoted. Needed for
        // styling_demo's `anim-pulse` / `anim-glow` (and any other
        // composite-promotable CSS animation) on wasm / mobile.
        //
        // Gate on `!in_compositor_inner_call` for the same reason
        // the dynamic-batch dispatch below is gated: when we're the
        // inner static-layer paint of `try_render_with_compositor`,
        // the caller runs its OWN overlay call after `composite_frame`
        // — running it here too composites the overlay into the
        // static cache (where it stays baked across subsequent
        // frames), then the outer call composites it AGAIN onto the
        // surface, producing a visible double-paint. The non-
        // compositor path (wasm / iOS / fuchsia) still needs both
        // calls since it never reaches `composite_frame`.
        if !self.in_compositor_inner_call {
            self.composite_css_layers_overlay(tree, target);
            // P4.3 sibling: motion-subtree textures.
            self.composite_motion_layers_overlay(tree, target);
        }

        // Dispatch 3D mesh draws captured during `tree.render_with_motion`.
        // Each `PendingMesh` carries a snapshot of the camera and lights
        // active when `canvas(|ctx, bounds| ctx.draw_mesh_data(...))` fired,
        // so the mesh pipeline renders at the correct pose even if the
        // closure's camera was transient. View-projection is computed
        // from the captured camera + the actual frame viewport so aspect
        // stays correct under window resizes.
        //
        // MVP scope: meshes render to the full frame target (no scissor
        // to the canvas bounds yet), composite on top of the 2D UI, and
        // sit under `render_overlays` so overlay panels still clip
        // cleanly over them. Per-canvas viewport clipping is a
        // follow-up once the first end-to-end demo proves the path.
        if !pending_meshes.is_empty() {
            dispatch_pending_meshes(&mut self.renderer, target, width, height, &pending_meshes);
        }
        if !pending_gpu_passes.is_empty() {
            dispatch_pending_gpu_passes(
                &mut self.renderer,
                target,
                width,
                height,
                scale_factor,
                &pending_gpu_passes,
            );
        }

        // Render overlays from RenderState
        self.render_overlays(render_state, width, height, target);

        // Render debug visualization if enabled (BLINC_DEBUG=text|layout|all)
        let debug = DebugMode::from_env();
        if debug.text {
            self.render_text_debug(target, &texts);
        }
        if debug.layout {
            let scale = tree.scale_factor();
            self.render_layout_debug(target, tree, scale);
        }
        if debug.motion {
            self.render_motion_debug(target, tree, width, height);
        }

        // Return scratch buffers for reuse on next frame
        self.return_scratch_elements(texts, svgs, images);

        // Periodic cache stats (every ~5s at 60fps)
        self.log_cache_stats();

        let t_total_p4 = p4_start.elapsed();
        let t_gpu = t_total_p4
            .saturating_sub(t_paint_walker)
            .saturating_sub(t_collect_elements);
        tracing::trace!(
            target: "blinc_app::frame_timing",
            paint_walker_us = t_paint_walker.as_micros() as u64,
            collect_elements_us = t_collect_elements.as_micros() as u64,
            gpu_us = t_gpu.as_micros() as u64,
            "p4_breakdown"
        );

        Ok(())
    }

    /// Render 3D-layer text/SVGs/images to an offscreen texture and blit with perspective.
    ///
    /// Elements inside a parent with `perspective` + `rotate-x`/`rotate-y` need to be
    /// rendered to a temporary offscreen texture and then blitted with the same perspective
    /// transform so they visually tilt with their parent's 3D transform.
    fn render_3d_layer_elements(
        &mut self,
        target: &wgpu::TextureView,
        info: &Transform3DLayerInfo,
        texts: &[TextElement],
        svgs: &[SvgElement],
        images: &[ImageElement],
        scale_factor: f32,
    ) {
        let [lx, ly, lw, lh] = info.layer_bounds;
        if lw <= 0.0 || lh <= 0.0 {
            return;
        }

        let tex_w = (lw.ceil() as u32).max(1);
        let tex_h = (lh.ceil() as u32).max(1);

        // Acquire offscreen texture
        let layer_tex = self.renderer.acquire_layer_texture((tex_w, tex_h), false);
        self.renderer
            .clear_target(&layer_tex.view, wgpu::Color::TRANSPARENT);

        // Set viewport to offscreen texture size
        self.renderer.set_viewport_override((tex_w, tex_h));

        // Render offset text glyphs
        if !texts.is_empty() {
            let mut layer_glyphs: Vec<GpuGlyph> = Vec::new();
            for text in texts {
                let alignment = match text.align {
                    TextAlign::Left => TextAlignment::Left,
                    TextAlign::Center => TextAlignment::Center,
                    TextAlign::Right => TextAlignment::Right,
                };

                let color = if text.motion_opacity < 1.0 {
                    [
                        text.color[0],
                        text.color[1],
                        text.color[2],
                        text.color[3] * text.motion_opacity,
                    ]
                } else {
                    text.color
                };

                let effective_width = if let Some(clip) = text.clip_bounds {
                    clip[2].min(text.width)
                } else {
                    text.width
                };
                let needs_wrap = text.wrap && effective_width < text.measured_width - 2.0;
                let wrap_width = Some(text.width);
                let font_name = text.font_family.name.as_deref();
                let generic = to_gpu_generic_font(text.font_family.generic);
                let font_weight = text.weight.weight();

                let (anchor, y_pos, use_layout_height) = match text.v_align {
                    TextVerticalAlign::Center => {
                        (TextAnchor::Center, text.y + text.height / 2.0, false)
                    }
                    TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                    TextVerticalAlign::Baseline => {
                        let baseline_y = text.y + text.ascender;
                        (TextAnchor::Baseline, baseline_y, false)
                    }
                };
                let layout_height = if use_layout_height {
                    Some(text.height)
                } else {
                    None
                };

                if let Ok(mut glyphs) = self.text_ctx.prepare_text_with_style(
                    &text.content,
                    text.x - lx,
                    y_pos - ly,
                    text.font_size,
                    color,
                    anchor,
                    alignment,
                    wrap_width,
                    needs_wrap,
                    font_name,
                    generic,
                    font_weight,
                    text.italic,
                    layout_height,
                    text.letter_spacing,
                ) {
                    // Offset clip bounds to layer-local coords
                    if let Some(clip) = text.clip_bounds {
                        for glyph in &mut glyphs {
                            glyph.clip_bounds = [clip[0] - lx, clip[1] - ly, clip[2], clip[3]];
                        }
                    }
                    layer_glyphs.extend(glyphs);
                }
            }

            if !layer_glyphs.is_empty() {
                self.render_text(&layer_tex.view, &layer_glyphs);
            }
        }

        // Render offset images (mutate in place — we own these from partition)
        if !images.is_empty() {
            let mut offset_images = images.to_vec();
            for img in &mut offset_images {
                img.x -= lx;
                img.y -= ly;
                if let Some(ref mut clip) = img.clip_bounds {
                    clip[0] -= lx;
                    clip[1] -= ly;
                }
                if let Some(ref mut scroll) = img.scroll_clip {
                    scroll[0] -= lx;
                    scroll[1] -= ly;
                }
            }
            self.render_images(&layer_tex.view, &offset_images, lw, lh, scale_factor);
        }

        // Render offset SVGs (mutate in place — we own these from partition)
        if !svgs.is_empty() {
            let mut offset_svgs = svgs.to_vec();
            for svg in &mut offset_svgs {
                svg.x -= lx;
                svg.y -= ly;
                if let Some(ref mut clip) = svg.clip_bounds {
                    clip[0] -= lx;
                    clip[1] -= ly;
                }
            }
            self.render_rasterized_svgs(&layer_tex.view, &offset_svgs, scale_factor);
        }

        // Restore viewport
        self.renderer.restore_viewport();

        // Blit with perspective transform
        self.renderer.blit_tight_texture_to_target(
            &layer_tex.view,
            (tex_w, tex_h),
            target,
            (lx, ly),
            (lw, lh),
            info.opacity,
            blinc_core::BlendMode::Normal,
            None,
            Some(info.transform_3d),
            0.0,
        );

        self.renderer.release_layer_texture(layer_tex);
    }

    /// Render a tree on top of existing content (no clear)
    ///
    /// This is used for overlay trees (modals, toasts, dialogs) that render
    /// on top of the main UI without clearing it.
    pub fn render_overlay_tree_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        // Get scale factor for HiDPI rendering
        let scale_factor = tree.scale_factor();

        // Create a single paint context for all layers with text rendering support
        let mut ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);

        // Render with motion animations applied (all layers to same context)
        tree.render_with_motion(&mut ctx, render_state);

        // Take the batch (mutable so CSS-transformed text primitives can be added)
        let mut batch = ctx.take_batch();

        // Collect text, SVG, image, and flow elements WITH motion state
        let (texts, svgs, images, _flows) =
            self.collect_render_elements_with_state(tree, Some(render_state));

        // Pre-load all images into cache before rendering
        self.preload_images(&images, width as f32, height as f32);

        // Prepare text glyphs with z_layer information
        let mut glyphs_by_layer: std::collections::BTreeMap<u32, Vec<GpuGlyph>> =
            std::collections::BTreeMap::new();
        let mut css_transformed_text_prims: Vec<GpuPrimitive> = Vec::new();
        for text in &texts {
            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            // Apply motion opacity to text color
            let color = if text.motion_opacity < 1.0 {
                [
                    text.color[0],
                    text.color[1],
                    text.color[2],
                    text.color[3] * text.motion_opacity,
                ]
            } else {
                text.color
            };

            // Determine wrap width
            let effective_width = if let Some(clip) = text.clip_bounds {
                clip[2].min(text.width)
            } else {
                text.width
            };

            let needs_wrap = text.wrap && effective_width < text.measured_width - 2.0;
            let wrap_width = Some(text.width);
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            let (anchor, y_pos, use_layout_height) = match text.v_align {
                TextVerticalAlign::Center => {
                    (TextAnchor::Center, text.y + text.height / 2.0, false)
                }
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    let baseline_y = text.y + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };
            let layout_height = if use_layout_height {
                Some(text.height)
            } else {
                None
            };

            if let Ok(glyphs) = self.text_ctx.prepare_text_with_style(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                color,
                anchor,
                alignment,
                wrap_width,
                needs_wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height,
                text.letter_spacing,
            ) {
                let mut glyphs = glyphs;
                if let Some(clip) = text.clip_bounds {
                    for glyph in &mut glyphs {
                        glyph.clip_bounds = clip;
                    }
                }

                if let Some(affine) = text.css_affine {
                    // CSS-transformed text: convert to SDF primitives with local_affine
                    // Pushed into fg_batch.primitives to render in the main SDF pass
                    let [a, b, c, d, tx, ty] = affine;
                    let tx_scaled = tx * scale_factor;
                    let ty_scaled = ty * scale_factor;
                    for glyph in &glyphs {
                        let gc_x = glyph.bounds[0] + glyph.bounds[2] / 2.0;
                        let gc_y = glyph.bounds[1] + glyph.bounds[3] / 2.0;
                        let new_gc_x = a * gc_x + c * gc_y + tx_scaled;
                        let new_gc_y = b * gc_x + d * gc_y + ty_scaled;
                        let mut prim = GpuPrimitive::from_glyph(glyph);
                        prim.bounds = [
                            new_gc_x - glyph.bounds[2] / 2.0,
                            new_gc_y - glyph.bounds[3] / 2.0,
                            glyph.bounds[2],
                            glyph.bounds[3],
                        ];
                        prim.local_affine = [a, b, c, d];
                        prim.set_z_layer(text.z_index);
                        css_transformed_text_prims.push(prim);
                    }
                } else {
                    glyphs_by_layer
                        .entry(text.z_index)
                        .or_default()
                        .extend(glyphs);
                }
            }
        }

        // SVGs are rendered as rasterized images (not tessellated paths) for better anti-aliasing
        // They will be rendered later via render_rasterized_svgs

        self.renderer.resize(width, height);

        // Bind the real glyph atlas to the SDF pipeline whenever
        // it's available. See the same block in `render_tree` for
        // the rationale — canvas `draw_text` calls route glyph
        // primitives through `batch.primitives`, which need the
        // real atlas bound for the PRIM_TEXT shader path.
        if let (Some(atlas), Some(color_atlas)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            if !css_transformed_text_prims.is_empty() {
                batch.primitives.append(&mut css_transformed_text_prims);
            }
            self.renderer.set_glyph_atlas(atlas, color_atlas);
        }

        // For overlay rendering, we DON'T have glass effects (overlays are simple)
        // Render primitives without clearing (LoadOp::Load)
        let max_z = batch.max_z_layer();
        let max_text_z = glyphs_by_layer.keys().cloned().max().unwrap_or(0);
        let max_layer = max_z.max(max_text_z);

        tracing::trace!(
            "render_overlay_tree: {} primitives, {} text layers, max_layer={}",
            batch.primitives.len(),
            glyphs_by_layer.len(),
            max_layer
        );

        // Render all layers using overlay mode (no clear)
        for z in 0..=max_layer {
            let layer_primitives = batch.primitives_for_layer(z);
            if !layer_primitives.is_empty() {
                tracing::trace!(
                    "render_overlay_tree: rendering {} primitives at z={}",
                    layer_primitives.len(),
                    z
                );
                self.renderer
                    .render_primitives_overlay(target, &layer_primitives);
            }

            if let Some(glyphs) = glyphs_by_layer.get(&z) {
                if !glyphs.is_empty() {
                    tracing::trace!(
                        "render_overlay_tree: rendering {} glyphs at z={}",
                        glyphs.len(),
                        z
                    );
                    self.render_text(target, glyphs);
                }
            }
        }

        // Images render on top
        self.render_images(target, &images, width as f32, height as f32, scale_factor);

        // Render foreground primitives (e.g. borders on top)
        if !batch.foreground_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &batch.foreground_primitives);
        }

        // Poll the device to free completed command buffers
        self.renderer.poll();

        // Render layout debug for overlay tree if enabled
        let debug = DebugMode::from_env();
        if debug.layout {
            let scale = tree.scale_factor();
            self.render_layout_debug(target, tree, scale);
        }
        if debug.motion {
            self.render_motion_debug(target, tree, width, height);
        }

        // Return scratch buffers for reuse on next frame
        self.return_scratch_elements(texts, svgs, images);

        Ok(())
    }

    /// Render overlays from RenderState (cursors, selections, focus rings)
    fn render_overlays(
        &mut self,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) {
        let overlays = render_state.overlays();
        if overlays.is_empty() {
            return;
        }

        // Create a paint context for overlays
        let mut overlay_ctx = GpuPaintContext::new(width as f32, height as f32);

        for overlay in overlays {
            match overlay {
                Overlay::Cursor {
                    position,
                    size,
                    color,
                    opacity,
                } => {
                    if *opacity > 0.0 {
                        // Apply opacity to cursor color
                        let cursor_color =
                            Color::rgba(color.r, color.g, color.b, color.a * opacity);
                        overlay_ctx.execute_command(&DrawCommand::FillRect {
                            rect: Rect::new(position.0, position.1, size.0, size.1),
                            corner_radius: CornerRadius::default(),
                            brush: Brush::Solid(cursor_color),
                        });
                    }
                }
                Overlay::Selection { rects: _, color: _ } => {
                    // TODO: Re-enable for real-time text selection
                    // Disabled for now to avoid blue mask issue after modal close
                }
                Overlay::FocusRing {
                    position,
                    size,
                    radius,
                    color,
                    thickness,
                } => {
                    overlay_ctx.execute_command(&DrawCommand::StrokeRect {
                        rect: Rect::new(position.0, position.1, size.0, size.1),
                        corner_radius: CornerRadius::uniform(*radius),
                        stroke: Stroke::new(*thickness),
                        brush: Brush::Solid(*color),
                    });
                }
            }
        }

        // Render overlays as an overlay pass (on top of existing content)
        let overlay_batch = overlay_ctx.take_batch();
        if !overlay_batch.is_empty() {
            self.renderer.render_overlay(target, &overlay_batch);
        }
    }
}

/// Convert layout's GenericFont to GPU's GenericFont
fn to_gpu_generic_font(generic: GenericFont) -> GpuGenericFont {
    match generic {
        GenericFont::System => GpuGenericFont::System,
        GenericFont::Monospace => GpuGenericFont::Monospace,
        GenericFont::Serif => GpuGenericFont::Serif,
        GenericFont::SansSerif => GpuGenericFont::SansSerif,
    }
}

/// Debug mode flags for visual debugging
///
/// Set environment variable `BLINC_DEBUG` to enable debug visualization:
/// - `text`: Show text bounding boxes and baselines
/// - `layout`: Show all element bounding boxes (useful for debugging hit-testing)
/// - `motion`: Show active animation stats overlay
/// - `all` or `1` or `true`: Show all debug visualizations
#[derive(Clone, Copy)]
pub struct DebugMode {
    /// Show text bounding boxes and baseline indicators
    pub text: bool,
    /// Show all element bounding boxes
    pub layout: bool,
    /// Show motion/animation debug info
    pub motion: bool,
}

impl DebugMode {
    /// Check environment variable and return debug mode configuration
    pub fn from_env() -> Self {
        let debug_value = std::env::var("BLINC_DEBUG")
            .map(|v| v.to_lowercase())
            .unwrap_or_default();

        let all = debug_value == "all" || debug_value == "1" || debug_value == "true";
        let text = all || debug_value == "text";
        let layout = all || debug_value == "layout";
        let motion = all || debug_value == "motion";

        Self {
            text,
            layout,
            motion,
        }
    }

    /// Check if any debug mode is enabled
    pub fn any_enabled(&self) -> bool {
        self.text || self.layout || self.motion
    }
}

/// Generate text decoration primitives (strikethrough and underline) grouped by z-layer
///
/// Creates decoration lines for text elements that have:
/// - strikethrough: horizontal line through the middle of the text
/// - underline: horizontal line below the text baseline
///
/// Returns a HashMap of z_index -> primitives for interleaved rendering with text
fn generate_text_decoration_primitives_by_layer(
    texts: &[TextElement],
) -> std::collections::HashMap<u32, Vec<GpuPrimitive>> {
    let mut primitives_by_layer: std::collections::HashMap<u32, Vec<GpuPrimitive>> =
        std::collections::HashMap::new();

    for text in texts {
        if !text.strikethrough && !text.underline {
            continue;
        }

        // Calculate text width for decorations
        let decoration_width = if text.wrap && text.measured_width > text.width {
            text.width
        } else {
            text.measured_width.min(text.width)
        };

        // Skip if there's no meaningful width
        if decoration_width <= 0.0 {
            continue;
        }

        // Line thickness: use CSS text-decoration-thickness if set, else scale with font size
        let line_thickness = text
            .decoration_thickness
            .unwrap_or_else(|| (text.font_size / 14.0).clamp(1.0, 3.0));

        // Decoration color: use CSS text-decoration-color if set, else use text color
        let dec_color = text.decoration_color.unwrap_or(text.color);

        let layer_primitives = primitives_by_layer.entry(text.z_index).or_default();

        // Calculate the actual baseline Y position based on vertical alignment
        // This must match the text rendering logic to position decorations correctly
        //
        // glyph_extent = ascender - descender (where descender is negative)
        // Typical descender is about -20% of ascender, so glyph_extent ≈ ascender * 1.2
        let descender_approx = -text.ascender * 0.2;
        let glyph_extent = text.ascender - descender_approx;

        let baseline_y = match text.v_align {
            TextVerticalAlign::Center => {
                // GPU: y_pos = text.y + text.height / 2.0, then y_offset = y_pos - glyph_extent / 2.0
                // Glyph top is at: text.y + text.height/2 - glyph_extent/2
                // Baseline is at: glyph_top + ascender
                let glyph_top = text.y + text.height / 2.0 - glyph_extent / 2.0;
                glyph_top + text.ascender
            }
            TextVerticalAlign::Top => {
                // GPU: y_pos = text.y, y_offset = y + (layout_height - glyph_extent) / 2.0
                // Glyph top is at: text.y + (text.height - glyph_extent) / 2.0
                // Baseline is at: glyph_top + ascender
                let glyph_top = text.y + (text.height - glyph_extent) / 2.0;
                glyph_top + text.ascender
            }
            TextVerticalAlign::Baseline => {
                // GPU: y_pos = text.y + ascender, y_offset = y_pos - ascender = text.y
                // Glyph top is at: text.y
                // Baseline is at: text.y + ascender
                text.y + text.ascender
            }
        };

        // Strikethrough: draw line through the center of lowercase letters (x-height center)
        if text.strikethrough {
            // x-height is typically ~50% of ascender, center of x-height is ~25% above baseline
            let strikethrough_y = baseline_y - text.ascender * 0.35;
            let mut strike_rect = GpuPrimitive::rect(
                text.x,
                strikethrough_y - line_thickness / 2.0,
                decoration_width,
                line_thickness,
            )
            .with_color(dec_color[0], dec_color[1], dec_color[2], dec_color[3]);

            // Apply clip bounds from text element if present
            if let Some(clip) = text.clip_bounds {
                strike_rect = strike_rect.with_clip_rect(clip[0], clip[1], clip[2], clip[3]);
            }
            layer_primitives.push(strike_rect);
        }

        // Underline: draw line just below the baseline (at text bottom)
        if text.underline {
            // Underline position: just below baseline, snapping to text bottom
            let underline_y = baseline_y + text.ascender * 0.05;
            let mut underline_rect = GpuPrimitive::rect(
                text.x,
                underline_y - line_thickness / 2.0,
                decoration_width,
                line_thickness,
            )
            .with_color(dec_color[0], dec_color[1], dec_color[2], dec_color[3]);

            // Apply clip bounds from text element if present
            if let Some(clip) = text.clip_bounds {
                underline_rect = underline_rect.with_clip_rect(clip[0], clip[1], clip[2], clip[3]);
            }
            layer_primitives.push(underline_rect);
        }
    }

    primitives_by_layer
}

/// Generate debug primitives for text elements
///
/// Creates visual overlays showing:
/// - Bounding box outline (cyan)
/// - Baseline position (magenta line)
/// - Ascender line (green, at top of bounding box)
/// - Descender line (yellow, at bottom of bounding box)
fn generate_text_debug_primitives(texts: &[TextElement]) -> Vec<GpuPrimitive> {
    let mut primitives = Vec::new();

    for text in texts {
        // Determine the actual text width for debug visualization:
        // - For non-wrapped text: use measured_width (actual rendered text width)
        // - For wrapped text: use layout width (container constrains the text)
        let debug_width = if text.wrap && text.measured_width > text.width {
            // Text is wrapping - use container width
            text.width
        } else {
            // Single line - use actual measured width (clamped to layout width)
            text.measured_width.min(text.width)
        };

        // Bounding box outline (cyan, semi-transparent)
        let bbox = GpuPrimitive::rect(text.x, text.y, debug_width, text.height)
            .with_color(0.0, 0.0, 0.0, 0.0) // Transparent fill
            .with_border(1.0, 0.0, 1.0, 1.0, 0.7); // Cyan border
        primitives.push(bbox);

        // Baseline indicator (magenta horizontal line)
        // The baseline is at y + ascender
        let baseline_y = text.y + text.ascender;
        let baseline = GpuPrimitive::rect(text.x, baseline_y - 0.5, debug_width, 1.0)
            .with_color(1.0, 0.0, 1.0, 0.6); // Magenta
        primitives.push(baseline);

        // Ascender line indicator (green, at top of text)
        // For v_baseline texts, this shows where the ascender sits
        let ascender_line = GpuPrimitive::rect(text.x, text.y - 0.5, debug_width, 1.0)
            .with_color(0.0, 1.0, 0.0, 0.4); // Green, more transparent
        primitives.push(ascender_line);

        // Descender line (yellow, at bottom of bounding box)
        let descender_y = text.y + text.height;
        let descender_line = GpuPrimitive::rect(text.x, descender_y - 0.5, debug_width, 1.0)
            .with_color(1.0, 1.0, 0.0, 0.4); // Yellow
        primitives.push(descender_line);
    }

    primitives
}

/// Collect all element bounds from the render tree for debug visualization
fn collect_debug_bounds(tree: &RenderTree, scale: f32) -> Vec<DebugBoundsElement> {
    let mut bounds = Vec::new();

    if let Some(root) = tree.root() {
        collect_debug_bounds_recursive(tree, root, (0.0, 0.0), 0, scale, &mut bounds);
    }

    bounds
}

/// Recursively collect bounds from all nodes
fn collect_debug_bounds_recursive(
    tree: &RenderTree,
    node: LayoutNodeId,
    parent_offset: (f32, f32),
    depth: u32,
    scale: f32,
    bounds: &mut Vec<DebugBoundsElement>,
) {
    use blinc_layout::renderer::ElementType;

    let Some(node_bounds) = tree.layout().get_bounds(node, parent_offset) else {
        return;
    };

    // Determine element type name
    let element_type = tree
        .get_render_node(node)
        .map(|n| match &n.element_type {
            ElementType::Div => "Div".to_string(),
            ElementType::Text(_) => "Text".to_string(),
            ElementType::StyledText(_) => "StyledText".to_string(),
            ElementType::Image(_) => "Image".to_string(),
            ElementType::Svg(_) => "Svg".to_string(),
            ElementType::Canvas(_) => "Canvas".to_string(),
        })
        .unwrap_or_else(|| "Unknown".to_string());

    // Add this element's bounds (with DPI scaling)
    bounds.push(DebugBoundsElement {
        x: node_bounds.x * scale,
        y: node_bounds.y * scale,
        width: node_bounds.width * scale,
        height: node_bounds.height * scale,
        element_type,
        depth,
    });

    // Get scroll offset for this node (scroll containers offset their children)
    let scroll_offset = tree.get_scroll_offset(node);

    // Calculate new offset for children (including scroll offset)
    let new_offset = (
        node_bounds.x + scroll_offset.0,
        node_bounds.y + scroll_offset.1,
    );

    // Recurse into children
    for child in tree.layout().children(node) {
        collect_debug_bounds_recursive(tree, child, new_offset, depth + 1, scale, bounds);
    }
}

/// Generate debug primitives for layout element bounds
///
/// Creates visual overlays showing:
/// - Colored outlines for each element's bounding box
/// - Colors cycle based on tree depth (red, green, blue, yellow, cyan, magenta)
fn generate_layout_debug_primitives(bounds: &[DebugBoundsElement]) -> Vec<GpuPrimitive> {
    let mut primitives = Vec::new();

    // Color palette for different depths (cycling)
    let colors: [(f32, f32, f32); 6] = [
        (1.0, 0.3, 0.3), // Red
        (0.3, 1.0, 0.3), // Green
        (0.3, 0.3, 1.0), // Blue
        (1.0, 1.0, 0.3), // Yellow
        (0.3, 1.0, 1.0), // Cyan
        (1.0, 0.3, 1.0), // Magenta
    ];

    for elem in bounds {
        // Skip very small elements (likely invisible)
        if elem.width < 1.0 || elem.height < 1.0 {
            continue;
        }

        let (r, g, b) = colors[(elem.depth as usize) % colors.len()];
        let alpha = 0.5; // Semi-transparent outline

        // Draw outline only (transparent fill with colored border)
        let rect = GpuPrimitive::rect(elem.x, elem.y, elem.width, elem.height)
            .with_color(0.0, 0.0, 0.0, 0.0) // Transparent fill
            .with_border(1.0, r, g, b, alpha); // Colored border

        primitives.push(rect);
    }

    primitives
}

/// Scale and translate a path for SVG rendering with tint
fn scale_and_translate_path(
    path: &blinc_core::Path,
    x: f32,
    y: f32,
    scale: f32,
) -> blinc_core::Path {
    use blinc_core::{PathCommand, Point, Vec2};

    if scale == 1.0 && x == 0.0 && y == 0.0 {
        return path.clone();
    }

    let transform_point = |p: Point| -> Point { Point::new(p.x * scale + x, p.y * scale + y) };

    let new_commands: Vec<PathCommand> = path
        .commands()
        .iter()
        .map(|cmd| match cmd {
            PathCommand::MoveTo(p) => PathCommand::MoveTo(transform_point(*p)),
            PathCommand::LineTo(p) => PathCommand::LineTo(transform_point(*p)),
            PathCommand::QuadTo { control, end } => PathCommand::QuadTo {
                control: transform_point(*control),
                end: transform_point(*end),
            },
            PathCommand::CubicTo {
                control1,
                control2,
                end,
            } => PathCommand::CubicTo {
                control1: transform_point(*control1),
                control2: transform_point(*control2),
                end: transform_point(*end),
            },
            PathCommand::ArcTo {
                radii,
                rotation,
                large_arc,
                sweep,
                end,
            } => PathCommand::ArcTo {
                radii: Vec2::new(radii.x * scale, radii.y * scale),
                rotation: *rotation,
                large_arc: *large_arc,
                sweep: *sweep,
                end: transform_point(*end),
            },
            PathCommand::Close => PathCommand::Close,
        })
        .collect();

    blinc_core::Path::from_commands(new_commands)
}

// ─────────────────────────────────────────────────────────────────────────────
// 3D mesh dispatch
// ─────────────────────────────────────────────────────────────────────────────

/// Dispatch every `PendingMesh` captured by `GpuPaintContext` to
/// `GpuRenderer::render_mesh_data` against the frame target.
///
/// Computes a view-projection matrix from each pending mesh's captured
/// `Camera` against the current viewport size (so aspect stays correct
/// under window resizes) and extracts the first `Light::Directional`
/// for the mesh pipeline's sun light. Other light types are ignored
/// for now — the mesh pipeline only takes a single directional input,
/// and widening that is follow-up work tracked alongside per-canvas
/// viewport clipping.
///
/// If a mesh's camera is `Camera::default()` the pose is identity /
/// zero-eye which produces an invisible frame; demos should always
/// `ctx.set_camera(&cam)` before calling `ctx.draw_mesh_data`. A
/// `tracing::warn!` surfaces the silent-empty case to avoid
/// head-scratching during demo authoring.
fn dispatch_pending_meshes(
    renderer: &mut GpuRenderer,
    target: &wgpu::TextureView,
    width: u32,
    height: u32,
    meshes: &[PendingMesh],
) {
    if meshes.is_empty() {
        return;
    }
    let aspect = if height > 0 {
        width as f32 / height as f32
    } else {
        1.0
    };

    // Dedupe environment cubemap uploads by `Arc` identity. Without
    // this, multi-mesh scenes pay a full cubemap re-upload per
    // primitive — 39 meshes × 6 faces × ~9 mips = ~2000 redundant
    // `queue.write_texture` calls per frame on assets like
    // buster_drone, which dominates frame time. Every `PendingMesh`
    // from the same `SceneKit3D` shares the same `Arc<CubemapData>`,
    // so a cheap `Arc::ptr_eq` is enough to skip.
    let mut last_env: Option<std::sync::Arc<blinc_core::layer::CubemapData>> = None;

    // ── Shadow frustum (first directional light, first scene) ──────────
    //
    // Compute ONE light_view_proj for the whole mesh batch. Using the
    // first scene's camera target as a focus point and scaling the
    // orthographic frustum off the camera distance keeps the shadow
    // map sized to roughly what the viewer can see. Good enough for
    // the single-scene case (the common case); multi-scene frames
    // with divergent targets would want per-scene shadow maps, which
    // is out of scope here.
    let first = &meshes[0];
    let (light_dir, _light_intensity_0) = first_directional_light(&first.lights);
    let light_view_proj = compute_shadow_matrix(&first.camera, light_dir);

    // ── Phase 1: shadow depth pass for every caster ────────────────────
    //
    // All shadow-caster meshes write into the single shadow_map before
    // ANY main pass samples it. The first caster clears, the rest
    // load. Non-casters skip the pass entirely.
    let mut shadow_index: usize = 0;
    for pending in meshes {
        if !pending.mesh.material.casts_shadows {
            continue;
        }
        let model = mat4_to_array(&pending.transform);
        renderer.render_mesh_shadow_pass(&pending.mesh, &model, &light_view_proj, shadow_index);
        shadow_index += 1;
    }
    // If no casters were drawn, skip shadow sampling entirely so the
    // main pass doesn't read a stale / uninitialized shadow map.
    let shadow_matrix: Option<&[f32; 16]> = if shadow_index > 0 {
        Some(&light_view_proj)
    } else {
        None
    };

    // ── Phase 2: main HDR/tonemap passes for every mesh ────────────────
    //
    // Sort OPAQUE + MASK before BLEND before dispatch. Weighted-blended
    // OIT (the renderer's transparency path) needs every opaque fragment
    // to have written its depth before any BLEND fragment runs its own
    // depth test; otherwise a BLEND mesh drawn mid-scene accumulates at
    // pixels a later-drawn opaque mesh would have occluded, and the OIT
    // composite washes those opaque pixels out.
    //
    // Callers submitting `draw_mesh_data(...)` in scene-graph order
    // (the most common case — e.g. gltf_animation_demo, cutegirl,
    // strangler) would otherwise need to sort manually at each call
    // site. Doing it once here, at the single dispatch seam, keeps
    // every asset correct without surfacing the OIT ordering invariant
    // as user-facing knowledge.
    //
    // `sort_by_key` is stable so within a mode the caller's submission
    // order is preserved — matters when a caller is already doing
    // back-to-front sorting inside the BLEND group.
    let mut ordered: Vec<&PendingMesh> = meshes.iter().collect();
    ordered.sort_by_key(|p| {
        matches!(
            p.mesh.material.alpha_mode,
            blinc_core::draw::AlphaMode::Blend
        ) as u8
    });

    let batch_count = ordered.len();
    for (batch_index, &pending) in ordered.iter().enumerate() {
        // Upload the environment cubemap only when its `Arc` identity
        // changes. The renderer's texture is overwritten on each real
        // upload, so only the last distinct environment matters.
        if let Some(ref env) = pending.env_cubemap {
            let is_new = last_env
                .as_ref()
                .map(|prev| !std::sync::Arc::ptr_eq(prev, env))
                .unwrap_or(true);
            if is_new {
                renderer.upload_environment_cubemap(env);
                last_env = Some(env.clone());
            }
        }

        // Use the canvas viewport aspect when available so the
        // perspective projection matches the clipped region, not the
        // full frame. Falls back to the frame aspect for full-viewport
        // mesh draws (no canvas wrapper).
        let vp_aspect = pending
            .viewport
            .map(|[_, _, w, h]| if h > 0.0 { w / h } else { 1.0 })
            .unwrap_or(aspect);
        let view_proj = camera_view_proj(&pending.camera, vp_aspect);
        let _inv_view_proj = mat4_inverse_flat(&view_proj);
        let camera_pos = [
            pending.camera.position.x,
            pending.camera.position.y,
            pending.camera.position.z,
        ];
        let lights = collect_directional_lights(&pending.lights);
        let model = mat4_to_array(&pending.transform);

        // Same clamp + zero-area skip as
        // `dispatch_pending_gpu_passes`. The mesh tonemap pass also
        // does `set_scissor_rect(...)` with this rect and the same
        // `vw.max(1.0) as u32` defensive pattern that turns a
        // clamped zero into a one-pixel overflow.
        let clamped_viewport = match pending.viewport {
            Some(r) => {
                let c = clamp_viewport(r, width, height);
                if c[2] < 1.0 || c[3] < 1.0 {
                    continue;
                }
                Some(c)
            }
            None => None,
        };
        renderer.render_mesh_data_batched(
            target,
            &pending.mesh,
            &model,
            &view_proj,
            camera_pos,
            &lights,
            shadow_matrix,
            clamped_viewport,
            batch_index,
            batch_count,
        );
    }
}

/// Dispatch every [`PendingGpuPass`] captured by `GpuPaintContext` to
/// its underlying [`blinc_gpu::CustomRenderPass`].
///
/// Lazy-initializes each pass on the first frame it dispatches (the
/// `GpuPass` wrapper carries the `initialized` flag internally). The
/// `RenderPassContext` we build here mirrors the one
/// [`CustomPassManager::execute_stage`] uses for renderer-registered
/// passes, with two differences: `view_proj` / `inv_view_proj` /
/// `camera_pos` are left `None` (no 3D camera implied for canvas-scoped
/// custom passes — users who want 3D should still register at the
/// `Scene3D` stage), and `viewport` is whatever the canvas closure
/// derived from its bounds (or `None` = full target).
fn dispatch_pending_gpu_passes(
    renderer: &mut GpuRenderer,
    target: &wgpu::TextureView,
    width: u32,
    height: u32,
    scale_factor: f32,
    passes: &[blinc_gpu::paint::PendingGpuPass],
) {
    if passes.is_empty() {
        return;
    }
    let device = renderer.device_arc();
    let queue = renderer.queue_arc();
    let format = renderer.surface_format();
    for pending in passes {
        // Clamp here so user pass code can pass the rect straight
        // to `set_scissor_rect` / `set_viewport` without writing
        // its own bounds-check. Window resize is the canonical
        // crash case: the paint walker computed the canvas
        // viewport against the previous frame's target size; if
        // the surface shrunk between paint and dispatch the
        // viewport overflows. wgpu validates scissor strictly and
        // panics on overflow.
        //
        // A clamped-to-zero rect (canvas dragged offscreen, or
        // sitting exactly on the bottom/right edge after a resize)
        // is skipped entirely. Passing the zero-area rect through
        // would invite the same footgun the upstream user code
        // defends against — `vh.max(1.0) as u32` re-inflates h=0
        // back to 1, which then overflows by one row at the bottom
        // edge. There's nothing to draw at zero area; just drop
        // the dispatch.
        let clamped_viewport = match pending.viewport {
            Some(r) => {
                let c = clamp_viewport(r, width, height);
                if c[2] < 1.0 || c[3] < 1.0 {
                    continue;
                }
                Some(c)
            }
            None => None,
        };
        let ctx = blinc_gpu::custom_pass::RenderPassContext {
            device: &device,
            queue: &queue,
            target,
            viewport_width: width,
            viewport_height: height,
            texture_format: format,
            scale_factor: scale_factor as f64,
            view_proj: None,
            inv_view_proj: None,
            camera_pos: None,
            viewport: clamped_viewport,
        };
        pending
            .pass
            .initialize_and_render(&device, &queue, format, &ctx);
    }
}

/// Clamp a viewport rect to a render-target extent. `[x, y, w, h]` in
/// physical pixels — both origin and size adjusted so `x + w ≤ tw`
/// and `y + h ≤ th`. Returns a zero-size rect (still inside the
/// target) when the input is fully outside.
fn clamp_viewport(rect: [f32; 4], target_w: u32, target_h: u32) -> [f32; 4] {
    let [x, y, w, h] = rect;
    let tw = target_w as f32;
    let th = target_h as f32;
    let x = x.max(0.0).min(tw);
    let y = y.max(0.0).min(th);
    let w = w.max(0.0).min((tw - x).max(0.0));
    let h = h.max(0.0).min((th - y).max(0.0));
    [x, y, w, h]
}

/// Build a view × projection matrix for a directional light illuminating
/// the scene the given camera is looking at.
///
/// This is a deliberately simple fit: we center the orthographic frustum
/// on the camera target and scale it off the camera distance. The result
/// covers roughly what the viewer can see, which is enough for a
/// single-drone / single-object scene (the main case today). A more
/// ambitious implementation would fit the frustum to the scene's actual
/// world-space AABB (or to the view frustum intersected with the scene
/// bounds), plus use cascaded shadow maps for landscapes.
///
/// Up-vector selection avoids the degenerate case where `light_dir` is
/// parallel to world-Y (makes the look-at cross product collapse).
fn compute_shadow_matrix(camera: &blinc_core::Camera, light_dir: [f32; 3]) -> [f32; 16] {
    let target = camera.target;
    let eye = camera.position;
    let dx = eye.x - target.x;
    let dy = eye.y - target.y;
    let dz = eye.z - target.z;
    let cam_dist = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);

    // Ortho frustum half-extent — ~1.0× camera distance covers the
    // full scene for typical framings (frame_camera multiplies scene
    // diagonal by 1.1, so camera distance ≈ scene diagonal).
    let half = cam_dist;

    // Light sits on the opposite side of the target from its direction,
    // far enough back that the near plane doesn't clip the scene.
    let ld = blinc_core::Vec3::new(light_dir[0], light_dir[1], light_dir[2]).normalize();
    let light_pos = blinc_core::Vec3::new(
        target.x - ld.x * cam_dist * 2.0,
        target.y - ld.y * cam_dist * 2.0,
        target.z - ld.z * cam_dist * 2.0,
    );

    // Pick an up-vector not parallel to the light direction.
    let up = if ld.y.abs() > 0.95 {
        blinc_core::Vec3::new(0.0, 0.0, 1.0)
    } else {
        blinc_core::Vec3::new(0.0, 1.0, 0.0)
    };

    let view = mat4_look_at(light_pos, target, up);
    let proj = mat4_orthographic_rh(-half, half, -half, half, 0.1, cam_dist * 4.0);
    mat4_mul_flat(&proj, &view)
}

/// Build a view × projection matrix for the captured `Camera`.
///
/// Right-handed coordinate system, +Y up. Matches the convention the
/// mesh shader expects (see `crates/blinc_gpu/src/shaders/mesh.wgsl`).
///
/// For `CameraProjection::Perspective`, the stored `aspect` field on
/// the projection is overridden by the frame's actual aspect so the
/// scene doesn't stretch on resize — the stored value is just a
/// fallback default from `Camera::perspective`.
fn camera_view_proj(camera: &blinc_core::Camera, frame_aspect: f32) -> [f32; 16] {
    let view = mat4_look_at(camera.position, camera.target, camera.up);
    let proj = match camera.projection {
        blinc_core::CameraProjection::Perspective {
            fov_y, near, far, ..
        } => mat4_perspective_rh(fov_y, frame_aspect, near, far),
        blinc_core::CameraProjection::Orthographic {
            left,
            right,
            bottom,
            top,
            near,
            far,
        } => mat4_orthographic_rh(left, right, bottom, top, near, far),
    };
    mat4_mul_flat(&proj, &view)
}

/// Extract the first `Light::Directional` from the snapshot, returning
/// a normalized direction vector and scalar intensity. Falls back to a
/// soft top-down fill if none is present so the demo never renders
/// pitch-black.
fn first_directional_light(lights: &[blinc_core::Light]) -> ([f32; 3], f32) {
    for light in lights {
        if let blinc_core::Light::Directional {
            direction,
            intensity,
            ..
        } = light
        {
            let d = direction.normalize();
            return ([d.x, d.y, d.z], *intensity);
        }
    }
    ([0.0, -1.0, 0.3], 0.8)
}

/// Collect every directional light for the mesh shader's multi-light
/// array. Capped at the shader's `MAX_DIR_LIGHTS`; if the scene has
/// no directional lights we hand back a single soft top-down default
/// so the mesh never renders pitch-black.
fn collect_directional_lights(lights: &[blinc_core::Light]) -> Vec<blinc_gpu::DirectionalLight> {
    let max = blinc_gpu::MAX_DIR_LIGHTS;
    let mut out: Vec<blinc_gpu::DirectionalLight> = Vec::with_capacity(max);
    for light in lights {
        if out.len() >= max {
            break;
        }
        if let blinc_core::Light::Directional {
            direction,
            color,
            intensity,
            ..
        } = light
        {
            let d = direction.normalize();
            out.push(blinc_gpu::DirectionalLight {
                direction: [d.x, d.y, d.z],
                intensity: *intensity,
                color: [color.r, color.g, color.b],
            });
        }
    }
    if out.is_empty() {
        out.push(blinc_gpu::DirectionalLight::DEFAULT);
    }
    out
}

/// Flatten a column-major `Mat4` to the `[f32; 16]` layout
/// `GpuRenderer::render_mesh_data` expects.
fn mat4_to_array(m: &blinc_core::Mat4) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = m.cols[col][row];
        }
    }
    out
}

/// Multiply two flat column-major 4×4 matrices (`a * b`), returning a
/// `[f32; 16]` in the same layout. Used to compose `proj * view` after
/// both are computed in `Mat4`/array form.
fn mat4_mul_flat(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k * 4 + row] * b[col * 4 + k];
            }
            out[col * 4 + row] = s;
        }
    }
    out
}

/// Right-handed look-at view matrix. Produces `[f32; 16]` directly
/// (column-major) for the downstream multiply.
fn mat4_look_at(
    eye: blinc_core::Vec3,
    target: blinc_core::Vec3,
    up: blinc_core::Vec3,
) -> [f32; 16] {
    let f = blinc_core::Vec3::new(target.x - eye.x, target.y - eye.y, target.z - eye.z).normalize();
    let r = f.cross(up).normalize();
    let u = r.cross(f);
    let tx = -(r.x * eye.x + r.y * eye.y + r.z * eye.z);
    let ty = -(u.x * eye.x + u.y * eye.y + u.z * eye.z);
    let tz = f.x * eye.x + f.y * eye.y + f.z * eye.z;
    // Column-major: col0 = [r.x, u.x, -f.x, 0], col1 = [r.y, u.y, -f.y, 0], ...
    [
        r.x, u.x, -f.x, 0.0, r.y, u.y, -f.y, 0.0, r.z, u.z, -f.z, 0.0, tx, ty, tz, 1.0,
    ]
}

/// Right-handed perspective projection. Maps view-space Z in `[-far, -near]`
/// to clip-space depth `[0, 1]` (wgpu convention). `fov_y` is radians.
fn mat4_perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_y * 0.5).tan();
    let nf = 1.0 / (near - far);
    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        far * nf,
        -1.0,
        0.0,
        0.0,
        far * near * nf,
        0.0,
    ]
}

/// Right-handed orthographic projection. Uses the same clip-space
/// depth range `[0, 1]` as the perspective variant so the mesh shader
/// can stay agnostic of the projection choice.
fn mat4_orthographic_rh(
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
    near: f32,
    far: f32,
) -> [f32; 16] {
    let rl = 1.0 / (right - left);
    let tb = 1.0 / (top - bottom);
    let fnn = 1.0 / (far - near);
    [
        2.0 * rl,
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 * tb,
        0.0,
        0.0,
        0.0,
        0.0,
        -fnn,
        0.0,
        -(right + left) * rl,
        -(top + bottom) * tb,
        -near * fnn,
        1.0,
    ]
}

/// Inverse of a column-major 4×4 matrix (GLU-style cofactor expansion).
fn mat4_inverse_flat(m: &[f32; 16]) -> [f32; 16] {
    let mut inv = [0.0f32; 16];
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
        + m[9] * m[7] * m[14]
        + m[13] * m[6] * m[11]
        - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
        - m[8] * m[7] * m[14]
        - m[12] * m[6] * m[11]
        + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
        + m[8] * m[7] * m[13]
        + m[12] * m[5] * m[11]
        - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
        - m[8] * m[6] * m[13]
        - m[12] * m[5] * m[10]
        + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
        - m[9] * m[3] * m[14]
        - m[13] * m[2] * m[11]
        + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
        + m[8] * m[3] * m[14]
        + m[12] * m[2] * m[11]
        - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
        - m[8] * m[3] * m[13]
        - m[12] * m[1] * m[11]
        + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
        + m[8] * m[2] * m[13]
        + m[12] * m[1] * m[10]
        - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
        + m[5] * m[3] * m[14]
        + m[13] * m[2] * m[7]
        - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
        - m[4] * m[3] * m[14]
        - m[12] * m[2] * m[7]
        + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
        + m[4] * m[3] * m[13]
        + m[12] * m[1] * m[7]
        - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
        - m[4] * m[2] * m[13]
        - m[12] * m[1] * m[6]
        + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
        - m[5] * m[3] * m[10]
        - m[9] * m[2] * m[7]
        + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
        + m[4] * m[3] * m[10]
        + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
        - m[4] * m[3] * m[9]
        - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
        + m[4] * m[2] * m[9]
        + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];
    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-12 {
        return [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
    }
    let id = 1.0 / det;
    for v in &mut inv {
        *v *= id;
    }
    inv
}
