//! Phong shading material

use super::{BlendMode, Material, MaterialType, Side, TextureHandle};
use blinc_core::Color;

/// Phong material (like Three.js MeshPhongMaterial)
///
/// Classic Blinn-Phong shading with diffuse, specular, and shininess.
#[derive(Clone, Debug)]
pub struct PhongMaterial {
    /// Diffuse color
    pub color: Color,
    /// Diffuse texture map
    pub map: Option<TextureHandle>,
    /// Specular color
    pub specular: Color,
    /// Shininess (specular power)
    pub shininess: f32,
    /// Specular map
    pub specular_map: Option<TextureHandle>,
    /// Emissive color
    pub emissive: Color,
    /// Emissive map
    pub emissive_map: Option<TextureHandle>,
    /// Emissive intensity
    pub emissive_intensity: f32,
    /// Normal map
    pub normal_map: Option<TextureHandle>,
    /// Normal scale
    pub normal_scale: f32,
    /// Opacity
    pub opacity: f32,
    /// Transparent
    pub transparent: bool,
    /// Alpha test
    pub alpha_test: f32,
    /// Which side to render
    pub side: Side,
    /// Wireframe mode
    pub wireframe: bool,
    /// Flat shading
    pub flat_shading: bool,
    /// Blend mode
    pub blend_mode: BlendMode,
}

impl Default for PhongMaterial {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            map: None,
            specular: Color::rgb(0.2, 0.2, 0.2),
            shininess: 30.0,
            specular_map: None,
            emissive: Color::BLACK,
            emissive_map: None,
            emissive_intensity: 1.0,
            normal_map: None,
            normal_scale: 1.0,
            opacity: 1.0,
            transparent: false,
            alpha_test: 0.0,
            side: Side::Front,
            wireframe: false,
            flat_shading: false,
            blend_mode: BlendMode::Normal,
        }
    }
}

impl PhongMaterial {
    /// Create a new Phong material
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with color
    pub fn with_color(color: Color) -> Self {
        Self {
            color,
            ..Default::default()
        }
    }

    /// Set diffuse color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set specular color
    pub fn specular(mut self, color: Color) -> Self {
        self.specular = color;
        self
    }

    /// Set shininess
    pub fn shininess(mut self, shininess: f32) -> Self {
        self.shininess = shininess.max(0.0);
        self
    }

    /// Set emissive
    pub fn emissive(mut self, color: Color, intensity: f32) -> Self {
        self.emissive = color;
        self.emissive_intensity = intensity;
        self
    }

    /// Set opacity
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        if opacity < 1.0 {
            self.transparent = true;
        }
        self
    }

    /// Set side
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }
}

impl Material for PhongMaterial {
    fn material_type(&self) -> MaterialType {
        MaterialType::Phong
    }

    fn base_color(&self) -> Color {
        self.color
    }

    fn is_transparent(&self) -> bool {
        self.transparent || self.opacity < 1.0
    }
}
