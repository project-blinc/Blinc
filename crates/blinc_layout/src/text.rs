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

use blinc_core::{Color, Shadow, Transform};
use taffy::prelude::*;

use crate::div::{
    ElementBuilder, ElementTypeId, FontFamily, FontWeight, TextAlign, TextRenderInfo,
    TextVerticalAlign,
};
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
    /// Text alignment (horizontal)
    align: TextAlign,
    /// Vertical alignment within bounding box
    v_align: TextVerticalAlign,
    /// Font weight
    weight: FontWeight,
    /// Font family category
    font_family: FontFamily,
    /// Taffy style for layout
    style: Style,
    /// Render layer
    render_layer: RenderLayer,
    /// Drop shadow
    shadow: Option<Shadow>,
    /// Transform
    transform: Option<Transform>,
    /// Whether to wrap text at container bounds (default: true)
    wrap: bool,
    /// Line height multiplier (default: 1.2)
    line_height: f32,
    /// Measured width of the text (before layout constraints)
    measured_width: f32,
    /// Word spacing in pixels (0.0 = normal)
    word_spacing: f32,
}

impl Text {
    /// Create a new text element
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            font_size: 14.0,
            color: Color::BLACK,
            align: TextAlign::default(),
            v_align: TextVerticalAlign::default(),
            weight: FontWeight::default(),
            font_family: FontFamily::default(),
            style: Style::default(),
            render_layer: RenderLayer::default(),
            shadow: None,
            transform: None,
            wrap: true,          // wrap by default
            line_height: 1.2,    // standard line height
            measured_width: 0.0, // will be set by update_size_estimate
            word_spacing: 0.0,   // normal word spacing
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

    // =========================================================================
    // Text Alignment
    // =========================================================================

    /// Set text alignment
    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Align text to the left (default)
    pub fn text_left(self) -> Self {
        self.align(TextAlign::Left)
    }

    /// Center text
    pub fn text_center(self) -> Self {
        self.align(TextAlign::Center)
    }

    /// Align text to the right
    pub fn text_right(self) -> Self {
        self.align(TextAlign::Right)
    }

    // =========================================================================
    // Vertical Alignment
    // =========================================================================

    /// Set vertical alignment within bounding box
    pub fn v_align(mut self, v_align: TextVerticalAlign) -> Self {
        self.v_align = v_align;
        self
    }

    /// Vertically center text with optical centering (cap-height based)
    ///
    /// Use this for single-line text in centered containers (like buttons)
    /// to get proper visual centering that accounts for descenders.
    pub fn v_center(self) -> Self {
        self.v_align(TextVerticalAlign::Center)
    }

    /// Position text at top of bounding box (default)
    ///
    /// Use this for multi-line text or text that should start at the top.
    pub fn v_top(self) -> Self {
        self.v_align(TextVerticalAlign::Top)
    }

    // =========================================================================
    // Font Weight
    // =========================================================================

    /// Set font weight
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Set font weight to thin (100)
    pub fn thin(self) -> Self {
        self.weight(FontWeight::Thin)
    }

    /// Set font weight to extra light (200)
    pub fn extra_light(self) -> Self {
        self.weight(FontWeight::ExtraLight)
    }

    /// Set font weight to light (300)
    pub fn light(self) -> Self {
        self.weight(FontWeight::Light)
    }

    /// Set font weight to normal/regular (400)
    pub fn normal(self) -> Self {
        self.weight(FontWeight::Normal)
    }

    /// Set font weight to medium (500)
    pub fn medium(self) -> Self {
        self.weight(FontWeight::Medium)
    }

    /// Set font weight to semi-bold (600)
    pub fn semibold(self) -> Self {
        self.weight(FontWeight::SemiBold)
    }

    /// Set font weight to bold (700)
    pub fn bold(self) -> Self {
        self.weight(FontWeight::Bold)
    }

    /// Set font weight to extra bold (800)
    pub fn extra_bold(self) -> Self {
        self.weight(FontWeight::ExtraBold)
    }

    /// Set font weight to black (900)
    pub fn black(self) -> Self {
        self.weight(FontWeight::Black)
    }

    // =========================================================================
    // Font Family
    // =========================================================================

    /// Set font family
    pub fn font_family(mut self, family: FontFamily) -> Self {
        self.font_family = family;
        // Re-measure since different fonts have different character widths
        self.update_size_estimate();
        self
    }

    /// Set a specific font by name (e.g., "Fira Code", "Inter")
    ///
    /// # Example
    ///
    /// ```ignore
    /// text("Hello").font("Inter")
    /// text("Code").font("Fira Code")
    /// ```
    pub fn font(self, name: impl Into<String>) -> Self {
        self.font_family(FontFamily::named(name))
    }

    /// Set a specific font with a fallback category
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_layout::prelude::*;
    /// text("Code").font_with_fallback("Fira Code", GenericFont::Monospace)
    /// ```
    pub fn font_with_fallback(
        self,
        name: impl Into<String>,
        fallback: crate::div::GenericFont,
    ) -> Self {
        self.font_family(FontFamily::named_with_fallback(name, fallback))
    }

    /// Use monospace font (for code)
    pub fn monospace(self) -> Self {
        self.font_family(FontFamily::monospace())
    }

    /// Use serif font
    pub fn serif(self) -> Self {
        self.font_family(FontFamily::serif())
    }

    /// Use sans-serif font
    pub fn sans_serif(self) -> Self {
        self.font_family(FontFamily::sans_serif())
    }

    // =========================================================================
    // Word Spacing
    // =========================================================================

    /// Set word spacing in pixels
    ///
    /// Positive values increase spacing, negative values decrease.
    /// Default is 0.0 (normal spacing).
    pub fn word_spacing(mut self, spacing: f32) -> Self {
        self.word_spacing = spacing;
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

    /// Update size using actual text measurement if available, otherwise estimate
    fn update_size_estimate(&mut self) {
        // Use the global text measurer with font family info
        let mut options = crate::text_measure::TextLayoutOptions::new();
        options.font_name = self.font_family.name.clone();
        options.generic_font = self.font_family.generic;

        let metrics = crate::text_measure::measure_text_with_options(
            &self.content,
            self.font_size,
            &options,
        );

        // Store measured width for render-time comparison
        self.measured_width = metrics.width;

        // Use measured width for layout. This allows text to respect
        // parent flex alignment (items_center, etc.).
        //
        // Text wrapping behavior:
        // - wrap=true (default): flex_shrink=1.0 + min_width=0 allows text to
        //   shrink below content size and wrap at render time
        // - wrap=false (.no_wrap()): flex_shrink=0.0 keeps natural width,
        //   text stays on single line
        self.style.size.width = Dimension::Length(metrics.width);
        self.style.size.height = Dimension::Length(metrics.height);

        if self.wrap {
            // Allow shrinking below content size so text can constrain to parent
            // and wrap at render time. min_width: 0 is essential for flexbox shrinking.
            self.style.flex_shrink = 1.0;
            self.style.min_size.width = Dimension::Length(0.0);
        } else {
            // Don't shrink - use natural size
            self.style.flex_shrink = 0.0;
            self.style.min_size.width = Dimension::Auto;
        }
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

    // =========================================================================
    // Shadow
    // =========================================================================

    /// Apply a drop shadow to this text
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Apply a drop shadow with the given parameters
    pub fn shadow_params(self, offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        self.shadow(Shadow::new(offset_x, offset_y, blur, color))
    }

    // =========================================================================
    // Transform
    // =========================================================================

    /// Apply a transform to this text
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }

    /// Translate this text by the given x and y offset
    pub fn translate(self, x: f32, y: f32) -> Self {
        self.transform(Transform::translate(x, y))
    }

    /// Scale this text uniformly
    pub fn scale(self, factor: f32) -> Self {
        self.transform(Transform::scale(factor, factor))
    }

    /// Rotate this text by the given angle in radians
    pub fn rotate(self, angle: f32) -> Self {
        self.transform(Transform::rotate(angle))
    }

    // =========================================================================
    // Text Wrapping
    // =========================================================================

    /// Disable word wrapping (text stays on single line)
    ///
    /// By default, text wraps at container bounds. Use this for headings
    /// or single-line text that should not wrap.
    pub fn no_wrap(mut self) -> Self {
        self.wrap = false;
        // Recalculate size to use measured width instead of percentage
        self.update_size_estimate();
        self
    }

    /// Set line height multiplier
    ///
    /// Default is 1.2. Increase for more spacing between lines.
    pub fn line_height(mut self, multiplier: f32) -> Self {
        self.line_height = multiplier;
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
            shadow: self.shadow,
            transform: self.transform.clone(),
            opacity: 1.0,
            clips_content: false,
            motion: None,
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
            align: self.align,
            weight: self.weight,
            v_align: self.v_align,
            wrap: self.wrap,
            line_height: self.line_height,
            measured_width: self.measured_width,
            font_family: self.font_family.clone(),
            word_spacing: self.word_spacing,
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
