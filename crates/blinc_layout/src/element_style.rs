//! Unified element styling
//!
//! Provides `ElementStyle` - a consistent style schema for all visual properties
//! that can be applied to layout elements. This enables:
//!
//! - Consistent API across `Div`, `StatefulDiv`, and other elements
//! - State-dependent styling with full property support
//! - Style composition and merging
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! // Create a style
//! let style = ElementStyle::new()
//!     .bg(Color::BLUE)
//!     .rounded(8.0)
//!     .shadow_md()
//!     .scale(1.0);
//!
//! // Use with stateful elements
//! stateful_button()
//!     .idle(ElementStyle::new().bg(Color::BLUE))
//!     .hovered(ElementStyle::new().bg(Color::LIGHT_BLUE).scale(1.02))
//!     .pressed(ElementStyle::new().bg(Color::DARK_BLUE).scale(0.98));
//! ```

use blinc_core::{Brush, Color, CornerRadius, Shadow, Transform};

use crate::element::{GlassMaterial, Material, MetallicMaterial, RenderLayer, WoodMaterial};

/// Visual style properties for an element
///
/// All properties are optional - when merging styles, only set properties
/// will override. This enables state-specific styling where you only
/// override the properties that change for that state.
#[derive(Clone, Default)]
pub struct ElementStyle {
    /// Background brush (solid color, gradient, or glass)
    pub background: Option<Brush>,
    /// Corner radius
    pub corner_radius: Option<CornerRadius>,
    /// Drop shadow
    pub shadow: Option<Shadow>,
    /// Transform (scale, rotate, translate)
    pub transform: Option<Transform>,
    /// Material effect (glass, metallic, wood)
    pub material: Option<Material>,
    /// Render layer ordering
    pub render_layer: Option<RenderLayer>,
    /// Opacity (0.0 = transparent, 1.0 = opaque)
    pub opacity: Option<f32>,
}

impl ElementStyle {
    /// Create a new empty style
    pub fn new() -> Self {
        Self::default()
    }

    // =========================================================================
    // Background
    // =========================================================================

    /// Set background color
    pub fn bg(mut self, color: impl Into<Brush>) -> Self {
        self.background = Some(color.into());
        self
    }

    /// Set background to a solid color
    pub fn bg_color(mut self, color: Color) -> Self {
        self.background = Some(Brush::Solid(color));
        self
    }

    /// Set background brush (for gradients, etc.)
    pub fn background(mut self, brush: Brush) -> Self {
        self.background = Some(brush);
        self
    }

    // =========================================================================
    // Corner Radius
    // =========================================================================

    /// Set uniform corner radius
    pub fn rounded(mut self, radius: f32) -> Self {
        self.corner_radius = Some(CornerRadius::uniform(radius));
        self
    }

    /// Set corner radius to full pill shape
    pub fn rounded_full(mut self) -> Self {
        self.corner_radius = Some(CornerRadius::uniform(9999.0));
        self
    }

    /// Set individual corner radii (top-left, top-right, bottom-right, bottom-left)
    pub fn rounded_corners(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.corner_radius = Some(CornerRadius::new(tl, tr, br, bl));
        self
    }

    /// Set corner radius directly
    pub fn corner_radius(mut self, radius: CornerRadius) -> Self {
        self.corner_radius = Some(radius);
        self
    }

    // =========================================================================
    // Shadow
    // =========================================================================

    /// Set drop shadow
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Set shadow with parameters
    pub fn shadow_params(self, offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        self.shadow(Shadow::new(offset_x, offset_y, blur, color))
    }

    /// Small shadow preset (2px offset, 4px blur)
    pub fn shadow_sm(self) -> Self {
        self.shadow(Shadow::new(0.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.1)))
    }

    /// Medium shadow preset (4px offset, 8px blur)
    pub fn shadow_md(self) -> Self {
        self.shadow(Shadow::new(0.0, 4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.15)))
    }

    /// Large shadow preset (8px offset, 16px blur)
    pub fn shadow_lg(self) -> Self {
        self.shadow(Shadow::new(0.0, 8.0, 16.0, Color::rgba(0.0, 0.0, 0.0, 0.2)))
    }

    /// Extra large shadow preset (12px offset, 24px blur)
    pub fn shadow_xl(self) -> Self {
        self.shadow(Shadow::new(
            0.0,
            12.0,
            24.0,
            Color::rgba(0.0, 0.0, 0.0, 0.25),
        ))
    }

    /// Explicitly clear shadow (override any inherited shadow)
    pub fn shadow_none(mut self) -> Self {
        // Use a fully transparent shadow to indicate "no shadow"
        self.shadow = Some(Shadow::new(0.0, 0.0, 0.0, Color::TRANSPARENT));
        self
    }

    // =========================================================================
    // Transform
    // =========================================================================

    /// Set transform
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }

    /// Scale uniformly
    pub fn scale(self, factor: f32) -> Self {
        self.transform(Transform::scale(factor, factor))
    }

    /// Scale with different x and y factors
    pub fn scale_xy(self, sx: f32, sy: f32) -> Self {
        self.transform(Transform::scale(sx, sy))
    }

    /// Translate by x and y offset
    pub fn translate(self, x: f32, y: f32) -> Self {
        self.transform(Transform::translate(x, y))
    }

    /// Rotate by angle in radians
    pub fn rotate(self, angle: f32) -> Self {
        self.transform(Transform::rotate(angle))
    }

    /// Rotate by angle in degrees
    pub fn rotate_deg(self, degrees: f32) -> Self {
        self.rotate(degrees * std::f32::consts::PI / 180.0)
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

    // =========================================================================
    // Merging
    // =========================================================================

    /// Merge another style on top of this one
    ///
    /// Properties from `other` will override properties in `self` if they are set.
    /// Unset properties in `other` will not override.
    pub fn merge(&self, other: &ElementStyle) -> ElementStyle {
        ElementStyle {
            background: other.background.clone().or_else(|| self.background.clone()),
            corner_radius: other.corner_radius.or(self.corner_radius),
            shadow: other.shadow.clone().or_else(|| self.shadow.clone()),
            transform: other.transform.clone().or_else(|| self.transform.clone()),
            material: other.material.clone().or_else(|| self.material.clone()),
            render_layer: other.render_layer.or(self.render_layer),
            opacity: other.opacity.or(self.opacity),
        }
    }

    /// Check if any property is set
    pub fn is_empty(&self) -> bool {
        self.background.is_none()
            && self.corner_radius.is_none()
            && self.shadow.is_none()
            && self.transform.is_none()
            && self.material.is_none()
            && self.render_layer.is_none()
            && self.opacity.is_none()
    }
}

/// Create a new element style
pub fn style() -> ElementStyle {
    ElementStyle::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_builder() {
        let s = style().bg(Color::BLUE).rounded(8.0).shadow_md().scale(1.05);

        assert!(s.background.is_some());
        assert!(s.corner_radius.is_some());
        assert!(s.shadow.is_some());
        assert!(s.transform.is_some());
    }

    #[test]
    fn test_style_merge() {
        let base = style().bg(Color::BLUE).rounded(8.0).shadow_sm();

        let hover = style().bg(Color::GREEN).scale(1.02);

        let merged = base.merge(&hover);

        // Background should be overridden
        assert!(matches!(merged.background, Some(Brush::Solid(c)) if c == Color::GREEN));
        // Corner radius should be preserved from base
        assert!(merged.corner_radius.is_some());
        // Shadow should be preserved from base
        assert!(merged.shadow.is_some());
        // Transform should come from hover
        assert!(merged.transform.is_some());
    }

    #[test]
    fn test_style_empty() {
        let empty = ElementStyle::new();
        assert!(empty.is_empty());

        let non_empty = style().bg(Color::RED);
        assert!(!non_empty.is_empty());
    }
}
