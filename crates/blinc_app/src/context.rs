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

mod binding_updates;
mod cache_lifecycle;
mod canvas_replay;
mod composite_replay;
mod compositor;
mod css_updates;
mod debug_rendering;
mod image_rendering;
mod mesh_rendering;
mod overlay_rendering;
mod svg_rendering;
mod tree_rendering;

use mesh_rendering::{dispatch_pending_gpu_passes, dispatch_pending_meshes};
use tree_rendering::{generate_text_decoration_primitives_by_layer, to_gpu_generic_font};

mod cached_text;
#[cfg(test)]
mod cached_text_clip_tests;
mod canvas_order;
#[cfg(test)]
mod composite_layers_tests;
mod composite_scratch;
#[cfg(test)]
mod foreground_layers_tests;
#[cfg(test)]
mod foreground_rgba_tests;
#[cfg(test)]
mod glass_path_clip_tests;
#[cfg(test)]
mod layer_text_clip_tests;
#[cfg(test)]
mod main_text_clip_tests;
#[cfg(test)]
mod motion_layers_tests;
#[cfg(test)]
mod motion_text_clip_tests;
#[cfg(test)]
mod native_path_clip_tests;
#[cfg(test)]
mod overlay_text_clip_tests;
#[cfg(test)]
mod path_brush_clip_tests;
#[cfg(test)]
mod prepared_glyph_clip_tests;
#[cfg(test)]
mod retained_canvas_tests;
#[cfg(test)]
mod rgba_affine_tests;
#[cfg(test)]
mod rgba_clip_properties_tests;
#[cfg(test)]
mod rgba_clip_tests;
#[cfg(test)]
mod rgba_order_tests;
#[cfg(test)]
mod rgba_path_clip_tests;
mod text_clip;

/// Bound image-cache growth while retaining the visible working set.
const IMAGE_CACHE_CAPACITY: usize = 256;

/// Maximum number of parsed SVG documents to cache
const SVG_CACHE_CAPACITY: usize = 128;

/// Compress only large, block-aligned images; keep emoji and small assets lossless.
fn bc_eligible(source_uri: &str, width: u32, height: u32) -> bool {
    const MIN_DIM: u32 = 256;
    if source_uri.starts_with("emoji://") {
        return false;
    }
    // BC textures require dimensions divisible by four.
    if width % 4 != 0 || height % 4 != 0 {
        return false;
    }
    width >= MIN_DIM && height >= MIN_DIM
}

/// Union nondegenerate primitive bounds in screen pixels; return None for an empty range.
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

/// Independent canvas output collected for ordered overlay dispatch. Retained canvases
/// paint with their ancestor group instead.
#[derive(Default)]
pub struct CanvasOverlay {
    /// Whole canvas batches when groups, paths or RGBA draws require paint order.
    pub ordered_batches: Vec<PrimitiveBatch>,
    /// Foreground draws kept with their own commands and auxiliary rows.
    pub foreground_batches: Vec<PrimitiveBatch>,
    pub primitives: Vec<blinc_gpu::primitives::GpuPrimitive>,
    pub foreground_primitives: Vec<blinc_gpu::primitives::GpuPrimitive>,
    pub dynamic_images: Vec<blinc_gpu::primitives::DynamicImage>,
    pub meshes: Vec<blinc_gpu::PendingMesh>,
    /// User-defined GPU passes scheduled via `DrawContext::run_gpu_pass`
    /// inside canvas closures. Drained from each canvas's scratch paint
    /// context and dispatched alongside `meshes`, after the SDF static
    /// cache has been blitted onto the frame target.
    pub gpu_passes: Vec<blinc_gpu::paint::PendingGpuPass>,
    /// Merged auxiliary rows, with primitive offsets rebased during collection.
    pub aux_data: Vec<[f32; 4]>,
}

/// Result of CSS cache patching. Damage bounds are valid only when patching succeeds.
#[derive(Debug, Clone)]
pub enum CssDeltaResult {
    /// Every record was patched in place. Caller dispatches the
    /// damaged-cache repaint and the cached text / SVG / image
    /// re-dispatch.
    Patched,
    /// These subtrees need re-emission before the patched cache can be replayed.
    PatchedWithReemits { reemit_nodes: Vec<LayoutNodeId> },
    /// The patch path couldn't proceed (cache missing, lock poisoned,
    /// opacity-zero ratio, layer-push-index drift). Caller takes the
    /// full slow path.
    Bail,
}

/// Cached node, primitive range and bounds saved before CSS re-emission replaces the live
/// record.
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
    /// Screen-space damage from the most recent binding update.
    last_binding_damage_rects: Vec<[f32; 4]>,
    /// Screen-space bounds of CSS records changed by the most recent patch.
    last_css_damage_rects: Vec<[f32; 4]>,
    /// Prepared glyphs grouped by z-layer for cache and damage replay.
    cached_glyphs_by_layer: Option<std::collections::BTreeMap<u32, cached_text::TextBatch>>,
    /// Same as `cached_glyphs_by_layer` but for the foreground-text
    /// pass — glyphs inside `.foreground()` elements, dispatched
    /// after the rest of the scene.
    cached_fg_glyphs: Option<cached_text::TextBatch>,
    /// Motion-subtree glyph primitives with their affine and owning clip rows, dispatched
    /// after dynamic geometry.
    cached_motion_subtree_text_prims: Option<cached_text::TextBatch>,
    /// Primitive ranges and promoted ancestors used to apply live CSS opacity to base-alpha
    /// glyphs.
    motion_subtree_text_patch: Vec<(std::ops::Range<usize>, blinc_layout::tree::StableNodeId)>,
    /// Promoted-region bounds, translation and scale at collect time, used to anchor live
    /// glyph transforms.
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
    cached_css_transformed_text_prims: Option<PrimitiveBatch>,
    /// Fade deadlines used to keep redraw active only while loaded images are fading.
    image_fade_deadlines: std::collections::HashMap<String, web_time::Instant>,
    // LRU cache for parsed SVG documents (avoids re-parsing)
    svg_cache: LruCache<u64, SvgDocument>,
    // Texture atlas for rasterized SVGs (single shared GPU texture, shelf-packed)
    svg_atlas: SvgAtlas,
    // Scratch buffers for per-frame allocations (reused to avoid allocations)
    scratch_texts: Vec<TextElement>,
    scratch_svgs: Vec<SvgElement>,
    scratch_images: Vec<ImageElement>,
    // Current cursor position in physical pixels (for @flow pointer input)
    cursor_pos: [f32; 2],
    // Whether the last render contained @flow shader elements (triggers continuous redraw)
    has_active_flows: bool,
    /// Whether image fades require another frame, tracked independently of flow shaders.
    has_pending_image_fade: bool,
    // Frame counter for periodic cache stats logging
    frame_count: u64,
    // Clear alpha for the current window: one for opaque surfaces, zero for transparent
    // surfaces.
    clear_alpha: f32,
    // Static-owner batch from the most recent full paint. Binding and CSS metadata use its
    // primitive indices.
    cached_bg_batch: Option<blinc_gpu::PrimitiveBatch>,
    /// Motion-owner batch kept outside the static texture and patched independently.
    cached_dynamic_batch: Option<blinc_gpu::PrimitiveBatch>,
    /// Suppress dynamic and foreground dispatch while the inner call paints the static
    /// cache.
    in_compositor_inner_call: bool,
    /// Promoted CSS textures keyed by node, paired with their allocated source extents.
    /// Live animation values are applied when blitting.
    css_composited_textures:
        std::collections::HashMap<u64, (blinc_gpu::renderer::LayerTexture, (u32, u32))>,
    /// Scratch fingerprints used to reuse unchanged CSS bakes.
    css_composited_scratch_hash: std::collections::HashMap<u64, u64>,
    /// Promoted motion-subtree textures and allocated source extents, keyed by node.
    motion_subtree_textures:
        std::collections::HashMap<u64, (blinc_gpu::renderer::LayerTexture, (u32, u32))>,
    /// Scratch fingerprints used to reuse unchanged motion bakes.
    motion_subtree_scratch_hash: std::collections::HashMap<u64, u64>,
    // Collected elements retained with the primitive cache. Active motion can require fresh
    // collection.
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
    /// Clip geometry and its owner's physical frame, shared across collected spans.
    path_clip: Option<Arc<text_clip::CollectedTextPath>>,
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
    /// Half the line box's leading, above the ascender
    half_leading: f32,
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
    /// Defer motion-subtree text until its dynamic geometry has been dispatched.
    in_motion_subtree: bool,
    /// Nearest promoted ancestor whose live opacity is applied at dispatch instead of
    /// collect time.
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
    /// Defer this SVG until its motion-subtree geometry has been dispatched.
    in_motion_subtree: bool,
    /// Paint layer used to order SVGs alongside primitives and text.
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

/// Atlas fullness, by shelf extent, above which a walk tries to reclaim.
/// Below it the atlas has room and a GC would only cost re-rasterisation.
const ATLAS_GC_PRESSURE: f32 = 0.85;

/// Walks a glyph may go undrawn before it is reclaimable. Generous
/// because reclaiming forces a re-walk, and a glyph that reappears has
/// to be rasterised again.
const ATLAS_GC_MAX_AGE: u64 = 120;

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
            scratch_texts: Vec::with_capacity(64), // Pre-allocate for text elements
            scratch_svgs: Vec::with_capacity(32),  // Pre-allocate for SVG elements
            scratch_images: Vec::with_capacity(32), // Pre-allocate for image elements
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

    /// Set clear alpha for the current window: one for opaque surfaces, zero for
    /// transparent surfaces.
    pub fn set_clear_alpha(&mut self, alpha: f32) {
        self.clear_alpha = alpha;
    }

    /// Register a custom pass. Scene3D passes receive the active camera context.
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
        // One atlas epoch per walk, bumped before anything is drawn so
        // this walk's glyphs land in the new epoch. Reclaim first, while
        // nothing from this walk has been touched yet: a glyph the last
        // walk drew is one epoch old and safe.
        self.text_ctx.begin_atlas_epoch();
        if self.text_ctx.atlas_utilization() >= ATLAS_GC_PRESSURE
            && self.text_ctx.gc_atlas(ATLAS_GC_MAX_AGE)
        {
            // Reclaimed space is zeroed and reused, so any cached batch
            // holding baked PRIM_TEXT UVs into it would sample whatever
            // lands there next. This walk rebuilds them.
            self.renderer.invalidate_static_layer();
        }

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

        // Capture promoted-ancestor opacity for separately dispatched text, SVG and image
        // content.
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
                        // Save the promoted region's collect-time geometry for live text
                        // transforms.
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
                None,  // No inherited text colour at the root
                None,  // No parent node
                None,  // No initial scroll clip
                None,  // No 3D layer ancestor
                false, // No ancestor pending motion at root
                false, // No ancestor motion container at root
                &composite_layer_opacities,
                1.0,  // Initial composite-layer opacity (no ancestor promoted)
                None, // No composite-promoted ancestor at root
                None, // No inherited text path at root
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
        // Inherited text colour, resolved as the walk descends rather than
        // copied into props beforehand, so a subtree rebuilt after the
        // stylesheet pass still inherits. Mirrors `render_text_recursive`.
        inherited_text_color: Option<[f32; 4]>,
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
        // Keep descendants during a pending opacity animation, including its zero endpoint.
        ancestor_pending_motion: bool,
        // Defer descendants of motion containers to the motion overlay streams.
        inside_motion_subtree: bool,
        // Live promoted-ancestor opacity for content dispatched separately from its baked
        // geometry.
        composite_layer_opacities: &std::collections::HashMap<blinc_layout::LayoutNodeId, f32>,
        // Inherited promoted opacity for images. Text and SVG use their promoted-ancestor
        // dispatch patch.
        inherited_composite_layer_opacity: f32,
        // Nearest composite-promoted ancestor with a playing CSS
        // animation, threaded down so text / SVG elements can record
        // it for the dispatch-time live patch.
        nearest_composite_ancestor: Option<blinc_layout::tree::StableNodeId>,
        inherited_text_path: Option<Arc<text_clip::CollectedTextPath>>,
    ) {
        use blinc_layout::Material;

        // Use animated bounds if this node has layout animation, otherwise use layout bounds
        // This ensures children are positioned correctly during layout animation transitions
        let Some(bounds) = tree.get_render_bounds(node, parent_offset) else {
            return;
        };

        let abs_x = bounds.x;
        let abs_y = bounds.y;

        // What this node set for itself wins; anything it did not set, it
        // inherits. This is what paints and what descends.
        let effective_text_color = tree
            .get_render_node(node)
            .and_then(|n| n.props.text_color)
            .or(inherited_text_color);

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

        // Translation is already included in the child offset; read only the remaining
        // binding transform here.
        let binding_scale = tree.get_motion_scale(node);
        let binding_opacity = tree.get_motion_opacity(node);

        // Bindings and motion FSMs both make this node a motion container.
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
        // Accumulate promoted opacity for separately dispatched descendants.
        let node_composite_layer_opacity =
            composite_layer_opacities.get(&node).copied().unwrap_or(1.0);
        let effective_composite_layer_opacity =
            inherited_composite_layer_opacity * node_composite_layer_opacity;
        // Record the nearest promoted ancestor for live text and SVG patches.
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

        // Keep transparent nodes while motion can make them visible, matching primitive
        // emission.
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
        // Propagate pending motion so zero-opacity descendants remain available for later
        // frames.
        let subtree_pending_motion = ancestor_pending_motion || has_pending_motion;
        if effective_motion_opacity <= 0.001 && !subtree_pending_motion {
            return;
        }

        // CSS visibility: hidden — skip rendering but preserve layout space
        if let Some(render_node) = tree.get_render_node(node) {
            if !render_node.props.visible || tree.layout_tree.is_display_none(node) {
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

        // Compose the node's CSS transform before collecting its own content.
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

        let text_path = tree
            .get_render_node(node)
            .and_then(|n| n.props.clip_path.as_ref())
            .and_then(|clip| {
                text_clip::CollectedTextPath::capture(
                    clip,
                    [abs_x, abs_y, bounds.width, bounds.height],
                    node_css_affine,
                    effective_motion_scale,
                    effective_motion_translate,
                    effective_motion_scale_center,
                    scale,
                    if tree.get_render_node(node).is_some_and(|n| {
                        n.props.rotate_x.is_some()
                            || n.props.rotate_y.is_some()
                            || n.props.perspective.is_some()
                    }) {
                        Some(node)
                    } else {
                        inside_3d_layer.as_ref().map(|info| info.node_id)
                    },
                )
            })
            .or(inherited_text_path);

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

                    // Keep glyphs at their base rasterization size and apply motion through
                    // the affine.
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

                    // Trace nontrivial motion values.
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
                        path_clip: text_path.clone(),
                        x: scaled_x,
                        y: scaled_y,
                        width: scaled_width,
                        height: scaled_height,
                        font_size: scaled_font_size,
                        color: effective_text_color.unwrap_or(text_data.color),
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
                        half_leading: text_data.half_leading * scale,
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
                        // Resolve currentColor from the effective inherited text color.
                        tint: svg_data.tint.or_else(|| {
                            effective_text_color
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
                    let scaled_half_leading = styled_data.half_leading * scale;

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
                                path_clip: text_path.clone(),
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
                                half_leading: scaled_half_leading * effective_motion_scale.1,
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
                effective_text_color,
                Some(node), // pass current node as parent for children
                child_scroll_clip,
                child_3d_layer.clone(),
                subtree_pending_motion,
                child_inside_motion_subtree,
                composite_layer_opacities,
                effective_composite_layer_opacity,
                nearest_composite_ancestor,
                text_path.clone(),
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

    /// Create a surface without replacing the renderer or device.
    ///
    /// # Safety
    /// The caller must uphold wgpu::Instance::create_surface_unsafe requirements.
    pub unsafe fn create_surface_unsafe(
        &self,
        target: wgpu::SurfaceTargetUnsafe,
    ) -> std::result::Result<wgpu::Surface<'static>, blinc_gpu::RendererError> {
        unsafe { self.renderer.create_surface_unsafe(target) }
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
        // Consume the atlas-growth notification; frame preparation binds the current atlas
        // views.
        let _ = self.text_ctx.take_atlas_grew();

        // Use the static compositor when the surface texture is available. Inner cache
        // paints pass no texture.
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
        // Trace time spent in each rendering stage.
        let p4_start = web_time::Instant::now();

        // Get scale factor for HiDPI rendering
        let scale_factor = tree.scale_factor();

        // Commit animation and promotion status for both direct and compositor rendering.
        let v2_statuses = tree.compute_animation_status();
        tree.commit_animation_status(&v2_statuses);

        // Refresh independent canvases and patch bindings when the cached batch permits it;
        // otherwise walk the tree.
        let used_fast_paint = try_fast_paint
            && self.redraw_canvases(tree, width, height)
            && self.apply_binding_deltas(tree, scale_factor);

        // Create a single paint context for all layers with text rendering support
        let mut ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);

        // A successful patch already contains this frame's geometry.
        if !used_fast_paint {
            tree.render_with_motion(&mut ctx, render_state);
        }
        let t_paint_walker = p4_start.elapsed();

        // Bake emitted CSS scratch batches, retaining textures whose content is unchanged.
        if !used_fast_paint {
            let scratch_batches = ctx.take_composite_layer_batches();
            if !scratch_batches.is_empty() {
                // Match scratch batches and regions by their slotmap key.
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
                    // Reuse the texture when intrinsic content is unchanged; live animation
                    // is applied by the blit.
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
            // Release textures for nodes that no longer have a promoted CSS region.
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

            // Bake promoted motion subtrees using the same fingerprint and texture-pool
            // rules as CSS layers.
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
            // Release motion textures after their promotion lapses.
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

        // Replace dynamic content after a full walk; patched fast frames retain the
        // existing owner batch.
        if !used_fast_paint {
            self.cached_dynamic_batch = Some(ctx.take_dynamic_batch());
        }

        // Collect mesh output for dispatch after 2D content.
        let pending_meshes = ctx.take_pending_meshes();
        // Custom GPU passes scheduled via `ctx.run_gpu_pass(...)` inside
        // canvas callbacks. Same lifecycle as `pending_meshes` —
        // dispatched alongside meshes on the legacy (non-overlay) path
        // a few hundred lines down.
        let pending_gpu_passes = ctx.take_pending_gpu_passes();

        // Collect text, SVG, images and flows, reusing compatible cached element lists.
        let collect_start = web_time::Instant::now();
        // Refresh collected positions for active motion and direct-write drift. Pure
        // timeline rotation can keep unrelated collected content cached.
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

        // Separate normal content, 3D bakes and motion overlay content before dispatch.
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
                // Route promoted text through its live-opacity overlay patch, including
                // foreground content.
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
                // Route promoted SVGs through the same live-opacity overlay as their text.
                motion_subtree_svgs.push(svg);
            } else {
                svgs.push(svg);
            }
        }
        // Stable layer sorting preserves SVG sibling order at equal z.
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
        let mut glyphs_by_layer: std::collections::BTreeMap<u32, cached_text::TextBatch> =
            std::collections::BTreeMap::new();
        let mut css_transformed_text_prims = PrimitiveBatch::new();
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

            // Constrain wrapping to the available text or clip width.
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
                    let baseline_y = text.y + text.half_leading + text.ascender;
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
                if let Ok(shadow_glyphs) = self.text_ctx.prepare_text_with_style(
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
                    let mut owned = text_clip::prepare_glyphs(text, shadow_glyphs, (width, height));
                    if let Some(affine) = text.css_affine {
                        cached_text::convert_affine(&mut owned, affine, scale_factor, text.z_index);
                        css_transformed_text_prims.merge(owned);
                    } else {
                        glyphs_by_layer
                            .entry(text.z_index)
                            .or_default()
                            .append(owned);
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
                Ok(glyphs) => {
                    tracing::trace!(
                        "render_tree_with_motion: prepared {} glyphs for '{}' (font={:?})",
                        glyphs.len(),
                        text.content,
                        font_name
                    );
                    let mut owned = text_clip::prepare_glyphs(text, glyphs, (width, height));
                    if let Some(affine) = text.css_affine {
                        cached_text::convert_affine(&mut owned, affine, scale_factor, text.z_index);
                        css_transformed_text_prims.merge(owned);
                    } else {
                        // Snap glyph origins to physical pixels for sharp atlas sampling.
                        for glyph in &mut owned.glyphs {
                            glyph.bounds[0] = glyph.bounds[0].round();
                            glyph.bounds[1] = glyph.bounds[1].round();
                        }
                        glyphs_by_layer
                            .entry(text.z_index)
                            .or_default()
                            .append(owned);
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
        let mut fg_glyphs = cached_text::TextBatch::default();
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
                    let baseline_y = text.y + text.half_leading + text.ascender;
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
                let mut owned = text_clip::prepare_glyphs(text, glyphs, (width, height));
                if let Some(affine) = text.css_affine {
                    cached_text::convert_affine(&mut owned, affine, scale_factor, text.z_index);
                }
                fg_glyphs.append(owned);
            }
        }

        // Prepare motion glyphs as primitives with their live affine and owning clip rows.
        let mut motion_subtree_text_prims = cached_text::TextBatch::default();
        let mut motion_subtree_text_patch: Vec<(
            std::ops::Range<usize>,
            blinc_layout::tree::StableNodeId,
        )> = Vec::new();
        for text in &motion_subtree_texts {
            let patch_start = motion_subtree_text_prims.primitives.len();
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
                    let baseline_y = text.y + text.half_leading + text.ascender;
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
                let mut owned = text_clip::prepare_glyphs(text, glyphs, (width, height));
                cached_text::convert_affine(
                    &mut owned,
                    text.css_affine.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
                    scale_factor,
                    text.z_index,
                );
                motion_subtree_text_prims.append(owned);
            }
            // Record the emitted range for the dispatch-time live
            // patch when this text sits under a composite-promoted
            // ancestor (its alpha was baked at BASE, factor excluded).
            let patch_end = motion_subtree_text_prims.primitives.len();
            if patch_end > patch_start {
                if let Some(ancestor) = text.composite_ancestor {
                    motion_subtree_text_patch.push((patch_start..patch_end, ancestor));
                }
            }
        }

        // Cache prepared text for replay. Motion text remains separate from the static
        // texture.
        self.cached_glyphs_by_layer = Some(glyphs_by_layer.clone());
        self.cached_fg_glyphs = Some(fg_glyphs.clone());
        self.cached_motion_subtree_text_prims = Some(motion_subtree_text_prims.clone());
        self.motion_subtree_text_patch = motion_subtree_text_patch;
        self.cached_motion_subtree_svgs = Some(motion_subtree_svgs.clone());
        self.cached_css_transformed_text_prims = Some(css_transformed_text_prims.clone());

        // Prepare foreground text decorations once for the dispatch branches below.
        let fg_decorations_by_layer = generate_text_decoration_primitives_by_layer(&fg_texts);

        tracing::trace!(
            "render_tree_with_motion: {} texts, {} fg texts, {} z-layers with glyphs, {} css-transformed",
            texts.len(),
            fg_texts.len(),
            glyphs_by_layer.len(),
            css_transformed_text_prims.primitives.len()
        );

        // SVGs are rendered as rasterized images (not tessellated paths) for better anti-aliasing
        // They will be rendered later via render_rasterized_svgs

        self.renderer.resize(width, height);

        // Bind the current atlas for CSS-transformed and canvas glyph primitives.
        if let (Some(atlas), Some(color_atlas)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            // The fast-path batch already contains these primitives and clip rows.
            if !used_fast_paint {
                batch.merge(css_transformed_text_prims);
            }
            self.renderer.set_glyph_atlas(atlas, color_atlas);
        }

        // Cache the emitted batch before dispatch changes its projected streams.
        tracing::trace!(
            target: "blinc_app::frame_timing",
            primitives = batch.primitives.len(),
            "cached_bg_batch_set",
        );
        self.cached_bg_batch = Some(batch.clone());
        if self.in_compositor_inner_call {
            batch.foreground_primitives.clear();
            batch.foreground_paths = Default::default();
            batch.foreground_layer_commands.clear();
            batch.foreground_dynamic_images.clear();
        }

        let has_glass = batch.glass_count() > 0;
        let has_layer_effects_in_batch = batch.has_layer_effects();
        let retained_group = tree.had_retained_canvas_painted()
            || tree
                .composite_bindings()
                .values()
                .any(|meta| !meta.dynamic_batch && !meta.primitive_range.is_empty());
        let clear = [0.0, 0.0, 0.0, self.clear_alpha as f64];
        let render_background = |renderer: &mut GpuRenderer, batch: &PrimitiveBatch| {
            if retained_group && batch.requires_ordered_rendering() {
                // Retained content can nest groups in the static owner. Replay
                // them together through the ordered single-sample compositor.
                renderer.clear_target(
                    target,
                    wgpu::Color {
                        r: clear[0],
                        g: clear[1],
                        b: clear[2],
                        a: clear[3],
                    },
                );
                renderer.render_overlay_msaa(target, batch, 1);
            } else {
                renderer.render_with_clear(target, batch, clear);
            }
        };

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
                // Render the backdrop, grouped content, images and glass in composition
                // order.
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
                render_background(&mut self.renderer, &batch);

                // Render dynamic images (video frames from draw_rgba_pixels)
                if !batch.dynamic_images.is_empty() {
                    self.renderer
                        .render_unordered_dynamic_images(target, &batch.dynamic_images);
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
            if batch.has_paths() && !batch.has_ordered_draws() {
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

            if has_layer_effects_in_batch {
                self.renderer.update_aux_data_for_batch(&batch);
                self.renderer.render_foreground(target, &batch);
            }

            // Interleaved z-layer rendering for proper text z-ordering in glass path
            let max_z = batch.max_z_layer();
            let max_text_z = glyphs_by_layer.keys().cloned().max().unwrap_or(0);
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            let max_decoration_z = decorations_by_layer.keys().cloned().max().unwrap_or(0);
            let max_glass_layer = max_z.max(max_text_z).max(max_decoration_z);

            // Render z=0 text first (before any z>0 primitives)
            if let Some(glyphs) = glyphs_by_layer.get(&0) {
                self.render_text(target, glyphs);
            }
            self.render_text_decorations_for_layer(target, &decorations_by_layer, 0);

            if max_glass_layer > 0 {
                let effect_indices = if batch.has_ordered_draws() {
                    (0..batch.primitives.len()).collect()
                } else {
                    batch.effect_layer_indices()
                };
                for z in 1..=max_glass_layer {
                    // Render primitives for this layer
                    let layer_primitives = if effect_indices.is_empty() {
                        batch.primitives_for_layer(z)
                    } else {
                        batch.primitives_for_layer_excluding_effects(z, &effect_indices)
                    };
                    if !layer_primitives.is_empty() {
                        self.renderer.update_aux_data_for_batch(&batch);
                        self.renderer
                            .render_primitives_overlay(target, &layer_primitives);
                    }

                    // Render text for this layer (interleaved for proper z-order)
                    if let Some(glyphs) = glyphs_by_layer.get(&z) {
                        self.render_text(target, glyphs);
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
            let has_layer_effects = batch.requires_ordered_rendering();

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
                // Preserve the shared auxiliary rows for primitive overlays and the
                // foreground streams drawn after glass.
                z0_batch.aux_data = batch.aux_data.clone();
                self.renderer.render_with_clear(
                    target,
                    &z0_batch,
                    [0.0, 0.0, 0.0, self.clear_alpha as f64],
                );

                // Render dynamic images (video frames)
                if !batch.dynamic_images.is_empty() {
                    self.renderer
                        .render_unordered_dynamic_images(target, &batch.dynamic_images);
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
                    // Draw this layer's primitives with the configured sample count.
                    let layer_primitives = batch.primitives_for_layer(z);
                    if !layer_primitives.is_empty() {
                        self.renderer.update_aux_data_for_batch(&batch);
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
                if batch.has_foreground_content() {
                    self.renderer.render_foreground(target, &batch);
                }

                // Render foreground text (inside foreground-layer elements, after foreground primitives)
                if !fg_glyphs.is_empty() {
                    self.render_text(target, &fg_glyphs);
                }
                for &z in fg_decorations_by_layer.keys() {
                    self.render_text_decorations_for_layer(target, &fg_decorations_by_layer, z);
                }
            } else {
                // Dispatch the complete batch so group commands and child ranges remain
                // together.
                render_background(&mut self.renderer, &batch);

                // Render dynamic images (video frames)
                if !batch.dynamic_images.is_empty() {
                    self.renderer
                        .render_unordered_dynamic_images(target, &batch.dynamic_images);
                }

                // Dispatch unordered paths separately with the configured sample count.
                if batch.has_paths() && !batch.has_ordered_draws() {
                    if self.sample_count > 1 {
                        self.renderer
                            .render_paths_overlay_msaa(target, &batch, self.sample_count);
                    } else {
                        self.renderer.render_paths_overlay(target, &batch);
                    }
                }

                self.render_images(target, &images, width as f32, height as f32, scale_factor);

                // Render foreground primitives (e.g. borders on top)
                if batch.has_foreground_content() {
                    self.renderer.render_foreground(target, &batch);
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
                    let effect_indices = if batch.has_ordered_draws() {
                        (0..batch.primitives.len()).collect()
                    } else {
                        batch.effect_layer_indices()
                    };
                    for z in 1..=max_layer {
                        // Render primitives for this z-layer
                        let layer_primitives = if effect_indices.is_empty() {
                            batch.primitives_for_layer(z)
                        } else {
                            batch.primitives_for_layer_excluding_effects(z, &effect_indices)
                        };
                        if !layer_primitives.is_empty() {
                            self.renderer.update_aux_data_for_batch(&batch);
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

        // Draw the dynamic owner after static content. The outer compositor owns this
        // dispatch during inner cache paints.
        if !self.in_compositor_inner_call {
            if let Some(ref dynamic_batch) = self.cached_dynamic_batch {
                if !dynamic_batch.primitives.is_empty()
                    || dynamic_batch.has_paths()
                    || dynamic_batch.has_ordered_draws()
                {
                    if dynamic_batch.requires_ordered_rendering() {
                        self.renderer.render_overlay_msaa(target, dynamic_batch, 1);
                    } else {
                        self.renderer.render_overlay(target, dynamic_batch);
                    }
                    // Restore static auxiliary rows before subsequent text and SVG
                    // dispatch.
                    self.renderer.update_aux_data_for_batch(&batch);
                }
                self.renderer.render_foreground(target, dynamic_batch);
                self.renderer.update_aux_data_for_batch(&batch);
            }
        }

        // Draw motion text and SVG after their dynamic geometry.
        self.dispatch_motion_subtree_overlay_pools(tree, target);

        // Draw promoted backgrounds before separately dispatched motion text and SVG.
        if !self.in_compositor_inner_call {
            self.composite_css_layers_overlay(tree, target);
            // Replay promoted motion textures.
            self.composite_motion_layers_overlay(tree, target);
        }

        // Dispatch captured mesh output after 2D content, using each mesh's camera,
        // lighting and viewport.
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
}

/// Debug overlays selected by BLINC_DEBUG: text, layout, motion or all.
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
