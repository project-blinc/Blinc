//! Color animation integration
//!
//! Provides wrappers for animating colors using spring physics.

use blinc_animation::{Spring, SpringConfig};
use blinc_core::Color;

/// Animated color using spring physics
#[derive(Clone, Debug)]
pub struct AnimatedColor {
    /// Red component spring
    r: Spring,
    /// Green component spring
    g: Spring,
    /// Blue component spring
    b: Spring,
    /// Alpha component spring
    a: Spring,
}

impl AnimatedColor {
    /// Create a new animated color
    pub fn new(color: Color) -> Self {
        let config = SpringConfig::stiff();
        Self {
            r: Spring::new(config, color.r),
            g: Spring::new(config, color.g),
            b: Spring::new(config, color.b),
            a: Spring::new(config, color.a),
        }
    }

    /// Create with spring configuration
    pub fn with_spring(color: Color, stiffness: f32, damping: f32) -> Self {
        let config = SpringConfig::new(stiffness, damping, 1.0);
        Self {
            r: Spring::new(config, color.r),
            g: Spring::new(config, color.g),
            b: Spring::new(config, color.b),
            a: Spring::new(config, color.a),
        }
    }

    /// Create with preset spring config
    pub fn with_config(color: Color, config: SpringConfig) -> Self {
        Self {
            r: Spring::new(config, color.r),
            g: Spring::new(config, color.g),
            b: Spring::new(config, color.b),
            a: Spring::new(config, color.a),
        }
    }

    /// Set target color
    pub fn set_target(&mut self, color: Color) {
        self.r.set_target(color.r);
        self.g.set_target(color.g);
        self.b.set_target(color.b);
        self.a.set_target(color.a);
    }

    /// Set color immediately (no animation)
    pub fn set_immediate(&mut self, color: Color) {
        let config = SpringConfig::stiff();
        self.r = Spring::new(config, color.r);
        self.g = Spring::new(config, color.g);
        self.b = Spring::new(config, color.b);
        self.a = Spring::new(config, color.a);
    }

    /// Get current color
    pub fn get(&self) -> Color {
        Color::rgba(self.r.value(), self.g.value(), self.b.value(), self.a.value())
    }

    /// Get target color
    pub fn target(&self) -> Color {
        Color::rgba(
            self.r.target(),
            self.g.target(),
            self.b.target(),
            self.a.target(),
        )
    }

    /// Check if animation is complete
    pub fn is_at_rest(&self) -> bool {
        self.r.is_settled()
            && self.g.is_settled()
            && self.b.is_settled()
            && self.a.is_settled()
    }

    /// Update animation
    pub fn update(&mut self, dt: f32) {
        self.r.step(dt);
        self.g.step(dt);
        self.b.step(dt);
        self.a.step(dt);
    }

    /// Set target alpha only
    pub fn set_target_alpha(&mut self, alpha: f32) {
        self.a.set_target(alpha);
    }

    /// Fade in
    pub fn fade_in(&mut self) {
        self.a.set_target(1.0);
    }

    /// Fade out
    pub fn fade_out(&mut self) {
        self.a.set_target(0.0);
    }
}

impl Default for AnimatedColor {
    fn default() -> Self {
        Self::new(Color::WHITE)
    }
}

impl From<Color> for AnimatedColor {
    fn from(color: Color) -> Self {
        Self::new(color)
    }
}

/// Color palette with animated transitions
#[derive(Clone, Debug)]
pub struct AnimatedPalette {
    /// Named colors in the palette
    colors: Vec<(String, AnimatedColor)>,
    /// Active color index
    active: usize,
}

impl AnimatedPalette {
    /// Create a new palette
    pub fn new() -> Self {
        Self {
            colors: Vec::new(),
            active: 0,
        }
    }

    /// Add a color to the palette
    pub fn add(&mut self, name: impl Into<String>, color: Color) {
        self.colors.push((name.into(), AnimatedColor::new(color)));
    }

    /// Add a color with spring config
    pub fn add_with_spring(
        &mut self,
        name: impl Into<String>,
        color: Color,
        stiffness: f32,
        damping: f32,
    ) {
        self.colors
            .push((name.into(), AnimatedColor::with_spring(color, stiffness, damping)));
    }

    /// Get color by name
    pub fn get(&self, name: &str) -> Option<Color> {
        self.colors
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c.get())
    }

    /// Get animated color by name
    pub fn get_animated(&mut self, name: &str) -> Option<&mut AnimatedColor> {
        self.colors
            .iter_mut()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c)
    }

    /// Set active color by name
    pub fn set_active(&mut self, name: &str) {
        if let Some(idx) = self.colors.iter().position(|(n, _)| n == name) {
            self.active = idx;
        }
    }

    /// Get active color
    pub fn active_color(&self) -> Option<Color> {
        self.colors.get(self.active).map(|(_, c)| c.get())
    }

    /// Update all colors
    pub fn update(&mut self, dt: f32) {
        for (_, color) in &mut self.colors {
            color.update(dt);
        }
    }
}

impl Default for AnimatedPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// Material color animator for animating material properties
#[derive(Clone, Debug)]
pub struct MaterialColorAnimator {
    /// Base color
    pub color: AnimatedColor,
    /// Emissive color
    pub emissive: AnimatedColor,
    /// Emissive intensity spring
    emissive_intensity: Spring,
}

impl MaterialColorAnimator {
    /// Create a new material color animator
    pub fn new(color: Color) -> Self {
        Self {
            color: AnimatedColor::new(color),
            emissive: AnimatedColor::new(Color::BLACK),
            emissive_intensity: Spring::new(SpringConfig::stiff(), 0.0),
        }
    }

    /// Set spring config for all properties
    pub fn with_spring(mut self, stiffness: f32, damping: f32) -> Self {
        self.color = AnimatedColor::with_spring(self.color.get(), stiffness, damping);
        self.emissive = AnimatedColor::with_spring(self.emissive.get(), stiffness, damping);
        let config = SpringConfig::new(stiffness, damping, 1.0);
        self.emissive_intensity = Spring::new(config, self.emissive_intensity.value());
        self
    }

    /// Set target base color
    pub fn set_target_color(&mut self, color: Color) {
        self.color.set_target(color);
    }

    /// Set target emissive
    pub fn set_target_emissive(&mut self, color: Color, intensity: f32) {
        self.emissive.set_target(color);
        self.emissive_intensity.set_target(intensity);
    }

    /// Get emissive intensity
    pub fn emissive_intensity(&self) -> f32 {
        self.emissive_intensity.value()
    }

    /// Start glowing
    pub fn glow(&mut self, color: Color, intensity: f32) {
        self.emissive.set_target(color);
        self.emissive_intensity.set_target(intensity);
    }

    /// Stop glowing
    pub fn stop_glow(&mut self) {
        self.emissive_intensity.set_target(0.0);
    }

    /// Update all animations
    pub fn update(&mut self, dt: f32) {
        self.color.update(dt);
        self.emissive.update(dt);
        self.emissive_intensity.step(dt);
    }

    /// Check if all animations are at rest
    pub fn is_at_rest(&self) -> bool {
        self.color.is_at_rest()
            && self.emissive.is_at_rest()
            && self.emissive_intensity.is_settled()
    }

    /// Apply to a basic material
    pub fn apply_to_basic(&self, material: &mut crate::materials::BasicMaterial) {
        material.color = self.color.get();
    }

    /// Apply to a standard material
    pub fn apply_to_standard(&self, material: &mut crate::materials::StandardMaterial) {
        material.color = self.color.get();
        material.emissive = self.emissive.get();
        material.emissive_intensity = self.emissive_intensity.value();
    }

    /// Apply to a phong material
    pub fn apply_to_phong(&self, material: &mut crate::materials::PhongMaterial) {
        material.color = self.color.get();
        material.emissive = self.emissive.get();
        material.emissive_intensity = self.emissive_intensity.value();
    }
}

impl Default for MaterialColorAnimator {
    fn default() -> Self {
        Self::new(Color::WHITE)
    }
}
