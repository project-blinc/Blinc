//! Material system

mod basic;
mod phong;
mod standard;

pub use basic::BasicMaterial;
pub use phong::PhongMaterial;
pub use standard::StandardMaterial;

use blinc_core::Color;

/// Material trait for all material types
pub trait Material: Send + Sync {
    /// Get the material type
    fn material_type(&self) -> MaterialType;

    /// Get base color
    fn base_color(&self) -> Color;

    /// Whether this material is transparent
    fn is_transparent(&self) -> bool {
        false
    }
}

/// Material types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialType {
    /// Unlit basic material
    Basic,
    /// PBR standard material
    Standard,
    /// Classic Phong shading
    Phong,
    /// Physical (extended PBR)
    Physical,
    /// Custom shader material
    Custom,
}

/// Handle to a material resource
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MaterialHandle(pub u64);

impl Default for MaterialHandle {
    fn default() -> Self {
        Self(0)
    }
}

/// Handle to a texture resource
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u64);

impl Default for TextureHandle {
    fn default() -> Self {
        Self(0)
    }
}

/// Which side of faces to render
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// Render front faces only
    #[default]
    Front,
    /// Render back faces only
    Back,
    /// Render both sides
    Double,
}

/// Blend mode for transparent materials
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    /// Standard alpha blending
    #[default]
    Normal,
    /// Additive blending
    Additive,
    /// Multiply blending
    Multiply,
}
