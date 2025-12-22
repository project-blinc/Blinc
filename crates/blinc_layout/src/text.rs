//! Text element builder
//!
//! Provides a builder for text elements that participate in layout:
//! ```rust
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! let label = text("Hello, World!")
//!     .size(16.0)
//!     .color(Color::WHITE);
//! ```

use blinc_core::Color;
use taffy::prelude::*;

use crate::div::{ElementBuilder, ElementTypeId, TextRenderInfo};
use crate::element::{RenderLayer, RenderProps};
use crate::tree::{LayoutNodeId, LayoutTree};

/// A text element builder
pub struct Text {
    /// The text content
    content: String,
    /// Font size in pixels
    font_size: f32,
    /// Text color
    color: Color,
    /// Taffy style for layout
    style: Style,
    /// Render layer
    render_layer: RenderLayer,
}

impl Text {
    /// Create a new text element
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            font_size: 14.0,
            color: Color::BLACK,
            style: Style::default(),
            render_layer: RenderLayer::default(),
        }
    }

    /// Set the font size
    pub fn size(mut self, size: f32) -> Self {
        self.font_size = size;
        // Update estimated layout size based on font size
        // This is a rough estimate; actual size depends on text content
        self.update_size_estimate();
        self
    }

    /// Set the text color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the render layer
    pub fn layer(mut self, layer: RenderLayer) -> Self {
        self.render_layer = layer;
        self
    }

    /// Render in foreground (on top of glass)
    pub fn foreground(self) -> Self {
        self.layer(RenderLayer::Foreground)
    }

    /// Get the text content
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the font size
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Get the text color
    pub fn text_color(&self) -> Color {
        self.color
    }

    /// Update size estimate based on content and font size
    fn update_size_estimate(&mut self) {
        // Rough estimate: average character width is ~0.5 * font_size
        let char_count = self.content.chars().count() as f32;
        let estimated_width = char_count * self.font_size * 0.5;
        let estimated_height = self.font_size * 1.2; // Line height

        self.style.size.width = Dimension::Length(estimated_width);
        self.style.size.height = Dimension::Length(estimated_height);
    }

    // =========================================================================
    // Layout properties (delegate to style)
    // =========================================================================

    /// Set margin on all sides (in 4px units)
    pub fn m(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::Length(units * 4.0);
        self.style.margin = Rect {
            left: px,
            right: px,
            top: px,
            bottom: px,
        };
        self
    }

    /// Set horizontal margin (in 4px units)
    pub fn mx(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::Length(units * 4.0);
        self.style.margin.left = px;
        self.style.margin.right = px;
        self
    }

    /// Set vertical margin (in 4px units)
    pub fn my(mut self, units: f32) -> Self {
        let px = LengthPercentageAuto::Length(units * 4.0);
        self.style.margin.top = px;
        self.style.margin.bottom = px;
        self
    }

    /// Set flex-grow
    pub fn flex_grow(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self
    }

    /// Set flex-shrink
    pub fn flex_shrink(mut self) -> Self {
        self.style.flex_shrink = 1.0;
        self
    }
}

impl ElementBuilder for Text {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        tree.create_node(self.style.clone())
    }

    fn render_props(&self) -> RenderProps {
        RenderProps {
            background: None,
            border_radius: Default::default(),
            layer: self.render_layer,
            material: None,
            node_id: None,
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[] // Text has no children
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Text
    }

    fn text_render_info(&self) -> Option<TextRenderInfo> {
        Some(TextRenderInfo {
            content: self.content.clone(),
            font_size: self.font_size,
            color: [self.color.r, self.color.g, self.color.b, self.color.a],
        })
    }
}

/// Convenience function to create a new text element
pub fn text(content: impl Into<String>) -> Text {
    let mut t = Text::new(content);
    t.update_size_estimate();
    t
}

/// Text element with render data for the renderer
#[derive(Clone)]
pub struct TextRenderData {
    /// The text content
    pub content: String,
    /// Font size in pixels
    pub font_size: f32,
    /// Text color as [r, g, b, a]
    pub color: [f32; 4],
}

impl Text {
    /// Get render data for this text element
    pub fn render_data(&self) -> TextRenderData {
        TextRenderData {
            content: self.content.clone(),
            font_size: self.font_size,
            color: [self.color.r, self.color.g, self.color.b, self.color.a],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_builder() {
        let t = text("Hello").size(16.0).color(Color::WHITE);

        assert_eq!(t.content(), "Hello");
        assert_eq!(t.font_size(), 16.0);
    }

    #[test]
    fn test_text_build() {
        let t = text("Test");

        let mut tree = LayoutTree::new();
        let _node = t.build(&mut tree);

        assert_eq!(tree.len(), 1);
    }
}
