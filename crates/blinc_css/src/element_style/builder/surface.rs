//! Clip paths, materials, render layer, and opacity.

use blinc_core::ClipPath;

use crate::element_style::*;

use crate::material::{GlassMaterial, Material, MetallicMaterial, RenderLayer, WoodMaterial};

impl ElementStyle {
    // =========================================================================
    // Clip-Path
    // =========================================================================

    /// Set CSS clip-path shape function
    pub fn clip_path(mut self, path: ClipPath) -> Self {
        self.clip_path = Some(path);
        self
    }

    // =========================================================================
    // Material
    // =========================================================================

    /// Set material effect
    pub fn material(mut self, material: Material) -> Self {
        // Glass materials also set the render layer to Glass
        if matches!(material, Material::Glass(_)) {
            self.render_layer = Some(RenderLayer::Glass);
        }
        self.material = Some(material);
        self
    }

    /// Apply a visual effect
    pub fn effect(self, effect: impl Into<Material>) -> Self {
        self.material(effect.into())
    }

    /// Apply glass material with default settings
    pub fn glass(self) -> Self {
        self.material(Material::Glass(GlassMaterial::new()))
    }

    /// Apply glass material with custom settings
    pub fn glass_custom(self, glass: GlassMaterial) -> Self {
        self.material(Material::Glass(glass))
    }

    /// Apply metallic material with default settings
    pub fn metallic(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::new()))
    }

    /// Apply chrome metallic preset
    pub fn chrome(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::chrome()))
    }

    /// Apply gold metallic preset
    pub fn gold(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::gold()))
    }

    /// Apply wood material with default settings
    pub fn wood(self) -> Self {
        self.material(Material::Wood(WoodMaterial::new()))
    }

    // =========================================================================
    // Layer
    // =========================================================================

    /// Set render layer
    pub fn layer(mut self, layer: RenderLayer) -> Self {
        self.render_layer = Some(layer);
        self
    }

    /// Render in foreground layer
    pub fn foreground(self) -> Self {
        self.layer(RenderLayer::Foreground)
    }

    /// Render in background layer
    pub fn layer_background(self) -> Self {
        self.layer(RenderLayer::Background)
    }

    // =========================================================================
    // Opacity
    // =========================================================================

    /// Set opacity (0.0 = transparent, 1.0 = opaque)
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity.clamp(0.0, 1.0));
        self
    }

    /// Fully opaque
    pub fn opaque(self) -> Self {
        self.opacity(1.0)
    }

    /// Semi-transparent (50% opacity)
    pub fn translucent(self) -> Self {
        self.opacity(0.5)
    }

    /// Fully transparent
    pub fn transparent(self) -> Self {
        self.opacity(0.0)
    }
}
