//! Point light

use super::{Light, LightType, ShadowConfig};
use crate::ecs::Component;
use blinc_core::Color;

/// Point light (omnidirectional)
///
/// Point lights emit light in all directions from a single point,
/// like a light bulb. Light intensity falls off with distance
/// based on the decay parameter.
#[derive(Clone, Debug)]
pub struct PointLight {
    /// Light color
    pub color: Color,
    /// Light intensity
    pub intensity: f32,
    /// Maximum distance of light influence (0 = infinite)
    pub distance: f32,
    /// Light decay factor (default 2 for physically correct)
    pub decay: f32,
    /// Whether this light casts shadows
    pub cast_shadows: bool,
    /// Shadow configuration
    pub shadow: ShadowConfig,
}

impl Component for PointLight {}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            intensity: 1.0,
            distance: 0.0,
            decay: 2.0,
            cast_shadows: false,
            shadow: ShadowConfig::default(),
        }
    }
}

impl PointLight {
    /// Create a new point light
    pub fn new(color: Color, intensity: f32) -> Self {
        Self {
            color,
            intensity,
            ..Default::default()
        }
    }

    /// Create a white point light
    pub fn white(intensity: f32) -> Self {
        Self::new(Color::WHITE, intensity)
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

    /// Set maximum distance
    pub fn distance(mut self, distance: f32) -> Self {
        self.distance = distance;
        self
    }

    /// Set decay factor
    pub fn decay(mut self, decay: f32) -> Self {
        self.decay = decay;
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

    /// Calculate attenuation at a given distance
    pub fn attenuation(&self, dist: f32) -> f32 {
        if self.distance > 0.0 && dist > self.distance {
            return 0.0;
        }
        if self.decay == 0.0 {
            return 1.0;
        }
        1.0 / (1.0 + dist.powf(self.decay))
    }
}

impl Light for PointLight {
    fn light_type(&self) -> LightType {
        LightType::Point
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
