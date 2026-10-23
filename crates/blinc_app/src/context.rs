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

mod element_collection;
mod frame_dispatch;
mod frame_text;
mod motion_rendering;
mod node_content;

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
