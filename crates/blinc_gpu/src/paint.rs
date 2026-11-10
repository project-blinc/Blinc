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
    Light, Mat4, MaterialId, MeshData, MeshId, MeshInstance, ParticleSystemData, Path, Point, Rect,
    Sdf3DViewport, SdfBuilder, Shadow, Size, Stroke, TextStyle, Transform,
};

use crate::primitives::{ClipType, GpuGlassPrimitive, GpuPrimitive, PrimitiveBatch};
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
mod circles;
mod clip;
mod context;
mod element_state;
mod native_clip;
mod overlay;
mod particles;
mod path_clip;
mod pending;
pub use pending::{GroupGpuPass, GroupMeshes, PendingFlow, PendingGpuPass, PendingMesh};
mod paths;
mod rects;
mod rgba;
mod routing;
mod sdf_builder;
mod shadows;
mod text_draw;
mod transform;
mod viewport3d;
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
        self.fill_path_impl(path, brush)
    }

    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, brush: Brush) {
        self.stroke_path_impl(path, stroke, brush)
    }

    fn fill_rect(&mut self, rect: Rect, corner_radius: CornerRadius, brush: Brush) {
        self.fill_rect_impl(rect, corner_radius, brush)
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
        self.fill_notch_impl(
            rect,
            corner_radii,
            corner_types,
            top_mod,
            bottom_mod,
            border,
            shadow,
            brush,
        )
    }

    fn fill_rect_with_per_side_border(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        brush: Brush,
        border_widths: [f32; 4],
        border_color: Color,
    ) {
        self.fill_rect_with_per_side_border_impl(
            rect,
            corner_radius,
            brush,
            border_widths,
            border_color,
        )
    }

    fn stroke_rect(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        stroke: &Stroke,
        brush: Brush,
    ) {
        self.stroke_rect_impl(rect, corner_radius, stroke, brush)
    }

    fn fill_circle(&mut self, center: Point, radius: f32, brush: Brush) {
        self.fill_circle_impl(center, radius, brush)
    }

    fn stroke_circle(&mut self, center: Point, radius: f32, stroke: &Stroke, brush: Brush) {
        self.stroke_circle_impl(center, radius, stroke, brush)
    }

    fn draw_text(&mut self, text: &str, origin: Point, style: &TextStyle) {
        self.draw_text_impl(text, origin, style)
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
        self.draw_shadow_impl(rect, corner_radius, shadow)
    }

    fn draw_inner_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow) {
        self.draw_inner_shadow_impl(rect, corner_radius, shadow)
    }

    fn draw_circle_shadow(&mut self, center: Point, radius: f32, shadow: Shadow) {
        self.draw_circle_shadow_impl(center, radius, shadow)
    }

    fn draw_circle_inner_shadow(&mut self, center: Point, radius: f32, shadow: Shadow) {
        self.draw_circle_inner_shadow_impl(center, radius, shadow)
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
        self.draw_mesh_data_impl(mesh, transform)
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
        self.billboard_draw_impl(_size, _transform, _facing, f)
    }

    fn viewport_3d_draw(
        &mut self,
        _rect: Rect,
        camera: &Camera,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        self.viewport_3d_draw_impl(_rect, camera, f)
    }

    fn draw_sdf_viewport(&mut self, rect: Rect, viewport: &Sdf3DViewport) {
        self.draw_sdf_viewport_impl(rect, viewport)
    }

    fn draw_particles(&mut self, rect: Rect, particle_data: &ParticleSystemData) {
        self.draw_particles_impl(rect, particle_data)
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
        self.run_gpu_pass_impl(hook, viewport)
    }

    fn draw_shader(
        &mut self,
        shader: &std::sync::Arc<blinc_core::FlowShader>,
        rect: blinc_core::Rect,
        inputs: &blinc_core::ShaderInputs,
    ) {
        self.draw_shader_impl(shader, rect, inputs)
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
