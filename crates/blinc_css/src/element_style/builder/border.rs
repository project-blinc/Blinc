//! Borders and outlines.

use blinc_core::Color;

use crate::element_style::*;

impl ElementStyle {
    // =========================================================================
    // Layout: Border
    // =========================================================================

    /// Set border width and color
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border_width = Some(width);
        self.border_color = Some(color);
        self
    }

    /// Set border width only
    pub fn border_w(mut self, width: f32) -> Self {
        self.border_width = Some(width);
        self
    }

    // =========================================================================
    // Layout: Outline
    // =========================================================================

    /// Set outline width and color
    pub fn outline(mut self, width: f32, color: Color) -> Self {
        self.outline_width = Some(width);
        self.outline_color = Some(color);
        self
    }

    /// Set outline width only
    pub fn outline_w(mut self, width: f32) -> Self {
        self.outline_width = Some(width);
        self
    }

    /// Set outline offset (gap between border and outline)
    pub fn outline_offset(mut self, offset: f32) -> Self {
        self.outline_offset = Some(offset);
        self
    }
}
