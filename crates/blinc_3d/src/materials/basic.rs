//! Basic unlit material

use super::{BlendMode, Material, MaterialType, Side, TextureHandle};
use blinc_core::Color;

/// Basic unlit material (like Three.js MeshBasicMaterial)
///
/// Renders with flat color/texture, no lighting calculations.
#[derive(Clone, Debug)]
pub struct BasicMaterial {
    /// Base color
    pub color: Color,
    /// Diffuse texture map
    pub map: Option<TextureHandle>,
    /// Opacity (0.0 = invisible, 1.0 = opaque)
    pub opacity: f32,
    /// Whether to use transparency
    pub transparent: bool,
    /// Wireframe rendering
    pub wireframe: bool,
    /// Wireframe line width
    pub wireframe_linewidth: f32,
    /// Which side to render
    pub side: Side,
    /// Blend mode
    pub blend_mode: BlendMode,
    /// Alpha test threshold (fragments below are discarded)
    pub alpha_test: f32,
    /// Depth test enabled
    pub depth_test: bool,
    /// Depth write enabled
    pub depth_write: bool,
}

impl Default for BasicMaterial {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            map: None,
            opacity: 1.0,
            transparent: false,
            wireframe: false,
            wireframe_linewidth: 1.0,
            side: Side::Front,
            blend_mode: BlendMode::Normal,
            alpha_test: 0.0,
            depth_test: true,
            depth_write: true,
        }
    }
}

impl BasicMaterial {
    /// Create a new basic material with default settings
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

    /// Set color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set texture map
    pub fn map(mut self, texture: TextureHandle) -> Self {
        self.map = Some(texture);
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

    /// Enable wireframe
    pub fn wireframe(mut self) -> Self {
        self.wireframe = true;
        self
    }

    /// Set side
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }
}

impl Material for BasicMaterial {
    fn material_type(&self) -> MaterialType {
        MaterialType::Basic
    }

    fn base_color(&self) -> Color {
        self.color
    }

    fn is_transparent(&self) -> bool {
        self.transparent || self.opacity < 1.0
    }
}
