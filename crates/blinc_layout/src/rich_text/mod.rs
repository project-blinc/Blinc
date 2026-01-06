//! Rich text element with inline formatting support
//!
//! Provides a text element that supports inline HTML-like formatting tags
//! for bold, italic, underline, strikethrough, colors, and links.
//!
//! # Example
//!
//! ```rust,ignore
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! // HTML-like tags are automatically parsed
//! rich_text("Hello <b>World</b>!")
//!     .size(16.0)
//!     .default_color(Color::WHITE);
//!
//! // Nested tags work naturally
//! rich_text("This is <b>bold and <i>italic</i></b> text");
//!
//! // Colors and links
//! rich_text("<span color=\"#FF0000\">Error:</span> <a href=\"https://help.com\">Click here</a>");
//!
//! // Range-based API for programmatic control
//! rich_text("Hello World")
//!     .bold_range(0..5)           // "Hello" is bold
//!     .color_range(6..11, Color::BLUE);  // "World" is blue
//! ```

use std::ops::Range;
use std::sync::Arc;

use blinc_core::events::event_types;
use blinc_core::{Color, Shadow, Transform};
use taffy::prelude::*;

use crate::div::{
    ElementBuilder, ElementTypeId, FontFamily, FontWeight, StyledTextRenderInfo,
    StyledTextSpanInfo, TextAlign, TextVerticalAlign,
};
use crate::element::{RenderLayer, RenderProps};
use crate::event_handler::EventHandlers;
use crate::styled_text::TextSpan;
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::link::open_url;

mod parser;

/// A clickable link region within rich text
#[derive(Clone, Debug)]
struct LinkRegion {
    /// Start byte index in content
    start: usize,
    /// End byte index in content
    end: usize,
    /// URL to open when clicked
    url: String,
    /// X position where this link starts (relative to element)
    x_start: f32,
    /// X position where this link ends
    x_end: f32,
}

/// A rich text element with inline formatting support
pub struct RichText {
    /// Plain text content (tags stripped)
    content: String,
    /// Style spans
    spans: Vec<TextSpan>,
    /// Font size in pixels
    font_size: f32,
    /// Default text color (for unspanned regions)
    default_color: Color,
    /// Text alignment (horizontal)
    align: TextAlign,
    /// Vertical alignment within bounding box
    v_align: TextVerticalAlign,
    /// Font weight
    weight: FontWeight,
    /// Whether to use italic style (global)
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
    /// Measured width of the text
    measured_width: f32,
    /// Measured ascender from font metrics
    ascender: f32,
    /// Cursor style when hovering
    cursor: Option<crate::element::CursorStyle>,
    /// Word spacing
    word_spacing: f32,
    /// Event handlers for interactivity (links)
    event_handlers: EventHandlers,
    /// Link data for click handling: (start_byte, end_byte, url, x_start, x_end)
    /// Pre-calculated during update_size_estimate for hit testing
    link_regions: Arc<Vec<LinkRegion>>,
}

impl RichText {
    /// Create a new rich text element with HTML-like markup parsing
    ///
    /// Supported tags:
    /// - `<b>`, `<strong>` - bold
    /// - `<i>`, `<em>` - italic
    /// - `<u>` - underline
    /// - `<s>`, `<strike>`, `<del>` - strikethrough
    /// - `<a href="url">` - links
    /// - `<span color="...">` - inline color
    ///
    /// HTML entities are decoded: `&lt;`, `&gt;`, `&amp;`, `&nbsp;`, etc.
    pub fn new(markup: impl Into<String>) -> Self {
        let markup = markup.into();
        let (content, spans) = parser::parse(&markup);

        // Check if there are any links
        let has_links = spans.iter().any(|s| s.link_url.is_some());

        let mut rich = Self {
            content,
            spans,
            font_size: 14.0,
            default_color: Color::BLACK,
            align: TextAlign::default(),
            v_align: TextVerticalAlign::default(),
            weight: FontWeight::default(),
            italic: false,
            font_family: FontFamily::default(),
            style: Style::default(),
            render_layer: RenderLayer::default(),
            shadow: None,
            transform: None,
            wrap: true,
            line_height: 1.2,
            measured_width: 0.0,
            ascender: 14.0 * 0.8,
            // Use pointer cursor if there are links, otherwise text cursor
            cursor: Some(if has_links {
                crate::element::CursorStyle::Pointer
            } else {
                crate::element::CursorStyle::Text
            }),
            word_spacing: 0.0,
            event_handlers: EventHandlers::new(),
            link_regions: Arc::new(Vec::new()),
        };
        rich.update_size_estimate();
        rich.setup_link_handlers();
        rich
    }

    /// Create from pre-built StyledText (for code/markdown integration)
    pub fn from_styled(styled: crate::styled_text::StyledText) -> Self {
        // Flatten StyledText lines into a single content string with spans
        let mut content = String::new();
        let mut spans = Vec::new();
        let mut offset = 0;

        for (i, line) in styled.lines.iter().enumerate() {
            if i > 0 {
                content.push('\n');
                offset += 1;
            }
            for span in &line.spans {
                spans.push(TextSpan {
                    start: span.start + offset,
                    end: span.end + offset,
                    color: span.color,
                    bold: span.bold,
                    italic: span.italic,
                    underline: span.underline,
                    strikethrough: span.strikethrough,
                    link_url: span.link_url.clone(),
                    token_type: span.token_type.clone(),
                });
            }
            content.push_str(&line.text);
            offset += line.text.len();
        }

        // Check if there are any links
        let has_links = spans.iter().any(|s| s.link_url.is_some());

        let mut rich = Self {
            content,
            spans,
            font_size: 14.0,
            default_color: Color::BLACK,
            align: TextAlign::default(),
            v_align: TextVerticalAlign::default(),
            weight: FontWeight::default(),
            italic: false,
            font_family: FontFamily::default(),
            style: Style::default(),
            render_layer: RenderLayer::default(),
            shadow: None,
            transform: None,
            wrap: true,
            line_height: 1.2,
            measured_width: 0.0,
            ascender: 14.0 * 0.8,
            cursor: Some(if has_links {
                crate::element::CursorStyle::Pointer
            } else {
                crate::element::CursorStyle::Text
            }),
            word_spacing: 0.0,
            event_handlers: EventHandlers::new(),
            link_regions: Arc::new(Vec::new()),
        };
        rich.update_size_estimate();
        rich.setup_link_handlers();
        rich
    }

    // =========================================================================
    // Range-based span builders
    // =========================================================================

    /// Add bold styling to a byte range
    pub fn bold_range(mut self, range: Range<usize>) -> Self {
        self.add_or_update_span(range, |span| span.bold = true);
        self
    }

    /// Add italic styling to a byte range
    pub fn italic_range(mut self, range: Range<usize>) -> Self {
        self.add_or_update_span(range, |span| span.italic = true);
        self
    }

    /// Add a color to a byte range
    pub fn color_range(mut self, range: Range<usize>, color: Color) -> Self {
        self.add_or_update_span(range, |span| span.color = color);
        self
    }

    /// Add underline to a byte range
    pub fn underline_range(mut self, range: Range<usize>) -> Self {
        self.add_or_update_span(range, |span| span.underline = true);
        self
    }

    /// Add strikethrough to a byte range
    pub fn strikethrough_range(mut self, range: Range<usize>) -> Self {
        self.add_or_update_span(range, |span| span.strikethrough = true);
        self
    }

    /// Add a link to a byte range
    pub fn link_range(mut self, range: Range<usize>, url: impl Into<String>) -> Self {
        let url = url.into();
        self.add_or_update_span(range, |span| {
            span.link_url = Some(url.clone());
            span.underline = true;
        });
        self
    }

    /// Helper to add or update a span for a range
    fn add_or_update_span(&mut self, range: Range<usize>, mut modifier: impl FnMut(&mut TextSpan)) {
        let range_start = range.start.min(self.content.len());
        let range_end = range.end.min(self.content.len());

        if range_start >= range_end {
            return;
        }

        // Check if there's an existing span that exactly matches this range
        for span in &mut self.spans {
            if span.start == range_start && span.end == range_end {
                modifier(span);
                return;
            }
        }

        // Create new span with TRANSPARENT color (sentinel for "inherit default_color")
        // This ensures the renderer will use the actual default_color at render time
        let mut new_span = TextSpan::colored(range_start, range_end, Color::TRANSPARENT);
        modifier(&mut new_span);
        self.spans.push(new_span);
    }

    // =========================================================================
    // Text properties
    // =========================================================================

    /// Set font size
    pub fn size(mut self, size: f32) -> Self {
        self.font_size = size;
        self.update_size_estimate();
        self
    }

    /// Set default text color (for regions without explicit color spans)
    pub fn default_color(mut self, color: Color) -> Self {
        self.default_color = color;
        self
    }

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

    /// Set vertical alignment
    pub fn v_align(mut self, v_align: TextVerticalAlign) -> Self {
        self.v_align = v_align;
        self
    }

    /// Vertically center text
    pub fn v_center(self) -> Self {
        self.v_align(TextVerticalAlign::Center)
    }

    /// Position text at top
    pub fn v_top(self) -> Self {
        self.v_align(TextVerticalAlign::Top)
    }

    /// Position text by baseline
    pub fn v_baseline(mut self) -> Self {
        self.v_align = TextVerticalAlign::Baseline;
        self.line_height = 1.0;
        self.update_size_estimate();
        self
    }

    /// Set font weight
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Set to bold weight
    pub fn bold(self) -> Self {
        self.weight(FontWeight::Bold)
    }

    /// Set italic style
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Set font family
    pub fn font_family(mut self, family: FontFamily) -> Self {
        self.font_family = family;
        self.update_size_estimate();
        self
    }

    /// Use monospace font
    pub fn monospace(self) -> Self {
        self.font_family(FontFamily::monospace())
    }

    /// Use sans-serif font
    pub fn sans_serif(self) -> Self {
        self.font_family(FontFamily::sans_serif())
    }

    /// Set line height multiplier
    pub fn line_height(mut self, multiplier: f32) -> Self {
        self.line_height = multiplier;
        self.update_size_estimate();
        self
    }

    /// Disable text wrapping
    pub fn no_wrap(mut self) -> Self {
        self.wrap = false;
        self.style.flex_shrink = 0.0;
        self
    }

    /// Set render layer
    pub fn layer(mut self, layer: RenderLayer) -> Self {
        self.render_layer = layer;
        self
    }

    /// Render in foreground
    pub fn foreground(self) -> Self {
        self.layer(RenderLayer::Foreground)
    }

    /// Apply drop shadow
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Set transform
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }

    /// Set cursor style
    pub fn cursor(mut self, cursor: crate::element::CursorStyle) -> Self {
        self.cursor = Some(cursor);
        self
    }

    // =========================================================================
    // Layout properties
    // =========================================================================

    /// Set width
    pub fn w(mut self, width: f32) -> Self {
        self.style.size.width = Dimension::Length(width);
        self
    }

    /// Set height
    pub fn h(mut self, height: f32) -> Self {
        self.style.size.height = Dimension::Length(height);
        self
    }

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

    /// Update size using text measurement
    fn update_size_estimate(&mut self) {
        let mut options = crate::text_measure::TextLayoutOptions::new();
        options.font_name = self.font_family.name.clone();
        options.generic_font = self.font_family.generic;
        options.font_weight = self.weight.weight();
        options.italic = self.italic;

        let metrics =
            crate::text_measure::measure_text_with_options(&self.content, self.font_size, &options);

        self.measured_width = metrics.width;
        self.ascender = metrics.ascender;

        self.style.size.width = Dimension::Length(metrics.width);
        let standardized_height = self.font_size * self.line_height;
        self.style.size.height = Dimension::Length(standardized_height);
        self.style.max_size.width = Dimension::Percent(1.0);

        if !self.wrap {
            self.style.flex_shrink = 0.0;
        }
    }

    /// Calculate approximate x positions for link regions based on character widths
    fn calculate_link_regions(&self) -> Vec<LinkRegion> {
        let mut regions = Vec::new();
        let content_len = self.content.len();

        // Use monospace approximation or measure each segment
        // For better accuracy, measure character by character up to each boundary
        let mut options = crate::text_measure::TextLayoutOptions::new();
        options.font_name = self.font_family.name.clone();
        options.generic_font = self.font_family.generic;
        options.font_weight = self.weight.weight();
        options.italic = self.italic;

        for span in &self.spans {
            if let Some(ref url) = span.link_url {
                let start = span.start.min(content_len);
                let end = span.end.min(content_len);

                if start >= end {
                    continue;
                }

                // Measure width of text before this link
                let x_start = if start > 0 {
                    let prefix = &self.content[..start];
                    let prefix_metrics = crate::text_measure::measure_text_with_options(
                        prefix,
                        self.font_size,
                        &options,
                    );
                    prefix_metrics.width
                } else {
                    0.0
                };

                // Measure width up to end of link
                let x_end = {
                    let to_end = &self.content[..end];
                    let to_end_metrics = crate::text_measure::measure_text_with_options(
                        to_end,
                        self.font_size,
                        &options,
                    );
                    to_end_metrics.width
                };

                regions.push(LinkRegion {
                    start,
                    end,
                    url: url.clone(),
                    x_start,
                    x_end,
                });
            }
        }

        regions
    }

    /// Set up click handlers for links
    fn setup_link_handlers(&mut self) {
        // Calculate link regions with their x positions
        let regions = self.calculate_link_regions();

        if regions.is_empty() {
            return;
        }

        // Store regions for hit testing
        self.link_regions = Arc::new(regions);
        let link_regions = Arc::clone(&self.link_regions);

        // Register click handler
        self.event_handlers.on(event_types::POINTER_UP, move |ctx| {
            let local_x = ctx.local_x;

            // Find which link was clicked (if any)
            for region in link_regions.iter() {
                if local_x >= region.x_start && local_x <= region.x_end {
                    open_url(&region.url);
                    break;
                }
            }
        });
    }

    /// Get the plain text content
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the spans
    pub fn spans(&self) -> &[TextSpan] {
        &self.spans
    }
}

impl ElementBuilder for RichText {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        tree.create_node(self.style.clone())
    }

    #[allow(deprecated)]
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
            pointer_events_none: false,
            cursor: self.cursor,
            motion_is_exiting: false,
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[] // RichText has no children
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::StyledText
    }

    fn event_handlers(&self) -> Option<&EventHandlers> {
        if self.event_handlers.is_empty() {
            None
        } else {
            Some(&self.event_handlers)
        }
    }

    fn styled_text_render_info(&self) -> Option<StyledTextRenderInfo> {
        Some(StyledTextRenderInfo {
            content: self.content.clone(),
            spans: self
                .spans
                .iter()
                .map(|span| StyledTextSpanInfo {
                    start: span.start,
                    end: span.end,
                    color: [span.color.r, span.color.g, span.color.b, span.color.a],
                    bold: span.bold,
                    italic: span.italic,
                    underline: span.underline,
                    strikethrough: span.strikethrough,
                    link_url: span.link_url.clone(),
                })
                .collect(),
            font_size: self.font_size,
            default_color: [
                self.default_color.r,
                self.default_color.g,
                self.default_color.b,
                self.default_color.a,
            ],
            align: self.align,
            v_align: self.v_align,
            font_family: self.font_family.clone(),
            line_height: self.line_height,
            weight: self.weight,
            italic: self.italic,
            ascender: self.ascender,
        })
    }
}

/// Create a new rich text element with HTML-like markup
pub fn rich_text(markup: impl Into<String>) -> RichText {
    RichText::new(markup)
}

/// Create a rich text element from pre-built StyledText
pub fn rich_text_styled(styled: crate::styled_text::StyledText) -> RichText {
    RichText::from_styled(styled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let rt = rich_text("Hello World");
        assert_eq!(rt.content(), "Hello World");
        assert!(rt.spans().is_empty());
    }

    #[test]
    fn test_bold_tag() {
        let rt = rich_text("Hello <b>World</b>!");
        assert_eq!(rt.content(), "Hello World!");
        assert_eq!(rt.spans().len(), 1);
        assert!(rt.spans()[0].bold);
        assert_eq!(rt.spans()[0].start, 6);
        assert_eq!(rt.spans()[0].end, 11);
    }

    #[test]
    fn test_nested_tags() {
        let rt = rich_text("<b>bold <i>and italic</i></b>");
        assert_eq!(rt.content(), "bold and italic");
        // Should have spans for bold and for bold+italic
        assert!(!rt.spans().is_empty());
    }

    #[test]
    fn test_range_api() {
        let rt = rich_text("Hello World")
            .bold_range(0..5)
            .color_range(6..11, Color::BLUE);

        assert_eq!(rt.spans().len(), 2);
    }

    #[test]
    fn test_entity_decoding() {
        let rt = rich_text("&lt;b&gt; &amp; &nbsp;");
        assert_eq!(rt.content(), "<b> & \u{00A0}");
    }

    #[test]
    fn test_link_has_url() {
        let rt = rich_text(r#"Visit <a href="https://example.com">our website</a> for info"#);
        assert_eq!(rt.content(), "Visit our website for info");
        assert_eq!(rt.spans().len(), 1);
        assert_eq!(
            rt.spans()[0].link_url,
            Some("https://example.com".to_string())
        );
        // Link should have underline
        assert!(rt.spans()[0].underline);
    }

    #[test]
    fn test_link_regions_calculated() {
        let rt = rich_text(r#"Click <a href="https://example.com">here</a>!"#);
        // The link_regions should have been populated
        assert!(!rt.link_regions.is_empty());
        assert_eq!(rt.link_regions.len(), 1);
        assert_eq!(rt.link_regions[0].url, "https://example.com");
        // x positions should be > 0 for x_end
        assert!(rt.link_regions[0].x_end > rt.link_regions[0].x_start);
    }

    #[test]
    fn test_event_handlers_registered_for_links() {
        let rt = rich_text(r#"Click <a href="https://example.com">here</a>!"#);
        // Should have event handlers registered
        assert!(!rt.event_handlers.is_empty());
    }
}
