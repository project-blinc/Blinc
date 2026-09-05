//! Text, form-element colors, and scrollbar styling.

use blinc_core::{Color, Shadow};

use crate::element_style::*;

impl ElementStyle {
    // =========================================================================
    // Text Properties
    // =========================================================================

    /// Set text color
    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }

    /// Set font size in pixels
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }

    /// Set font weight
    pub fn font_weight(mut self, weight: crate::text::FontWeight) -> Self {
        self.font_weight = Some(weight);
        self
    }

    /// Set font style
    pub fn font_style(mut self, style: FontStyle) -> Self {
        self.font_style = Some(style);
        self
    }

    /// Set text decoration
    pub fn text_decoration(mut self, decoration: TextDecoration) -> Self {
        self.text_decoration = Some(decoration);
        self
    }

    /// Set line height multiplier
    pub fn line_height(mut self, height: f32) -> Self {
        self.line_height = Some(height);
        self
    }

    /// Set text alignment
    pub fn text_align(mut self, align: crate::text::TextAlign) -> Self {
        self.text_align = Some(align);
        self
    }

    /// Set letter spacing in pixels
    pub fn letter_spacing(mut self, spacing: f32) -> Self {
        self.letter_spacing = Some(spacing);
        self
    }

    /// Set text shadow
    pub fn text_shadow(mut self, shadow: Shadow) -> Self {
        self.text_shadow = Some(shadow);
        self
    }

    // =========================================================================
    // Form Element Colors
    // =========================================================================

    /// Set caret (cursor) color for text inputs
    pub fn caret_color(mut self, color: Color) -> Self {
        self.caret_color = Some(color);
        self
    }

    /// Set text selection highlight color
    pub fn selection_color(mut self, color: Color) -> Self {
        self.selection_color = Some(color);
        self
    }

    /// Set placeholder text color
    pub fn placeholder_color(mut self, color: Color) -> Self {
        self.placeholder_color = Some(color);
        self
    }

    /// Set accent color for form controls
    pub fn accent_color(mut self, color: Color) -> Self {
        self.accent_color = Some(color);
        self
    }

    // =========================================================================
    // Scrollbar
    // =========================================================================

    /// Set scrollbar colors (thumb, track)
    pub fn scrollbar_color(mut self, thumb: Color, track: Color) -> Self {
        self.scrollbar_color = Some((thumb, track));
        self
    }

    /// Set scrollbar width mode
    pub fn scrollbar_width(mut self, width: ScrollbarWidth) -> Self {
        self.scrollbar_width = Some(width);
        self
    }
}
