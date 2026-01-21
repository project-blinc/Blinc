//! Standard PBR material

use super::{BlendMode, Material, MaterialType, Side, TextureHandle};
use blinc_core::Color;

/// Standard PBR material (like Three.js MeshStandardMaterial)
///
/// Uses metallic-roughness workflow for physically-based rendering.
#[derive(Clone, Debug)]
pub struct StandardMaterial {
    /// Base color (albedo)
    pub color: Color,
    /// Albedo texture map
    pub map: Option<TextureHandle>,
    /// Metalness factor (0.0 = dielectric, 1.0 = metal)
    pub metalness: f32,
    /// Metalness texture map
    pub metalness_map: Option<TextureHandle>,
    /// Roughness factor (0.0 = smooth/mirror, 1.0 = rough)
    pub roughness: f32,
    /// Roughness texture map
    pub roughness_map: Option<TextureHandle>,
    /// Normal map
    pub normal_map: Option<TextureHandle>,
    /// Normal map scale
    pub normal_scale: f32,
    /// Ambient occlusion map
    pub ao_map: Option<TextureHandle>,
    /// AO intensity
    pub ao_intensity: f32,
    /// Emissive color
    pub emissive: Color,
    /// Emissive map
    pub emissive_map: Option<TextureHandle>,
    /// Emissive intensity
    pub emissive_intensity: f32,
    /// Opacity
    pub opacity: f32,
    /// Transparent
    pub transparent: bool,
    /// Alpha test threshold
    pub alpha_test: f32,
    /// Which side to render
    pub side: Side,
    /// Wireframe mode
    pub wireframe: bool,
    /// Flat shading (per-face normals)
    pub flat_shading: bool,
    /// Blend mode
    pub blend_mode: BlendMode,
    /// Environment map intensity
    pub env_map_intensity: f32,
}

impl Default for StandardMaterial {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            map: None,
            metalness: 0.0,
            metalness_map: None,
            roughness: 0.5,
            roughness_map: None,
            normal_map: None,
            normal_scale: 1.0,
            ao_map: None,
            ao_intensity: 1.0,
            emissive: Color::BLACK,
            emissive_map: None,
            emissive_intensity: 1.0,
            opacity: 1.0,
            transparent: false,
            alpha_test: 0.0,
            side: Side::Front,
            wireframe: false,
            flat_shading: false,
            blend_mode: BlendMode::Normal,
            env_map_intensity: 1.0,
        }
    }
}

impl StandardMaterial {
    /// Create a new standard material
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

    /// Create a metal material
    pub fn metal(color: Color, roughness: f32) -> Self {
        Self {
            color,
            metalness: 1.0,
            roughness,
            ..Default::default()
        }
    }

    /// Create a plastic/dielectric material
    pub fn plastic(color: Color, roughness: f32) -> Self {
        Self {
            color,
            metalness: 0.0,
            roughness,
            ..Default::default()
        }
    }

    /// Set color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set metalness
    pub fn metalness(mut self, metalness: f32) -> Self {
        self.metalness = metalness.clamp(0.0, 1.0);
        self
    }

    /// Set roughness
    pub fn roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness.clamp(0.0, 1.0);
        self
    }

    /// Set emissive color and intensity
    pub fn emissive(mut self, color: Color, intensity: f32) -> Self {
        self.emissive = color;
        self.emissive_intensity = intensity;
        self
    }

    /// Set normal map
    pub fn normal_map(mut self, map: TextureHandle, scale: f32) -> Self {
        self.normal_map = Some(map);
        self.normal_scale = scale;
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

impl Material for StandardMaterial {
    fn material_type(&self) -> MaterialType {
        MaterialType::Standard
    }

    fn base_color(&self) -> Color {
        self.color
    }

    fn is_transparent(&self) -> bool {
        self.transparent || self.opacity < 1.0
    }
}
