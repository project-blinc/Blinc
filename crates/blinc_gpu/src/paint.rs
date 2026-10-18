//! Paint Context - GPU-backed DrawContext implementation
//!
//! This module provides `GpuPaintContext`, a GPU-accelerated implementation of
//! the `DrawContext` trait that translates drawing commands into GPU primitives
//! for efficient rendering.
//!
//! # Architecture
//!
//! ```text
//! DrawContext commands
//!        │
//!        ▼
//! ┌─────────────────┐
//! │ GpuPaintContext │  ← Translates commands to GPU primitives
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  PrimitiveBatch │  ← Batched GPU-ready data
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │   GpuRenderer   │  ← Executes render passes
//! └─────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use blinc_gpu::GpuPaintContext;
//! use blinc_core::{DrawContext, DrawContextExt, Color, Rect};
//!
//! let mut ctx = GpuPaintContext::new(800.0, 600.0);
//!
//! // Draw using the DrawContext API
//! ctx.fill_rect(Rect::new(10.0, 10.0, 100.0, 50.0), 8.0.into(), Color::BLUE.into());
//!
//! // Get the batched primitives for GPU rendering
//! let batch = ctx.take_batch();
//! renderer.render(&target, &batch);
//! ```

use blinc_core::{
    Affine2D, BillboardFacing, BlendMode, Brush, Camera, ClipShape, Color, CornerRadius,
    CubemapData, DrawCommand, DrawContext, Environment, FillRule, ImageId, ImageOptions,
    LayerConfig, LayerId, Light, Mat4, MaterialId, MeshData, MeshId, MeshInstance,
    ParticleBlendMode, ParticleEmitterShape, ParticleForce, ParticleSystemData, Path, Point, Rect,
    Sdf3DViewport, SdfBuilder, Shadow, ShapeId, Size, Stroke, TextStyle, Transform,
};

/// Set in a polygon clip's packed vertex count when it is filled even-odd.
const POLYGON_EVEN_ODD: u32 = 1 << 31;

use crate::path::{
    DEFAULT_TESSELLATION_TOLERANCE, extract_brush_info, tessellate_fill_with_tolerance,
    tessellate_stroke_with_tolerance,
};
use crate::primitives::{
    ClipType, FillType, GlassType, GpuGlassPrimitive, GpuPrimitive, PrimitiveBatch, PrimitiveType,
    Sdf3DUniform, Viewport3D,
};
use crate::text::TextRenderingContext;

// ─────────────────────────────────────────────────────────────────────────────
// Transform Stack
// ─────────────────────────────────────────────────────────────────────────────

/// Combined 2D transform state (for future optimization)
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct TransformState {
    /// Combined affine transform
    affine: Affine2D,
    /// Combined opacity
    opacity: f32,
    /// Current blend mode
    blend_mode: BlendMode,
}

impl Default for TransformState {
    fn default() -> Self {
        Self {
            affine: Affine2D::IDENTITY,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layer Stack
// ─────────────────────────────────────────────────────────────────────────────

/// State for a single layer in the stack
///
/// Tracks the configuration and rendering state when a layer is pushed,
/// so it can be properly restored when the layer is popped.
#[derive(Clone, Debug)]
struct LayerState {
    /// The layer configuration
    config: LayerConfig,
    /// Starting primitive index when this layer was pushed
    primitive_start: usize,
    /// Starting foreground primitive index
    foreground_primitive_start: usize,
    /// Starting path vertex index
    path_start: usize,
    /// Starting foreground path vertex index
    foreground_path_start: usize,
    /// Parent state stack indices (transform, opacity, blend, clip)
    parent_state_indices: (usize, usize, usize, usize),
}

/// Clip stack entry: (shape, optional polygon aux_data metadata (aux_offset, vertex_count), overflow_fade)
type ClipStackEntry = (ClipShape, Option<(u32, u32)>, [f32; 4]);

// ─────────────────────────────────────────────────────────────────────────────
// Pending 3D Mesh Draw
// ─────────────────────────────────────────────────────────────────────────────

/// A 3D mesh draw captured inside a canvas callback.
///
/// `GpuPaintContext` can't invoke `GpuRenderer::render_mesh_data` directly
/// from inside `DrawContext::draw_mesh_data` — the paint context is a
/// batch builder and has no handle to the renderer. So instead the
/// override records the call (mesh data, model transform, a snapshot of
/// the active camera + lights) and the outer render loop drains the list
/// after `take_batch` and dispatches to `renderer.render_mesh_data` with
/// the frame's real target. See `GpuPaintContext::take_pending_meshes`.
///
/// Fields are `pub` so the caller in `blinc_app::context` can read them
/// without going through accessors. Don't construct these directly —
/// go through `DrawContext::draw_mesh_data`.
#[derive(Clone, Debug)]
pub struct PendingMesh {
    /// Mesh geometry + PBR material. `Arc` so the capture is cheap and
    /// the same mesh can be drawn at multiple transforms without cloning
    /// the vertex/index buffers each time.
    pub mesh: std::sync::Arc<MeshData>,
    /// Model transform (world-space placement of this instance).
    pub transform: Mat4,
    /// Snapshot of the active camera at draw time. The outer render
    /// loop turns this into a view-projection matrix using the frame's
    /// actual viewport size so aspect stays correct under resizes.
    pub camera: Camera,
    /// Snapshot of the active lights at draw time. For MVP, only the
    /// first `Light::Directional` contributes (shadow pass disabled);
    /// point / spot / ambient lights are ignored until the mesh
    /// pipeline grows support for them.
    pub lights: Vec<Light>,
    /// Screen-space viewport rect in physical pixels [x, y, w, h].
    /// When `Some`, the renderer applies `set_viewport` + `set_scissor_rect`
    /// to clip the mesh to this region. When `None`, the mesh renders to
    /// the full frame target.
    pub viewport: Option<[f32; 4]>,
    /// Pre-generated environment cubemap for IBL reflections. When `Some`,
    /// the renderer uploads this data to the cubemap texture (if it differs
    /// from what is currently bound). When `None`, the renderer's default
    /// neutral gray fallback is used.
    pub env_cubemap: Option<std::sync::Arc<CubemapData>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Pending Custom GPU Pass
// ─────────────────────────────────────────────────────────────────────────────

/// A user-defined GPU pass captured inside a canvas closure via
/// [`blinc_core::draw::DrawContext::run_gpu_pass`].
///
/// Same lifecycle pattern as [`PendingMesh`]: the paint context can't
/// dispatch directly (it has no renderer handle), so the override
/// records the pass + the canvas viewport here and the outer render
/// loop drains the list after `take_batch` and invokes each pass with
/// the frame's real device / queue / target.
///
/// `pass` carries an `Arc<Mutex<_>>` internally, so cloning + dispatch
/// don't fight the user's `Fn` canvas closure.
#[derive(Clone)]
pub struct PendingGpuPass {
    /// The wrapped pass. Cloning is cheap (Arc bump). The dispatch site
    /// calls [`crate::GpuPass::initialize_and_render`] which lazy-inits
    /// on first frame.
    pub pass: crate::custom_pass::GpuPass,
    /// Canvas viewport rect in physical pixels `[x, y, w, h]`. `Some`
    /// when the pass was scheduled inside a canvas with bounds (the
    /// typical case via `set_3d_viewport_bounds`); `None` for full-frame
    /// dispatch. The renderer plumbs this through `RenderPassContext::viewport`
    /// so the pass can `set_viewport` + `set_scissor_rect` to clip to
    /// the canvas region.
    pub viewport: Option<[f32; 4]>,
}

// ─────────────────────────────────────────────────────────────────────────────
// GPU Paint Context
// ─────────────────────────────────────────────────────────────────────────────

/// GPU-backed implementation of DrawContext
///
/// This translates high-level drawing commands into GPU primitives that can
/// be efficiently rendered by the `GpuRenderer`.
pub struct GpuPaintContext<'a> {
    /// Batched primitives ready for GPU rendering. Holds the STATIC
    /// content of the scene — everything not inside a motion-bound
    /// subtree. Painted into the compositor's static cache; survives
    /// across motion-binding frames untouched.
    batch: PrimitiveBatch,
    /// Batched primitives for motion-bound subtrees. Emit destination
    /// switches to this batch when `motion_subtree_depth > 0`. Not
    /// painted into the static cache — dispatched per-frame as an
    /// overlay after the cache blit so apply_binding_deltas can patch
    /// it in-place without invalidating the cache.
    dynamic_batch: PrimitiveBatch,
    /// How deep the walker currently is inside motion-bound subtrees.
    /// Incremented on `push_motion_subtree`, decremented on
    /// `pop_motion_subtree`. While > 0, emit methods route to
    /// `dynamic_batch` instead of `batch`. Counter rather than bool
    /// so nested motion bindings (rare but possible) still route
    /// correctly.
    motion_subtree_depth: u32,
    /// How deep the walker currently is inside `is_overlay_root`
    /// subtrees specifically (portals / popovers / dropdowns /
    /// dialogs / toasts). Tracked separately from
    /// `motion_subtree_depth` because the renderer uses it to bucket
    /// `CanvasPaintRecord`s into a post-dyn-batch dispatch so a
    /// canvas-emitted caret inside a focused cn::input lives ABOVE
    /// the popover's bg/border in z-order. A motion-bound canvas
    /// outside an overlay keeps its existing pre-dyn-batch
    /// canvas-overlay slot.
    overlay_subtree_depth: u32,
    /// Per-node scratch batches for composite-promotable CSS-animated
    /// subtrees. When [`Self::push_composite_layer`] sets
    /// `active_composite_layer = Some(node)`, every subsequent emit
    /// (until the matching `pop`) routes into
    /// `composite_layer_batches[node]` instead of the bg batch.
    ///
    /// Each batch is rasterized at full paint into its own
    /// `LayerTexture` (Phase 3) and composited per frame with the
    /// active CSS animation transform (Phase 4) — the texture is
    /// the cached pixel-form of the subtree at scale=1, no animation
    /// transform applied.
    composite_layer_batches: std::collections::HashMap<u64, PrimitiveBatch>,
    /// Active composite-layer routing target. `Some(node_id)` while
    /// the walker is inside a composite-promoted CSS subtree;
    /// `None` outside. For now, nesting is not supported — we treat
    /// the outermost promoted subtree as authoritative and any
    /// nested promotion's emits also route there (acceptable for
    /// first cut because the CSS animation values are evaluated
    /// at each visit during full paint regardless).
    active_composite_layer: Option<u64>,
    /// Stack of clip-stack depths captured by each active
    /// [`Self::push_composite_layer`]. The outermost active push
    /// determines the "ancestor clip base" — `clip_stack[..base]`
    /// were inherited from ancestors and are EXCLUDED from per-
    /// primitive clip computation while emission is routed to the
    /// outer scratch batch. Inner pushes for nested promoted
    /// subtrees stack on top; their own intrinsic clips (anything
    /// pushed AFTER their own snapshot) remain visible to the
    /// emitter. The bake site captures the outermost base as the
    /// texture's "ambient clip" and the per-frame blit re-applies
    /// it via the blit shader's scissor.
    ///
    /// Single-slot `Option<usize>` was insufficient because nested
    /// motion promotions inside one widget (e.g. `cn::switch`'s
    /// `on_layer` and `animated_thumb` motion siblings, or any
    /// promoted ancestor + promoted descendant pair) caused the
    /// inner pop to clear the outer push's state — leaving the
    /// outer subtree's later emissions with the wrong clip base
    /// and the wrong scratch routing implicit assumptions.
    composite_layer_clip_base: Vec<usize>,
    /// Transform stack
    transform_stack: Vec<Affine2D>,
    /// Opacity stack
    opacity_stack: Vec<f32>,
    /// Blend mode stack
    blend_mode_stack: Vec<BlendMode>,
    /// Clip stack (for tracking, actual clipping done in shader)
    clip_stack: Vec<ClipStackEntry>,
    /// Viewport size
    viewport: Size,
    /// Whether we're in a 3D context
    is_3d: bool,
    /// Current camera (for 3D mode)
    camera: Option<Camera>,
    /// Lights for 3D rendering
    lights: Vec<Light>,
    /// Text rendering context (optional, for draw_text support)
    text_ctx: Option<&'a mut TextRenderingContext>,
    /// Whether we're rendering to the foreground layer (after glass)
    is_foreground: bool,
    /// Current z-layer for interleaved rendering (used by Stack for proper z-ordering)
    z_layer: u32,
    /// Stack of active layers for offscreen rendering
    layer_stack: Vec<LayerState>,
    // 3D transform transient fields (set per-element, reset after)
    current_3d_sin_ry: f32,
    current_3d_cos_ry: f32,
    current_3d_sin_rx: f32,
    current_3d_cos_rx: f32,
    current_3d_perspective_d: f32,
    current_3d_shape_type: f32,
    current_3d_depth: f32,
    current_3d_ambient: f32,
    current_3d_specular: f32,
    current_3d_translate_z: f32,
    current_3d_light: [f32; 4],
    current_3d_group_shapes: Vec<crate::primitives::ShapeDesc>,
    // CSS filter transient fields (set per-element, reset after)
    current_filter_a: [f32; 4], // grayscale, invert, sepia, hue_rotate_rad
    current_filter_b: [f32; 4], // brightness, contrast, saturate, 0
    // Mask gradient transient fields (set per-element, reset after)
    current_mask_params: [f32; 4], // gradient geometry
    current_mask_info: [f32; 4],   // [mask_type, start_alpha, end_alpha, 0]
    // Corner shape transient fields (set per-element, reset after)
    current_corner_shape: [f32; 4], // superellipse n per corner (default [1.0; 4] = round)
    // Overflow fade: pending value consumed by next push_clip
    pending_overflow_fade: [f32; 4], // [top, right, bottom, left] in CSS pixels
    /// Fill rule of the next polygon clip, taken by `push_clip`.
    pending_clip_fill_rule: FillRule,
    /// 3D mesh draws captured during this frame. `draw_mesh_data` pushes
    /// here; the outer render loop drains with `take_pending_meshes`
    /// after `take_batch` and dispatches each to
    /// `GpuRenderer::render_mesh_data` against the frame's target.
    pending_meshes: Vec<PendingMesh>,
    /// 3D viewport bounds (logical pixels). Set by SceneKit3D before
    /// `draw_mesh_data` so the viewport rect can be computed from the
    /// transform stack position + these bounds.
    mesh_viewport_bounds: Option<(f32, f32)>,
    /// Environment cubemap data set by `set_environment_cubemap`. Captured
    /// into each `PendingMesh` so the renderer can upload it.
    pending_env: Option<std::sync::Arc<CubemapData>>,
    /// User-defined GPU passes scheduled via `run_gpu_pass`. Drained by
    /// the outer render loop with `take_pending_gpu_passes` and
    /// dispatched alongside (and using the same lifecycle as) the
    /// pending-mesh path.
    pending_gpu_passes: Vec<PendingGpuPass>,
}

impl<'a> GpuPaintContext<'a> {
    /// Create a new GPU paint context
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            batch: PrimitiveBatch::new(),
            transform_stack: vec![Affine2D::IDENTITY],
            opacity_stack: vec![1.0],
            blend_mode_stack: vec![BlendMode::Normal],
            clip_stack: Vec::new(),
            viewport: Size::new(width, height),
            is_3d: false,
            camera: None,
            lights: Vec::new(),
            text_ctx: None,
            is_foreground: false,
            z_layer: 0,
            layer_stack: Vec::new(),
            current_3d_sin_ry: 0.0,
            current_3d_cos_ry: 1.0,
            current_3d_sin_rx: 0.0,
            current_3d_cos_rx: 1.0,
            current_3d_perspective_d: 0.0,
            current_3d_shape_type: 0.0,
            current_3d_depth: 0.0,
            current_3d_ambient: 0.3,
            current_3d_specular: 32.0,
            current_3d_translate_z: 0.0,
            current_3d_light: [0.0, -1.0, 0.5, 0.8],
            current_3d_group_shapes: Vec::new(),
            current_filter_a: [0.0, 0.0, 0.0, 0.0],
            current_filter_b: [1.0, 1.0, 1.0, 0.0],
            current_mask_params: [0.0; 4],
            current_mask_info: [0.0; 4],
            current_corner_shape: [1.0; 4],
            pending_overflow_fade: [0.0; 4],
            pending_clip_fill_rule: FillRule::NonZero,
            pending_meshes: Vec::new(),
            mesh_viewport_bounds: None,
            pending_env: None,
            pending_gpu_passes: Vec::new(),
            dynamic_batch: PrimitiveBatch::new(),
            motion_subtree_depth: 0,
            overlay_subtree_depth: 0,
            composite_layer_batches: std::collections::HashMap::new(),
            active_composite_layer: None,
            composite_layer_clip_base: Vec::new(),
        }
    }

    /// Set whether we're rendering to the foreground layer
    ///
    /// When true, primitives are pushed to the foreground batch (rendered after glass).
    /// When false (default), primitives go to the background batch.
    pub fn set_foreground(&mut self, is_foreground: bool) {
        self.is_foreground = is_foreground;
    }

    /// Create a new GPU paint context with text rendering support
    pub fn with_text_context(
        width: f32,
        height: f32,
        text_ctx: &'a mut TextRenderingContext,
    ) -> Self {
        Self {
            batch: PrimitiveBatch::new(),
            transform_stack: vec![Affine2D::IDENTITY],
            opacity_stack: vec![1.0],
            blend_mode_stack: vec![BlendMode::Normal],
            clip_stack: Vec::new(),
            viewport: Size::new(width, height),
            is_3d: false,
            camera: None,
            lights: Vec::new(),
            text_ctx: Some(text_ctx),
            is_foreground: false,
            z_layer: 0,
            layer_stack: Vec::new(),
            current_3d_sin_ry: 0.0,
            current_3d_cos_ry: 1.0,
            current_3d_sin_rx: 0.0,
            current_3d_cos_rx: 1.0,
            current_3d_perspective_d: 0.0,
            current_3d_shape_type: 0.0,
            current_3d_depth: 0.0,
            current_3d_ambient: 0.3,
            current_3d_specular: 32.0,
            current_3d_translate_z: 0.0,
            current_3d_light: [0.0, -1.0, 0.5, 0.8],
            current_3d_group_shapes: Vec::new(),
            current_filter_a: [0.0, 0.0, 0.0, 0.0],
            current_filter_b: [1.0, 1.0, 1.0, 0.0],
            current_mask_params: [0.0; 4],
            current_mask_info: [0.0; 4],
            current_corner_shape: [1.0; 4],
            pending_overflow_fade: [0.0; 4],
            pending_clip_fill_rule: FillRule::NonZero,
            pending_meshes: Vec::new(),
            mesh_viewport_bounds: None,
            pending_env: None,
            pending_gpu_passes: Vec::new(),
            dynamic_batch: PrimitiveBatch::new(),
            motion_subtree_depth: 0,
            overlay_subtree_depth: 0,
            composite_layer_batches: std::collections::HashMap::new(),
            active_composite_layer: None,
            composite_layer_clip_base: Vec::new(),
        }
    }

    /// Set the text rendering context
    pub fn set_text_context(&mut self, text_ctx: &'a mut TextRenderingContext) {
        self.text_ctx = Some(text_ctx);
    }

    /// Mark that the walker has entered a motion-bound subtree.
    /// All subsequent emit calls (primitives, paths, glass, etc.)
    /// route into `dynamic_batch` instead of `batch` until the
    /// matching `pop_motion_subtree`. Nested motion subtrees track
    /// via a depth counter — only the outermost pop returns to
    /// static-batch emission.
    pub fn push_motion_subtree(&mut self) {
        self.motion_subtree_depth = self.motion_subtree_depth.saturating_add(1);
    }

    /// Pair with [`Self::push_motion_subtree`]. No-op when depth
    /// is already zero (defensive — should never happen in
    /// well-formed walker traversal but avoids underflow if a
    /// future caller forgets balance).
    pub fn pop_motion_subtree(&mut self) {
        self.motion_subtree_depth = self.motion_subtree_depth.saturating_sub(1);
    }

    /// Mark that the walker has entered an `is_overlay_root` subtree
    /// (portal / popover / dropdown / dialog / toast). Tracked
    /// independently of `motion_subtree_depth` so the renderer can
    /// route canvases inside an overlay into a post-dyn-batch
    /// dispatch — keeping caret canvases above the popover bg/border
    /// their host input paints.
    pub fn push_overlay_subtree(&mut self) {
        self.overlay_subtree_depth = self.overlay_subtree_depth.saturating_add(1);
    }

    /// Pair with [`Self::push_overlay_subtree`]. Saturating sub so a
    /// stray pop never underflows.
    pub fn pop_overlay_subtree(&mut self) {
        self.overlay_subtree_depth = self.overlay_subtree_depth.saturating_sub(1);
    }

    /// `true` if the walker is currently inside one or more motion-
    /// bound subtrees. Read by the emit helpers to choose between
    /// `batch` (static) and `dynamic_batch`.
    #[inline]
    pub fn in_motion_subtree(&self) -> bool {
        self.motion_subtree_depth > 0
    }

    /// Begin routing subsequent emit calls into the per-node scratch
    /// batch for `node_id` instead of the bg batch.
    ///
    /// Used by the walker to capture a composite-promotable CSS
    /// subtree's primitives into its own buffer — at end of paint
    /// the buffer is rasterized into a `LayerTexture` and the
    /// per-frame composite blits that texture with the current
    /// animation transform applied. The bg batch never sees these
    /// primitives, so the static cache stays unchanged across
    /// animation ticks.
    ///
    /// Nested composite layers are not supported; the second push
    /// silently keeps routing to the outermost layer. Callers must
    /// pair each push with [`Self::pop_composite_layer`].
    pub fn push_composite_layer(&mut self, node_id: u64) {
        // Snapshot the clip-stack depth at promotion time. Any clip
        // the SUBTREE ITSELF pushes (e.g. an inner overflow_clip on
        // a child) stacks on top of this base and IS baked into the
        // texture; ancestor clips below the OUTERMOST `clip_base`
        // are stripped during emission and re-applied at blit time
        // via the motion-overlay scissor. See [`Self::get_clip_data`]
        // for the strip logic.
        //
        // We push the clip-base unconditionally so push/pop are
        // balanced, even for nested promotions. The active scratch
        // routing target (`active_composite_layer`) only updates
        // for the OUTERMOST push — nested promoted nodes still emit
        // into the outer's scratch batch, matching the
        // "no-nesting" routing contract while keeping clip-base
        // bookkeeping correct.
        self.composite_layer_clip_base.push(self.clip_stack.len());
        if self.active_composite_layer.is_none() {
            self.active_composite_layer = Some(node_id);
            // Pre-create the entry so emit sites see a stable
            // mutable handle and don't pay a hashmap lookup on
            // every primitive.
            self.composite_layer_batches.entry(node_id).or_default();
        }
    }

    /// Pair with [`Self::push_composite_layer`]. Pops the innermost
    /// clip-base snapshot; when that was the outermost push, also
    /// clears the active scratch routing target.
    pub fn pop_composite_layer(&mut self) {
        self.composite_layer_clip_base.pop();
        if self.composite_layer_clip_base.is_empty() {
            self.active_composite_layer = None;
        }
    }

    /// Return the AABB of the ancestor clips that were stripped
    /// from primitive emission while a composite layer was active.
    /// Returns `None` when no composite layer ever pushed in this
    /// paint, or when the layer was pushed at clip depth 0 (no
    /// ancestor clips to strip).
    ///
    /// Called from the per-frame motion overlay AFTER pop: the saved
    /// `composite_layer_clip_base` is consumed by the walker's record
    /// site (which copies it onto the `DynamicRegion`); this getter
    /// answers the snapshot itself for callers that need to peek mid-
    /// paint without copying the stack.
    pub fn ambient_clip_aabb(&self, clip_base: usize) -> Option<[f32; 4]> {
        self.ambient_clip_rounded(clip_base).map(|(aabb, _)| aabb)
    }

    /// Capture the ambient clip as both its intersected AABB AND the
    /// topmost rounded-rect corner radius if one exists in the
    /// ancestor stack. The compositor overlay uses this to drive the
    /// blit shader's rounded-rect scissor — without the radius, a
    /// motion-bound subtree blitted across a rounded ancestor (the
    /// progress indicator inside an overflow_clip-rounded track, the
    /// switch thumb inside a rounded pill) gets its corners squared
    /// off by an AABB scissor.
    ///
    /// Returns `(aabb_xywh, corner_radius_tl_tr_br_bl)`. The radius
    /// is `[0; 4]` when no rounded-rect clip is present in the
    /// ancestor chain (plain rects, polygons, no clip) — semantically
    /// "square corners," which the blit shader handles as a normal
    /// AABB scissor.
    ///
    /// For multiple stacked rounded-rect clips the TOPMOST one wins
    /// — same heuristic `get_clip_data` uses for its scissor's corner
    /// radius. Robust handling of arbitrary intersections is
    /// follow-up work; the topmost-wins fallback matches what users
    /// see when the bake path is OFF.
    pub fn ambient_clip_rounded(&self, clip_base: usize) -> Option<([f32; 4], [f32; 4])> {
        if clip_base == 0 {
            return None;
        }
        let mut min_x = f32::NEG_INFINITY;
        let mut min_y = f32::NEG_INFINITY;
        let mut max_x = f32::INFINITY;
        let mut max_y = f32::INFINITY;
        let mut any = false;
        // Walk top-down so the LAST RoundedRect we see is the
        // outermost — but we want the TOPMOST (innermost), which is
        // the LAST entry in the iteration order. Track the most-
        // recent rounded-rect we've seen and let later iterations
        // overwrite it.
        let mut topmost_rounded: Option<([f32; 4], [f32; 4])> = None;
        for (shape, _, _) in &self.clip_stack[..clip_base] {
            let (x0, y0, x1, y1) = match shape {
                ClipShape::Rect(r) => (r.x(), r.y(), r.x() + r.width(), r.y() + r.height()),
                ClipShape::RoundedRect {
                    rect,
                    corner_radius,
                    ..
                } => {
                    let bounds = [rect.x(), rect.y(), rect.width(), rect.height()];
                    let radii = [
                        corner_radius.top_left,
                        corner_radius.top_right,
                        corner_radius.bottom_right,
                        corner_radius.bottom_left,
                    ];
                    topmost_rounded = Some((bounds, radii));
                    (
                        rect.x(),
                        rect.y(),
                        rect.x() + rect.width(),
                        rect.y() + rect.height(),
                    )
                }
                _ => continue,
            };
            min_x = min_x.max(x0);
            min_y = min_y.max(y0);
            max_x = max_x.min(x1);
            max_y = max_y.min(y1);
            any = true;
        }
        if !any || max_x <= min_x || max_y <= min_y {
            return None;
        }
        let aabb = [min_x, min_y, max_x - min_x, max_y - min_y];
        // If a rounded-rect exists in the ancestor chain AND the
        // intersected AABB equals its bounds, return its radii. When
        // the intersection trims the rounded-rect's edges (a tighter
        // ancestor rect cuts into it), the rounded corners may no
        // longer be at the intersection's edge — fall back to square
        // scissor in that case. This matches `get_clip_data`'s
        // dominant-corner logic conservatively without re-implementing
        // it here.
        let radius = match topmost_rounded {
            Some((bounds, radii)) if aabb == bounds => radii,
            _ => [0.0; 4],
        };
        Some((aabb, radius))
    }

    /// Outermost active composite-layer clip-base snapshot — the
    /// ancestor-clip cutoff that applies for primitives emitted
    /// inside the outermost push. Used by the walker to record the
    /// ambient clip on `DynamicRegion::MotionSubtreeTexture` and by
    /// the strip-clip path in `get_clip_data`. Returns `None`
    /// outside a composite-layer scope.
    pub fn composite_layer_clip_base(&self) -> Option<usize> {
        self.composite_layer_clip_base.first().copied()
    }

    /// Drain the per-node composite-layer scratch batches. Called
    /// after the walker finishes; the compositor reads each batch's
    /// primitives + AABB to rasterize it into a `LayerTexture`.
    pub fn take_composite_layer_batches(
        &mut self,
    ) -> std::collections::HashMap<u64, PrimitiveBatch> {
        std::mem::take(&mut self.composite_layer_batches)
    }

    /// The active batch for emission. While inside a composite-
    /// layer scope, points at the per-node scratch batch; otherwise
    /// while inside a motion-bound subtree, points at `dynamic_batch`;
    /// otherwise at `batch`. Internal helper used by every primitive /
    /// path / glass emit site so the routing is centralised.
    #[inline]
    fn active_batch_mut(&mut self) -> &mut PrimitiveBatch {
        if let Some(node_id) = self.active_composite_layer {
            self.composite_layer_batches.entry(node_id).or_default()
        } else if self.motion_subtree_depth > 0 {
            &mut self.dynamic_batch
        } else {
            &mut self.batch
        }
    }

    /// Effective foreground routing for primitive emission.
    ///
    /// Honors `is_foreground` normally, but forces BACKGROUND routing
    /// while a composite layer is active. Inside a composite-promoted
    /// subtree the whole scratch batch is baked to one texture and
    /// blitted as a unit, so the global foreground/background split is
    /// meaningless there — and worse, routing to
    /// `foreground_primitives` strands those prims: the region bracket
    /// (`bg_primitive_count`), its AABB (`bg_primitive_aabb`) and the
    /// bake (`render_subtree_to_layer_texture`) all read only
    /// `.primitives`. A `.foreground()` panel with a CSS enter
    /// animation would otherwise emit its bg/border/shadow into
    /// `foreground_primitives`, record an empty region (end == start),
    /// never bake, and never blit — the exact select/combobox fade
    /// break. Collapsing to `.primitives` while promoted makes the
    /// existing single-list bracket + bake work unchanged.
    #[inline]
    fn emit_foreground(&self) -> bool {
        self.is_foreground && self.active_composite_layer.is_none()
    }

    /// Immutable counterpart to [`Self::active_batch_mut`] — used
    /// by readers like `bg_primitive_count` and `bg_primitive_aabb`
    /// so they bracket the right batch.
    #[inline]
    fn active_batch(&self) -> &PrimitiveBatch {
        if let Some(node_id) = self.active_composite_layer {
            // If push happened, the entry must exist; fall back to
            // `batch` defensively so reads don't panic on an
            // unbalanced API call.
            self.composite_layer_batches
                .get(&node_id)
                .unwrap_or(&self.batch)
        } else if self.motion_subtree_depth > 0 {
            &self.dynamic_batch
        } else {
            &self.batch
        }
    }

    /// Drain the dynamic batch and return it. The caller (compositor
    /// fast path) dispatches it per frame on top of the static cache
    /// blit. Leaves `dynamic_batch` empty for the next paint pass.
    pub fn take_dynamic_batch(&mut self) -> PrimitiveBatch {
        std::mem::take(&mut self.dynamic_batch)
    }

    /// Get the current transform
    fn current_affine(&self) -> Affine2D {
        self.transform_stack
            .last()
            .copied()
            .unwrap_or(Affine2D::IDENTITY)
    }

    /// Get the current combined opacity
    fn combined_opacity(&self) -> f32 {
        self.opacity_stack.iter().product()
    }

    /// Transform a point by the current transform
    fn transform_point(&self, p: Point) -> Point {
        let affine = self.current_affine();
        // elements = [a, b, c, d, tx, ty]
        // | a  c  tx |   | x |
        // | b  d  ty | * | y |
        // | 0  0   1 |   | 1 |
        Point::new(
            affine.elements[0] * p.x + affine.elements[2] * p.y + affine.elements[4],
            affine.elements[1] * p.x + affine.elements[3] * p.y + affine.elements[5],
        )
    }

    /// Extract the uniform scale factor from the current transform.
    /// This accounts for DPI scaling and any CSS transforms.
    fn current_uniform_scale(&self) -> f32 {
        let affine = self.current_affine();
        let [a, b, c, d, ..] = affine.elements;
        let det = a * d - b * c;
        det.abs().sqrt().max(1e-6)
    }
}

/// Scale-aware tessellation tolerance.
///
/// The path tessellators flatten curves with a tolerance measured
/// in path-space units. When the path is later drawn at a larger
/// transform scale (canvas zoom, CSS `transform: scale()`, DPI
/// scale), one path-unit of tolerance becomes `scale` screen-pixel
/// units of deviation — at large zoom, polylines visibly
/// polygonalise. Dividing the default tolerance by `scale` keeps
/// the screen-space deviation roughly constant ≈ 0.2 px regardless
/// of zoom level. The cost is more triangles when zoomed in, which
/// is exactly the trade you want for crisp curves.
fn scale_aware_tolerance(scale: f32) -> f32 {
    (DEFAULT_TESSELLATION_TOLERANCE / scale.max(1e-3)).max(0.005)
}

impl<'a> GpuPaintContext<'a> {
    // Stub impl block to keep the original `impl` open below — the
    // helper above had to live at module scope so it can be called
    // from `fill_path` / `stroke_path` without the borrow checker
    // tripping on `&mut self.batch` simultaneous with `&self`.

    /// Transform a rect by the current transform (rotation+skew safe)
    ///
    /// Transforms the center of the rect through the full affine. Uses the
    /// determinant-based uniform scale for dimensions so that skew transforms
    /// don't inflate the bounds (the local_affine carries the full 2x2 to the shader).
    fn transform_rect(&self, rect: Rect) -> Rect {
        let affine = self.current_affine();
        let [a, b, c, d, ..] = affine.elements;

        // Uniform scale = sqrt(|det|) — extracts DPI + any uniform element scale.
        // This is exact for area-preserving transforms (rotation, skew) and a good
        // approximation for non-uniform scales.
        let det = a * d - b * c;
        let uniform_scale = det.abs().sqrt().max(1e-6);

        // Transform the CENTER (not origin)
        let center = Point::new(
            rect.origin.x + rect.size.width * 0.5,
            rect.origin.y + rect.size.height * 0.5,
        );
        let tc = self.transform_point(center);
        let sw = rect.size.width * uniform_scale;
        let sh = rect.size.height * uniform_scale;

        Rect::new(tc.x - sw * 0.5, tc.y - sh * 0.5, sw, sh)
    }

    /// Extract rotation sin/cos from the current affine transform
    ///
    /// Returns `[sin_rz, cos_rz, sin_ry, cos_ry]` ready for GpuPrimitive.rotation.
    /// Derives sin/cos directly from affine components without atan2.
    /// The Y rotation slots are filled from the 3D transient state.
    fn current_rotation_sincos(&self) -> [f32; 4] {
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let scale = (a * a + b * b).sqrt();
        if scale < 1e-6 {
            return [0.0, 1.0, self.current_3d_sin_ry, self.current_3d_cos_ry];
        }
        [
            b / scale,
            a / scale,
            self.current_3d_sin_ry,
            self.current_3d_cos_ry,
        ]
    }

    /// Get the DPI scale factor from the current affine transform.
    /// On Retina 2x displays this returns ~2.0, on 1x displays ~1.0.
    /// Used to scale 3D parameters (depth, perspective_d, translate_z) from
    /// logical/CSS pixels to physical pixels to match prim.bounds.
    fn current_dpi_scale(&self) -> f32 {
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();
        (scale_x + scale_y) * 0.5
    }

    /// Extract the normalized local 2x2 affine [a, b, c, d] from the current transform.
    ///
    /// This removes the uniform scale (DPI + uniform element scale) so that the
    /// remaining 2x2 captures rotation, skew, and non-uniform scale ratios.
    /// The shader uses this to apply the full inverse transform to sample points,
    /// enabling correct SDF evaluation for skewed/rotated elements.
    fn current_local_affine(&self) -> [f32; 4] {
        let affine = self.current_affine();
        let [a, b, c, d, ..] = affine.elements;
        let det = a * d - b * c;
        let uniform_scale = det.abs().sqrt().max(1e-6);
        [
            a / uniform_scale,
            b / uniform_scale,
            c / uniform_scale,
            d / uniform_scale,
        ]
    }

    /// Get the current 3D perspective params for GpuPrimitive.perspective.
    /// perspective_d is scaled to physical pixels to match prim.bounds.
    fn current_perspective_params(&self) -> [f32; 4] {
        let scale = self.current_dpi_scale();
        [
            self.current_3d_sin_rx,
            self.current_3d_cos_rx,
            self.current_3d_perspective_d * scale,
            self.current_3d_shape_type,
        ]
    }

    /// Get the current 3D SDF params for GpuPrimitive.sdf_3d.
    /// depth and translate_z are scaled to physical pixels to match prim.bounds.
    fn current_sdf_3d_params(&self) -> [f32; 4] {
        let scale = self.current_dpi_scale();
        [
            self.current_3d_depth * scale,
            self.current_3d_ambient,
            self.current_3d_specular,
            self.current_3d_translate_z * scale,
        ]
    }

    /// Get the current 3D light params for GpuPrimitive.light
    fn current_light_params(&self) -> [f32; 4] {
        self.current_3d_light
    }

    /// Set 3D rotation and perspective for the current element
    pub fn set_3d_transform(&mut self, rx_rad: f32, ry_rad: f32, perspective_d: f32) {
        self.current_3d_sin_rx = rx_rad.sin();
        self.current_3d_cos_rx = rx_rad.cos();
        self.current_3d_sin_ry = ry_rad.sin();
        self.current_3d_cos_ry = ry_rad.cos();
        self.current_3d_perspective_d = perspective_d;
    }

    /// Set 3D shape parameters for the current element
    pub fn set_3d_shape(&mut self, shape_type: f32, depth: f32, ambient: f32, specular: f32) {
        self.current_3d_shape_type = shape_type;
        self.current_3d_depth = depth;
        self.current_3d_ambient = ambient;
        self.current_3d_specular = specular;
    }

    /// Set 3D light parameters for the current element
    pub fn set_3d_light(&mut self, direction: [f32; 3], intensity: f32) {
        self.current_3d_light = [direction[0], direction[1], direction[2], intensity];
    }

    /// Set translate-z offset for the current 3D element
    pub fn set_3d_translate_z(&mut self, z: f32) {
        self.current_3d_translate_z = z;
    }

    /// Set group shape descriptors for compound 3D rendering
    pub fn set_3d_group(&mut self, shapes: &[crate::primitives::ShapeDesc]) {
        self.current_3d_group_shapes = shapes.to_vec();
    }

    /// Reset 3D transient state to defaults (call after rendering each element)
    pub fn clear_3d(&mut self) {
        self.current_3d_sin_ry = 0.0;
        self.current_3d_cos_ry = 1.0;
        self.current_3d_sin_rx = 0.0;
        self.current_3d_cos_rx = 1.0;
        self.current_3d_perspective_d = 0.0;
        self.current_3d_shape_type = 0.0;
        self.current_3d_depth = 0.0;
        self.current_3d_ambient = 0.3;
        self.current_3d_specular = 32.0;
        self.current_3d_translate_z = 0.0;
        self.current_3d_light = [0.0, -1.0, 0.5, 0.8];
        self.current_3d_group_shapes.clear();
    }

    /// Set CSS filter parameters for the current element
    #[allow(clippy::too_many_arguments)]
    pub fn set_css_filter(
        &mut self,
        grayscale: f32,
        invert: f32,
        sepia: f32,
        hue_rotate_deg: f32,
        brightness: f32,
        contrast: f32,
        saturate: f32,
    ) {
        self.current_filter_a = [grayscale, invert, sepia, hue_rotate_deg.to_radians()];
        self.current_filter_b = [brightness, contrast, saturate, 0.0];
    }

    /// Reset CSS filter state to identity (call after rendering each element)
    pub fn clear_css_filter(&mut self) {
        self.current_filter_a = [0.0, 0.0, 0.0, 0.0];
        self.current_filter_b = [1.0, 1.0, 1.0, 0.0];
    }

    /// Set mask gradient parameters for the current element
    pub fn set_mask_gradient(&mut self, params: [f32; 4], info: [f32; 4]) {
        self.current_mask_params = params;
        self.current_mask_info = info;
    }

    /// Reset mask gradient state
    pub fn clear_mask_gradient(&mut self) {
        self.current_mask_params = [0.0; 4];
        self.current_mask_info = [0.0; 4];
    }

    /// Set corner shape (superellipse n per corner)
    pub fn set_corner_shape_values(&mut self, shape: [f32; 4]) {
        self.current_corner_shape = shape;
    }

    /// Reset corner shape to round (default)
    pub fn clear_corner_shape_values(&mut self) {
        self.current_corner_shape = [1.0; 4];
    }

    /// Set pending overflow fade distances (consumed by next push_clip)
    pub fn set_overflow_fade_values(&mut self, fade: [f32; 4]) {
        self.pending_overflow_fade = fade;
    }

    /// Get the current clip fade from the clip stack
    /// Returns the topmost non-zero fade, scaled by DPI
    fn get_clip_fade(&self) -> [f32; 4] {
        for (_clip, _poly_meta, fade) in self.clip_stack.iter().rev() {
            if fade[0] > 0.0 || fade[1] > 0.0 || fade[2] > 0.0 || fade[3] > 0.0 {
                // Fade distances are in CSS pixels; scale by transform
                let affine = self.current_affine();
                let sx = (affine.elements[0] * affine.elements[0]
                    + affine.elements[1] * affine.elements[1])
                    .sqrt();
                let sy = (affine.elements[2] * affine.elements[2]
                    + affine.elements[3] * affine.elements[3])
                    .sqrt();
                return [
                    fade[0] * sy, // top
                    fade[1] * sx, // right
                    fade[2] * sy, // bottom
                    fade[3] * sx, // left
                ];
            }
        }
        [0.0; 4]
    }

    /// Scale corner radius by the current transform's average scale factor
    fn scale_corner_radius(&self, corner_radius: CornerRadius) -> CornerRadius {
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();
        let avg_scale = (scale_x + scale_y) / 2.0;

        CornerRadius::new(
            corner_radius.top_left * avg_scale,
            corner_radius.top_right * avg_scale,
            corner_radius.bottom_right * avg_scale,
            corner_radius.bottom_left * avg_scale,
        )
    }

    /// Transform gradient parameters by the current transform
    /// For linear gradients, transforms (x1, y1, x2, y2) to screen space
    /// For radial gradients, transforms (cx, cy, radius, 0) to screen space
    /// Convert ObjectBoundingBox gradient coords (0..1) to local rect pixel coords.
    fn obb_to_rect_coords(
        brush: &Brush,
        params: [f32; 4],
        rect: Rect,
        fill_type: FillType,
    ) -> [f32; 4] {
        let is_obb = matches!(
            brush,
            Brush::Gradient(blinc_core::Gradient::Linear {
                space: blinc_core::GradientSpace::ObjectBoundingBox,
                ..
            }) | Brush::Gradient(blinc_core::Gradient::Radial {
                space: blinc_core::GradientSpace::ObjectBoundingBox,
                ..
            }) | Brush::Gradient(blinc_core::Gradient::Conic {
                space: blinc_core::GradientSpace::ObjectBoundingBox,
                ..
            })
        );
        if !is_obb || fill_type == FillType::Solid {
            return params;
        }
        let is_radial = fill_type == FillType::RadialGradient;
        if is_radial {
            [
                rect.x() + params[0] * rect.width(),
                rect.y() + params[1] * rect.height(),
                params[2] * rect.width().max(rect.height()),
                params[3],
            ]
        } else {
            [
                rect.x() + params[0] * rect.width(),
                rect.y() + params[1] * rect.height(),
                rect.x() + params[2] * rect.width(),
                rect.y() + params[3] * rect.height(),
            ]
        }
    }

    fn transform_gradient_params(&self, params: [f32; 4], is_radial: bool) -> [f32; 4] {
        if is_radial {
            // Radial gradient: (cx, cy, radius, 0)
            let center = self.transform_point(Point::new(params[0], params[1]));
            // Scale radius by average scale factor
            let affine = self.current_affine();
            let a = affine.elements[0];
            let b = affine.elements[1];
            let c = affine.elements[2];
            let d = affine.elements[3];
            let scale_x = (a * a + b * b).sqrt();
            let scale_y = (c * c + d * d).sqrt();
            let avg_scale = (scale_x + scale_y) / 2.0;
            [center.x, center.y, params[2] * avg_scale, params[3]]
        } else {
            // Linear gradient: (x1, y1, x2, y2)
            let start = self.transform_point(Point::new(params[0], params[1]));
            let end = self.transform_point(Point::new(params[2], params[3]));
            [start.x, start.y, end.x, end.y]
        }
    }

    /// Transform a clip shape by the current transform
    /// Note: For rotated transforms, this computes the axis-aligned bounding box
    fn transform_clip_shape(&self, shape: ClipShape) -> ClipShape {
        let affine = self.current_affine();

        // Check if this is identity transform (common case)
        if affine == Affine2D::IDENTITY {
            return shape;
        }

        // Uniform scale factor extracted from the affine — captures DPI
        // scaling (Retina = ~2.0) plus any uniform element scale. Used
        // by the polygon branch below to bring element-local vertices
        // into physical pixels without applying rotation.
        let [a, b, c, d, ..] = affine.elements;
        let det = a * d - b * c;
        let uniform_scale = det.abs().sqrt().max(1e-6);

        match shape {
            ClipShape::Rect(rect) => {
                // Transform all four corners and compute AABB
                let corners = [
                    Point::new(rect.x(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y() + rect.height()),
                    Point::new(rect.x(), rect.y() + rect.height()),
                ];

                let transformed: Vec<Point> =
                    corners.iter().map(|p| self.transform_point(*p)).collect();

                let min_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::INFINITY, f32::min);
                let max_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                let min_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min);
                let max_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max);

                ClipShape::Rect(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
            }
            ClipShape::RoundedRect {
                rect,
                corner_radius,
                corner_shape,
            } => {
                // Transform corners and compute AABB
                let corners = [
                    Point::new(rect.x(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y()),
                    Point::new(rect.x() + rect.width(), rect.y() + rect.height()),
                    Point::new(rect.x(), rect.y() + rect.height()),
                ];

                let transformed: Vec<Point> =
                    corners.iter().map(|p| self.transform_point(*p)).collect();

                let min_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::INFINITY, f32::min);
                let max_x = transformed
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                let min_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min);
                let max_y = transformed
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max);

                // Scale the corner radii by the average scale factor
                let a = affine.elements[0];
                let b = affine.elements[1];
                let c = affine.elements[2];
                let d = affine.elements[3];
                let scale_x = (a * a + b * b).sqrt();
                let scale_y = (c * c + d * d).sqrt();
                let avg_scale = (scale_x + scale_y) * 0.5;

                let scaled_radius = CornerRadius::new(
                    corner_radius.top_left * avg_scale,
                    corner_radius.top_right * avg_scale,
                    corner_radius.bottom_right * avg_scale,
                    corner_radius.bottom_left * avg_scale,
                );

                ClipShape::RoundedRect {
                    rect: Rect::new(min_x, min_y, max_x - min_x, max_y - min_y),
                    corner_radius: scaled_radius,
                    corner_shape,
                }
            }
            ClipShape::Circle { center, radius } => {
                let transformed_center = self.transform_point(center);

                // For non-uniform scale, circle becomes ellipse - compute approximate radius
                let a = affine.elements[0];
                let b = affine.elements[1];
                let c = affine.elements[2];
                let d = affine.elements[3];
                let scale_x = (a * a + b * b).sqrt();
                let scale_y = (c * c + d * d).sqrt();

                if (scale_x - scale_y).abs() < 0.001 {
                    // Uniform scale - keep as circle
                    ClipShape::Circle {
                        center: transformed_center,
                        radius: radius * scale_x,
                    }
                } else {
                    // Non-uniform scale - convert to ellipse
                    ClipShape::Ellipse {
                        center: transformed_center,
                        radii: blinc_core::Vec2::new(radius * scale_x, radius * scale_y),
                    }
                }
            }
            ClipShape::Ellipse { center, radii } => {
                let transformed_center = self.transform_point(center);

                let a = affine.elements[0];
                let b = affine.elements[1];
                let c = affine.elements[2];
                let d = affine.elements[3];
                let scale_x = (a * a + b * b).sqrt();
                let scale_y = (c * c + d * d).sqrt();

                ClipShape::Ellipse {
                    center: transformed_center,
                    radii: blinc_core::Vec2::new(radii.x * scale_x, radii.y * scale_y),
                }
            }
            ClipShape::Path(path) => {
                // Path clipping with transforms not supported - keep as-is
                ClipShape::Path(path)
            }
            ClipShape::Polygon(pts) => {
                // Polygon vertices stay in element-local coords (no
                // rotation / translation) so the fragment shader's
                // `sp - prim.bounds.xy` test rotates with the element —
                // any rotation reflected in prim.rotation / local_affine
                // (CSS, motion-binding spring, timeline updated by
                // apply_binding_deltas) naturally rotates the polygon.
                //
                // BUT prim.bounds.xy is in physical pixels (DPI-scaled),
                // so element-local coords need to be in physical pixels
                // too — otherwise on Retina (2x) a 24-px logical element
                // would have local_p ranging 0..48 but vertices 0..24,
                // clipping the arc to the top-left quarter and making
                // most of the ring invisible. Apply only the uniform
                // (DPI) scale; skip the rotation portion of the affine.
                let scale = uniform_scale;
                let scaled: Vec<Point> = pts
                    .iter()
                    .map(|p| Point::new(p.x * scale, p.y * scale))
                    .collect();
                ClipShape::Polygon(scaled)
            }
        }
    }

    /// Convert a Brush to GPU color components and gradient parameters
    /// Returns (color1, color2, gradient_params, fill_type)
    /// Note: Glass brushes are handled separately in fill methods - this returns transparent
    fn brush_to_colors(&self, brush: &Brush) -> ([f32; 4], [f32; 4], [f32; 4], FillType) {
        let opacity = self.combined_opacity();
        match brush {
            Brush::Solid(color) => {
                let c = [color.r, color.g, color.b, color.a * opacity];
                // Default gradient params (not used for solid)
                (c, c, [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Glass(_) => {
                // Glass is handled via glass primitives, not regular primitives
                // Return transparent as a fallback (should never be used)
                ([0.0; 4], [0.0; 4], [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Image(_) => {
                // Image backgrounds are handled separately via the image pipeline
                // Return transparent as a fallback
                ([0.0; 4], [0.0; 4], [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Blur(_) => {
                // Blur is handled via glass primitives, not regular primitives
                // Return transparent as a fallback (should never be used)
                ([0.0; 4], [0.0; 4], [0.0, 0.0, 1.0, 0.0], FillType::Solid)
            }
            Brush::Gradient(gradient) => {
                let (stops, fill_type, gradient_params) = match gradient {
                    blinc_core::Gradient::Linear {
                        start, end, stops, ..
                    } => {
                        // Linear gradient: (x1, y1, x2, y2) in user space
                        (
                            stops,
                            FillType::LinearGradient,
                            [start.x, start.y, end.x, end.y],
                        )
                    }
                    blinc_core::Gradient::Radial {
                        center,
                        radius,
                        stops,
                        ..
                    } => {
                        // Radial gradient: (cx, cy, radius, 0) in user space
                        (
                            stops,
                            FillType::RadialGradient,
                            [center.x, center.y, *radius, 0.0],
                        )
                    }
                    // Conic gradients treated as radial for now
                    blinc_core::Gradient::Conic { center, stops, .. } => (
                        stops,
                        FillType::RadialGradient,
                        [center.x, center.y, 100.0, 0.0],
                    ),
                };

                let (c1, c2) = if stops.len() >= 2 {
                    let s1 = &stops[0];
                    let s2 = &stops[stops.len() - 1];
                    (
                        [s1.color.r, s1.color.g, s1.color.b, s1.color.a * opacity],
                        [s2.color.r, s2.color.g, s2.color.b, s2.color.a * opacity],
                    )
                } else if !stops.is_empty() {
                    let c = &stops[0].color;
                    let arr = [c.r, c.g, c.b, c.a * opacity];
                    (arr, arr)
                } else {
                    ([1.0, 1.0, 1.0, opacity], [1.0, 1.0, 1.0, opacity])
                };

                (c1, c2, gradient_params, fill_type)
            }
        }
    }

    /// Get clip data from the current clip stack.
    ///
    /// Returns `(clip_bounds, clip_radius, clip_corner_shape, clip_type)`.
    /// `clip_corner_shape` is a per-corner superellipse `n` parameter
    /// matching the [`CornerShape`] encoding (n=1.0 = round, 2.0 =
    /// squircle, 0.0 = bevel, 100.0 = square, -1.0 = scoop). When the
    /// active rounded clip was pushed with
    /// [`ClipShape::rounded_rect_shaped`], the shader's clip
    /// evaluator uses this to call `sd_shaped_rect` so the clip
    /// curve matches the parent's fill — otherwise the clip stays
    /// circular (default `[1.0; 4]`).
    ///
    /// For multiple rect clips, computes the intersection of all
    /// clips. For mixed clip types, uses the topmost clip
    /// (conservative approximation). Stacked RoundedRect clips with
    /// different `corner_shape` values currently collapse to the
    /// last-seen shape — typical UI use is a single squircle parent
    /// pushing `overflow:clip`, where this is exact.
    ///
    /// Corner radius handling: a rectangular clip (non-rounded) will
    /// reset the corner radius to 0 for any corners it covers, so a
    /// child with overflow_clip() doesn't inherit rounded corners
    /// from a parent.
    fn get_clip_data(&self) -> ([f32; 4], [f32; 4], [f32; 4], ClipType) {
        // P4.3 Option B clip-aware bake: while inside the OUTERMOST
        // composite layer, ancestor clips below the snapshot base
        // are stripped from per-primitive clip rects. They get
        // captured separately and re-applied at blit time by
        // `composite_*_layers_overlay` (the rounded radius is
        // threaded through via `current_clip_rounded` so flex-
        // siblings sitting inside a rounded-rect parent get their
        // scissor rounded too). Only clips pushed AFTER
        // `push_composite_layer` (i.e. intrinsic clips the subtree
        // itself emits, like an inner overflow_clip on a descendant)
        // participate in the per-primitive clip rect.
        let start = self.composite_layer_clip_base.first().copied().unwrap_or(0);
        let active_clips = &self.clip_stack[start..];
        if active_clips.is_empty() {
            // No clip - use large bounds
            return (
                [-10000.0, -10000.0, 100000.0, 100000.0],
                [0.0; 4],
                [1.0; 4],
                ClipType::None,
            );
        }

        // Try to compute intersection of all rect clips
        // Start with very large bounds
        let mut intersect_min_x = f32::NEG_INFINITY;
        let mut intersect_min_y = f32::NEG_INFINITY;
        let mut intersect_max_x = f32::INFINITY;
        let mut intersect_max_y = f32::INFINITY;
        let mut has_rect_clips = false;

        // Track corner radii with their source bounds
        // Each corner's radius is only valid if the intersection edge matches the source edge
        // Format: (radius, source_min_x, source_min_y, source_max_x, source_max_y)
        let mut corner_sources: [(f32, f32, f32, f32, f32); 4] = [
            (
                0.0,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::INFINITY,
            ), // top_left
            (
                0.0,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::INFINITY,
            ), // top_right
            (
                0.0,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::INFINITY,
            ), // bottom_right
            (
                0.0,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::INFINITY,
            ), // bottom_left
        ];

        // Track whether the topmost clip is a plain Rect (not rounded)
        let mut topmost_is_plain_rect = false;

        // Track corner_shape (superellipse n) per corner, paired with
        // the radius source above. Default `1.0` = round; gets
        // overwritten with the source `ClipShape::RoundedRect`'s
        // `corner_shape` whenever that source contributes the
        // dominant radius for the corner.
        let mut corner_n_sources = [1.0_f32; 4];

        for (clip, _poly_meta, _fade) in active_clips {
            match clip {
                ClipShape::Rect(rect) => {
                    // Intersect with this rect
                    intersect_min_x = intersect_min_x.max(rect.x());
                    intersect_min_y = intersect_min_y.max(rect.y());
                    intersect_max_x = intersect_max_x.min(rect.x() + rect.width());
                    intersect_max_y = intersect_max_y.min(rect.y() + rect.height());
                    has_rect_clips = true;
                    topmost_is_plain_rect = true;
                }
                ClipShape::RoundedRect {
                    rect,
                    corner_radius,
                    corner_shape,
                } => {
                    let rx = rect.x();
                    let ry = rect.y();
                    let rmax_x = rect.x() + rect.width();
                    let rmax_y = rect.y() + rect.height();

                    // Intersect with this rect
                    intersect_min_x = intersect_min_x.max(rx);
                    intersect_min_y = intersect_min_y.max(ry);
                    intersect_max_x = intersect_max_x.min(rmax_x);
                    intersect_max_y = intersect_max_y.min(rmax_y);

                    // Track corner radii with their source bounds
                    // Only update if this corner radius is larger (take max).
                    // Pair each corner's `n` with the radius it came
                    // from so the squircle curve survives the
                    // intersection rebuild below.
                    if corner_radius.top_left > corner_sources[0].0 {
                        corner_sources[0] = (corner_radius.top_left, rx, ry, rmax_x, rmax_y);
                        corner_n_sources[0] = corner_shape.top_left;
                    }
                    if corner_radius.top_right > corner_sources[1].0 {
                        corner_sources[1] = (corner_radius.top_right, rx, ry, rmax_x, rmax_y);
                        corner_n_sources[1] = corner_shape.top_right;
                    }
                    if corner_radius.bottom_right > corner_sources[2].0 {
                        corner_sources[2] = (corner_radius.bottom_right, rx, ry, rmax_x, rmax_y);
                        corner_n_sources[2] = corner_shape.bottom_right;
                    }
                    if corner_radius.bottom_left > corner_sources[3].0 {
                        corner_sources[3] = (corner_radius.bottom_left, rx, ry, rmax_x, rmax_y);
                        corner_n_sources[3] = corner_shape.bottom_left;
                    }

                    has_rect_clips = true;
                    topmost_is_plain_rect = false;
                }
                // For non-rect clips, fall back to topmost-only behavior
                ClipShape::Circle { .. }
                | ClipShape::Ellipse { .. }
                | ClipShape::Path(_)
                | ClipShape::Polygon(_) => {
                    // Can't easily intersect with circles/ellipses/paths/polygons
                    // Fall through to use the topmost clip
                    topmost_is_plain_rect = false;
                }
            }
        }

        // Find the topmost non-rect clip anywhere in the stack (circle,
        // ellipse, polygon, path). When one exists it becomes the shape
        // clip for the primitive and any rect clips contribute only to
        // the scissor bounds. Previously this scanned only
        // `clip_stack.last()`, so pushing a rect clip on top of a
        // polygon (e.g. a precomp's inner canvas rect stacked on a
        // matte polygon) silently dropped the polygon — which is what
        // made `blinc_lottie`'s track mattes look unclipped when the
        // matted layer was a precomp. The GPU still only evaluates one
        // non-rect shape per primitive, so we stop at the first one
        // found walking top-down.
        let topmost_non_rect_idx = active_clips.iter().rposition(|(c, _, _)| {
            matches!(
                c,
                ClipShape::Circle { .. }
                    | ClipShape::Ellipse { .. }
                    | ClipShape::Polygon(_)
                    | ClipShape::Path(_)
            )
        });
        let topmost_is_non_rect = topmost_non_rect_idx.is_some();

        // If we have rect clips AND the topmost clip is rect-based, use the intersection
        if has_rect_clips && !topmost_is_non_rect {
            let width = (intersect_max_x - intersect_min_x).max(0.0);
            let height = (intersect_max_y - intersect_min_y).max(0.0);

            // Determine final corner radii based on whether intersection edges are within
            // the corner radius region of the source. A corner should be rounded if the
            // intersection boundary is close enough to the source corner that it would
            // visually clip through the rounded area.
            //
            // For each corner, we check if the intersection edge is within (radius + epsilon)
            // of the source edge. If so, apply the rounded corner to prevent visual clipping.

            let mut radii = [0.0f32; 4];

            // Top-left corner: check if intersection is within corner radius region
            let (r, src_min_x, src_min_y, _, _) = corner_sources[0];
            if r > 0.0 {
                let dist_from_left = intersect_min_x - src_min_x;
                let dist_from_top = intersect_min_y - src_min_y;
                // If intersection is within the corner radius region, apply rounding
                if dist_from_left < r && dist_from_top < r {
                    radii[0] = (r - dist_from_left.max(0.0)).max(0.0).min(r);
                }
            }

            // Top-right corner
            let (r, _, src_min_y, src_max_x, _) = corner_sources[1];
            if r > 0.0 {
                let dist_from_right = src_max_x - intersect_max_x;
                let dist_from_top = intersect_min_y - src_min_y;
                if dist_from_right < r && dist_from_top < r {
                    radii[1] = (r - dist_from_right.max(0.0)).max(0.0).min(r);
                }
            }

            // Bottom-right corner
            let (r, _, _, src_max_x, src_max_y) = corner_sources[2];
            if r > 0.0 {
                let dist_from_right = src_max_x - intersect_max_x;
                let dist_from_bottom = src_max_y - intersect_max_y;
                if dist_from_right < r && dist_from_bottom < r {
                    radii[2] = (r - dist_from_right.max(0.0)).max(0.0).min(r);
                }
            }

            // Bottom-left corner
            let (r, src_min_x, _, _, src_max_y) = corner_sources[3];
            if r > 0.0 {
                let dist_from_left = intersect_min_x - src_min_x;
                let dist_from_bottom = src_max_y - intersect_max_y;
                if dist_from_left < r && dist_from_bottom < r {
                    radii[3] = (r - dist_from_left.max(0.0)).max(0.0).min(r);
                }
            }

            // Corner shape: use the source `n` for any corner whose
            // radius survived the intersection; default to round
            // (n=1) for corners that intersected away to 0 radius.
            let corner_shape = [
                if radii[0] > 0.0 {
                    corner_n_sources[0]
                } else {
                    1.0
                },
                if radii[1] > 0.0 {
                    corner_n_sources[1]
                } else {
                    1.0
                },
                if radii[2] > 0.0 {
                    corner_n_sources[2]
                } else {
                    1.0
                },
                if radii[3] > 0.0 {
                    corner_n_sources[3]
                } else {
                    1.0
                },
            ];

            return (
                [intersect_min_x, intersect_min_y, width, height],
                radii,
                corner_shape,
                ClipType::Rect,
            );
        }

        // Fall back to topmost clip for non-rect clips.
        // For non-rect clips (circle, ellipse, polygon), clip_bounds carries the
        // parent rect scissor (from accumulated rect clips) and clip_radius carries
        // the shape-specific data. The shader applies both rect scissor AND shape clip.
        let scissor_bounds = if has_rect_clips {
            let width = (intersect_max_x - intersect_min_x).max(0.0);
            let height = (intersect_max_y - intersect_min_y).max(0.0);
            [intersect_min_x, intersect_min_y, width, height]
        } else {
            [-10000.0, -10000.0, 100000.0, 100000.0]
        };

        // Prefer the topmost non-rect clip (if any) for the shape
        // entry; otherwise fall back to the topmost clip outright
        // (which will be a rect / rounded-rect and lands in the rect-
        // intersection branches below). Scanning instead of just
        // `last()` is what lets a rect clip stacked on top of a
        // polygon matte still resolve to the polygon as the shape.
        let shape_idx = topmost_non_rect_idx.unwrap_or(active_clips.len() - 1);
        let (clip, poly_meta, _fade) = &active_clips[shape_idx];
        match clip {
            ClipShape::Rect(rect) => (
                [rect.x(), rect.y(), rect.width(), rect.height()],
                [0.0; 4],
                [1.0; 4],
                ClipType::Rect,
            ),
            ClipShape::RoundedRect {
                rect,
                corner_radius,
                corner_shape,
            } => (
                [rect.x(), rect.y(), rect.width(), rect.height()],
                [
                    corner_radius.top_left,
                    corner_radius.top_right,
                    corner_radius.bottom_right,
                    corner_radius.bottom_left,
                ],
                [
                    corner_shape.top_left,
                    corner_shape.top_right,
                    corner_shape.bottom_right,
                    corner_shape.bottom_left,
                ],
                ClipType::Rect,
            ),
            ClipShape::Circle { center, radius } => (
                // clip_bounds = rect scissor, clip_radius = [cx, cy, radius, 0]
                scissor_bounds,
                [center.x, center.y, *radius, 0.0],
                [1.0; 4],
                ClipType::Circle,
            ),
            ClipShape::Ellipse { center, radii } => (
                // clip_bounds = rect scissor, clip_radius = [cx, cy, rx, ry]
                scissor_bounds,
                [center.x, center.y, radii.x, radii.y],
                [1.0; 4],
                ClipType::Ellipse,
            ),
            ClipShape::Polygon(_) => {
                // clip_bounds = rect scissor, clip_radius = [even_odd, 0, vertex_count, aux_offset]
                let (aux_offset, packed) = poly_meta.unwrap_or((0, 0));
                let even_odd = packed & POLYGON_EVEN_ODD != 0;
                let vertex_count = packed & !POLYGON_EVEN_ODD;
                (
                    scissor_bounds,
                    [
                        if even_odd { 1.0 } else { 0.0 },
                        0.0,
                        vertex_count as f32,
                        aux_offset as f32,
                    ],
                    [1.0; 4],
                    ClipType::Polygon,
                )
            }
            ClipShape::Path(_) => {
                // Path clipping not supported in GPU - fall back to no clip
                (
                    [-10000.0, -10000.0, 100000.0, 100000.0],
                    [0.0; 4],
                    [1.0; 4],
                    ClipType::None,
                )
            }
        }
    }

    /// Take the accumulated batch for rendering
    pub fn take_batch(&mut self) -> PrimitiveBatch {
        std::mem::take(&mut self.batch)
    }

    /// Take the accumulated 3D mesh draws captured during this frame.
    ///
    /// Every call to [`DrawContext::draw_mesh_data`] inside a canvas
    /// callback pushes one [`PendingMesh`] here. The outer render loop
    /// drains this list after `take_batch`, computes a
    /// view-projection matrix from each entry's captured camera against
    /// the frame's real target size, and dispatches to
    /// `GpuRenderer::render_mesh_data`. See `blinc_app::context` for
    /// the dispatch site.
    pub fn take_pending_meshes(&mut self) -> Vec<PendingMesh> {
        std::mem::take(&mut self.pending_meshes)
    }

    /// Take the user-defined GPU passes scheduled this frame.
    ///
    /// Every call to [`blinc_core::draw::DrawContext::run_gpu_pass`] inside a
    /// canvas callback pushes one [`PendingGpuPass`] here. The outer
    /// render loop drains this list after `take_batch` and dispatches
    /// each via `GpuPass::initialize_and_render` against the frame's
    /// real device / queue / target. See `blinc_app::context` for the
    /// dispatch site.
    pub fn take_pending_gpu_passes(&mut self) -> Vec<PendingGpuPass> {
        std::mem::take(&mut self.pending_gpu_passes)
    }

    /// Get a reference to the current batch
    pub fn batch(&self) -> &PrimitiveBatch {
        &self.batch
    }

    /// Get a mutable reference to the current batch
    pub fn batch_mut(&mut self) -> &mut PrimitiveBatch {
        &mut self.batch
    }

    /// Clear the batch
    pub fn clear(&mut self) {
        self.batch.clear();
        self.transform_stack = vec![Affine2D::IDENTITY];
        self.opacity_stack = vec![1.0];
        self.blend_mode_stack = vec![BlendMode::Normal];
        self.clip_stack.clear();
        self.layer_stack.clear();
        self.is_3d = false;
        self.camera = None;
    }

    /// Snapshot the current transform-stack depth. Pair with
    /// [`Self::restore_transform_stack`] to GUARANTEE the stack
    /// returns to this depth at the end of a paint section, even
    /// when an inner code path leaks a push (early return, conditional
    /// pop with mutated gate, panic-recover, etc.). Without this,
    /// continuous_redraw frames that re-run the walker compound any
    /// single-call leak — observed as `current_affine` doubling per
    /// frame (`a`/`d` going 2 → 4 → 8 → 16) and the canvas appearing
    /// to zoom out exponentially.
    pub fn transform_stack_depth(&self) -> usize {
        self.transform_stack.len()
    }

    /// Truncate the transform stack back to the snapshot depth
    /// returned by [`Self::transform_stack_depth`]. Safe to call when
    /// the stack is already at-or-below that depth (no-op then).
    pub fn restore_transform_stack(&mut self, depth: usize) {
        if self.transform_stack.len() > depth {
            self.transform_stack.truncate(depth);
        }
    }

    /// Apply opacity to a brush by modifying the color's alpha channel
    /// Pre-sample a multi-stop gradient at every tessellated vertex's
    /// path-local position. Returns `None` for solids, 2-stop
    /// gradients (the SDF shader's `mix(color, color2, t)` path
    /// handles them precisely per-fragment), or non-gradient
    /// brushes. Output aligns with `tessellated.vertices`.
    fn sample_per_vertex_gradient(
        tessellated: &crate::path::TessellatedPath,
        brush: &Brush,
    ) -> Option<Vec<blinc_core::Color>> {
        let Brush::Gradient(gradient) = brush else {
            return None;
        };
        let stops = gradient.stops();
        if stops.len() <= 2 {
            return None;
        }
        // Multi-stop per-vertex sampling is gated behind an opt-in
        // env flag until the layer-ordering regression it introduced
        // in the interactive_volume scene is root-caused. Without
        // this gate, the default 2-stop `mix(first, last, t)`
        // fallback keeps rendering order identical to the
        // known-working baseline.
        if std::env::var("BLINC_MULTISTOP_VERTEX").ok().as_deref() != Some("1") {
            return None;
        }
        let sample = |p: [f32; 2]| -> blinc_core::Color {
            let t = match gradient {
                blinc_core::Gradient::Linear { start, end, .. } => {
                    let dx = end.x - start.x;
                    let dy = end.y - start.y;
                    let len_sq = dx * dx + dy * dy;
                    if len_sq > 1e-6 {
                        (((p[0] - start.x) * dx + (p[1] - start.y) * dy) / len_sq).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                }
                blinc_core::Gradient::Radial { center, radius, .. } => {
                    let dx = p[0] - center.x;
                    let dy = p[1] - center.y;
                    let d = (dx * dx + dy * dy).sqrt();
                    (d / radius.max(1e-5)).clamp(0.0, 1.0)
                }
                blinc_core::Gradient::Conic { center, .. } => {
                    // Approximate conic as radial until we add conic shader
                    // support — mirrors `extract_brush_info`'s handling.
                    let dx = p[0] - center.x;
                    let dy = p[1] - center.y;
                    let d = (dx * dx + dy * dy).sqrt();
                    (d / 100.0).clamp(0.0, 1.0)
                }
            };
            Self::sample_stops(stops, t)
        };
        Some(
            tessellated
                .vertices
                .iter()
                .map(|v| sample(v.position))
                .collect(),
        )
    }

    /// Piecewise-linear interpolation through a sorted gradient stop
    /// list at parameter `t`. Clamps outside the first/last stop
    /// offsets. Mirrors `blinc_gpu::path::sample_gradient_stops`
    /// without re-exporting it.
    fn sample_stops(stops: &[blinc_core::GradientStop], t: f32) -> blinc_core::Color {
        if stops.is_empty() {
            return blinc_core::Color::TRANSPARENT;
        }
        if t <= stops[0].offset {
            return stops[0].color;
        }
        let last = stops.len() - 1;
        if t >= stops[last].offset {
            return stops[last].color;
        }
        for i in 0..last {
            let s0 = &stops[i];
            let s1 = &stops[i + 1];
            if t >= s0.offset && t <= s1.offset {
                let range = s1.offset - s0.offset;
                if range < 1e-6 {
                    return s0.color;
                }
                let local = (t - s0.offset) / range;
                return blinc_core::Color {
                    r: s0.color.r + (s1.color.r - s0.color.r) * local,
                    g: s0.color.g + (s1.color.g - s0.color.g) * local,
                    b: s0.color.b + (s1.color.b - s0.color.b) * local,
                    a: s0.color.a + (s1.color.a - s0.color.a) * local,
                };
            }
        }
        stops[last].color
    }

    fn apply_opacity_to_brush(brush: Brush, opacity: f32) -> Brush {
        if opacity >= 1.0 {
            return brush;
        }
        match brush {
            Brush::Solid(color) => {
                Brush::Solid(Color::rgba(color.r, color.g, color.b, color.a * opacity))
            }
            // For gradients, we'd need to modify each stop's color
            // For now, return as-is since SVGs typically use solid colors
            other => other,
        }
    }

    /// Resize the viewport
    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport = Size::new(width, height);
    }

    /// Execute a list of recorded draw commands
    pub fn execute_commands(&mut self, commands: &[DrawCommand]) {
        for cmd in commands {
            self.execute_command(cmd);
        }
    }

    /// Execute a single draw command
    pub fn execute_command(&mut self, cmd: &DrawCommand) {
        match cmd {
            DrawCommand::PushTransform(t) => self.push_transform(t.clone()),
            DrawCommand::PopTransform => self.pop_transform(),
            DrawCommand::PushClip(shape) => self.push_clip(shape.clone()),
            DrawCommand::PopClip => self.pop_clip(),
            DrawCommand::PushOpacity(o) => self.push_opacity(*o),
            DrawCommand::PopOpacity => self.pop_opacity(),
            DrawCommand::PushBlendMode(m) => self.push_blend_mode(*m),
            DrawCommand::PopBlendMode => self.pop_blend_mode(),
            DrawCommand::FillPath { path, brush } => self.fill_path(path, brush.clone()),
            DrawCommand::StrokePath {
                path,
                stroke,
                brush,
            } => self.stroke_path(path, stroke, brush.clone()),
            DrawCommand::FillRect {
                rect,
                corner_radius,
                brush,
            } => self.fill_rect(*rect, *corner_radius, brush.clone()),
            DrawCommand::StrokeRect {
                rect,
                corner_radius,
                stroke,
                brush,
            } => self.stroke_rect(*rect, *corner_radius, stroke, brush.clone()),
            DrawCommand::FillCircle {
                center,
                radius,
                brush,
            } => self.fill_circle(*center, *radius, brush.clone()),
            DrawCommand::StrokeCircle {
                center,
                radius,
                stroke,
                brush,
            } => self.stroke_circle(*center, *radius, stroke, brush.clone()),
            DrawCommand::DrawText {
                text,
                origin,
                style,
            } => self.draw_text(text, *origin, style),
            DrawCommand::DrawImage {
                image,
                rect,
                options,
            } => self.draw_image(*image, *rect, options),
            DrawCommand::DrawShadow {
                rect,
                corner_radius,
                shadow,
            } => self.draw_shadow(*rect, *corner_radius, *shadow),
            DrawCommand::DrawInnerShadow {
                rect,
                corner_radius,
                shadow,
            } => self.draw_inner_shadow(*rect, *corner_radius, *shadow),
            DrawCommand::DrawCircleShadow {
                center,
                radius,
                shadow,
            } => self.draw_circle_shadow(*center, *radius, *shadow),
            DrawCommand::DrawCircleInnerShadow {
                center,
                radius,
                shadow,
            } => self.draw_circle_inner_shadow(*center, *radius, *shadow),
            DrawCommand::SetCamera(camera) => self.set_camera(camera),
            DrawCommand::DrawMesh {
                mesh,
                material,
                transform,
            } => self.draw_mesh(*mesh, *material, *transform),
            DrawCommand::DrawMeshInstanced { mesh, instances } => {
                self.draw_mesh_instanced(*mesh, instances)
            }
            DrawCommand::AddLight(light) => self.add_light(light.clone()),
            DrawCommand::SetEnvironment(env) => self.set_environment(env),
            DrawCommand::PushLayer(config) => self.push_layer(config.clone()),
            DrawCommand::PopLayer => self.pop_layer(),
            DrawCommand::SampleLayer {
                id,
                source_rect,
                dest_rect,
            } => self.sample_layer(*id, *source_rect, *dest_rect),
        }
    }
}

impl<'a> DrawContext for GpuPaintContext<'a> {
    fn push_transform(&mut self, transform: Transform) {
        let current = self.current_affine();
        let new_transform = match transform {
            Transform::Affine2D(affine) => current.then(&affine),
            Transform::Mat4(_) => {
                // For 3D transforms in 2D context, just use identity
                // Real 3D handling would need a separate 3D rendering path
                current
            }
        };
        self.transform_stack.push(new_transform);
    }

    fn pop_transform(&mut self) {
        if self.transform_stack.len() > 1 {
            self.transform_stack.pop();
        }
    }

    fn transform_stack_depth(&self) -> usize {
        self.transform_stack.len()
    }

    fn restore_transform_stack(&mut self, depth: usize) {
        if self.transform_stack.len() > depth {
            self.transform_stack.truncate(depth);
        }
    }

    fn current_transform(&self) -> Transform {
        Transform::Affine2D(self.current_affine())
    }

    fn push_clip(&mut self, shape: ClipShape) {
        // Transform the clip shape by the current transform
        // Note: This only works correctly for translate + uniform scale transforms.
        // Rotation transforms are approximated (the bounding box is used).
        let transformed_shape = self.transform_clip_shape(shape);
        // For polygon clips, pack vertices into the ACTIVE batch's aux_data
        // and store metadata. The active batch is `dynamic_batch` when
        // emitting inside a motion-bound subtree — primitives there carry
        // `clip_radius.w = aux_offset` indexing into the dispatched batch's
        // own aux_data buffer, so vertices written to `self.batch.aux_data`
        // wouldn't be reachable at draw time. The matching pop is in
        // `pop_clip`; the aux_data entries stay around for the rest of the
        // paint pass (every later primitive emitted under this clip needs
        // them in place when its render-pass dispatches).
        // The rule is taken by every push, so it never reaches a later clip.
        let rule = std::mem::take(&mut self.pending_clip_fill_rule);
        let rule_bit = if rule == FillRule::EvenOdd {
            POLYGON_EVEN_ODD
        } else {
            0
        };
        let poly_meta = if let ClipShape::Polygon(ref pts) = transformed_shape {
            let active = self.active_batch_mut();
            let aux_offset = active.aux_data.len() as u32;
            let vertex_count = pts.len() as u32;
            // Pack vertices as vec4s: (x0, y0, x1, y1) — 2 vertices per vec4
            let mut i = 0;
            while i < pts.len() {
                let x0 = pts[i].x;
                let y0 = pts[i].y;
                let (x1, y1) = if i + 1 < pts.len() {
                    (pts[i + 1].x, pts[i + 1].y)
                } else {
                    (0.0, 0.0) // padding
                };
                active.aux_data.push([x0, y0, x1, y1]);
                i += 2;
            }
            Some((aux_offset, vertex_count | rule_bit))
        } else {
            None
        };
        let fade = std::mem::replace(&mut self.pending_overflow_fade, [0.0; 4]);
        self.clip_stack.push((transformed_shape, poly_meta, fade));
    }

    fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    fn push_opacity(&mut self, opacity: f32) {
        self.opacity_stack.push(opacity);
    }

    fn pop_opacity(&mut self) {
        if self.opacity_stack.len() > 1 {
            self.opacity_stack.pop();
        }
    }

    fn push_blend_mode(&mut self, mode: BlendMode) {
        self.blend_mode_stack.push(mode);
    }

    fn pop_blend_mode(&mut self) {
        if self.blend_mode_stack.len() > 1 {
            self.blend_mode_stack.pop();
        }
    }

    fn set_foreground_layer(&mut self, is_foreground: bool) {
        self.is_foreground = is_foreground;
    }

    fn set_z_layer(&mut self, layer: u32) {
        self.z_layer = layer;
    }

    fn z_layer(&self) -> u32 {
        self.z_layer
    }

    fn bg_primitive_count(&self) -> usize {
        // Reads the ACTIVE batch's count so primitive-range bracketing
        // (e.g. `composite_bg_start` / `composite_bg_end` in the
        // walker) lands in the same batch the emit calls wrote into.
        // While inside a motion-bound subtree, that's `dynamic_batch`;
        // otherwise `batch`.
        self.active_batch().primitives.len()
    }

    fn bg_layer_command_count(&self) -> usize {
        // Layer commands always live on the BG batch (push_layer
        // routes to `batch.layer_commands` regardless of motion-
        // subtree depth). Match that routing so the index the
        // walker captures lines up with what Phase 4's CSS patch
        // path reads from `cached_bg_batch.layer_commands`.
        self.batch.layer_commands.len()
    }

    fn push_motion_subtree(&mut self) {
        // Override the no-op trait default with our depth-counter
        // tracker so emit sites route to `dynamic_batch`.
        GpuPaintContext::push_motion_subtree(self)
    }

    fn pop_motion_subtree(&mut self) {
        GpuPaintContext::pop_motion_subtree(self)
    }

    fn push_overlay_subtree(&mut self) {
        // Override the no-op trait default. Inherent methods on
        // the concrete `GpuPaintContext` don't dispatch via
        // `&mut dyn DrawContext`; without this forwarder the
        // walker's push at an overlay-root boundary would be a
        // silent no-op and canvas inserts inside would route into
        // the global pool.
        GpuPaintContext::push_overlay_subtree(self)
    }

    fn pop_overlay_subtree(&mut self) {
        GpuPaintContext::pop_overlay_subtree(self)
    }

    fn in_overlay_subtree(&self) -> bool {
        self.overlay_subtree_depth > 0
    }

    fn push_composite_layer(&mut self, node_id: u64) {
        // Override the no-op trait default with the per-node scratch
        // routing so the promoted CSS subtree's emits land in
        // `composite_layer_batches[node_id]`.
        GpuPaintContext::push_composite_layer(self, node_id)
    }

    fn pop_composite_layer(&mut self) {
        GpuPaintContext::pop_composite_layer(self)
    }

    fn bg_primitive_aabb(&self, start: usize, end: usize) -> Option<[f32; 4]> {
        let prims = &self.active_batch().primitives;
        if start >= end || end > prims.len() {
            return None;
        }
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for p in &prims[start..end] {
            let [x, y, w, h] = p.bounds;
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            // Drop shadows render OUTSIDE `bounds`: the shadow vertex
            // shader expands the quad by `blur * 3 + |offset|` and the
            // fragment adds spread. Without padding here, the
            // composite-layer bake (which sizes its tight texture from
            // this AABB) crops the halo, so promoted subtrees lose
            // their box-shadow for the duration of the promotion.
            // 3 = Shadow, 5 = CircleShadow (inner shadows paint inside
            // `bounds` and need no padding).
            let expand = if p.type_info[0] == 3 || p.type_info[0] == 5 {
                p.shadow[2] * 3.0 + p.shadow[0].abs() + p.shadow[1].abs() + p.shadow[3].max(0.0)
            } else {
                0.0
            };
            min_x = min_x.min(x - expand);
            min_y = min_y.min(y - expand);
            max_x = max_x.max(x + w + expand);
            max_y = max_y.max(y + h + expand);
        }
        if min_x.is_finite() && max_x > min_x && max_y > min_y {
            Some([min_x, min_y, max_x - min_x, max_y - min_y])
        } else {
            None
        }
    }

    fn current_affine_elements(&self) -> [f32; 6] {
        self.current_affine().elements
    }

    fn current_clip_aabb(&self) -> Option<[f32; 4]> {
        if self.clip_stack.is_empty() {
            return None;
        }
        // Each clip in the stack is already in screen coordinates
        // (push_clip transforms by the active affine on entry). The
        // effective bounds is the intersection of every entry's
        // AABB. Non-rect clips (circles, paths, polygons) contribute
        // their bounding box — the compositor overlay uses the
        // result as a scissor rect, so a looser-than-necessary AABB
        // is safe (over-includes pixels) while a tighter bound would
        // be incorrect.
        let mut min_x = f32::NEG_INFINITY;
        let mut min_y = f32::NEG_INFINITY;
        let mut max_x = f32::INFINITY;
        let mut max_y = f32::INFINITY;
        for (shape, _, _) in &self.clip_stack {
            let (sx, sy, sw, sh) = match shape {
                ClipShape::Rect(r) => (r.x(), r.y(), r.width(), r.height()),
                ClipShape::RoundedRect { rect, .. } => {
                    (rect.x(), rect.y(), rect.width(), rect.height())
                }
                ClipShape::Circle { center, radius } => (
                    center.x - radius,
                    center.y - radius,
                    radius * 2.0,
                    radius * 2.0,
                ),
                ClipShape::Ellipse { center, radii } => (
                    center.x - radii.x,
                    center.y - radii.y,
                    radii.x * 2.0,
                    radii.y * 2.0,
                ),
                ClipShape::Polygon(pts) => {
                    if pts.is_empty() {
                        continue;
                    }
                    let mut pmin_x = f32::INFINITY;
                    let mut pmin_y = f32::INFINITY;
                    let mut pmax_x = f32::NEG_INFINITY;
                    let mut pmax_y = f32::NEG_INFINITY;
                    for p in pts {
                        pmin_x = pmin_x.min(p.x);
                        pmin_y = pmin_y.min(p.y);
                        pmax_x = pmax_x.max(p.x);
                        pmax_y = pmax_y.max(p.y);
                    }
                    (pmin_x, pmin_y, pmax_x - pmin_x, pmax_y - pmin_y)
                }
                ClipShape::Path(_) => continue, // skip — no easy AABB
            };
            min_x = min_x.max(sx);
            min_y = min_y.max(sy);
            max_x = max_x.min(sx + sw);
            max_y = max_y.min(sy + sh);
        }
        if max_x <= min_x || max_y <= min_y {
            return Some([min_x, min_y, 0.0, 0.0]);
        }
        Some([min_x, min_y, max_x - min_x, max_y - min_y])
    }

    fn current_clip_rounded(&self) -> Option<([f32; 4], [f32; 4])> {
        // Same heuristic the inherent `ambient_clip_rounded` uses:
        // walk the full clip stack, intersect AABBs, and track the
        // topmost rounded-rect's radius. Only return the radius when
        // the topmost rounded-rect's bounds equal the intersected
        // AABB — otherwise an ancestor rect has trimmed it and the
        // radius no longer maps cleanly to the scissor edge.
        self.ambient_clip_rounded(self.clip_stack.len())
    }

    fn set_3d_transform(&mut self, rx_rad: f32, ry_rad: f32, perspective_d: f32) {
        self.current_3d_sin_rx = rx_rad.sin();
        self.current_3d_cos_rx = rx_rad.cos();
        self.current_3d_sin_ry = ry_rad.sin();
        self.current_3d_cos_ry = ry_rad.cos();
        self.current_3d_perspective_d = perspective_d;
    }

    fn set_3d_shape(&mut self, shape_type: f32, depth: f32, ambient: f32, specular: f32) {
        self.current_3d_shape_type = shape_type;
        self.current_3d_depth = depth;
        self.current_3d_ambient = ambient;
        self.current_3d_specular = specular;
    }

    fn set_3d_light(&mut self, direction: [f32; 3], intensity: f32) {
        self.current_3d_light = [direction[0], direction[1], direction[2], intensity];
    }

    fn set_3d_translate_z(&mut self, z: f32) {
        self.current_3d_translate_z = z;
    }

    fn set_3d_group_raw(&mut self, shapes: &[[f32; 16]]) {
        use crate::primitives::ShapeDesc;
        self.current_3d_group_shapes = shapes
            .iter()
            .map(|arr| ShapeDesc {
                offset: [arr[0], arr[1], arr[2], arr[3]],
                params: [arr[4], arr[5], arr[6], arr[7]],
                half_ext: [arr[8], arr[9], arr[10], arr[11]],
                color: [arr[12], arr[13], arr[14], arr[15]],
            })
            .collect();
    }

    fn clear_3d(&mut self) {
        self.current_3d_sin_ry = 0.0;
        self.current_3d_cos_ry = 1.0;
        self.current_3d_sin_rx = 0.0;
        self.current_3d_cos_rx = 1.0;
        self.current_3d_perspective_d = 0.0;
        self.current_3d_shape_type = 0.0;
        self.current_3d_depth = 0.0;
        self.current_3d_ambient = 0.3;
        self.current_3d_specular = 32.0;
        self.current_3d_translate_z = 0.0;
        self.current_3d_light = [0.0, -1.0, 0.5, 0.8];
        self.current_3d_group_shapes.clear();
    }

    fn set_css_filter(
        &mut self,
        grayscale: f32,
        invert: f32,
        sepia: f32,
        hue_rotate_deg: f32,
        brightness: f32,
        contrast: f32,
        saturate: f32,
    ) {
        self.current_filter_a = [grayscale, invert, sepia, hue_rotate_deg.to_radians()];
        self.current_filter_b = [brightness, contrast, saturate, 0.0];
    }

    fn clear_css_filter(&mut self) {
        self.current_filter_a = [0.0, 0.0, 0.0, 0.0];
        self.current_filter_b = [1.0, 1.0, 1.0, 0.0];
    }

    fn set_mask_gradient(&mut self, params: [f32; 4], info: [f32; 4]) {
        self.current_mask_params = params;
        self.current_mask_info = info;
    }

    fn clear_mask_gradient(&mut self) {
        self.current_mask_params = [0.0; 4];
        self.current_mask_info = [0.0; 4];
    }

    fn set_corner_shape(&mut self, shape: [f32; 4]) {
        self.current_corner_shape = shape;
    }

    fn clear_corner_shape(&mut self) {
        self.current_corner_shape = [1.0; 4];
    }

    fn set_overflow_fade(&mut self, fade: [f32; 4]) {
        self.pending_overflow_fade = fade;
    }

    fn clear_overflow_fade(&mut self) {
        self.pending_overflow_fade = [0.0; 4];
    }

    fn set_clip_fill_rule(&mut self, rule: FillRule) {
        self.pending_clip_fill_rule = rule;
    }

    fn fill_path(&mut self, path: &Path, brush: Brush) {
        // Apply current opacity to the brush
        let opacity = self.combined_opacity();
        let brush = Self::apply_opacity_to_brush(brush, opacity);

        // Solid-color fills route through the SDF primitive stream so
        // they interleave with text (and any other SDF draws) in
        // submission order. The tessellated-path pipeline sits in its
        // own draw call after every SDF pipeline, so a shape filled
        // through it would paint on top of text submitted earlier —
        // Lottie layers whose stack put text on top of a shape came
        // out with the text behind the shape. The SDF-stream route
        // avoids that entirely by sharing the primitive dispatch.
        //
        // Gradients take the same SDF-stream route via the PRIM_MESH
        // fill_type path: the SDF core shader already rasterises
        // linear and radial gradients per-fragment; routing them
        // through `push_mesh_primitives_brush` keeps gradient fills
        // in submission order with solids. Previously gradients went
        // to the path pipeline which drew in a single batch AFTER
        // every SDF primitive, so solids authored later in the
        // Lottie stack covered gradients authored earlier — the
        // "brown face / purple BG / seeker invisible" symptom on
        // the interactive_volume scene.
        if matches!(brush, Brush::Solid(_) | Brush::Gradient(_)) {
            let tess_tolerance = scale_aware_tolerance(self.current_uniform_scale());
            let mut tessellated = tessellate_fill_with_tolerance(path, &brush, tess_tolerance);
            let affine = self.current_affine();
            // Pre-sample per-vertex colours for multi-stop gradients
            // BEFORE the affine transform — gradient stops are
            // defined in the path's local coordinate space, so
            // sampling has to happen there. The SDF shader's built-in
            // linear / radial math only handles 2 stops (color +
            // color2); per-vertex sampling + barycentric interpolation
            // reproduces the authored multi-stop ramp at a fidelity
            // that tracks tessellation density (lyon's 0.2 px
            // tolerance keeps triangles small enough that per-vertex
            // Gouraud-style colour is visually close to true
            // multi-stop).
            let per_vertex_colors = Self::sample_per_vertex_gradient(&tessellated, &brush);
            for vertex in &mut tessellated.vertices {
                let x = vertex.position[0];
                let y = vertex.position[1];
                vertex.position[0] =
                    affine.elements[0] * x + affine.elements[2] * y + affine.elements[4];
                vertex.position[1] =
                    affine.elements[1] * x + affine.elements[3] * y + affine.elements[5];
            }
            if !tessellated.is_empty() {
                let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();
                self.push_mesh_primitives_brush(
                    &tessellated,
                    &brush,
                    &affine,
                    per_vertex_colors.as_deref(),
                    clip_bounds,
                    clip_radius,
                    clip_corner_shape,
                    clip_type,
                );
            }
            return;
        }

        // Glass / image / other brush types: fall back to the
        // tessellated-path pipeline. These are rare in Lottie and
        // the path pipeline handles them today.
        let brush_info = extract_brush_info(&brush);

        let tess_tolerance = scale_aware_tolerance(self.current_uniform_scale());
        let mut tessellated = tessellate_fill_with_tolerance(path, &brush, tess_tolerance);

        let affine = self.current_affine();
        for vertex in &mut tessellated.vertices {
            let x = vertex.position[0];
            let y = vertex.position[1];
            vertex.position[0] =
                affine.elements[0] * x + affine.elements[2] * y + affine.elements[4];
            vertex.position[1] =
                affine.elements[1] * x + affine.elements[3] * y + affine.elements[5];
        }

        if !tessellated.is_empty() {
            let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

            if self.emit_foreground() {
                self.active_batch_mut()
                    .push_foreground_path_with_brush_info(
                        tessellated,
                        clip_bounds,
                        clip_radius,
                        clip_type,
                        &brush_info,
                    );
            } else {
                self.active_batch_mut().push_path_with_brush_info(
                    tessellated,
                    clip_bounds,
                    clip_radius,
                    clip_type,
                    &brush_info,
                );
            }
        }
    }

    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, brush: Brush) {
        // Apply current opacity to the brush
        let opacity = self.combined_opacity();
        let brush = Self::apply_opacity_to_brush(brush, opacity);

        // Solid + gradient strokes share the SDF-stream route so
        // submission order lines up with text. See `fill_path` for
        // the rationale.
        if matches!(brush, Brush::Solid(_) | Brush::Gradient(_)) {
            let tess_tolerance = scale_aware_tolerance(self.current_uniform_scale());
            let mut tessellated =
                tessellate_stroke_with_tolerance(path, stroke, &brush, tess_tolerance);
            let affine = self.current_affine();
            let per_vertex_colors = Self::sample_per_vertex_gradient(&tessellated, &brush);
            for vertex in &mut tessellated.vertices {
                let x = vertex.position[0];
                let y = vertex.position[1];
                vertex.position[0] =
                    affine.elements[0] * x + affine.elements[2] * y + affine.elements[4];
                vertex.position[1] =
                    affine.elements[1] * x + affine.elements[3] * y + affine.elements[5];
            }
            if !tessellated.is_empty() {
                let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();
                self.push_mesh_primitives_brush(
                    &tessellated,
                    &brush,
                    &affine,
                    per_vertex_colors.as_deref(),
                    clip_bounds,
                    clip_radius,
                    clip_corner_shape,
                    clip_type,
                );
            }
            return;
        }

        let brush_info = extract_brush_info(&brush);

        let tess_tolerance = scale_aware_tolerance(self.current_uniform_scale());
        let mut tessellated =
            tessellate_stroke_with_tolerance(path, stroke, &brush, tess_tolerance);

        let affine = self.current_affine();
        for vertex in &mut tessellated.vertices {
            let x = vertex.position[0];
            let y = vertex.position[1];
            vertex.position[0] =
                affine.elements[0] * x + affine.elements[2] * y + affine.elements[4];
            vertex.position[1] =
                affine.elements[1] * x + affine.elements[3] * y + affine.elements[5];
        }

        if !tessellated.is_empty() {
            let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

            if self.emit_foreground() {
                self.active_batch_mut()
                    .push_foreground_path_with_brush_info(
                        tessellated,
                        clip_bounds,
                        clip_radius,
                        clip_type,
                        &brush_info,
                    );
            } else {
                self.active_batch_mut().push_path_with_brush_info(
                    tessellated,
                    clip_bounds,
                    clip_radius,
                    clip_type,
                    &brush_info,
                );
            }
        }
    }

    fn fill_rect(&mut self, rect: Rect, corner_radius: CornerRadius, brush: Brush) {
        // Diagnostic for the recurring bg-split-in-half artefact.
        // Set BLINC_DEBUG_FILL_RECT=1 to print every UNIQUE solid /
        // gradient fill (deduplicated by bounds + brush bits across
        // every frame). With dedup, opening an affected widget adds
        // a small number of lines to the log — what we want to see
        // is whether the bug correlates with two fills at the SAME
        // bounds with different colors (stacked paint) or a single
        // gradient brush sneaking through. The dump includes
        // clip_bounds / clip_type / clip_radius captured at emit
        // time so we can see whether the active clip stack is
        // truncating the primitive to half-height in the affected
        // (canvas-underneath) configurations.
        if matches!(std::env::var("BLINC_DEBUG_FILL_RECT").as_deref(), Ok("1")) {
            use std::sync::{Mutex, OnceLock};
            static SEEN: OnceLock<Mutex<std::collections::HashSet<u64>>> = OnceLock::new();
            let seen = SEEN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
            // Cheap stable hash of (bounds, color) → u64. We bit-cast
            // the floats so any change in any field produces a
            // different hash.
            let hash_key = |bits: &[u32]| -> u64 {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut h = DefaultHasher::new();
                bits.hash(&mut h);
                h.finish()
            };
            let (cb, cr, _ccs, ct) = self.get_clip_data();
            let ct_name = match ct {
                ClipType::None => "none",
                ClipType::Rect => "rect",
                ClipType::Circle => "circle",
                ClipType::Ellipse => "ellipse",
                ClipType::Polygon => "polygon",
            };
            match &brush {
                Brush::Solid(c) => {
                    let key = hash_key(&[
                        rect.x().to_bits(),
                        rect.y().to_bits(),
                        rect.width().to_bits(),
                        rect.height().to_bits(),
                        c.r.to_bits(),
                        c.g.to_bits(),
                        c.b.to_bits(),
                        c.a.to_bits(),
                    ]);
                    if seen.lock().unwrap().insert(key) {
                        eprintln!(
                            "[fill_rect#{:x}] solid bounds=({:.1},{:.1},{:.1},{:.1}) color=({:.3},{:.3},{:.3},{:.3}) cr=({:.1},{:.1},{:.1},{:.1}) clip={}({:.1},{:.1},{:.1},{:.1}) clip_r=({:.1},{:.1},{:.1},{:.1}) fg={}",
                            key & 0xFFFF,
                            rect.x(),
                            rect.y(),
                            rect.width(),
                            rect.height(),
                            c.r,
                            c.g,
                            c.b,
                            c.a,
                            corner_radius.top_left,
                            corner_radius.top_right,
                            corner_radius.bottom_right,
                            corner_radius.bottom_left,
                            ct_name,
                            cb[0],
                            cb[1],
                            cb[2],
                            cb[3],
                            cr[0],
                            cr[1],
                            cr[2],
                            cr[3],
                            self.is_foreground,
                        );
                    }
                }
                Brush::Gradient(_) => {
                    let key = hash_key(&[
                        rect.x().to_bits(),
                        rect.y().to_bits(),
                        rect.width().to_bits(),
                        rect.height().to_bits(),
                        0x6E61_7472, // tag bytes so gradient + same bounds don't dedup
                    ]);
                    if seen.lock().unwrap().insert(key) {
                        eprintln!(
                            "[fill_rect#{:x}] GRADIENT bounds=({:.1},{:.1},{:.1},{:.1}) clip={}({:.1},{:.1},{:.1},{:.1})",
                            key & 0xFFFF,
                            rect.x(),
                            rect.y(),
                            rect.width(),
                            rect.height(),
                            ct_name,
                            cb[0],
                            cb[1],
                            cb[2],
                            cb[3],
                        );
                    }
                }
                Brush::Blur(_) | Brush::Image(_) | Brush::Glass(_) => {}
            }
        }
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);

        // Handle glass brush specially - push to glass primitives
        if let Brush::Glass(style) = &brush {
            let mut glass = GpuGlassPrimitive::new(
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            )
            .with_corner_radius_per_corner(
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            )
            .with_blur(style.blur)
            .with_tint(style.tint.r, style.tint.g, style.tint.b, style.tint.a)
            .with_saturation(style.saturation)
            .with_brightness(style.brightness)
            .with_noise(style.noise)
            .with_border_thickness(style.border_thickness);

            // Apply border color if specified
            if let Some(bc) = style.border_color {
                glass = glass.with_border_color(bc.r, bc.g, bc.b, bc.a);
            }

            // Apply shadow if present in the glass style
            // Shadow values are in CSS px but glass bounds are DPI-scaled screen px,
            // so we must scale shadow params to match.
            if let Some(ref shadow) = style.shadow {
                let [a, b, c, d, ..] = self.current_affine().elements;
                let scale = (a * d - b * c).abs().sqrt().max(1e-6);
                glass = glass.with_shadow_offset(
                    shadow.blur * scale,
                    shadow.color.a, // Use color alpha as opacity
                    shadow.offset_x * scale,
                    shadow.offset_y * scale,
                );
            }

            // Apply current clip bounds to glass primitive (for scroll containers)
            let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();
            match clip_type {
                ClipType::None => {}
                ClipType::Rect => {
                    // Check if this is a rounded rect clip (non-zero radius)
                    let has_radius = clip_radius.iter().any(|&r| r > 0.0);
                    if has_radius {
                        glass = glass.with_clip_rounded_rect_per_corner(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                            clip_radius[0],
                            clip_radius[1],
                            clip_radius[2],
                            clip_radius[3],
                        );
                    } else {
                        glass = glass.with_clip_rect(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                        );
                    }
                }
                ClipType::Circle | ClipType::Ellipse | ClipType::Polygon => {
                    // For circle/ellipse/polygon clips, use bounding rect for now
                    // Full support would require shader changes
                    glass = glass.with_clip_rect(
                        clip_bounds[0] - clip_bounds[2],
                        clip_bounds[1] - clip_bounds[3],
                        clip_bounds[2] * 2.0,
                        clip_bounds[3] * 2.0,
                    );
                }
            }

            // Set glass type based on simple flag
            if style.simple {
                glass = glass.with_glass_type(GlassType::Simple);
            }

            if style.depth > 0 {
                self.active_batch_mut().push_nested_glass(glass);
            } else {
                self.active_batch_mut().push_glass(glass);
            }
            return;
        }

        // Handle Blur brush - convert to glass primitive with just blur and optional tint
        if let Brush::Blur(style) = &brush {
            let mut glass = GpuGlassPrimitive::new(
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            )
            .with_corner_radius_per_corner(
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            )
            .with_blur(style.radius)
            .with_saturation(1.0) // No saturation adjustment for pure blur
            .with_brightness(1.0); // No brightness adjustment

            // Apply tint if specified
            if let Some(ref tint) = style.tint {
                glass = glass.with_tint(tint.r, tint.g, tint.b, tint.a * style.opacity);
            } else {
                // Default to slight white tint for visibility
                glass = glass.with_tint(1.0, 1.0, 1.0, 0.1 * style.opacity);
            }

            // Apply current clip bounds to glass primitive
            let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();
            match clip_type {
                ClipType::None => {}
                ClipType::Rect => {
                    let has_radius = clip_radius.iter().any(|&r| r > 0.0);
                    if has_radius {
                        glass = glass.with_clip_rounded_rect_per_corner(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                            clip_radius[0],
                            clip_radius[1],
                            clip_radius[2],
                            clip_radius[3],
                        );
                    } else {
                        glass = glass.with_clip_rect(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                        );
                    }
                }
                ClipType::Circle | ClipType::Ellipse | ClipType::Polygon => {
                    glass = glass.with_clip_rect(
                        clip_bounds[0] - clip_bounds[2],
                        clip_bounds[1] - clip_bounds[3],
                        clip_bounds[2] * 2.0,
                        clip_bounds[3] * 2.0,
                    );
                }
            }

            self.active_batch_mut().push_glass(glass);
            return;
        }

        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Convert OBB (0..1) gradient coords to rect-local pixel coords
        let gradient_params = Self::obb_to_rect_coords(&brush, gradient_params, rect, fill_type);

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        // Pack group shape descriptors into aux_data if this is a 3D group
        let mut border = [0.0_f32; 4];
        if !self.current_3d_group_shapes.is_empty() {
            let aux_offset = self.batch.aux_data.len() as f32;
            let shape_count = self.current_3d_group_shapes.len() as f32;

            // Find max depth across all child shapes for AABB
            let mut max_depth: f32 = 1.0;
            for shape in &self.current_3d_group_shapes {
                max_depth = max_depth.max(shape.params[1]); // params[1] = depth
            }

            // Push each ShapeDesc as 4 vec4s into aux_data
            for shape in &self.current_3d_group_shapes {
                self.batch.aux_data.push(shape.offset);
                self.batch.aux_data.push(shape.params);
                self.batch.aux_data.push(shape.half_ext);
                self.batch.aux_data.push(shape.color);
            }

            // border[0] = normal border width (unused for 3D groups)
            // border[1] = group shape count
            // border[2] = aux_data offset (in vec4 units)
            // border[3] = max depth for group AABB
            border = [0.0, shape_count, aux_offset, max_depth];
        }

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color,
            color2,
            border,
            border_color: [0.0; 4],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: transformed_gradient_params,
            rotation: self.current_rotation_sincos(),
            local_affine: self.current_local_affine(),
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::Rect as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
        };

        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }

        // Visible companion to the BLINC_DEBUG_FILL_RECT log: when
        // enabled, paint a thin coloured outline around every fill.
        // The outline colour is derived from the same per-frame hash
        // we'd otherwise log, so two stacked fills at the same
        // bounds with the same brush get the SAME outline colour
        // (visually overdrawn — looks normal) while two fills with
        // DIFFERENT colour or geometry get DIFFERENT outline
        // colours, immediately surfacing the bug visually.
        // Self-recursion is safe because `stroke_rect` doesn't call
        // back into `fill_rect`.
        if matches!(std::env::var("BLINC_DEBUG_FILL_RECT").as_deref(), Ok("1"))
            && !matches!(&brush, Brush::Blur(_) | Brush::Image(_) | Brush::Glass(_))
        {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            rect.x().to_bits().hash(&mut h);
            rect.y().to_bits().hash(&mut h);
            rect.width().to_bits().hash(&mut h);
            rect.height().to_bits().hash(&mut h);
            if let Brush::Solid(c) = &brush {
                c.r.to_bits().hash(&mut h);
                c.g.to_bits().hash(&mut h);
                c.b.to_bits().hash(&mut h);
                c.a.to_bits().hash(&mut h);
            }
            let hue = (h.finish() & 0xFFFF) as f32 / 65536.0;
            // Pleasant saturated outline regardless of underlying
            // bg: 6-segment hue wheel, full saturation.
            let (r, g, b) = match (hue * 6.0) as u32 {
                0 => (1.0, hue * 6.0, 0.0),
                1 => (2.0 - hue * 6.0, 1.0, 0.0),
                2 => (0.0, 1.0, hue * 6.0 - 2.0),
                3 => (0.0, 4.0 - hue * 6.0, 1.0),
                4 => (hue * 6.0 - 4.0, 0.0, 1.0),
                _ => (1.0, 0.0, 6.0 - hue * 6.0),
            };
            let outline = Color::rgba(r, g, b, 0.95);
            self.stroke_rect(
                rect,
                corner_radius,
                &Stroke::new(1.0),
                Brush::Solid(outline),
            );
        }
    }

    fn fill_notch(
        &mut self,
        rect: Rect,
        corner_radii: [f32; 4],
        corner_types: [f32; 4],
        top_mod: [f32; 4],
        bottom_mod: [f32; 4],
        border: Option<(f32, Color)>,
        shadow: Option<Shadow>,
        brush: Brush,
    ) {
        let transformed = self.transform_rect(rect);

        // DPI-scale the per-corner radii. `scale_corner_radius` wants a
        // `CornerRadius` struct, so we route through that — the magnitudes
        // match field-for-field.
        let scaled_radii = self.scale_corner_radius(CornerRadius {
            top_left: corner_radii[0],
            top_right: corner_radii[1],
            bottom_right: corner_radii[2],
            bottom_left: corner_radii[3],
        });
        let scaled_radii_arr = [
            scaled_radii.top_left,
            scaled_radii.top_right,
            scaled_radii.bottom_right,
            scaled_radii.bottom_left,
        ];

        // Top/bottom edge modifiers: (type, width, height_or_depth, corner_r).
        // Scale the geometric dimensions uniformly by the current DPI +
        // element scale, leaving the modifier type untouched. Non-uniform
        // scale / rotation / skew is already applied via `local_affine` in
        // the fragment shader, so we only need the isotropic scale here.
        let dpi_scale = self.current_dpi_scale();
        let scale_mod = |m: [f32; 4]| -> [f32; 4] {
            [m[0], m[1] * dpi_scale, m[2] * dpi_scale, m[3] * dpi_scale]
        };
        let scaled_top_mod = scale_mod(top_mod);
        let scaled_bottom_mod = scale_mod(bottom_mod);

        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Transform gradient params into rect-local then screen space, just
        // like `fill_rect` does.
        let gradient_params = Self::obb_to_rect_coords(&brush, gradient_params, rect, fill_type);
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        // Shadow (DPI-scaled). The shader's PRIM_NOTCH shadow path
        // traces the notch's actual outline via `sd_notch` so the
        // drop shadow follows the shape's outer edge (including
        // concave arcs, bulges, scoops, cuts, peaks) rather than the
        // rectangular bbox. The notch's element-level canvas clip is
        // now opt-in (via `overflow_clip`), so the shadow's blur
        // expansion can render past the element's layout box.
        let shadow_vec = if let Some(sh) = shadow {
            [
                sh.offset_x * dpi_scale,
                sh.offset_y * dpi_scale,
                sh.blur * dpi_scale,
                sh.spread * dpi_scale,
            ]
        } else {
            [0.0; 4]
        };
        let shadow_color_vec = shadow
            .map(|s| [s.color.r, s.color.g, s.color.b, s.color.a])
            .unwrap_or([0.0; 4]);

        // Border (DPI-scaled).
        let (border_vec, border_color_vec) = if let Some((width, color)) = border {
            (
                [width * dpi_scale, 0.0, 0.0, 0.0],
                [color.r, color.g, color.b, color.a],
            )
        } else {
            ([0.0; 4], [0.0; 4])
        };

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: scaled_radii_arr,
            color,
            color2,
            border: border_vec,
            border_color: border_color_vec,
            shadow: shadow_vec,
            shadow_color: shadow_color_vec,
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: transformed_gradient_params,
            rotation: self.current_rotation_sincos(),
            local_affine: self.current_local_affine(),
            // 3D slots repurposed for notch parameters — see `PrimitiveType::Notch`
            // doc comment in blinc_gpu::primitives for the contract.
            perspective: scaled_top_mod,
            sdf_3d: scaled_bottom_mod,
            light: corner_types,
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::Notch as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
        };

        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }
    }

    fn fill_rect_with_per_side_border(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        brush: Brush,
        border_widths: [f32; 4],
        border_color: Color,
    ) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Scale border widths by transform
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();

        let scaled_borders = [
            border_widths[0] * scale_y, // top (vertical scale)
            border_widths[1] * scale_x, // right (horizontal scale)
            border_widths[2] * scale_y, // bottom (vertical scale)
            border_widths[3] * scale_x, // left (horizontal scale)
        ];

        // Convert OBB (0..1) gradient coords to rect-local pixel coords
        let gradient_params = Self::obb_to_rect_coords(&brush, gradient_params, rect, fill_type);

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        let opacity = self.combined_opacity();
        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color,
            color2,
            border: scaled_borders,
            border_color: [
                border_color.r,
                border_color.g,
                border_color.b,
                border_color.a * opacity,
            ],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: transformed_gradient_params,
            rotation: self.current_rotation_sincos(),
            local_affine: self.current_local_affine(),
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::Rect as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
        };

        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }
    }

    fn stroke_rect(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        stroke: &Stroke,
        brush: Brush,
    ) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let (color, _color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Mirror BLINC_DEBUG_FILL_RECT for stroke_rect — CSS outlines
        // (focus rings) emit through this path, and the bg-split-in-
        // half symptom catches the focused outline too, so the dump
        // has to see both fills and strokes to triangulate which
        // ancestor clip is truncating the visible rect.
        if matches!(std::env::var("BLINC_DEBUG_FILL_RECT").as_deref(), Ok("1")) {
            use std::sync::{Mutex, OnceLock};
            static SEEN: OnceLock<Mutex<std::collections::HashSet<u64>>> = OnceLock::new();
            let seen = SEEN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            rect.x().to_bits().hash(&mut h);
            rect.y().to_bits().hash(&mut h);
            rect.width().to_bits().hash(&mut h);
            rect.height().to_bits().hash(&mut h);
            stroke.width.to_bits().hash(&mut h);
            color[0].to_bits().hash(&mut h);
            color[1].to_bits().hash(&mut h);
            color[2].to_bits().hash(&mut h);
            color[3].to_bits().hash(&mut h);
            let key = h.finish();
            if seen.lock().unwrap().insert(key) {
                let ct_name = match clip_type {
                    ClipType::None => "none",
                    ClipType::Rect => "rect",
                    ClipType::Circle => "circle",
                    ClipType::Ellipse => "ellipse",
                    ClipType::Polygon => "polygon",
                };
                eprintln!(
                    "[stroke_rect#{:x}] bounds=({:.1},{:.1},{:.1},{:.1}) w={:.1} color=({:.3},{:.3},{:.3},{:.3}) cr=({:.1},{:.1},{:.1},{:.1}) clip={}({:.1},{:.1},{:.1},{:.1}) clip_r=({:.1},{:.1},{:.1},{:.1}) fg={}",
                    key & 0xFFFF,
                    rect.x(),
                    rect.y(),
                    rect.width(),
                    rect.height(),
                    stroke.width,
                    color[0],
                    color[1],
                    color[2],
                    color[3],
                    corner_radius.top_left,
                    corner_radius.top_right,
                    corner_radius.bottom_right,
                    corner_radius.bottom_left,
                    ct_name,
                    clip_bounds[0],
                    clip_bounds[1],
                    clip_bounds[2],
                    clip_bounds[3],
                    clip_radius[0],
                    clip_radius[1],
                    clip_radius[2],
                    clip_radius[3],
                    self.is_foreground,
                );
            }
        }

        // Scale border width by the current transform's uniform scale (DPI + CSS transforms)
        let scaled_border_width = stroke.width * self.current_uniform_scale();

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color: [0.0, 0.0, 0.0, 0.0], // Transparent fill
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [scaled_border_width, 0.0, 0.0, 0.0],
            border_color: color,
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params,
            rotation: self.current_rotation_sincos(),
            local_affine: self.current_local_affine(),
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::Rect as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
        };

        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }
    }

    fn fill_circle(&mut self, center: Point, radius: f32, brush: Brush) {
        let transformed_center = self.transform_point(center);
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale = ((a * a + b * b).sqrt() + (c * c + d * d).sqrt()) / 2.0;
        let transformed_radius = radius * scale;

        // Handle glass brush specially - push to glass primitives
        if let Brush::Glass(style) = &brush {
            let mut glass = GpuGlassPrimitive::circle(
                transformed_center.x,
                transformed_center.y,
                transformed_radius,
            )
            .with_blur(style.blur)
            .with_tint(style.tint.r, style.tint.g, style.tint.b, style.tint.a)
            .with_saturation(style.saturation)
            .with_brightness(style.brightness)
            .with_noise(style.noise)
            .with_border_thickness(style.border_thickness);
            if let Some(bc) = style.border_color {
                glass = glass.with_border_color(bc.r, bc.g, bc.b, bc.a);
            }
            if style.depth > 0 {
                self.active_batch_mut().push_nested_glass(glass);
            } else {
                self.active_batch_mut().push_glass(glass);
            }
            return;
        }

        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Convert OBB (0..1) gradient coords to circle bounding rect pixel coords
        let circle_rect = Rect::new(
            center.x - radius,
            center.y - radius,
            radius * 2.0,
            radius * 2.0,
        );
        let gradient_params =
            Self::obb_to_rect_coords(&brush, gradient_params, circle_rect, fill_type);

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - transformed_radius,
                transformed_center.y - transformed_radius,
                transformed_radius * 2.0,
                transformed_radius * 2.0,
            ],
            corner_radius: [0.0; 4], // Not used for circles
            color,
            color2,
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: transformed_gradient_params,
            rotation: [0.0, 1.0, 0.0, 1.0],
            local_affine: [1.0, 0.0, 0.0, 1.0],
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::Circle as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
        };

        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }
    }

    fn stroke_circle(&mut self, center: Point, radius: f32, stroke: &Stroke, brush: Brush) {
        let transformed_center = self.transform_point(center);
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale = ((a * a + b * b).sqrt() + (c * c + d * d).sqrt()) / 2.0;
        let transformed_radius = radius * scale;

        let (color, _, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Convert OBB (0..1) gradient coords to circle bounding rect pixel coords
        let circle_rect = Rect::new(
            center.x - radius,
            center.y - radius,
            radius * 2.0,
            radius * 2.0,
        );
        let gradient_params =
            Self::obb_to_rect_coords(&brush, gradient_params, circle_rect, fill_type);

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - transformed_radius,
                transformed_center.y - transformed_radius,
                transformed_radius * 2.0,
                transformed_radius * 2.0,
            ],
            corner_radius: [0.0; 4],
            color: [0.0, 0.0, 0.0, 0.0], // Transparent fill
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [stroke.width * scale, 0.0, 0.0, 0.0],
            border_color: color,
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: transformed_gradient_params,
            rotation: [0.0, 1.0, 0.0, 1.0],
            local_affine: [1.0, 0.0, 0.0, 1.0],
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::Circle as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
        };

        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }
    }

    fn draw_text(&mut self, text: &str, origin: Point, style: &TextStyle) {
        use blinc_core::{TextAlign, TextBaseline};
        use blinc_text::{TextAlignment, TextAnchor};

        // Check if text context is available
        if self.text_ctx.is_none() {
            return;
        }

        // Transform origin by current transform (includes DPI scale
        // + any CSS / scroll translation from ancestors).
        let transformed_origin = self.transform_point(origin);

        // Extract uniform scale (DPI + container scale) so the glyph
        // rasterisation matches the post-transform size on screen.
        // Without this the text_ctx rasterises at CSS-pixel size
        // while the vertex shader places the quad at physical-pixel
        // coordinates — text renders at roughly half-size on 2×
        // retina and is horizontally offset because the glyph
        // advances don't keep up with the transformed origin.
        let uniform_scale = self.current_uniform_scale();
        let scaled_size = style.size * uniform_scale;

        // Glyph rasterisation FLOOR + CEILING. Without these the
        // raster size tracks the post-transform display size 1:1,
        // and the atlas size-quantiser (`(font_size*2.0).round()`)
        // creates a fresh bucket per ~0.5px display-size band.
        // Zoom-out below ~4px hits empty hinted bitmaps and the
        // text disappears; zoom-in pumps a new atlas entry for
        // every zoom step, releasing + re-rasterising every glyph
        // on the screen each frame the zoom moves — that's the
        // "lag worsens as I zoom in deeper, persists when I zoom
        // back out" symptom.
        //
        // Cap raster at [FLOOR..CEILING], scale the quad to make
        // up the remainder. At display sizes between FLOOR and
        // CEILING the raster matches the display exactly
        // (`quad_scale == 1.0`, original pixel-perfect path).
        // Beyond CEILING the same atlas glyph is reused regardless
        // of zoom; the magnified quad samples it with Linear
        // mag_filter for smoothness.
        const TEXT_RASTER_FLOOR: f32 = 6.0;
        // Ceiling lowered from 48 → 32. The atlas allocates one
        // shelf per (font_id, glyph_id, size) tuple; each shelf
        // takes raster_size² pixels (+padding). A canvas zooming
        // 1x → max accumulates buckets at raster=12, 24, 36
        // (display scaled_size before clamp). Higher ceiling
        // means each bucket is bigger AND more sizes used. At 32
        // the shelf is 56% smaller than at 48 and only sizes
        // 6, 12, 18, 24, 32 land before the clamp engages —
        // enough resolution for legible text up to ~150% the
        // ceiling display size without the atlas filling on
        // intermediate zoom levels and starving HUD text inserts.
        const TEXT_RASTER_CEILING: f32 = 32.0;
        let raster_size = scaled_size.clamp(TEXT_RASTER_FLOOR, TEXT_RASTER_CEILING);
        let quad_scale = if raster_size > 0.0 {
            scaled_size / raster_size
        } else {
            1.0
        };

        // Get current opacity
        let opacity = self.combined_opacity();

        // Resolve the current rect clip (if any) so glyphs outside
        // the active scissor get clipped. `get_clip_data` returns a
        // large default bound when the stack is empty; when a rect
        // clip is active the third element carries its `ClipType`,
        // and we propagate that onto each glyph so the shader's
        // `PRIM_TEXT` branch gates `clip_edge_alpha` properly.
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();
        let clip_kind_u32 = clip_type as u32;

        // Convert TextStyle color to [f32; 4] with opacity applied
        let color = [
            style.color.r,
            style.color.g,
            style.color.b,
            style.color.a * opacity,
        ];

        // Map TextAlign to TextAlignment
        let alignment = match style.align {
            TextAlign::Left => TextAlignment::Left,
            TextAlign::Center => TextAlignment::Center,
            TextAlign::Right => TextAlignment::Right,
        };

        // Map TextBaseline to TextAnchor
        let anchor = match style.baseline {
            TextBaseline::Top => TextAnchor::Top,
            TextBaseline::Middle => TextAnchor::Center,
            TextBaseline::Alphabetic => TextAnchor::Baseline,
            TextBaseline::Bottom => TextAnchor::Baseline, // Approximate with baseline
        };

        // Map `TextStyle::family` onto the font name that
        // `prepare_text_with_style` passes into
        // `blinc_text::Renderer::resolve_font_with_style`. The
        // previous `prepare_text_with_options` route always passed
        // `font_name=None`, so any caller-set family on `TextStyle`
        // (e.g. a Lottie text layer asking for "Cal Sans") silently
        // dropped through to the default SansSerif fallback. Treat
        // the blinc_core default "system-ui" as "no preference" so
        // Blinc's own generic-font routing still kicks in.
        let family_opt = if style.family == "system-ui" || style.family.is_empty() {
            None
        } else {
            Some(style.family.as_str())
        };
        let (weight_u16, italic_flag) = match style.weight {
            blinc_core::FontWeight::Thin => (100u16, false),
            blinc_core::FontWeight::Light => (300u16, false),
            blinc_core::FontWeight::Regular => (400u16, false),
            blinc_core::FontWeight::Medium => (500u16, false),
            blinc_core::FontWeight::Bold => (700u16, false),
            blinc_core::FontWeight::Black => (900u16, false),
        };

        // Now borrow text_ctx and prepare glyphs
        let text_ctx = self.text_ctx.as_mut().unwrap();
        if let Ok(mut glyphs) = text_ctx.prepare_text_with_style(
            text,
            transformed_origin.x,
            transformed_origin.y,
            raster_size,
            color,
            anchor,
            alignment,
            None,  // No width constraint
            false, // No wrap for canvas text
            family_opt,
            blinc_text::GenericFont::SansSerif,
            weight_u16,
            italic_flag,
            None,
            style.letter_spacing,
        ) {
            // When raster size was clamped (floor for zoom-out OR
            // ceiling for zoom-in), the layout above ran at
            // `raster_size`. Scale each glyph quad's offset from
            // the text origin AND its w/h by `quad_scale` so the
            // prim covers the intended display footprint while
            // sampling the clamped atlas region. UV bounds stay
            // unchanged. `quad_scale != 1.0` covers BOTH the
            // minify direction (deep zoom-out → Linear min_filter
            // smooth fade) and the magnify direction (deep zoom-in
            // → Linear mag_filter smooth upscale).
            if (quad_scale - 1.0).abs() > 1e-4 {
                for glyph in &mut glyphs {
                    let dx = glyph.bounds[0] - transformed_origin.x;
                    let dy = glyph.bounds[1] - transformed_origin.y;
                    glyph.bounds[0] = transformed_origin.x + dx * quad_scale;
                    glyph.bounds[1] = transformed_origin.y + dy * quad_scale;
                    glyph.bounds[2] *= quad_scale;
                    glyph.bounds[3] *= quad_scale;
                }
            }
            // Apply current clip bounds and fade to all glyphs
            let glyph_clip_fade = self.get_clip_fade();
            for glyph in &mut glyphs {
                glyph.clip_bounds = clip_bounds;
                glyph.clip_fade = glyph_clip_fade;
            }

            // Route each glyph straight into the primitive stream
            // instead of the `batch.glyphs` vec. That vec is only
            // drained by `convert_glyphs_to_primitives` /
            // `get_unified_foreground_primitives` on the *foreground*
            // batch — canvas elements render in the background layer
            // by default, so pushing into `glyphs` would silently
            // drop their text. `GpuPrimitive::from_glyph` carries
            // the glyph's texture coords + colour so the SDF
            // pipeline that consumes `primitives` renders identical
            // output. We also stamp the active clip onto each prim
            // so the shader's PRIM_TEXT branch respects scroll /
            // overflow-clip boundaries (canvas text used to spill
            // past its element because the default was
            // `ClipType::None` regardless of stack state).
            for glyph in glyphs {
                let mut prim = GpuPrimitive::from_glyph(&glyph);
                prim.type_info[2] = clip_kind_u32;
                prim.clip_bounds = clip_bounds;
                prim.clip_radius = clip_radius;
                if self.emit_foreground() {
                    self.active_batch_mut().push_foreground(prim);
                } else {
                    self.active_batch_mut().push(prim);
                }
            }
        }
    }

    fn draw_image(&mut self, _image: ImageId, _rect: Rect, _options: &ImageOptions) {
        // Image rendering would require:
        // 1. Texture loading and caching
        // 2. A separate image rendering pipeline
        // This is a placeholder for now
    }

    fn draw_rgba_pixels(&mut self, data: &[u8], width: u32, height: u32, dest: Rect) {
        let transformed = self.transform_rect(dest);
        let opacity = self.current_opacity();
        // Capture the active clip (the portal/node-body clip pushed
        // around a canvas closure) so the image shader clips the upload
        // to it in-shader — the same per-primitive clip SDF uses.
        // Without this a canvas-emitted RGBA upload paints full-coverage
        // on top of everything; the no-clip sentinel keeps root callers
        // (full-window video, camera preview) full-coverage.
        let (clip_bounds, clip_radius, _cs, _t) = self.get_clip_data();
        self.batch
            .dynamic_images
            .push(crate::primitives::DynamicImage {
                data: data.to_vec(),
                width,
                height,
                dest: transformed,
                opacity,
                corner_radius: 0.0,
                clip_bounds,
                clip_radius,
            });
    }

    fn draw_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Scale shadow values by the current transform's uniform scale (DPI + CSS transforms).
        // Shadow offset, blur, and spread are in logical pixels but the shader
        // operates in physical pixel space after the DPI transform.
        let s = self.current_uniform_scale();

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color: [0.0, 0.0, 0.0, 0.0], // Shadow is not filled
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [
                shadow.offset_x * s,
                shadow.offset_y * s,
                shadow.blur * s,
                shadow.spread * s,
            ],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            rotation: self.current_rotation_sincos(),
            local_affine: self.current_local_affine(),
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::Shadow as u32,
                FillType::Solid as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
        };

        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }
    }

    fn draw_inner_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Scale shadow values by the current transform's uniform scale (DPI + CSS transforms)
        let s = self.current_uniform_scale();

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color: [0.0, 0.0, 0.0, 0.0], // Inner shadow is not filled
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [
                shadow.offset_x * s,
                shadow.offset_y * s,
                shadow.blur * s,
                shadow.spread * s,
            ],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            rotation: self.current_rotation_sincos(),
            local_affine: self.current_local_affine(),
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::InnerShadow as u32,
                FillType::Solid as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
        };

        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }
    }

    fn draw_circle_shadow(&mut self, center: Point, radius: f32, shadow: Shadow) {
        let transformed_center = self.transform_point(center);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Store circle as bounds where the circle fits
        let size = radius * 2.0;
        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - radius,
                transformed_center.y - radius,
                size,
                size,
            ],
            corner_radius: [radius, radius, radius, radius], // Used as circle radius indicator
            color: [0.0, 0.0, 0.0, 0.0],
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            rotation: [0.0, 1.0, 0.0, 1.0],
            local_affine: [1.0, 0.0, 0.0, 1.0],
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::CircleShadow as u32,
                FillType::Solid as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
        };

        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }
    }

    fn draw_circle_inner_shadow(&mut self, center: Point, radius: f32, shadow: Shadow) {
        let transformed_center = self.transform_point(center);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        let size = radius * 2.0;
        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - radius,
                transformed_center.y - radius,
                size,
                size,
            ],
            corner_radius: [radius, radius, radius, radius],
            color: [0.0, 0.0, 0.0, 0.0],
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            rotation: [0.0, 1.0, 0.0, 1.0],
            local_affine: [1.0, 0.0, 0.0, 1.0],
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::CircleInnerShadow as u32,
                FillType::Solid as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
        };

        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }
    }

    fn sdf_build(&mut self, f: &mut dyn FnMut(&mut dyn SdfBuilder)) {
        let mut builder = GpuSdfBuilder::new(self);
        f(&mut builder);
    }

    fn set_camera(&mut self, camera: &Camera) {
        self.camera = Some(camera.clone());
        self.is_3d = true;
    }

    fn draw_mesh(&mut self, _mesh: MeshId, _material: MaterialId, _transform: Mat4) {
        // Cached-mesh path (MeshId / MaterialId pair). Not wired —
        // `draw_mesh_data` below is the direct path and the one the
        // canvas widget uses.
    }

    fn draw_mesh_instanced(&mut self, _mesh: MeshId, _instances: &[MeshInstance]) {
        // Cached-mesh instanced path. Not wired — see `draw_mesh`.
    }

    fn set_3d_viewport_bounds(&mut self, width: f32, height: f32) {
        self.mesh_viewport_bounds = Some((width, height));
    }

    fn draw_mesh_data(&mut self, mesh: std::sync::Arc<MeshData>, transform: Mat4) {
        let camera = self.camera.clone().unwrap_or_default();

        let viewport = self.mesh_viewport_bounds.map(|(lw, lh)| {
            let tl = self.transform_point(blinc_core::Point::new(0.0, 0.0));
            let br = self.transform_point(blinc_core::Point::new(lw, lh));
            [tl.x, tl.y, (br.x - tl.x).abs(), (br.y - tl.y).abs()]
        });
        // Keep the bounds live for the rest of this canvas render.
        // Clearing them after the first draw made single-mesh demos
        // work but silently broke multi-mesh scenes: subsequent meshes
        // received `viewport: None`, and their tonemap passes rendered
        // to the FULL FRAME instead of the canvas rect — each mesh
        // overwriting the previous output, leaving only the last-drawn
        // mesh visible. `set_3d_viewport_bounds` is called once per
        // canvas render, so overwriting here is redundant; next
        // canvas's bounds will overwrite on their own.

        // No clone — just Arc::clone (pointer bump, not 81.5 MB copy)
        self.pending_meshes.push(PendingMesh {
            mesh,
            transform,
            camera,
            lights: self.lights.clone(),
            viewport,
            env_cubemap: self.pending_env.clone(),
        });
    }

    fn add_light(&mut self, light: Light) {
        self.lights.push(light);
    }

    fn set_environment(&mut self, _env: &Environment) {
        // 3D environment is not yet implemented
    }

    fn set_environment_cubemap(&mut self, data: std::sync::Arc<CubemapData>) {
        self.pending_env = Some(data);
    }

    fn billboard_draw(
        &mut self,
        _size: Size,
        _transform: Mat4,
        _facing: BillboardFacing,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        // For now, just execute the 2D content without the billboard transform
        // Real implementation would require 3D projection
        f(self);
    }

    fn viewport_3d_draw(
        &mut self,
        _rect: Rect,
        camera: &Camera,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        // Set up 3D context
        let was_3d = self.is_3d;
        let old_camera = self.camera.take();
        self.set_camera(camera);

        // Execute 3D drawing
        f(self);

        // Restore 2D context
        self.is_3d = was_3d;
        self.camera = old_camera;
    }

    fn draw_sdf_viewport(&mut self, rect: Rect, viewport: &Sdf3DViewport) {
        // Transform the rect to screen coordinates (like fill_rect does)
        let transformed = self.transform_rect(rect);

        // Get current clip bounds and intersect with the viewport
        let (clip_bounds, _, _, _) = self.get_clip_data();
        let clip_min_x = clip_bounds[0];
        let clip_min_y = clip_bounds[1];
        let clip_max_x = clip_bounds[0] + clip_bounds[2];
        let clip_max_y = clip_bounds[1] + clip_bounds[3];

        // Original viewport bounds
        let orig_x = transformed.x();
        let orig_y = transformed.y();
        let orig_w = transformed.width();
        let orig_h = transformed.height();

        // Intersect viewport with clip region
        let clipped_x = orig_x.max(clip_min_x);
        let clipped_y = orig_y.max(clip_min_y);
        let clipped_right = (orig_x + orig_w).min(clip_max_x);
        let clipped_bottom = (orig_y + orig_h).min(clip_max_y);
        let clipped_w = (clipped_right - clipped_x).max(0.0);
        let clipped_h = (clipped_bottom - clipped_y).max(0.0);

        // Skip if viewport is fully clipped
        if clipped_w <= 0.0 || clipped_h <= 0.0 {
            return;
        }

        // Calculate UV offset and scale for clipped viewports
        let uv_offset_x = if orig_w > 0.0 {
            (clipped_x - orig_x) / orig_w
        } else {
            0.0
        };
        let uv_offset_y = if orig_h > 0.0 {
            (clipped_y - orig_y) / orig_h
        } else {
            0.0
        };
        let uv_scale_x = if orig_w > 0.0 {
            clipped_w / orig_w
        } else {
            1.0
        };
        let uv_scale_y = if orig_h > 0.0 {
            clipped_h / orig_h
        } else {
            1.0
        };

        // Create the uniform data for the shader
        // Must match the WGSL SdfUniform struct layout exactly
        let uniforms = Sdf3DUniform {
            camera_pos: [
                viewport.camera_pos.x,
                viewport.camera_pos.y,
                viewport.camera_pos.z,
                1.0,
            ],
            camera_dir: [
                viewport.camera_dir.x,
                viewport.camera_dir.y,
                viewport.camera_dir.z,
                0.0,
            ],
            camera_up: [
                viewport.camera_up.x,
                viewport.camera_up.y,
                viewport.camera_up.z,
                0.0,
            ],
            camera_right: [
                viewport.camera_right.x,
                viewport.camera_right.y,
                viewport.camera_right.z,
                0.0,
            ],
            // Use ORIGINAL resolution for correct aspect ratio calculation
            resolution: [orig_w, orig_h],
            time: viewport.time,
            fov: viewport.fov,
            max_steps: viewport.max_steps,
            max_distance: viewport.max_distance,
            epsilon: viewport.epsilon,
            _padding: 0.0,
            uv_offset: [uv_offset_x, uv_offset_y],
            uv_scale: [uv_scale_x, uv_scale_y],
        };

        // Create and push the 3D viewport with CLIPPED bounds
        let viewport_3d = Viewport3D {
            shader_wgsl: viewport.shader_wgsl.clone(),
            uniforms,
            bounds: [clipped_x, clipped_y, clipped_w, clipped_h],
            lights: viewport.lights.clone(),
        };

        self.batch.push_viewport_3d(viewport_3d);
    }

    fn draw_particles(&mut self, rect: Rect, particle_data: &ParticleSystemData) {
        use crate::particles::{GpuEmitter, GpuForce};
        use crate::primitives::ParticleViewport3D;

        // Transform the rect to screen coordinates
        let transformed = self.transform_rect(rect);

        // Get current clip bounds and intersect with the viewport
        let (clip_bounds, _, _, _) = self.get_clip_data();
        let clip_min_x = clip_bounds[0];
        let clip_min_y = clip_bounds[1];
        let clip_max_x = clip_bounds[0] + clip_bounds[2];
        let clip_max_y = clip_bounds[1] + clip_bounds[3];

        // Original viewport bounds
        let orig_x = transformed.x();
        let orig_y = transformed.y();
        let orig_w = transformed.width();
        let orig_h = transformed.height();

        // Intersect viewport with clip region
        let clipped_x = orig_x.max(clip_min_x);
        let clipped_y = orig_y.max(clip_min_y);
        let clipped_right = (orig_x + orig_w).min(clip_max_x);
        let clipped_bottom = (orig_y + orig_h).min(clip_max_y);
        let clipped_w = (clipped_right - clipped_x).max(0.0);
        let clipped_h = (clipped_bottom - clipped_y).max(0.0);

        // Skip if viewport is fully clipped
        if clipped_w <= 0.0 || clipped_h <= 0.0 {
            return;
        }

        // Skip if system is not playing
        if !particle_data.playing {
            return;
        }

        // Convert emitter shape to GPU format
        let (shape_type, shape_params) = match &particle_data.emitter {
            ParticleEmitterShape::Point => (0u32, [0.0f32; 4]),
            ParticleEmitterShape::Sphere { radius } => (1u32, [*radius, 0.0, 0.0, 0.0]),
            ParticleEmitterShape::Hemisphere { radius } => (2u32, [*radius, 0.0, 0.0, 0.0]),
            ParticleEmitterShape::Cone { angle, radius } => (3u32, [*angle, *radius, 0.0, 0.0]),
            ParticleEmitterShape::Box { half_extents } => {
                (4u32, [half_extents.x, half_extents.y, half_extents.z, 0.0])
            }
            ParticleEmitterShape::Circle { radius } => (5u32, [*radius, 0.0, 0.0, 0.0]),
        };

        // Create GPU emitter
        let emitter = GpuEmitter {
            position_shape: [
                particle_data.emitter_position.x,
                particle_data.emitter_position.y,
                particle_data.emitter_position.z,
                shape_type as f32,
            ],
            shape_params,
            direction_randomness: [
                particle_data.direction.x,
                particle_data.direction.y,
                particle_data.direction.z,
                particle_data.direction_randomness,
            ],
            emission_config: [
                particle_data.emission_rate,
                particle_data.burst_count, // burst count for one-shot effects
                0.0,                       // spawn accumulated (deprecated)
                particle_data.gravity_scale,
            ],
            lifetime_speed: [
                particle_data.lifetime.0,
                particle_data.lifetime.1,
                particle_data.start_speed.0,
                particle_data.start_speed.1,
            ],
            size_config: [
                particle_data.start_size.0,
                particle_data.start_size.1,
                particle_data.end_size.0,
                particle_data.end_size.1,
            ],
            start_color: [
                particle_data.start_color.r,
                particle_data.start_color.g,
                particle_data.start_color.b,
                particle_data.start_color.a,
            ],
            mid_color: [
                particle_data.mid_color.r,
                particle_data.mid_color.g,
                particle_data.mid_color.b,
                particle_data.mid_color.a,
            ],
            end_color: [
                particle_data.end_color.r,
                particle_data.end_color.g,
                particle_data.end_color.b,
                particle_data.end_color.a,
            ],
        };

        // Convert forces to GPU format
        let forces: Vec<GpuForce> = particle_data
            .forces
            .iter()
            .map(|force| match force {
                ParticleForce::Gravity(dir) => GpuForce {
                    type_strength: [0.0, 1.0, 0.0, 0.0],
                    direction_params: [dir.x, dir.y, dir.z, 0.0],
                },
                ParticleForce::Wind {
                    direction,
                    strength,
                    turbulence,
                } => GpuForce {
                    type_strength: [1.0, *strength, 0.0, 0.0],
                    direction_params: [direction.x, direction.y, direction.z, *turbulence],
                },
                ParticleForce::Vortex {
                    axis,
                    center: _,
                    strength,
                } => GpuForce {
                    type_strength: [2.0, *strength, 0.0, 0.0],
                    direction_params: [axis.x, axis.y, axis.z, 0.0],
                },
                ParticleForce::Drag(coefficient) => GpuForce {
                    type_strength: [3.0, *coefficient, 0.0, 0.0],
                    direction_params: [0.0, 0.0, 0.0, 0.0],
                },
                ParticleForce::Turbulence {
                    strength,
                    frequency,
                } => GpuForce {
                    type_strength: [4.0, *strength, 0.0, 0.0],
                    direction_params: [0.0, 0.0, 0.0, *frequency],
                },
                ParticleForce::Attractor { position, strength } => GpuForce {
                    type_strength: [5.0, *strength, 0.0, 0.0],
                    direction_params: [position.x, position.y, position.z, 0.0],
                },
            })
            .collect();

        // Determine blend mode
        let blend_mode = match particle_data.blend_mode {
            ParticleBlendMode::Alpha => 0,
            ParticleBlendMode::Additive => 1,
            ParticleBlendMode::Multiply => 2,
        };

        // Create and push the particle viewport
        let viewport = ParticleViewport3D {
            emitter,
            forces,
            max_particles: particle_data.max_particles,
            bounds: [clipped_x, clipped_y, clipped_w, clipped_h],
            camera_pos: [
                particle_data.camera_pos.x,
                particle_data.camera_pos.y,
                particle_data.camera_pos.z,
            ],
            camera_target: [
                particle_data.camera_pos.x + particle_data.camera_dir.x,
                particle_data.camera_pos.y + particle_data.camera_dir.y,
                particle_data.camera_pos.z + particle_data.camera_dir.z,
            ],
            camera_up: [
                particle_data.camera_up.x,
                particle_data.camera_up.y,
                particle_data.camera_up.z,
            ],
            fov: 0.8, // Default FOV
            time: particle_data.time,
            delta_time: particle_data.delta_time,
            blend_mode,
            playing: particle_data.playing,
        };

        self.batch.push_particle_viewport(viewport);
    }

    fn push_layer(&mut self, config: LayerConfig) {
        // Record current state indices for restoration on pop
        let state = LayerState {
            config: config.clone(),
            primitive_start: self.batch.primitive_count(),
            foreground_primitive_start: self.batch.foreground_primitive_count(),
            path_start: self.batch.path_vertex_count(),
            foreground_path_start: self.batch.foreground_path_vertex_count(),
            parent_state_indices: (
                self.transform_stack.len(),
                self.opacity_stack.len(),
                self.blend_mode_stack.len(),
                self.clip_stack.len(),
            ),
        };
        self.layer_stack.push(state);

        // Apply layer's blend mode if not Normal
        if config.blend_mode != BlendMode::Normal {
            self.blend_mode_stack.push(config.blend_mode);
        }

        // Apply layer's opacity if less than 1.0
        if config.opacity < 1.0 {
            self.opacity_stack.push(config.opacity);
        }

        // Record layer command for GPU renderer to process
        self.batch
            .push_layer_command(crate::primitives::LayerCommand::Push {
                config: config.clone(),
            });
    }

    fn pop_layer(&mut self) {
        if let Some(state) = self.layer_stack.pop() {
            // Restore parent state by trimming stacks to their saved indices
            let (transform_idx, opacity_idx, blend_idx, clip_idx) = state.parent_state_indices;

            // Only truncate if we pushed additional state for this layer
            // (don't go below the base state)
            if self.transform_stack.len() > transform_idx {
                self.transform_stack.truncate(transform_idx.max(1));
            }
            if self.opacity_stack.len() > opacity_idx {
                self.opacity_stack.truncate(opacity_idx.max(1));
            }
            if self.blend_mode_stack.len() > blend_idx {
                self.blend_mode_stack.truncate(blend_idx.max(1));
            }
            if self.clip_stack.len() > clip_idx {
                self.clip_stack.truncate(clip_idx);
            }

            // Record layer command for GPU renderer to process
            self.batch
                .push_layer_command(crate::primitives::LayerCommand::Pop);
        }
    }

    fn sample_layer(&mut self, id: LayerId, source_rect: Rect, dest_rect: Rect) {
        // Record sample command for GPU renderer to process
        self.batch
            .push_layer_command(crate::primitives::LayerCommand::Sample {
                id,
                source: source_rect,
                dest: dest_rect,
            });
    }

    fn viewport_size(&self) -> Size {
        self.viewport
    }

    fn is_3d_context(&self) -> bool {
        self.is_3d
    }

    fn current_opacity(&self) -> f32 {
        self.combined_opacity()
    }

    fn current_blend_mode(&self) -> BlendMode {
        self.blend_mode_stack
            .last()
            .copied()
            .unwrap_or(BlendMode::Normal)
    }

    fn run_gpu_pass(
        &mut self,
        hook: &dyn blinc_core::draw::GpuPassHook,
        viewport: Option<blinc_core::Rect>,
    ) {
        // Downcast through the canonical wrapper type. Any other
        // `GpuPassHook` impl is treated as an opaque no-op — the only
        // sanctioned construction route is `blinc_gpu::GpuPass::new`.
        let Some(pass) = hook.as_any().downcast_ref::<crate::custom_pass::GpuPass>() else {
            return;
        };
        // Resolve the viewport rect in physical pixels. Explicit
        // `Some(rect)` from the caller wins (caller is usually the
        // canvas closure handing through `bounds.rect()`). Otherwise
        // fall back to the current clip-stack AABB, which gives a
        // sensible default when the wrapping widget has already pushed
        // a clip for its layout bounds. `None` at both layers means
        // "render to the full frame target".
        let pixel_viewport = match viewport {
            Some(rect) => {
                let tl = self.transform_point(rect.origin);
                let br = self.transform_point(blinc_core::Point::new(
                    rect.origin.x + rect.size.width,
                    rect.origin.y + rect.size.height,
                ));
                Some([tl.x, tl.y, (br.x - tl.x).abs(), (br.y - tl.y).abs()])
            }
            None => self.current_clip_aabb(),
        };
        self.pending_gpu_passes.push(PendingGpuPass {
            pass: pass.clone(),
            viewport: pixel_viewport,
        });
    }
}

impl<'a> GpuPaintContext<'a> {
    /// Emit one `GpuPrimitive` per tessellated triangle. Each
    /// primitive's three corners live at `aux_data[off+0].xy`,
    /// `aux_data[off+0].zw`, `aux_data[off+1].xy`; the SDF vertex
    /// shader pulls them directly for `vertex_index` 0..2 and
    /// collapses 3..5 to a zero-area degenerate so the underlying
    /// 6-vertex quad draw still works. Hardware rasterises each
    /// triangle the same way the old tessellated-path pipeline did,
    /// so the fragment shader only needs solid fill + silhouette AA
    /// — no per-pixel triangle walk.
    ///
    /// For anti-aliasing, each triangle also carries which of its
    /// three edges lie on the mesh silhouette (edges that appear in
    /// exactly one triangle, identified here via a shared-edge
    /// count pass). The vertex shader emits a per-vertex barycentric
    /// varying; hardware interpolates it across the triangle, and
    /// the fragment shader uses `fwidth` on the relevant barycentric
    /// component to produce a 1-pixel smoothstep at each silhouette
    /// edge. Interior tessellation seams carry a zero flag, so the
    /// fragment keeps full alpha across them and adjacent triangles
    /// don't leave visible bands.
    #[allow(clippy::too_many_arguments)]
    fn push_mesh_primitives_brush(
        &mut self,
        tessellated: &crate::path::TessellatedPath,
        brush: &Brush,
        affine: &blinc_core::Affine2D,
        per_vertex_colors: Option<&[blinc_core::Color]>,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_corner_shape: [f32; 4],
        clip_type: ClipType,
    ) {
        use crate::primitives::PrimitiveType;
        use std::collections::HashMap;
        let indices = &tessellated.indices;
        let vertices = &tessellated.vertices;
        if indices.len() < 3 {
            return;
        }

        // Edge-occurrence count: (low_index, high_index) → count.
        // Silhouette edges appear exactly once across the whole
        // tessellation; interior edges appear twice.
        let mut edge_counts: HashMap<(u32, u32), u32> = HashMap::new();
        let mut i = 0;
        while i + 2 < indices.len() {
            let i0 = indices[i];
            let i1 = indices[i + 1];
            let i2 = indices[i + 2];
            i += 3;
            for (a, b) in [(i0, i1), (i1, i2), (i2, i0)] {
                let key = if a < b { (a, b) } else { (b, a) };
                *edge_counts.entry(key).or_insert(0) += 1;
            }
        }
        let is_silhouette = |a: u32, b: u32| -> f32 {
            let key = if a < b { (a, b) } else { (b, a) };
            match edge_counts.get(&key).copied().unwrap_or(0) {
                1 => 1.0,
                _ => 0.0,
            }
        };

        // Encode brush into gradient fields shared across every
        // triangle emitted for this fill. The SDF core shader reads
        // `color` + `color2` + `gradient_params` via `fill_type`
        // (PRIM_MESH honours the same fill_type branches as rect /
        // circle / ellipse), so we transform gradient endpoints from
        // path-local to screen space once and reuse them per
        // triangle.
        let apply = |pt: [f32; 2]| -> [f32; 2] {
            [
                affine.elements[0] * pt[0] + affine.elements[2] * pt[1] + affine.elements[4],
                affine.elements[1] * pt[0] + affine.elements[3] * pt[1] + affine.elements[5],
            ]
        };
        let scale = {
            let a = affine.elements[0];
            let b = affine.elements[1];
            let c = affine.elements[2];
            let d = affine.elements[3];
            let sx = (a * a + b * b).sqrt();
            let sy = (c * c + d * d).sqrt();
            (sx + sy) * 0.5
        };
        let (fill_type, color_arr, color2_arr, grad_params) = match brush {
            Brush::Solid(c) => (
                0u32,
                [c.r, c.g, c.b, c.a],
                [c.r, c.g, c.b, c.a],
                [0.0, 0.0, 0.0, 0.0],
            ),
            Brush::Gradient(g) => {
                let first = g.first_color();
                let last = g.last_color();
                match g {
                    blinc_core::Gradient::Linear { start, end, .. } => {
                        let s = apply([start.x, start.y]);
                        let e = apply([end.x, end.y]);
                        (
                            1u32,
                            [first.r, first.g, first.b, first.a],
                            [last.r, last.g, last.b, last.a],
                            [s[0], s[1], e[0], e[1]],
                        )
                    }
                    blinc_core::Gradient::Radial { center, radius, .. } => {
                        let c = apply([center.x, center.y]);
                        (
                            2u32,
                            [first.r, first.g, first.b, first.a],
                            [last.r, last.g, last.b, last.a],
                            [c[0], c[1], radius * scale, 0.0],
                        )
                    }
                    blinc_core::Gradient::Conic { center, .. } => {
                        let c = apply([center.x, center.y]);
                        (
                            2u32,
                            [first.r, first.g, first.b, first.a],
                            [last.r, last.g, last.b, last.a],
                            [c[0], c[1], 100.0 * scale, 0.0],
                        )
                    }
                }
            }
            _ => (
                0u32,
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                [0.0, 0.0, 0.0, 0.0],
            ),
        };
        let clip_type_u = clip_type as u32;

        let mut i = 0;
        while i + 2 < indices.len() {
            let i0 = indices[i];
            let i1 = indices[i + 1];
            let i2 = indices[i + 2];
            i += 3;

            let v0 = vertices[i0 as usize].position;
            let v1 = vertices[i1 as usize].position;
            let v2 = vertices[i2 as usize].position;

            let ax = v1[0] - v0[0];
            let ay = v1[1] - v0[1];
            let bx = v2[0] - v0[0];
            let by = v2[1] - v0[1];
            let area2 = ax * by - ay * bx;
            if area2.abs() < 1e-4 {
                continue;
            }

            let aux_offset = self.batch.aux_data.len() as u32;
            self.batch.aux_data.push([v0[0], v0[1], v1[0], v1[1]]);
            self.batch.aux_data.push([v2[0], v2[1], 0.0, 0.0]);
            // Multi-stop gradient: append 3 per-vertex colours right
            // after the triangle positions. The shader reads them at
            // `aux_data[aux_offset + 2..+5]` when
            // `type_info.w == 1u` and does barycentric interpolation
            // via `mesh_bary`, bypassing the fill_type switch so the
            // authored ramp is reproduced smoothly across the
            // triangle.
            if let Some(colors) = per_vertex_colors {
                let c0 = colors[i0 as usize];
                let c1 = colors[i1 as usize];
                let c2 = colors[i2 as usize];
                self.batch.aux_data.push([c0.r, c0.g, c0.b, c0.a]);
                self.batch.aux_data.push([c1.r, c1.g, c1.b, c1.a]);
                self.batch.aux_data.push([c2.r, c2.g, c2.b, c2.a]);
            }

            let min_x = v0[0].min(v1[0]).min(v2[0]);
            let min_y = v0[1].min(v1[1]).min(v2[1]);
            let max_x = v0[0].max(v1[0]).max(v2[0]);
            let max_y = v0[1].max(v1[1]).max(v2[1]);

            // Silhouette flags per edge. `corner_radius` is otherwise
            // unused for mesh primitives (no rounded corners on a
            // raw triangle), so repurpose its three spare slots:
            //   [0] = edge v0→v1 (bary.z == 0)
            //   [1] = edge v1→v2 (bary.x == 0)
            //   [2] = edge v2→v0 (bary.y == 0)
            let s01 = is_silhouette(i0, i1);
            let s12 = is_silhouette(i1, i2);
            let s20 = is_silhouette(i2, i0);

            let mesh_flag: u32 = if per_vertex_colors.is_some() { 1 } else { 0 };

            let mut prim = GpuPrimitive {
                bounds: [min_x, min_y, max_x - min_x, max_y - min_y],
                color: color_arr,
                color2: color2_arr,
                gradient_params: grad_params,
                border: [0.0, 0.0, aux_offset as f32, 0.0],
                corner_radius: [s01, s12, s20, 0.0],
                clip_bounds,
                clip_radius,
                clip_corner_shape,
                type_info: [
                    PrimitiveType::Mesh as u32,
                    fill_type,
                    clip_type_u,
                    mesh_flag,
                ],
                ..GpuPrimitive::default()
            };
            prim.local_affine = [1.0, 0.0, 0.0, 1.0];

            if self.emit_foreground() {
                self.active_batch_mut().push_foreground(prim);
            } else {
                self.active_batch_mut().push(prim);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GPU SDF Builder
// ─────────────────────────────────────────────────────────────────────────────

/// SDF builder that directly emits GPU primitives
struct GpuSdfBuilder<'a, 'b> {
    ctx: &'a mut GpuPaintContext<'b>,
    shapes: Vec<SdfShapeData>,
}

#[derive(Clone, Debug)]
enum SdfShapeData {
    Rect {
        rect: Rect,
        corner_radius: CornerRadius,
    },
    Circle {
        center: Point,
        radius: f32,
    },
    Ellipse {
        center: Point,
        radii: (f32, f32),
    },
}

impl<'a, 'b> GpuSdfBuilder<'a, 'b> {
    fn new(ctx: &'a mut GpuPaintContext<'b>) -> Self {
        Self {
            ctx,
            shapes: Vec::new(),
        }
    }

    fn add_shape(&mut self, shape: SdfShapeData) -> ShapeId {
        let id = ShapeId(self.shapes.len() as u32);
        self.shapes.push(shape);
        id
    }
}

impl<'a, 'b> SdfBuilder for GpuSdfBuilder<'a, 'b> {
    fn rect(&mut self, rect: Rect, corner_radius: CornerRadius) -> ShapeId {
        self.add_shape(SdfShapeData::Rect {
            rect,
            corner_radius,
        })
    }

    fn circle(&mut self, center: Point, radius: f32) -> ShapeId {
        self.add_shape(SdfShapeData::Circle { center, radius })
    }

    fn ellipse(&mut self, center: Point, radii: blinc_core::Vec2) -> ShapeId {
        self.add_shape(SdfShapeData::Ellipse {
            center,
            radii: (radii.x, radii.y),
        })
    }

    fn line(&mut self, _from: Point, _to: Point, _width: f32) -> ShapeId {
        // Line SDF would need a custom primitive type
        ShapeId(self.shapes.len() as u32)
    }

    fn arc(
        &mut self,
        _center: Point,
        _radius: f32,
        _start: f32,
        _end: f32,
        _width: f32,
    ) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn quad_bezier(&mut self, _p0: Point, _p1: Point, _p2: Point, _width: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn union(&mut self, _a: ShapeId, _b: ShapeId) -> ShapeId {
        // Boolean operations would require more complex SDF evaluation
        ShapeId(self.shapes.len() as u32)
    }

    fn subtract(&mut self, _a: ShapeId, _b: ShapeId) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn intersect(&mut self, _a: ShapeId, _b: ShapeId) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn smooth_union(&mut self, _a: ShapeId, _b: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn smooth_subtract(&mut self, _a: ShapeId, _b: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn smooth_intersect(&mut self, _a: ShapeId, _b: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn round(&mut self, _shape: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn outline(&mut self, _shape: ShapeId, _width: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn offset(&mut self, _shape: ShapeId, _distance: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn fill(&mut self, shape: ShapeId, brush: Brush) {
        if let Some(shape_data) = self.shapes.get(shape.0 as usize) {
            match shape_data.clone() {
                SdfShapeData::Rect {
                    rect,
                    corner_radius,
                } => {
                    self.ctx.fill_rect(rect, corner_radius, brush);
                }
                SdfShapeData::Circle { center, radius } => {
                    self.ctx.fill_circle(center, radius, brush);
                }
                SdfShapeData::Ellipse { center, radii } => {
                    // Ellipse would need its own primitive type
                    // For now, approximate with the larger radius
                    let radius = radii.0.max(radii.1);
                    self.ctx.fill_circle(center, radius, brush);
                }
            }
        }
    }

    fn stroke(&mut self, shape: ShapeId, stroke: &Stroke, brush: Brush) {
        if let Some(shape_data) = self.shapes.get(shape.0 as usize) {
            match shape_data.clone() {
                SdfShapeData::Rect {
                    rect,
                    corner_radius,
                } => {
                    self.ctx.stroke_rect(rect, corner_radius, stroke, brush);
                }
                SdfShapeData::Circle { center, radius } => {
                    self.ctx.stroke_circle(center, radius, stroke, brush);
                }
                SdfShapeData::Ellipse { center, radii } => {
                    let radius = radii.0.max(radii.1);
                    self.ctx.stroke_circle(center, radius, stroke, brush);
                }
            }
        }
    }

    fn shadow(&mut self, shape: ShapeId, shadow: Shadow) {
        if let Some(shape_data) = self.shapes.get(shape.0 as usize) {
            match shape_data.clone() {
                SdfShapeData::Rect {
                    rect,
                    corner_radius,
                } => {
                    self.ctx.draw_shadow(rect, corner_radius, shadow);
                }
                SdfShapeData::Circle { center, radius } => {
                    let rect = Rect::new(
                        center.x - radius,
                        center.y - radius,
                        radius * 2.0,
                        radius * 2.0,
                    );
                    self.ctx.draw_shadow(rect, radius.into(), shadow);
                }
                SdfShapeData::Ellipse { center, radii } => {
                    let rect = Rect::new(
                        center.x - radii.0,
                        center.y - radii.1,
                        radii.0 * 2.0,
                        radii.1 * 2.0,
                    );
                    self.ctx.draw_shadow(rect, CornerRadius::default(), shadow);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::Color;

    #[test]
    fn test_gpu_paint_context_creation() {
        let ctx = GpuPaintContext::new(800.0, 600.0);
        assert_eq!(ctx.viewport_size(), Size::new(800.0, 600.0));
        assert!(!ctx.is_3d_context());
        assert_eq!(ctx.current_opacity(), 1.0);
    }

    #[test]
    fn test_fill_rect() {
        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        ctx.fill_rect(
            Rect::new(10.0, 20.0, 100.0, 50.0),
            8.0.into(),
            Color::BLUE.into(),
        );

        assert_eq!(ctx.batch().primitive_count(), 1);
    }

    #[test]
    fn test_transform_stack() {
        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        ctx.push_transform(Transform::translate(10.0, 20.0));
        ctx.fill_rect(
            Rect::new(0.0, 0.0, 100.0, 50.0),
            0.0.into(),
            Color::RED.into(),
        );

        let batch = ctx.batch();
        let prim = &batch.primitives[0];

        // The rect should be translated
        assert_eq!(prim.bounds[0], 10.0);
        assert_eq!(prim.bounds[1], 20.0);
    }

    #[test]
    fn test_opacity_stack() {
        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        ctx.push_opacity(0.5);
        ctx.push_opacity(0.5);

        assert_eq!(ctx.current_opacity(), 0.25);

        ctx.pop_opacity();
        assert_eq!(ctx.current_opacity(), 0.5);
    }

    #[test]
    fn test_execute_commands() {
        use blinc_core::RecordingContext;

        let mut recording = RecordingContext::new(Size::new(800.0, 600.0));
        recording.fill_rect(
            Rect::new(10.0, 20.0, 100.0, 50.0),
            4.0.into(),
            Color::GREEN.into(),
        );

        let commands = recording.take_commands();

        let mut ctx = GpuPaintContext::new(800.0, 600.0);
        ctx.execute_commands(&commands);

        assert_eq!(ctx.batch().primitive_count(), 1);
    }

    #[test]
    fn test_layer_stack_tracking() {
        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        // Initial state
        assert_eq!(ctx.layer_stack.len(), 0);
        assert_eq!(ctx.current_opacity(), 1.0);
        assert_eq!(ctx.current_blend_mode(), BlendMode::Normal);

        // Push a layer with opacity and blend mode
        let config = LayerConfig {
            id: None,
            position: None,
            size: None,
            blend_mode: BlendMode::Multiply,
            opacity: 0.5,
            depth: false,
            effects: Vec::new(),
            transform_3d: None,
        };
        ctx.push_layer(config);

        // Layer should be tracked
        assert_eq!(ctx.layer_stack.len(), 1);
        // Blend mode and opacity should be applied
        assert_eq!(ctx.current_opacity(), 0.5);
        assert_eq!(ctx.current_blend_mode(), BlendMode::Multiply);

        // Draw something within the layer
        ctx.fill_rect(
            Rect::new(10.0, 10.0, 100.0, 100.0),
            0.0.into(),
            Color::RED.into(),
        );

        // Pop the layer
        ctx.pop_layer();

        // State should be restored
        assert_eq!(ctx.layer_stack.len(), 0);
        assert_eq!(ctx.current_opacity(), 1.0);
        assert_eq!(ctx.current_blend_mode(), BlendMode::Normal);
    }

    #[test]
    fn test_nested_layers() {
        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        // Push first layer
        let config1 = LayerConfig {
            id: None,
            position: None,
            size: None,
            blend_mode: BlendMode::Normal,
            opacity: 0.8,
            depth: false,
            effects: Vec::new(),
            transform_3d: None,
        };
        ctx.push_layer(config1);
        assert_eq!(ctx.layer_stack.len(), 1);
        assert_eq!(ctx.current_opacity(), 0.8);

        // Push second layer (nested)
        let config2 = LayerConfig {
            id: None,
            position: None,
            size: None,
            blend_mode: BlendMode::Screen,
            opacity: 0.5,
            depth: false,
            effects: Vec::new(),
            transform_3d: None,
        };
        ctx.push_layer(config2);
        assert_eq!(ctx.layer_stack.len(), 2);
        // Opacity should be combined: 0.8 * 0.5 = 0.4
        assert!((ctx.current_opacity() - 0.4).abs() < 0.001);
        assert_eq!(ctx.current_blend_mode(), BlendMode::Screen);

        // Pop second layer
        ctx.pop_layer();
        assert_eq!(ctx.layer_stack.len(), 1);
        assert_eq!(ctx.current_opacity(), 0.8);

        // Pop first layer
        ctx.pop_layer();
        assert_eq!(ctx.layer_stack.len(), 0);
        assert_eq!(ctx.current_opacity(), 1.0);
    }

    /// P4.3 Option B — when a composite layer is active, primitives
    /// emitted into the scratch batch must NOT carry the ancestor
    /// clip rect; that ancestor clip is re-applied at blit time. A
    /// primitive emitted inside the layer should report
    /// `ClipType::None` even though an ancestor clip is on the stack.
    ///
    /// IGNORED 2026-05-26 — strip-clip behavior temporarily disabled
    /// because the AABB scissor passed to the blit doesn't preserve
    /// the parent's corner radius, producing visible artifacts on
    /// `cn::switch` (rounded track corners get squared off). Re-
    /// enable when the bake captures the rounded-rect parent's radius
    /// too and passes it as the blit shader's scissor radius.
    #[test]
    fn composite_layer_strips_ancestor_clip_from_emit() {
        use blinc_core::DrawContext as _;
        use blinc_core::Rect;
        use blinc_core::layer::ClipShape;

        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        // Outer clip simulating an `.overflow_clip()` track at
        // [100, 100, 200, 50].
        ctx.push_clip(ClipShape::Rect(Rect::new(100.0, 100.0, 200.0, 50.0)));
        let outer_clip_aabb = ctx.current_clip_aabb();
        assert_eq!(outer_clip_aabb, Some([100.0, 100.0, 200.0, 50.0]));

        // Promote the subtree to a composite layer.
        ctx.push_composite_layer(0xDEAD_BEEF);
        assert_eq!(ctx.composite_layer_clip_base(), Some(1));

        // Emit a primitive INSIDE the composite layer. Its
        // `clip_rect` should be the "no clip" sentinel because the
        // ancestor clip is stripped.
        ctx.fill_rect(
            Rect::new(0.0, 0.0, 50.0, 50.0),
            0.0.into(),
            Color::BLUE.into(),
        );
        let batches = ctx.take_composite_layer_batches();
        let scratch = batches
            .get(&0xDEAD_BEEF)
            .expect("scratch batch should exist for promoted node");
        assert_eq!(scratch.primitives.len(), 1);
        let prim_clip = scratch.primitives[0].clip_bounds;
        // Sentinel "no clip" bounds from `get_clip_data`.
        assert_eq!(prim_clip, [-10000.0, -10000.0, 100000.0, 100000.0]);

        // `ambient_clip_aabb` peels the ancestor for the overlay.
        let ambient = ctx.ambient_clip_aabb(1);
        assert_eq!(ambient, Some([100.0, 100.0, 200.0, 50.0]));

        ctx.pop_composite_layer();
        assert_eq!(ctx.composite_layer_clip_base(), None);
        ctx.pop_clip();
    }

    /// P4.3 Option B — clips pushed INSIDE the composite layer (i.e.
    /// the subtree's own intrinsic clips) still apply to primitives
    /// emitted within them. Only ancestors below the snapshot are
    /// stripped.
    ///
    /// IGNORED 2026-05-26 — paired with the strip-clip test above;
    /// re-enable both when the strip-clip behavior comes back with
    /// rounded-corner-aware scissor support.
    #[test]
    fn composite_layer_preserves_inner_clip() {
        use blinc_core::DrawContext as _;
        use blinc_core::Rect;
        use blinc_core::layer::ClipShape;

        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        // Outer ancestor clip (stripped).
        ctx.push_clip(ClipShape::Rect(Rect::new(0.0, 0.0, 50.0, 50.0)));
        ctx.push_composite_layer(42);

        // Inner clip pushed by the subtree itself (e.g. a descendant
        // overflow_clip). This MUST land in per-primitive clip rect
        // because the bake is supposed to capture intrinsic clipping.
        ctx.push_clip(ClipShape::Rect(Rect::new(200.0, 200.0, 100.0, 100.0)));

        ctx.fill_rect(
            Rect::new(0.0, 0.0, 1000.0, 1000.0),
            0.0.into(),
            Color::RED.into(),
        );
        let batches = ctx.take_composite_layer_batches();
        let scratch = batches.get(&42).expect("scratch batch exists");
        let prim_clip = scratch.primitives[0].clip_bounds;
        // Inner clip (NOT the outer ancestor) lands on the primitive.
        assert_eq!(prim_clip, [200.0, 200.0, 100.0, 100.0]);

        ctx.pop_clip();
        ctx.pop_composite_layer();
        ctx.pop_clip();
    }

    /// P4.3 Option B — nested composite layers. When a promoted
    /// subtree contains a descendant that's ALSO promoted, the
    /// inner push must not clear the outer's clip-base on its pop.
    /// Previously stored as `Option<usize>`, a single-slot field
    /// silently dropped the outer's state on the inner's pop —
    /// leaving subsequent emits in the outer subtree with no
    /// strip-clip applied (so the outer's primitives leaked into
    /// scratch with their ancestor clips re-attached). The Vec
    /// stack form restores correct behaviour. The outer batch
    /// continues to receive all emits — nested promotion shares
    /// scratch routing with the outer (an intentional simplification
    /// matching the no-nested-routing contract; only the clip-base
    /// accounting is stack-aware).
    #[test]
    fn composite_layer_nested_push_preserves_outer_clip_base() {
        use blinc_core::DrawContext as _;
        use blinc_core::Rect;
        use blinc_core::layer::ClipShape;

        let mut ctx = GpuPaintContext::new(800.0, 600.0);

        // Ancestor clip (outer-of-outer). Should be stripped from
        // every primitive emitted inside any composite layer.
        ctx.push_clip(ClipShape::Rect(Rect::new(0.0, 0.0, 800.0, 600.0)));

        // Outer promoted subtree.
        ctx.push_composite_layer(0xAAAA);
        assert_eq!(ctx.composite_layer_clip_base(), Some(1));

        // Inner promoted subtree (nested). Old single-slot field
        // would have OVERWRITTEN outer's clip_base here; the Vec
        // pushes a new snapshot on top.
        ctx.push_composite_layer(0xBBBB);
        assert_eq!(
            ctx.composite_layer_clip_base(),
            Some(1),
            "outermost clip_base (1) must remain the cutoff while inner is active"
        );

        // Inner emits a primitive. clip_rect strips ancestors below
        // outer base (1) — i.e., the 800x600 ancestor.
        ctx.fill_rect(
            Rect::new(10.0, 10.0, 20.0, 20.0),
            0.0.into(),
            Color::BLUE.into(),
        );

        // Inner pop. Previously this would have cleared the outer's
        // state to None; with Vec it just pops the inner snapshot.
        ctx.pop_composite_layer();
        assert_eq!(
            ctx.composite_layer_clip_base(),
            Some(1),
            "outer's clip_base must survive inner pop"
        );

        // Outer continues to emit; its primitives still strip the
        // ancestor.
        ctx.fill_rect(
            Rect::new(50.0, 50.0, 30.0, 30.0),
            0.0.into(),
            Color::RED.into(),
        );

        let batches = ctx.take_composite_layer_batches();
        let scratch = batches.get(&0xAAAA).expect("outer scratch exists");
        // Both primitives (inner + outer emits) route into outer's
        // scratch batch (no-nested-routing contract) and both have
        // their ancestor clip stripped.
        assert_eq!(scratch.primitives.len(), 2);
        for prim in &scratch.primitives {
            assert_eq!(
                prim.clip_bounds,
                [-10000.0, -10000.0, 100000.0, 100000.0],
                "ancestor clip stripped for both inner- and outer-emitted prims"
            );
        }

        // Outer pop now clears the active state entirely.
        ctx.pop_composite_layer();
        assert_eq!(ctx.composite_layer_clip_base(), None);
        ctx.pop_clip();
    }
}
