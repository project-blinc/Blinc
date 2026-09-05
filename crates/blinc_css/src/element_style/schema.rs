//! The resolved style schema.
//!
//! `ElementStyle` is the one place every visual property lands, whatever
//! set it: a builder call, a `css!` literal, or a parsed stylesheet rule.
//! Every field is optional, so a style records only what it was told and
//! merges cleanly over another.
//!
//! The builder methods that fill these fields live in `super::builder`.

use blinc_core::{
    BlendMode, Brush, ClipPath, Color, CornerRadius, CornerShape, OverflowFade, PointerEvents,
    Shadow, Transform,
};

use crate::element_style::*;
use crate::material::CursorStyle;

use crate::material::{Material, RenderLayer};
use crate::parser::{CssAnimation, CssTransitionSet};

/// Visual style properties for an element
///
/// All properties are optional - when merging styles, only set properties
/// will override. This enables state-specific styling where you only
/// override the properties that change for that state.
#[derive(Clone, Default, Debug)]
pub struct ElementStyle {
    // =========================================================================
    // Visual Properties
    // =========================================================================
    /// Background brush (solid color, gradient, or glass)
    pub background: Option<Brush>,
    /// Corner radius
    pub corner_radius: Option<CornerRadius>,
    /// Corner shape (superellipse parameter per corner)
    pub corner_shape: Option<CornerShape>,
    /// Drop shadow stack (empty = none, 1 = single, 2-3 = compound layered shadows).
    pub shadow: Vec<Shadow>,
    /// Transform (scale, rotate, translate)
    pub transform: Option<Transform>,
    /// Material effect (glass, metallic, wood)
    pub material: Option<Material>,
    /// Render layer ordering
    pub render_layer: Option<RenderLayer>,
    /// Opacity (0.0 = transparent, 1.0 = opaque)
    pub opacity: Option<f32>,
    /// Text foreground color
    pub text_color: Option<blinc_core::Color>,
    /// Font size in pixels
    pub font_size: Option<f32>,
    /// Text shadow (offset, blur, color)
    pub text_shadow: Option<Shadow>,
    /// Font weight (100-900)
    pub font_weight: Option<crate::text::FontWeight>,
    /// Font style (upright or italic)
    pub font_style: Option<FontStyle>,
    /// Text decoration (underline, line-through, etc.)
    pub text_decoration: Option<TextDecoration>,
    /// Line height multiplier
    pub line_height: Option<f32>,
    /// Text alignment (left, center, right)
    pub text_align: Option<crate::text::TextAlign>,
    /// Letter spacing in pixels
    pub letter_spacing: Option<f32>,
    /// 2D rotation angle in degrees (original CSS value, avoids lossy atan2 decomposition)
    pub rotate: Option<f32>,
    /// Scale X factor (original CSS value)
    pub scale_x: Option<f32>,
    /// Scale Y factor (original CSS value)
    pub scale_y: Option<f32>,
    /// Skew X angle in degrees
    pub skew_x: Option<f32>,
    /// Skew Y angle in degrees
    pub skew_y: Option<f32>,
    /// Transform origin as percentages [x%, y%] (default 50%, 50% = center)
    pub transform_origin: Option<[f32; 2]>,
    /// CSS animation configuration (animation: name duration timing delay iteration-count direction fill-mode)
    pub animation: Option<CssAnimation>,
    /// CSS transition configuration (transition: property duration timing delay)
    pub transition: Option<CssTransitionSet>,

    // =========================================================================
    // 3D Transform Properties
    // =========================================================================
    /// X-axis rotation in degrees (3D tilt)
    pub rotate_x: Option<f32>,
    /// Y-axis rotation in degrees (3D turn)
    pub rotate_y: Option<f32>,
    /// Perspective distance in pixels
    pub perspective: Option<f32>,
    /// 3D shape type: "box", "sphere", "cylinder", "torus", "capsule"
    pub shape_3d: Option<String>,
    /// 3D extrusion depth in pixels
    pub depth: Option<f32>,
    /// Light direction (x, y, z)
    pub light_direction: Option<[f32; 3]>,
    /// Light intensity (0.0 - 1.0+)
    pub light_intensity: Option<f32>,
    /// Ambient light level (0.0 - 1.0)
    pub ambient: Option<f32>,
    /// Specular power (higher = sharper highlights)
    pub specular: Option<f32>,
    /// Z-axis translation in pixels (positive = toward viewer)
    pub translate_z: Option<f32>,
    /// 3D boolean operation type: "union", "subtract", "intersect", "smooth-union", "smooth-subtract", "smooth-intersect"
    pub op_3d: Option<String>,
    /// Blend radius for smooth boolean operations (in pixels)
    pub blend_3d: Option<f32>,

    // =========================================================================
    // Clip-Path Property
    // =========================================================================
    /// CSS clip-path shape function
    pub clip_path: Option<ClipPath>,
    /// CSS filter functions (grayscale, invert, sepia, brightness, contrast, saturate, hue-rotate)
    pub filter: Option<CssFilter>,

    // =========================================================================
    // Layout Properties
    // =========================================================================
    /// Width (pixels, percentage, or auto/fit-content)
    pub width: Option<StyleDimension>,
    /// Height (pixels, percentage, or auto/fit-content)
    pub height: Option<StyleDimension>,
    /// Minimum width in pixels
    pub min_width: Option<f32>,
    /// Minimum height in pixels
    pub min_height: Option<f32>,
    /// Maximum width in pixels
    pub max_width: Option<f32>,
    /// Maximum height in pixels
    pub max_height: Option<f32>,

    /// Display mode (flex, block, none)
    pub display: Option<StyleDisplay>,
    /// Flex direction (row, column, row-reverse, column-reverse)
    pub flex_direction: Option<StyleFlexDirection>,
    /// Flex wrap
    pub flex_wrap: Option<bool>,
    /// Flex grow factor
    pub flex_grow: Option<f32>,
    /// Flex shrink factor
    pub flex_shrink: Option<f32>,

    /// Align items on cross axis
    pub align_items: Option<StyleAlign>,
    /// Justify content on main axis
    pub justify_content: Option<StyleJustify>,
    /// Align self (override parent's align-items)
    pub align_self: Option<StyleAlign>,

    /// Padding (all sides in pixels)
    pub padding: Option<SpacingRect>,
    /// Margin (all sides in pixels)
    pub margin: Option<SpacingRect>,
    /// Uniform gap between children in pixels
    pub gap: Option<f32>,

    /// Overflow behavior (shorthand, sets both axes)
    pub overflow: Option<StyleOverflow>,
    /// Overflow behavior for X-axis only
    pub overflow_x: Option<StyleOverflow>,
    /// Overflow behavior for Y-axis only
    pub overflow_y: Option<StyleOverflow>,
    /// Overflow fade distances (smooth alpha fade at clip edges)
    pub overflow_fade: Option<OverflowFade>,

    /// Border width in pixels
    pub border_width: Option<f32>,
    /// Border color
    pub border_color: Option<Color>,

    /// Outline width in pixels
    pub outline_width: Option<f32>,
    /// Outline color
    pub outline_color: Option<Color>,
    /// Outline offset in pixels (gap between border and outline)
    pub outline_offset: Option<f32>,

    // =========================================================================
    // Form Element Properties
    // =========================================================================
    /// Caret (cursor) color for text inputs
    pub caret_color: Option<Color>,
    /// Text selection highlight color
    pub selection_color: Option<Color>,
    /// Placeholder text color (applied via ::placeholder pseudo-element)
    pub placeholder_color: Option<Color>,
    /// Accent color for form controls (checkmarks, radio dots)
    pub accent_color: Option<Color>,

    // =========================================================================
    // Scrollbar Properties
    // =========================================================================
    /// Scrollbar thumb and track colors (CSS scrollbar-color: thumb track)
    pub scrollbar_color: Option<(Color, Color)>,
    /// Scrollbar width mode (CSS scrollbar-width: auto|thin|none)
    pub scrollbar_width: Option<ScrollbarWidth>,

    // =========================================================================
    // SVG Properties
    // =========================================================================
    /// SVG fill color
    pub fill: Option<Color>,
    /// SVG stroke color
    pub stroke: Option<Color>,
    /// SVG stroke width in pixels
    pub stroke_width: Option<f32>,
    /// SVG stroke-dasharray pattern (alternating dash/gap lengths)
    pub stroke_dasharray: Option<Vec<f32>>,
    /// SVG stroke-dashoffset in pixels
    pub stroke_dashoffset: Option<f32>,
    /// SVG path `d` attribute data (for path morphing)
    pub svg_path_data: Option<String>,

    /// CSS position (static, relative, absolute)
    pub position: Option<StylePosition>,
    /// Top inset in pixels (for positioned elements)
    pub top: Option<f32>,
    /// Right inset in pixels (for positioned elements)
    pub right: Option<f32>,
    /// Bottom inset in pixels (for positioned elements)
    pub bottom: Option<f32>,
    /// Left inset in pixels (for positioned elements)
    pub left: Option<f32>,
    /// CSS z-index for controlling render order
    pub z_index: Option<i32>,
    /// CSS visibility (visible or hidden — hidden keeps layout space but doesn't render)
    pub visibility: Option<StyleVisibility>,

    // =========================================================================
    // Image Properties
    // =========================================================================
    /// object-fit: how the image fills its container (cover, contain, fill, scale-down, none)
    pub object_fit: Option<u8>,
    /// object-position: alignment of image within its container [x, y] in 0.0-1.0 range
    pub object_position: Option<[f32; 2]>,
    /// loading: image loading strategy (0 = eager, 1 = lazy)
    pub loading_strategy: Option<u8>,
    /// Placeholder type for lazy-loaded images (0=none, 1=color, 2=image, 3=skeleton)
    pub image_placeholder_type: Option<u8>,
    /// Placeholder color [r, g, b, a] for lazy-loaded images
    pub image_placeholder_color: Option<[f32; 4]>,
    /// Placeholder image source URL/path (for image_placeholder_type == 2)
    pub image_placeholder_image: Option<String>,
    /// Image fade-in duration in milliseconds
    pub fade_duration_ms: Option<u32>,

    // =========================================================================
    // Interaction Properties
    // =========================================================================
    /// CSS pointer-events (auto, none)
    pub pointer_events: Option<PointerEvents>,
    /// CSS cursor style
    pub cursor: Option<CursorStyle>,

    // =========================================================================
    // Blend Mode
    // =========================================================================
    /// CSS mix-blend-mode
    pub mix_blend_mode: Option<BlendMode>,

    // =========================================================================
    // Text Decoration Enhancements
    // =========================================================================
    /// Text decoration line color (CSS text-decoration-color)
    pub text_decoration_color: Option<Color>,
    /// Text decoration line thickness in pixels (CSS text-decoration-thickness)
    pub text_decoration_thickness: Option<f32>,

    // =========================================================================
    // Text Overflow
    // =========================================================================
    /// CSS text-overflow (clip or ellipsis)
    pub text_overflow: Option<TextOverflow>,
    /// CSS white-space (normal, nowrap, pre, pre-wrap)
    pub white_space: Option<WhiteSpace>,
    /// CSS mask-image (URL or gradient)
    pub mask_image: Option<blinc_core::MaskImage>,
    /// CSS mask-mode (alpha or luminance)
    pub mask_mode: Option<blinc_core::MaskMode>,

    // =========================================================================
    // Flow DAG Reference
    // =========================================================================
    /// Name of a @flow DAG to apply to this element
    pub flow: Option<String>,

    // =========================================================================
    // Pointer Query
    // =========================================================================
    /// Pointer tracking configuration (enables continuous pointer data on this element)
    pub pointer_space: Option<crate::pointer::PointerSpaceConfig>,

    // =========================================================================
    // Dynamic Properties (calc with env vars — evaluated per-frame)
    // =========================================================================
    /// Properties whose values are `calc()` expressions containing `env()` references.
    /// These are evaluated per-frame by `apply_pointer_styles()` with live pointer data.
    pub dynamic_properties: Option<Vec<DynamicProperty>>,
}

impl ElementStyle {
    /// Create a new empty style
    pub fn new() -> Self {
        Self::default()
    }
}

/// Create a new element style
pub fn style() -> ElementStyle {
    ElementStyle::new()
}
