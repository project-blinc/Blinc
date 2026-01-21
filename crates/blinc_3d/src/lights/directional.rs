//! Directional light

use super::{Light, LightType, ShadowConfig};
use crate::ecs::Component;
use blinc_core::Color;

/// Directional light (like sunlight)
///
/// Directional lights emit parallel rays in a specific direction,
/// simulating a light source at infinite distance (like the sun).
/// The position of the light affects shadow calculations but not
/// the lighting direction itself.
#[derive(Clone, Debug)]
pub struct DirectionalLight {
    /// Light color
    pub color: Color,
    /// Light intensity
    pub intensity: f32,
    /// Whether this light casts shadows
    pub cast_shadows: bool,
    /// Shadow configuration
    pub shadow: ShadowConfig,
    /// Shadow camera orthographic size
    pub shadow_camera_size: f32,
}

impl Component for DirectionalLight {}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            intensity: 1.0,
            cast_shadows: false,
            shadow: ShadowConfig::default(),
            shadow_camera_size: 10.0,
        }
    }
}

impl DirectionalLight {
    /// Create a new directional light
    pub fn new(color: Color, intensity: f32) -> Self {
        Self {
            color,
            intensity,
            ..Default::default()
        }
    }

    /// Create a white directional light
    pub fn white(intensity: f32) -> Self {
        Self::new(Color::WHITE, intensity)
    }

    /// Create a sunlight-colored directional light
    pub fn sun() -> Self {
        Self::new(Color::rgb(1.0, 0.96, 0.9), 1.0)
    }

    /// Set color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set intensity
    pub fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    /// Enable shadow casting
    pub fn with_shadows(mut self) -> Self {
        self.cast_shadows = true;
        self
    }

    /// Configure shadow
    pub fn shadow_config(mut self, config: ShadowConfig) -> Self {
        self.shadow = config;
        self
    }

    /// Set shadow camera size
    pub fn shadow_camera_size(mut self, size: f32) -> Self {
        self.shadow_camera_size = size;
        self
    }
}

impl Light for DirectionalLight {
    fn light_type(&self) -> LightType {
        LightType::Directional
    }

    fn color(&self) -> Color {
        self.color
    }

    fn intensity(&self) -> f32 {
        self.intensity
    }

    fn casts_shadows(&self) -> bool {
        self.cast_shadows
    }
}
