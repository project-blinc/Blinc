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
    CubemapData, DrawContext, Environment, FillRule, ImageId, ImageOptions, LayerConfig, LayerId,
    Light, Mat4, MaterialId, MeshData, MeshId, MeshInstance, ParticleBlendMode,
    ParticleEmitterShape, ParticleForce, ParticleSystemData, Path, Point, Rect, Sdf3DViewport,
    SdfBuilder, Shadow, Size, Stroke, TextStyle, Transform,
};

use crate::path::{
    DEFAULT_TESSELLATION_TOLERANCE, extract_brush_info, tessellate_fill_with_tolerance,
    tessellate_stroke_with_tolerance,
};
use crate::primitives::{
    ClipType, FillType, GlassType, GpuGlassPrimitive, GpuPrimitive, PrimitiveBatch, PrimitiveType,
    Sdf3DUniform, Viewport3D,
};
use crate::text::TextRenderingContext;

mod aux_data;
mod cross_stream;
mod glass;
mod gpu_pass;
pub use glass::GroupGlass;
mod mesh;
pub(crate) use mesh::camera_signature;
mod batch;
mod brush;
mod clip;
mod element_state;
mod native_clip;
mod overlay;
mod path_clip;
mod rgba;
mod routing;
mod sdf_builder;
mod transform;
use aux_data::{AuxBatch, ClipData, PolygonClipData};
use sdf_builder::GpuSdfBuilder;

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
    cross_stream_static: bool,
    cross_stream: Option<(crate::primitives::cross_stream::GroupId, usize, usize, bool)>,
    overlay_order: Option<u32>,
    captures_glass: bool,
    /// The layer configuration
    config: LayerConfig,
    /// Batch that owns this group's commands and range snapshots.
    command_batch: AuxBatch,
    /// Whether the commands use the foreground-local cursor stream.
    foreground: bool,
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

/// Clip stack entry: shape, optional clip metadata, overflow fade.
type ClipStackEntry = (ClipShape, Option<ClipData>, [f32; 4]);

// ─────────────────────────────────────────────────────────────────────────────
// Pending 3D Mesh Draw
// ─────────────────────────────────────────────────────────────────────────────

/// A mesh instance and its captured scene state. Native groups retain compatible
/// consecutive instances together; the outer render loop drains ungrouped instances.
#[derive(Clone, Debug)]
pub struct PendingMesh {
    /// Shared geometry and PBR material; capture does not copy vertex/index data.
    pub mesh: std::sync::Arc<MeshData>,
    /// Model transform (world-space placement of this instance).
    pub transform: Mat4,
    /// Camera snapshot; perspective aspect follows the scene viewport at dispatch.
    pub camera: Camera,
    /// Light snapshot. Dispatch uses directional lights and a shared scene shadow map.
    pub lights: Vec<Light>,
    /// Screen-space viewport rect in physical pixels [x, y, w, h].
    /// When `Some`, the renderer applies `set_viewport` + `set_scissor_rect`
    /// to clip the mesh to this region. When `None`, the mesh renders to
    /// the full frame target.
    pub viewport: Option<[f32; 4]>,
    /// Optional shared environment cubemap uploaded before this instance is drawn.
    pub env_cubemap: Option<std::sync::Arc<CubemapData>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Pending Custom GPU Pass
// ─────────────────────────────────────────────────────────────────────────────

/// A canvas GPU callback and its physical viewport. Native group commands
/// retain this record; the outer render loop drains ungrouped callbacks.
#[derive(Clone)]
pub struct PendingGpuPass {
    /// Shared callback, initialized on its first execution.
    pub pass: crate::custom_pass::GpuPass,
    /// Physical `[x, y, width, height]` passed to the callback; None means full frame.
    pub viewport: Option<[f32; 4]>,
    /// A flow shader the renderer draws itself, with `pass` inert. `viewport` is then the part of
    /// `flow.bounds` the clip leaves visible.
    pub flow: Option<PendingFlow>,
}

/// A flow shader drawn by `DrawContext::draw_shader`.
#[derive(Clone)]
pub struct PendingFlow {
    pub shader: std::sync::Arc<blinc_core::FlowShader>,
    /// Physical `[x, y, width, height]` the shader's `uv` covers.
    pub bounds: [f32; 4],
    pub inputs: blinc_core::ShaderInputs,
    /// Physical corner radius.
    pub corner_radius: f32,
}

/// A mesh scene retained by a native visual group.
#[derive(Clone, Debug)]
pub struct GroupMeshes {
    /// Consecutive instances sharing camera, viewport and paint coverage.
    pub meshes: Vec<PendingMesh>,
    /// Coverage, explicit opacity and clips applied to the completed scene.
    pub paint: crate::primitives::DynamicImage,
    /// Original physical frame extent used by the scene pipeline.
    pub frame_size: (u32, u32),
}

/// Callback state retained by a native visual group.
#[derive(Clone)]
pub struct GroupGpuPass {
    /// Callback and its original viewport.
    pub pending: PendingGpuPass,
    /// Coverage, explicit opacity and push-time clips for replacing the edited pixels.
    pub paint: crate::primitives::DynamicImage,
    /// Physical frame extent exposed to the callback.
    pub frame_size: (u32, u32),
    /// Display scale exposed to the callback.
    pub scale_factor: f64,
}

impl std::fmt::Debug for GroupGpuPass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupGpuPass")
            .field("viewport", &self.pending.viewport)
            .field("paint", &self.paint)
            .field("frame_size", &self.frame_size)
            .field("scale_factor", &self.scale_factor)
            .finish_non_exhaustive()
    }
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
    /// Outermost motion scope retained by an open static group.
    static_motion_depth: Option<u32>,
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
    overlay_visit_sequence: u32,
    overlay_root_pending: Option<u32>,
    overlay_layer_scope: Option<overlay::OverlayLayerScope>,
    overlay_ancestor_ids: std::collections::HashMap<
        (u64, crate::primitives::cross_stream::GroupId),
        crate::primitives::cross_stream::GroupId,
    >,
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
    rejected_cross_stream_groups: std::collections::HashSet<u64>,
    next_cross_stream_local: u64,
    cross_stream_static_depth: usize,
    deferred_text: bool,
    deferred_svg: bool,
    deferred_image: bool,
    glass_layers: bool,
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
    /// Fill rule of the next polygon or path clip, taken by `push_clip`.
    pending_clip_fill_rule: FillRule,
    /// Ungrouped mesh instances drained by the outer render loop.
    pending_meshes: Vec<PendingMesh>,
    /// 3D viewport bounds (logical pixels). Set by SceneKit3D before
    /// `draw_mesh_data` so the viewport rect can be computed from the
    /// transform stack position + these bounds.
    mesh_viewport_bounds: Option<(f32, f32)>,
    /// Environment cubemap data set by `set_environment_cubemap`. Captured
    /// into each `PendingMesh` so the renderer can upload it.
    pending_env: Option<std::sync::Arc<CubemapData>>,
    /// Ungrouped canvas callbacks dispatched by the outer render loop.
    pending_gpu_passes: Vec<PendingGpuPass>,
    gpu_pass_scale_factor: f64,
    last_group_mesh: Option<(AuxBatch, bool, usize, [usize; 8])>,
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
            gpu_pass_scale_factor: 1.0,
            last_group_mesh: None,
            dynamic_batch: PrimitiveBatch::new(),
            motion_subtree_depth: 0,
            static_motion_depth: None,
            overlay_subtree_depth: 0,
            overlay_visit_sequence: 0,
            overlay_root_pending: None,
            overlay_layer_scope: None,
            overlay_ancestor_ids: std::collections::HashMap::new(),
            composite_layer_batches: std::collections::HashMap::new(),
            active_composite_layer: None,
            composite_layer_clip_base: Vec::new(),
            rejected_cross_stream_groups: std::collections::HashSet::new(),
            next_cross_stream_local: 0,
            cross_stream_static_depth: 0,
            deferred_text: false,
            deferred_svg: false,
            deferred_image: false,
            glass_layers: false,
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
            gpu_pass_scale_factor: 1.0,
            last_group_mesh: None,
            dynamic_batch: PrimitiveBatch::new(),
            motion_subtree_depth: 0,
            static_motion_depth: None,
            overlay_subtree_depth: 0,
            overlay_visit_sequence: 0,
            overlay_root_pending: None,
            overlay_layer_scope: None,
            overlay_ancestor_ids: std::collections::HashMap::new(),
            composite_layer_batches: std::collections::HashMap::new(),
            active_composite_layer: None,
            composite_layer_clip_base: Vec::new(),
            rejected_cross_stream_groups: std::collections::HashSet::new(),
            next_cross_stream_local: 0,
            cross_stream_static_depth: 0,
            deferred_text: false,
            deferred_svg: false,
            deferred_image: false,
            glass_layers: false,
        }
    }

    /// Set the text rendering context
    pub fn set_text_context(&mut self, text_ctx: &'a mut TextRenderingContext) {
        self.text_ctx = Some(text_ctx);
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
        self.push_clip_impl(shape)
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
        // Range snapshots must use the same owner as push_layer.
        match self.layer_command_batch() {
            AuxBatch::Static => self.batch.layer_commands.len(),
            AuxBatch::Dynamic => self.dynamic_batch.layer_commands.len(),
            AuxBatch::Composite(node) => self
                .composite_layer_batches
                .get(&node)
                .map_or(0, |b| b.layer_commands.len()),
        }
    }

    fn composite_layer_bounds(&self) -> Option<[f32; 4]> {
        self.active_composite_layer.and_then(|_| {
            crate::GpuRenderer::composite_subtree_bounds(
                self.active_batch(),
                [self.viewport.width, self.viewport.height],
            )
        })
    }

    fn bg_batch_is_composite(&self) -> bool {
        self.active_composite_layer.is_some()
    }

    fn bg_batch_is_dynamic(&self) -> bool {
        self.aux_batch() == AuxBatch::Dynamic
    }

    fn can_promote_composite_layer(&self) -> bool {
        !self.has_static_group() && !self.layer_stack.iter().any(|s| s.cross_stream.is_some())
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
        self.bg_primitive_aabb_impl(start, end)
    }

    fn current_affine_elements(&self) -> [f32; 6] {
        self.current_affine().elements
    }

    fn current_clip_aabb(&self) -> Option<[f32; 4]> {
        self.current_clip_aabb_impl()
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
            self.emit_native_path(tessellated, &brush_info);
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
            self.emit_native_path(tessellated, &brush_info);
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

            self.emit_native_glass(glass, style.depth > 0);
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

            self.emit_native_glass(glass, false);
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
            let aux_offset = self.active_batch_mut().aux_data.len() as f32;
            let shape_count = self.current_3d_group_shapes.len() as f32;

            // Find max depth across all child shapes for AABB
            let mut max_depth: f32 = 1.0;
            for shape in &self.current_3d_group_shapes {
                max_depth = max_depth.max(shape.params[1]); // params[1] = depth
            }

            // Push each ShapeDesc as 4 vec4s into aux_data
            for index in 0..self.current_3d_group_shapes.len() {
                let shape = self.current_3d_group_shapes[index];
                self.active_batch_mut().aux_data.extend_from_slice(&[
                    shape.offset,
                    shape.params,
                    shape.half_ext,
                    shape.color,
                ]);
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
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);

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
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
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
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
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
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
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
            self.emit_native_glass(glass, style.depth > 0);
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
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
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
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
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
                self.emit_native_primitive(prim);
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
        self.emit_rgba_pixels(data, width, height, dest);
    }

    fn draw_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow) {
        // A shadow with no colour paints nothing, but the shader would still run over its whole
        // blurred area.
        if shadow.color.a * self.combined_opacity() <= 0.0 {
            return;
        }
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
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
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
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
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
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
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
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
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
        // The canvas viewport remains active for every mesh in the scene.
        let pending = PendingMesh {
            mesh,
            transform,
            camera,
            lights: self.lights.clone(),
            viewport,
            env_cubemap: self.pending_env.clone(),
        };
        if !self.capture_group_mesh(pending.clone()) {
            self.pending_meshes.push(pending);
        }
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
        self.push_native_layer(config, None);
    }

    fn pop_layer(&mut self) {
        self.pop_native_layer();
    }

    fn push_cross_stream_layer(&mut self, id: u64, config: LayerConfig) -> bool {
        self.begin_cross_stream_layer(id, config)
    }

    fn supports_deferred_text(&self) -> bool {
        self.deferred_text
    }

    fn defer_text(&mut self, id: u64) -> bool {
        self.capture_text(id)
    }

    fn supports_deferred_svg(&self) -> bool {
        self.deferred_svg
    }

    fn defer_svg(&mut self, id: u64) -> bool {
        self.capture_svg(id)
    }

    fn supports_deferred_image(&self) -> bool {
        self.deferred_image
    }
    fn supports_glass_layers(&self) -> bool {
        self.glass_layers
    }
    fn push_glass_layer(&mut self, id: u64, config: LayerConfig) -> bool {
        if !self.glass_layers
            || !self.emit_foreground()
            || !self.begin_cross_stream_layer(id, config)
        {
            return false;
        }
        self.layer_stack.last_mut().unwrap().captures_glass = true;
        true
    }
    fn defer_image(&mut self, id: u64) -> bool {
        self.capture_image(id)
    }

    fn sample_layer(&mut self, id: LayerId, source_rect: Rect, dest_rect: Rect) {
        let Some(paint) = self.image_geometry(dest_rect) else {
            return;
        };
        let command = crate::primitives::LayerCommand::Sample {
            id,
            source: source_rect,
            dest: dest_rect,
            paint: Some(Box::new(paint)),
        };
        let foreground = self.emit_foreground();
        let owner = self.layer_command_batch();
        let batch = self.layer_command_batch_mut(owner);
        if foreground {
            batch.push_foreground_layer_command(command);
        } else {
            batch.push_layer_command(command);
        }
    }

    fn viewport_size(&self) -> Size {
        self.viewport
    }

    fn is_3d_context(&self) -> bool {
        self.is_3d
    }

    fn current_opacity(&self) -> f32 {
        // Queries and late image draws need the effective opacity even when
        // primitive colours leave group opacity to the compositor.
        self.opacity_stack.iter().product::<f32>()
            * self
                .layer_stack
                .iter()
                .map(|state| state.config.opacity)
                .filter(|opacity| *opacity < 1.0)
                .product::<f32>()
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
        self.last_group_mesh = None;
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
        let pending = PendingGpuPass {
            pass: pass.clone(),
            viewport: pixel_viewport,
            flow: None,
        };
        if !self.capture_group_gpu_pass(pending.clone(), viewport.is_none()) {
            self.pending_gpu_passes.push(pending);
        }
    }

    fn draw_shader(
        &mut self,
        shader: &std::sync::Arc<blinc_core::FlowShader>,
        rect: blinc_core::Rect,
        inputs: &blinc_core::ShaderInputs,
    ) {
        self.last_group_mesh = None;
        let tl = self.transform_point(rect.origin);
        let br = self.transform_point(blinc_core::Point::new(
            rect.origin.x + rect.size.width,
            rect.origin.y + rect.size.height,
        ));
        let (left, top) = (tl.x.min(br.x), tl.y.min(br.y));
        let (width, height) = ((br.x - tl.x).abs(), (br.y - tl.y).abs());
        let mut visible = [left, top, width, height];
        // The clip leaves part of the bounds visible; the shader still maps `uv` over all of them.
        if let Some([cx, cy, cw, ch]) = self.current_clip_aabb() {
            let (x0, y0) = (left.max(cx), top.max(cy));
            let (x1, y1) = ((left + width).min(cx + cw), (top + height).min(cy + ch));
            visible = [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)];
        }
        if visible[2] < 1.0 || visible[3] < 1.0 {
            return;
        }
        let scale = if rect.size.width > f32::EPSILON {
            width / rect.size.width
        } else {
            1.0
        };
        let pending = PendingGpuPass {
            pass: crate::custom_pass::GpuPass::none(),
            viewport: Some(visible),
            flow: Some(PendingFlow {
                shader: shader.clone(),
                bounds: [left, top, width, height],
                inputs: *inputs,
                corner_radius: inputs.corner_radius * scale,
            }),
        };
        if !self.capture_group_gpu_pass(pending.clone(), false) {
            self.pending_gpu_passes.push(pending);
        }
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
        // Resolve the shared clip once for this mesh, rather than per triangle.
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type, clip_shape) =
            if let Some((bounds, pointer)) = self.native_primitive_path_clip() {
                (
                    bounds,
                    [0.0; 4],
                    [1.0; 4],
                    ClipType::Rect,
                    [pointer, 0.0, 0.0, 0.0],
                )
            } else {
                (
                    clip_bounds,
                    clip_radius,
                    clip_corner_shape,
                    clip_type,
                    [0.0; 4],
                )
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

            let aux_data = &mut self.active_batch_mut().aux_data;
            let aux_offset = aux_data.len() as u32;
            aux_data.push([v0[0], v0[1], v1[0], v1[1]]);
            aux_data.push([v2[0], v2[1], 0.0, 0.0]);
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
                aux_data.push([c0.r, c0.g, c0.b, c0.a]);
                aux_data.push([c1.r, c1.g, c1.b, c1.a]);
                aux_data.push([c2.r, c2.g, c2.b, c2.a]);
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
                clip_shape,
                type_info: [
                    PrimitiveType::Mesh as u32,
                    fill_type,
                    clip_type_u,
                    mesh_flag,
                ],
                ..GpuPrimitive::default()
            };
            prim.local_affine = [1.0, 0.0, 0.0, 1.0];

            self.push_native_primitive(prim);
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
