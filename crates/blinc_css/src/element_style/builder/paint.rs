//! Surface paint: background, corner radius and shape, shadow.

use blinc_core::{Brush, Color, CornerRadius, CornerShape, Shadow};

use crate::element_style::*;
use blinc_theme::{ShadowTokens, ThemeState};

impl ElementStyle {
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
    // Corner Shape
    // =========================================================================

    /// Set uniform corner shape (superellipse n parameter)
    pub fn corner_shape(mut self, n: f32) -> Self {
        self.corner_shape = Some(CornerShape::uniform(n));
        self
    }

    /// Set per-corner shape values (top-left, top-right, bottom-right, bottom-left)
    pub fn corner_shapes(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.corner_shape = Some(CornerShape::new(tl, tr, br, bl));
        self
    }

    /// Bevel corners (straight diagonal)
    pub fn corner_bevel(mut self) -> Self {
        self.corner_shape = Some(CornerShape::BEVEL);
        self
    }

    /// Squircle corners (smoother than round)
    pub fn corner_squircle(mut self) -> Self {
        self.corner_shape = Some(CornerShape::SQUIRCLE);
        self
    }

    /// Scoop corners (concave inward)
    pub fn corner_scoop(mut self) -> Self {
        self.corner_shape = Some(CornerShape::SCOOP);
        self
    }

    // -------------------------------------------------------------------------
    // Theme-based corner radii
    // -------------------------------------------------------------------------

    /// Set corner radius to theme's small radius
    pub fn rounded_sm(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_sm)
    }

    /// Set corner radius to theme's default radius
    pub fn rounded_default(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_default)
    }

    /// Set corner radius to theme's medium radius
    pub fn rounded_md(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_md)
    }

    /// Set corner radius to theme's large radius
    pub fn rounded_lg(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_lg)
    }

    /// Set corner radius to theme's extra large radius
    pub fn rounded_xl(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_xl)
    }

    /// Set corner radius to theme's 2xl radius
    pub fn rounded_2xl(self) -> Self {
        self.rounded(ThemeState::get().radii().radius_2xl)
    }

    /// Set corner radius to none (0)
    pub fn rounded_none(self) -> Self {
        self.rounded(0.0)
    }

    // =========================================================================
    // Shadow
    // =========================================================================

    /// Set a single drop shadow (replaces any existing stack).
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = vec![shadow];
        self
    }

    /// Set a compound drop shadow stack (multiple layered shadows).
    pub fn shadow_stack(mut self, shadows: Vec<Shadow>) -> Self {
        self.shadow = shadows;
        self
    }

    /// Set shadow with parameters
    pub fn shadow_params(self, offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        self.shadow(Shadow::new(offset_x, offset_y, blur, color))
    }

    /// Small shadow preset using theme colors
    pub fn shadow_sm(self) -> Self {
        self.shadow_stack(theme_shadow_stack(|s| &s.shadow_sm))
    }

    /// Medium shadow preset using theme colors
    pub fn shadow_md(self) -> Self {
        self.shadow_stack(theme_shadow_stack(|s| &s.shadow_md))
    }

    /// Large shadow preset using theme colors
    pub fn shadow_lg(self) -> Self {
        self.shadow_stack(theme_shadow_stack(|s| &s.shadow_lg))
    }

    /// Extra large shadow preset using theme colors
    pub fn shadow_xl(self) -> Self {
        self.shadow_stack(theme_shadow_stack(|s| &s.shadow_xl))
    }

    /// Explicitly clear shadow (override any inherited shadow)
    pub fn shadow_none(mut self) -> Self {
        // Use a fully transparent single-layer shadow to indicate "no shadow"
        self.shadow = vec![Shadow::new(0.0, 0.0, 0.0, Color::TRANSPARENT)];
        self
    }
}

/// Look up a theme shadow stack and convert it to a `Vec<blinc_core::Shadow>`.
fn theme_shadow_stack<F>(pick: F) -> Vec<Shadow>
where
    F: Fn(&ShadowTokens) -> &[blinc_theme::Shadow],
{
    let shadows = ThemeState::get().shadows();
    pick(&shadows).iter().map(Shadow::from).collect()
}
