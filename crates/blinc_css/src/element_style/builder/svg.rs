//! SVG paint and image fitting.

use blinc_core::Color;

use crate::element_style::*;

impl ElementStyle {
    // =========================================================================
    // SVG Properties
    // =========================================================================

    /// Set SVG fill color
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    /// Set SVG stroke color
    pub fn stroke(mut self, color: Color) -> Self {
        self.stroke = Some(color);
        self
    }

    /// Set SVG stroke width
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = Some(width);
        self
    }

    /// Set SVG stroke-dasharray pattern
    pub fn stroke_dasharray(mut self, pattern: Vec<f32>) -> Self {
        self.stroke_dasharray = Some(pattern);
        self
    }

    /// Set SVG stroke-dashoffset
    pub fn stroke_dashoffset(mut self, offset: f32) -> Self {
        self.stroke_dashoffset = Some(offset);
        self
    }

    /// Set SVG path data (d attribute)
    pub fn svg_path_data(mut self, data: impl Into<String>) -> Self {
        self.svg_path_data = Some(data.into());
        self
    }

    // =========================================================================
    // Image Properties
    // =========================================================================

    /// Set object-fit (0=cover, 1=contain, 2=fill, 3=scale-down, 4=none)
    pub fn object_fit(mut self, fit: u8) -> Self {
        self.object_fit = Some(fit);
        self
    }

    /// Set object-position as [x, y] in 0.0-1.0 range
    pub fn object_position(mut self, x: f32, y: f32) -> Self {
        self.object_position = Some([x, y]);
        self
    }
}
