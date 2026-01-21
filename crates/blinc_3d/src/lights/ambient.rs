//! Ambient light

use super::{Light, LightType};
use crate::ecs::Component;
use blinc_core::Color;

/// Ambient light providing uniform illumination
///
/// Ambient light illuminates all objects in the scene equally,
/// regardless of their position or orientation. Use sparingly
/// as it can flatten the appearance of objects.
#[derive(Clone, Debug)]
pub struct AmbientLight {
    /// Light color
    pub color: Color,
    /// Light intensity
    pub intensity: f32,
}

impl Component for AmbientLight {}

impl Default for AmbientLight {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            intensity: 0.1,
        }
    }
}

impl AmbientLight {
    /// Create a new ambient light
    pub fn new(color: Color, intensity: f32) -> Self {
        Self { color, intensity }
    }

    /// Create with white color
    pub fn white(intensity: f32) -> Self {
        Self {
            color: Color::WHITE,
            intensity,
        }
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
}

impl Light for AmbientLight {
    fn light_type(&self) -> LightType {
        LightType::Ambient
    }

    fn color(&self) -> Color {
        self.color
    }

    fn intensity(&self) -> f32 {
        self.intensity
    }
}
