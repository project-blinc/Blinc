//! Cubemap-based skybox

use crate::materials::TextureHandle;
use blinc_core::Color;

/// Cubemap skybox using texture
///
/// Renders a skybox from a cubemap texture (6 faces) or equirectangular HDR image.
///
/// # Example
///
/// ```ignore
/// let texture = world.add_texture(load_hdri("sky.hdr"));
/// let skybox = CubemapSkybox::new(texture);
/// ```
#[derive(Clone, Debug)]
pub struct CubemapSkybox {
    /// Cubemap or equirectangular texture handle
    pub texture: TextureHandle,
    /// Rotation around Y axis (radians)
    pub rotation: f32,
    /// Exposure/brightness multiplier
    pub exposure: f32,
    /// Tint color
    pub tint: Color,
    /// Blur amount (0 = sharp, 1 = fully blurred)
    pub blur: f32,
}

impl CubemapSkybox {
    /// Create from texture handle
    pub fn new(texture: TextureHandle) -> Self {
        Self {
            texture,
            rotation: 0.0,
            exposure: 1.0,
            tint: Color::WHITE,
            blur: 0.0,
        }
    }

    /// Set rotation (radians)
    pub fn with_rotation(mut self, rotation: f32) -> Self {
        self.rotation = rotation;
        self
    }

    /// Set exposure
    pub fn with_exposure(mut self, exposure: f32) -> Self {
        self.exposure = exposure;
        self
    }

    /// Set tint color
    pub fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }

    /// Set blur amount
    pub fn with_blur(mut self, blur: f32) -> Self {
        self.blur = blur.clamp(0.0, 1.0);
        self
    }

    /// Rotate by degrees
    pub fn rotate_degrees(&mut self, degrees: f32) {
        self.rotation += degrees * std::f32::consts::PI / 180.0;
    }
}
