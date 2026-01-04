//! Text element builder
//!
//! Provides a builder for text elements that participate in layout.
//! HTML entities are automatically decoded (e.g., `&amp;` becomes `&`).
//!
//! ```rust
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! let label = text("Hello, World!")
//!     .size(16.0)
//!     .color(Color::WHITE);
//!
//! // HTML entities are decoded automatically
//! let special = text("&copy; 2024 &mdash; All rights reserved");
//! // Renders as: © 2024 — All rights reserved
//!
//! // Emoji work directly
//! let emoji = text("Hello 😀 World 🎉");
//! ```

use blinc_core::{Color, Shadow, Transform};
use html_escape::decode_html_entities;
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
    /// Whether to use italic style
    italic: bool,
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
    /// Measured ascender from font metrics (distance from baseline to top)
    ascender: f32,
    /// Whether text has strikethrough decoration
    strikethrough: bool,
    /// Whether text has underline decoration
    underline: bool,
    /// Cursor style when hovering over this text (default: Text cursor)
    cursor: Option<crate::element::CursorStyle>,
}

impl Text {
    /// Create a new text element
    ///
    /// HTML entities in the content are automatically decoded:
    /// - Named entities: `&amp;`, `&nbsp;`, `&copy;`, etc.
    /// - Decimal entities: `&#65;`, `&#8364;`, etc.
    /// - Hexadecimal entities: `&#x41;`, `&#x20AC;`, etc.
    pub fn new(content: impl Into<String>) -> Self {
        // Decode HTML entities (e.g., &amp; -> &, &copy; -> ©)
        let raw_content = content.into();
        let decoded_content = decode_html_entities(&raw_content).into_owned();

        let mut text = Self {
            content: decoded_content,
            font_size: 14.0,
            color: Color::BLACK,
            align: TextAlign::default(),
            v_align: TextVerticalAlign::default(),
            weight: FontWeight::default(),
            italic: false,
            font_family: FontFamily::default(),
            style: Style::default(),
            render_layer: RenderLayer::default(),
            shadow: None,
            transform: None,
            wrap: true,           // wrap by default
            line_height: 1.2,     // standard line height
            measured_width: 0.0,  // will be set by update_size_estimate
            word_spacing: 0.0,    // normal word spacing
            ascender: 14.0 * 0.8, // will be set by update_size_estimate
            strikethrough: false,
            underline: false,
            cursor: Some(crate::element::CursorStyle::Text), // Text cursor by default
        };
        text.update_size_estimate();
        text
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

    /// Position text by baseline for inline text alignment
    ///
    /// Use this for inline text elements that should align by baseline with
    /// other text elements (e.g., mixing fonts in a paragraph).
    /// This uses a standardized baseline position to ensure different fonts align.
    /// Also sets line_height to 1.0 for tighter vertical bounds.
    pub fn v_baseline(mut self) -> Self {
        self.v_align = TextVerticalAlign::Baseline;
        // Use line_height of 1.0 for baseline alignment to minimize extra vertical space
        self.line_height = 1.0;
        self.update_size_estimate();
        self
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
    // Font Style (Italic)
    // =========================================================================

    /// Set italic style
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Check if text is italic
    pub fn is_italic(&self) -> bool {
        self.italic
    }

    /// Set strikethrough decoration
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    /// Check if text has strikethrough decoration
    pub fn is_strikethrough(&self) -> bool {
        self.strikethrough
    }

    /// Set underline decoration
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    /// Check if text has underline decoration
    pub fn is_underline(&self) -> bool {
        self.underline
    }

    /// Set cursor style for this text
    pub fn cursor(mut self, cursor: crate::element::CursorStyle) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Remove cursor style (use default cursor from parent or window)
    pub fn no_cursor(mut self) -> Self {
        self.cursor = None;
        self
    }

    /// Set cursor to default arrow (removes text cursor)
    pub fn cursor_default(self) -> Self {
        self.cursor(crate::element::CursorStyle::Default)
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
        options.font_weight = self.weight.weight();
        options.italic = self.italic;

        let metrics =
            crate::text_measure::measure_text_with_options(&self.content, self.font_size, &options);

        // Store measured width for render-time comparison
        self.measured_width = metrics.width;

        // Store actual ascender from font metrics for baseline alignment
        self.ascender = metrics.ascender;

        // Text sizing for flex layouts:
        // Use measured width as basis, constrained by max_width: 100%
        // This allows:
        // - Short text to be centered by flexbox (takes natural width)
        // - Long text to wrap at parent boundary (max 100%)
        // - text_center() to center within text bounds
        self.style.size.width = Dimension::Length(metrics.width);

        // Use a standardized height based on font_size * line_height for layout purposes.
        // This ensures all text at the same font size has consistent height regardless
        // of font weight/style (regular vs bold fonts have different internal metrics).
        // The actual rendering uses font metrics, but layout should be consistent.
        let standardized_height = self.font_size * self.line_height;
        self.style.size.height = Dimension::Length(standardized_height);
        self.style.max_size.width = Dimension::Percent(1.0);

        if !self.wrap {
            // No wrapping: don't shrink, keep natural size
            self.style.flex_shrink = 0.0;
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
            border_color: None,
            border_width: 0.0,
            border_sides: Default::default(),
            layer: self.render_layer,
            material: None,
            node_id: None,
            shadow: self.shadow,
            transform: self.transform.clone(),
            opacity: 1.0,
            clips_content: false,
            motion: None,
            motion_stable_id: None,
            motion_should_replay: false,
            is_stack_layer: false,
            cursor: self.cursor,
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
            italic: self.italic,
            v_align: self.v_align,
            wrap: self.wrap,
            line_height: self.line_height,
            measured_width: self.measured_width,
            font_family: self.font_family.clone(),
            word_spacing: self.word_spacing,
            ascender: self.ascender,
            strikethrough: self.strikethrough,
            underline: self.underline,
        })
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        Some(&self.style)
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

    #[test]
    fn test_html_entity_decoding() {
        // Named entities
        let t = text("&amp;");
        assert_eq!(t.content(), "&");

        let t = text("&lt;div&gt;");
        assert_eq!(t.content(), "<div>");

        let t = text("&copy; 2024");
        assert_eq!(t.content(), "© 2024");

        // Decimal entities
        let t = text("&#65;&#66;&#67;");
        assert_eq!(t.content(), "ABC");

        // Hexadecimal entities
        let t = text("&#x41;&#x42;&#x43;");
        assert_eq!(t.content(), "ABC");

        // Mixed content
        let t = text("Price: &euro;100 &mdash; Great deal!");
        assert_eq!(t.content(), "Price: €100 — Great deal!");

        // Emoji passthrough (not entities, just Unicode)
        let t = text("Hello 😀 World");
        assert_eq!(t.content(), "Hello 😀 World");

        // Emoji via hex entity
        let t = text("&#x1F600;");
        assert_eq!(t.content(), "😀");
    }

    #[test]
    fn test_plain_text_unchanged() {
        // Plain text without entities should be unchanged
        let t = text("Hello World");
        assert_eq!(t.content(), "Hello World");

        let t = text("No entities here @ all #123");
        assert_eq!(t.content(), "No entities here @ all #123");
    }
}
