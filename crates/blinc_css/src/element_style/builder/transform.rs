//! Transforms.
//!
//! The 2D functions compose into one affine matrix. The 3D ones are kept
//! as separate fields because the renderer applies them outside it.

use blinc_core::Transform;

use crate::element_style::*;

impl ElementStyle {
    // =========================================================================
    // Transform
    // =========================================================================

    /// Set transform
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }

    /// Scale uniformly
    pub fn scale(self, factor: f32) -> Self {
        self.transform(Transform::scale(factor, factor))
    }

    /// Scale with different x and y factors
    pub fn scale_xy(self, sx: f32, sy: f32) -> Self {
        self.transform(Transform::scale(sx, sy))
    }

    /// Translate by x and y offset
    pub fn translate(self, x: f32, y: f32) -> Self {
        self.transform(Transform::translate(x, y))
    }

    /// Rotate by angle in radians
    pub fn rotate(self, angle: f32) -> Self {
        self.transform(Transform::rotate(angle))
    }

    /// Rotate by angle in degrees
    pub fn rotate_deg(self, degrees: f32) -> Self {
        self.rotate(degrees * std::f32::consts::PI / 180.0)
    }

    // =========================================================================
    // 3D Transform
    // =========================================================================

    /// Set X-axis rotation in degrees (3D tilt)
    pub fn rotate_x_deg(mut self, degrees: f32) -> Self {
        self.rotate_x = Some(degrees);
        self
    }

    /// Set Y-axis rotation in degrees (3D turn)
    pub fn rotate_y_deg(mut self, degrees: f32) -> Self {
        self.rotate_y = Some(degrees);
        self
    }

    /// Set perspective distance in pixels
    pub fn perspective_px(mut self, px: f32) -> Self {
        self.perspective = Some(px);
        self
    }

    /// Set 3D shape type
    pub fn shape_3d(mut self, shape: impl Into<String>) -> Self {
        self.shape_3d = Some(shape.into());
        self
    }

    /// Set 3D extrusion depth in pixels
    pub fn depth_px(mut self, px: f32) -> Self {
        self.depth = Some(px);
        self
    }

    /// Set light direction
    pub fn light_direction(mut self, x: f32, y: f32, z: f32) -> Self {
        self.light_direction = Some([x, y, z]);
        self
    }

    /// Set light intensity
    pub fn light_intensity(mut self, intensity: f32) -> Self {
        self.light_intensity = Some(intensity);
        self
    }

    /// Set ambient light level
    pub fn ambient_light(mut self, level: f32) -> Self {
        self.ambient = Some(level);
        self
    }

    /// Set specular power
    pub fn specular_power(mut self, power: f32) -> Self {
        self.specular = Some(power);
        self
    }

    /// Set translate-z offset in pixels (positive = toward viewer)
    pub fn translate_z_px(mut self, px: f32) -> Self {
        self.translate_z = Some(px);
        self
    }

    /// Set 3D boolean operation type
    pub fn op_3d_type(mut self, op: &str) -> Self {
        self.op_3d = Some(op.to_string());
        self
    }

    /// Set blend radius for smooth boolean operations
    pub fn blend_3d_px(mut self, px: f32) -> Self {
        self.blend_3d = Some(px);
        self
    }

    // =========================================================================
    // Transform Extras
    // =========================================================================

    /// Set skew X angle in degrees
    pub fn skew_x(mut self, deg: f32) -> Self {
        self.skew_x = Some(deg);
        self
    }

    /// Set skew Y angle in degrees
    pub fn skew_y(mut self, deg: f32) -> Self {
        self.skew_y = Some(deg);
        self
    }

    /// Set transform origin as percentages [x%, y%] (50, 50 = center)
    pub fn transform_origin(mut self, x: f32, y: f32) -> Self {
        self.transform_origin = Some([x, y]);
        self
    }
}
