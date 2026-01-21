//! Spot light

use super::{Light, LightType, ShadowConfig};
use crate::ecs::Component;
use blinc_core::Color;

/// Spot light (cone-shaped)
///
/// Spot lights emit light in a cone from a single point in a
/// specific direction. Use angle to control the cone width and
/// penumbra to control the soft edge falloff.
#[derive(Clone, Debug)]
pub struct SpotLight {
    /// Light color
    pub color: Color,
    /// Light intensity
    pub intensity: f32,
    /// Maximum distance of light influence (0 = infinite)
    pub distance: f32,
    /// Cone angle in radians (maximum is PI/2)
    pub angle: f32,
    /// Penumbra factor (0 = hard edge, 1 = fully soft edge)
    pub penumbra: f32,
    /// Light decay factor (default 2 for physically correct)
    pub decay: f32,
    /// Whether this light casts shadows
    pub cast_shadows: bool,
    /// Shadow configuration
    pub shadow: ShadowConfig,
}

impl Component for SpotLight {}

impl Default for SpotLight {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            intensity: 1.0,
            distance: 0.0,
            angle: std::f32::consts::PI / 6.0, // 30 degrees
            penumbra: 0.0,
            decay: 2.0,
            cast_shadows: false,
            shadow: ShadowConfig::default(),
        }
    }
}

impl SpotLight {
    /// Create a new spot light
    pub fn new(color: Color, intensity: f32) -> Self {
        Self {
            color,
            intensity,
            ..Default::default()
        }
    }

    /// Create a white spot light
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

    /// Set cone angle in radians
    pub fn angle(mut self, angle: f32) -> Self {
        self.angle = angle.min(std::f32::consts::FRAC_PI_2);
        self
    }

    /// Set cone angle in degrees
    pub fn angle_degrees(mut self, degrees: f32) -> Self {
        self.angle = (degrees.min(90.0) * std::f32::consts::PI / 180.0);
        self
    }

    /// Set penumbra factor
    pub fn penumbra(mut self, penumbra: f32) -> Self {
        self.penumbra = penumbra.clamp(0.0, 1.0);
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

    /// Get the inner cone cosine
    pub fn inner_cone_cos(&self) -> f32 {
        self.angle.cos()
    }

    /// Get the outer cone cosine (with penumbra)
    pub fn outer_cone_cos(&self) -> f32 {
        let outer_angle = self.angle * (1.0 + self.penumbra);
        outer_angle.min(std::f32::consts::FRAC_PI_2).cos()
    }
}

impl Light for SpotLight {
    fn light_type(&self) -> LightType {
        LightType::Spot
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
