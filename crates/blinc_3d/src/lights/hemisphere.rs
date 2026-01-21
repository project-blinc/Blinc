//! Hemisphere light

use super::{Light, LightType};
use crate::ecs::Component;
use blinc_core::Color;

/// Hemisphere light (sky/ground gradient)
///
/// Hemisphere lights provide ambient illumination that varies
/// based on the surface normal's orientation relative to the
/// light's up direction. Surfaces facing up receive the sky
/// color, while surfaces facing down receive the ground color.
#[derive(Clone, Debug)]
pub struct HemisphereLight {
    /// Sky color (color for surfaces facing up)
    pub sky_color: Color,
    /// Ground color (color for surfaces facing down)
    pub ground_color: Color,
    /// Light intensity
    pub intensity: f32,
}

impl Component for HemisphereLight {}

impl Default for HemisphereLight {
    fn default() -> Self {
        Self {
            sky_color: Color::rgb(0.6, 0.75, 1.0), // Light blue sky
            ground_color: Color::rgb(0.4, 0.3, 0.2), // Brown ground
            intensity: 0.5,
        }
    }
}

impl HemisphereLight {
    /// Create a new hemisphere light
    pub fn new(sky_color: Color, ground_color: Color, intensity: f32) -> Self {
        Self {
            sky_color,
            ground_color,
            intensity,
        }
    }

    /// Create a standard outdoor hemisphere light
    pub fn outdoor() -> Self {
        Self::default()
    }

    /// Create an indoor hemisphere light
    pub fn indoor() -> Self {
        Self {
            sky_color: Color::rgb(0.9, 0.9, 0.95),
            ground_color: Color::rgb(0.3, 0.3, 0.3),
            intensity: 0.4,
        }
    }

    /// Set sky color
    pub fn sky_color(mut self, color: Color) -> Self {
        self.sky_color = color;
        self
    }

    /// Set ground color
    pub fn ground_color(mut self, color: Color) -> Self {
        self.ground_color = color;
        self
    }

    /// Set intensity
    pub fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    /// Get interpolated color for a given up-facing factor (0 = down, 1 = up)
    pub fn color_at(&self, up_factor: f32) -> Color {
        Color::lerp(&self.ground_color, &self.sky_color, up_factor)
    }
}

impl Light for HemisphereLight {
    fn light_type(&self) -> LightType {
        LightType::Hemisphere
    }

    fn color(&self) -> Color {
        // Return sky color as the primary color
        self.sky_color
    }

    fn intensity(&self) -> f32 {
        self.intensity
    }
}
