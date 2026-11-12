//! Per-element transient state: 3D, CSS filter, mask gradient, corner shape and overflow fade.

use super::GpuPaintContext;

impl GpuPaintContext<'_> {
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
}
