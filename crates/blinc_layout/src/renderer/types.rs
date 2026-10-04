//! Top-level types referenced by the render-tree machinery.
//!
//! Pure data declarations + the `LayoutRenderer` trait. No `impl
//! RenderTree` blocks live here — those are split across the
//! sibling submodules. Only `StyledTextSpan` carries an inline
//! `impl` because its constructors are part of the public surface.
//!
//! Everything in here is re-exported from `renderer/mod.rs` so
//! external callers continue to spell paths as
//! `crate::renderer::TextData`, `crate::renderer::ElementType`, etc.

use std::any::Any;
use std::sync::{Arc, Mutex};

use blinc_core::{Color, CornerRadius, DrawContext};

use crate::canvas::CanvasData;
use crate::element::{ElementBounds, GlassMaterial, RenderProps};
use crate::tree::LayoutNodeId;

/// A computed glass panel ready for GPU rendering
///
/// This contains all the information needed to render a glass effect,
/// with bounds computed from the layout system.
///
/// # Deprecated
/// Use `Brush::Glass` instead. Glass is now rendered as part of the
/// normal render pipeline - just use `fill_rect` with a glass brush.
#[deprecated(
    since = "0.2.0",
    note = "Use Brush::Glass instead. Glass is now integrated into the normal render pipeline."
)]
#[derive(Clone, Debug)]
pub struct GlassPanel {
    /// Absolute bounds (x, y, width, height)
    pub bounds: ElementBounds,
    /// Corner radii
    pub corner_radius: CornerRadius,
    /// Glass material properties
    pub material: GlassMaterial,
    /// The layout node this panel belongs to
    pub node_id: LayoutNodeId,
}

/// Debug statistics for the render tree
///
/// Used by `BLINC_DEBUG=motion` to display animation stats.
#[derive(Clone, Debug, Default)]
pub struct RenderTreeDebugStats {
    /// Number of active visual (FLIP) animations
    pub visual_animation_count: usize,
    /// Number of registered visual animation configs
    pub visual_animation_config_count: usize,
    /// Number of active layout animations
    pub layout_animation_count: usize,
    /// Number of pre-computed animated bounds
    pub animated_bounds_count: usize,
    /// Total number of render nodes
    pub render_node_count: usize,
    /// Number of scroll physics instances
    pub scroll_physics_count: usize,
}

/// Stores an element's type for rendering
#[derive(Clone)]
pub enum ElementType {
    /// A div/container element
    Div,
    /// A text element with content
    Text(TextData),
    /// Styled text with multiple color spans (for syntax highlighting)
    StyledText(StyledTextData),
    /// An SVG element
    Svg(SvgData),
    /// An image element
    Image(ImageData),
    /// A canvas element with custom render callback
    Canvas(CanvasData),
}

/// Text data for rendering
#[derive(Clone)]
pub struct TextData {
    pub content: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub align: crate::div::TextAlign,
    pub weight: crate::div::FontWeight,
    /// Whether to use italic style
    pub italic: bool,
    pub v_align: crate::div::TextVerticalAlign,
    /// Whether to wrap text at container bounds
    pub wrap: bool,
    /// Line height multiplier
    pub line_height: f32,
    /// Measured width (before layout constraints)
    pub measured_width: f32,
    /// Font family category
    pub font_family: crate::div::FontFamily,
    /// Word spacing in pixels (0.0 = normal)
    pub word_spacing: f32,
    /// Letter spacing in pixels (0.0 = normal)
    pub letter_spacing: f32,
    /// Font ascender in pixels (distance from baseline to top)
    pub ascender: f32,
    /// Half the line box's leading, above the ascender. CSS puts the
    /// baseline this far below the box top, before the ascender.
    pub half_leading: f32,
    /// Whether text has strikethrough decoration
    pub strikethrough: bool,
    /// Whether text has underline decoration
    pub underline: bool,
}

/// A styled span within rich text
#[derive(Clone, Debug)]
pub struct StyledTextSpan {
    /// Start byte index in text
    pub start: usize,
    /// End byte index in text (exclusive)
    pub end: usize,
    /// RGBA color
    pub color: [f32; 4],
    /// Whether text is bold
    pub bold: bool,
    /// Whether text is italic
    pub italic: bool,
    /// Whether text has underline decoration
    pub underline: bool,
    /// Whether text has strikethrough decoration
    pub strikethrough: bool,
    /// Optional link URL (for clickable spans)
    pub link_url: Option<String>,
}

impl StyledTextSpan {
    /// Create a new styled text span with just color (no decorations)
    pub fn new(start: usize, end: usize, color: [f32; 4]) -> Self {
        Self {
            start,
            end,
            color,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            link_url: None,
        }
    }

    /// Create from a TextSpan (from styled_text module)
    pub fn from_text_span(span: &crate::styled_text::TextSpan) -> Self {
        Self {
            start: span.start,
            end: span.end,
            color: span.color.to_array(),
            bold: span.bold,
            italic: span.italic,
            underline: span.underline,
            strikethrough: span.strikethrough,
            link_url: span.link_url.clone(),
        }
    }
}

/// Styled text data for rendering with multiple color spans
#[derive(Clone)]
pub struct StyledTextData {
    /// The full text content
    pub content: String,
    /// Color spans (must cover entire text, sorted by start position)
    pub spans: Vec<StyledTextSpan>,
    /// Default color for unspanned regions
    pub default_color: [f32; 4],
    /// Font size
    pub font_size: f32,
    /// Text alignment
    pub align: crate::div::TextAlign,
    /// Vertical alignment
    pub v_align: crate::div::TextVerticalAlign,
    /// Font family
    pub font_family: crate::div::FontFamily,
    /// Line height multiplier
    pub line_height: f32,
    /// Default font weight (for unspanned regions)
    pub weight: crate::div::FontWeight,
    /// Default italic style (for unspanned regions)
    pub italic: bool,
    /// Measured ascender for consistent baseline alignment
    pub ascender: f32,
    /// Half the line box's leading, above the ascender
    pub half_leading: f32,
}

/// SVG data for rendering
#[derive(Clone)]
pub struct SvgData {
    pub source: Arc<str>,
    pub tint: Option<Color>,
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: Option<f32>,
}

/// Image data for rendering
#[derive(Clone)]
pub struct ImageData {
    /// Image source (file path, URL, or base64 data)
    pub source: String,
    /// Object-fit mode (0=cover, 1=contain, 2=fill, 3=scale-down, 4=none)
    pub object_fit: u8,
    /// Object position (x: 0.0-1.0, y: 0.0-1.0)
    pub object_position: [f32; 2],
    /// Opacity (0.0 - 1.0)
    pub opacity: f32,
    /// Border radius for rounded corners
    pub border_radius: f32,
    /// Tint color [r, g, b, a]
    pub tint: [f32; 4],
    /// Filter: [grayscale, sepia, brightness, contrast, saturate, hue_rotate, invert, blur]
    pub filter: [f32; 8],
    /// Loading strategy: 0 = Eager (default), 1 = Lazy
    pub loading_strategy: u8,
    /// Placeholder type: 0 = None, 1 = Color, 2 = Image, 3 = Skeleton
    pub placeholder_type: u8,
    /// Placeholder color [r, g, b, a]
    pub placeholder_color: [f32; 4],
    /// Placeholder image source (only used when placeholder_type == 2)
    pub placeholder_image: Option<String>,
    /// Fade-in duration in milliseconds (0 = no fade)
    pub fade_duration_ms: u32,
}

/// Node data for rendering
#[derive(Clone)]
pub struct RenderNode {
    /// Render properties
    pub props: RenderProps,
    /// Element type
    pub element_type: ElementType,
}

/// Trait for rendering layout trees with text, SVG, and glass support
///
/// Implement this trait to provide custom rendering for your platform.
/// The renderer handles:
/// - Background/foreground DrawContext separation for glass effects
/// - Text rendering at layout-computed positions
/// - SVG rendering at layout-computed positions
pub trait LayoutRenderer {
    /// Get the background DrawContext (for elements behind glass)
    fn background(&mut self) -> &mut dyn DrawContext;

    /// Get the foreground DrawContext (for elements on top of glass)
    fn foreground(&mut self) -> &mut dyn DrawContext;

    /// Render text to the foreground layer at absolute position
    ///
    /// Called for text elements that are children of glass elements.
    /// Position is absolute (after applying all parent transforms).
    #[allow(clippy::too_many_arguments)]
    fn render_text_foreground(
        &mut self,
        content: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        font_size: f32,
        color: [f32; 4],
        align: crate::div::TextAlign,
        weight: crate::div::FontWeight,
    );

    /// Render text to the background layer at absolute position
    #[allow(clippy::too_many_arguments)]
    fn render_text_background(
        &mut self,
        content: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        font_size: f32,
        color: [f32; 4],
        align: crate::div::TextAlign,
        weight: crate::div::FontWeight,
    );

    /// Render an SVG to the foreground layer at absolute position
    fn render_svg_foreground(
        &mut self,
        source: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        tint: Option<Color>,
    );

    /// Render an SVG to the background layer at absolute position
    fn render_svg_background(
        &mut self,
        source: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        tint: Option<Color>,
    );
}

/// Type-erased node state storage
pub type NodeStateStorage = Arc<Mutex<dyn Any + Send>>;

/// Storage for computed layout bounds (shared with ElementRef)
pub type LayoutBoundsStorage = Arc<Mutex<Option<ElementBounds>>>;

/// Callback type for layout bounds change notifications
pub type LayoutBoundsCallback = Arc<dyn Fn(ElementBounds) + Send + Sync>;

/// Entry for layout bounds storage with optional change callback
pub struct LayoutBoundsEntry {
    /// The shared storage for bounds
    pub storage: LayoutBoundsStorage,
    /// Optional callback when bounds change (width or height differ from previous)
    pub on_change: Option<LayoutBoundsCallback>,
}

/// Callback type for on_ready notifications when an element is laid out and rendered
///
/// The callback receives the element's computed bounds after layout.
/// This is triggered once per element after its first successful layout computation.
pub type OnReadyCallback = Arc<dyn Fn(ElementBounds) + Send + Sync>;

/// Entry for on_ready callbacks
pub struct OnReadyEntry {
    /// The callback to invoke when the element is ready
    pub callback: OnReadyCallback,
    /// Whether this callback has been triggered (only fires once)
    pub triggered: bool,
}
