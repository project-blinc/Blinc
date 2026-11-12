//! Construction and configuration of the paint context.

use super::GpuPaintContext;
use crate::primitives::PrimitiveBatch;
use crate::text::TextRenderingContext;
use blinc_core::{Affine2D, BlendMode, FillRule, Size};

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
