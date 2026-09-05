//! Unified element styling
//!
//! Provides `ElementStyle` - a consistent style schema for all visual properties
//! that can be applied to layout elements. This enables:
//!
//! - Consistent API across `Div`, `StatefulDiv`, and other elements
//! - State-dependent styling with full property support
//! - Style composition and merging
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_core::Color;
//!
//! // Create a style
//! let style = ElementStyle::new()
//!     .bg(Color::BLUE)
//!     .rounded(8.0)
//!     .shadow_md()
//!     .scale(1.0);
//!
//! // Use with stateful elements
//! stateful_button()
//!     .idle(ElementStyle::new().bg(Color::BLUE))
//!     .hovered(ElementStyle::new().bg(Color::LIGHT_BLUE).scale(1.02))
//!     .pressed(ElementStyle::new().bg(Color::DARK_BLUE).scale(0.98));
//! ```

use crate::calc::CalcExpr;
use crate::material::CursorStyle;
use blinc_core::{
    BlendMode, Brush, ClipPath, Color, CornerRadius, CornerShape, OverflowFade, PointerEvents,
    Shadow, Transform,
};

/// A CSS property whose value is a dynamic `calc()` expression containing `env()` references.
/// These are evaluated per-frame with the current pointer query state.
#[derive(Clone, Debug)]
pub enum DynamicProperty {
    Opacity(CalcExpr),
    RotateX(CalcExpr),
    RotateY(CalcExpr),
    Perspective(CalcExpr),
    CornerRadius(CalcExpr),
    TranslateZ(CalcExpr),
    Depth(CalcExpr),
    BorderWidth(CalcExpr),
    /// 2D skew-x (in degrees) — composited into props.transform (Affine2D)
    SkewX(CalcExpr),
    /// 2D skew-y (in degrees) — composited into props.transform (Affine2D)
    SkewY(CalcExpr),
    /// 2D rotate (in degrees) — composited into props.transform (Affine2D)
    Rotate(CalcExpr),
    /// 2D scale-x — composited into props.transform (Affine2D)
    ScaleX(CalcExpr),
    /// 2D scale-y — composited into props.transform (Affine2D)
    ScaleY(CalcExpr),
}

impl DynamicProperty {
    /// Returns true if this dynamic property modifies `props.transform` (Affine2D).
    pub fn is_transform(&self) -> bool {
        matches!(
            self,
            DynamicProperty::SkewX(_)
                | DynamicProperty::SkewY(_)
                | DynamicProperty::Rotate(_)
                | DynamicProperty::ScaleX(_)
                | DynamicProperty::ScaleY(_)
        )
    }

    /// Get the CalcExpr from this dynamic property.
    pub fn expr(&self) -> &CalcExpr {
        match self {
            DynamicProperty::Opacity(e)
            | DynamicProperty::RotateX(e)
            | DynamicProperty::RotateY(e)
            | DynamicProperty::Perspective(e)
            | DynamicProperty::CornerRadius(e)
            | DynamicProperty::TranslateZ(e)
            | DynamicProperty::Depth(e)
            | DynamicProperty::BorderWidth(e)
            | DynamicProperty::SkewX(e)
            | DynamicProperty::SkewY(e)
            | DynamicProperty::Rotate(e)
            | DynamicProperty::ScaleX(e)
            | DynamicProperty::ScaleY(e) => e,
        }
    }
}

/// CSS filter functions applied to an element
///
/// Each field corresponds to a CSS filter function.
/// Default/identity values: grayscale=0, invert=0, sepia=0, hue_rotate=0,
/// brightness=1, contrast=1, saturate=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CssFilter {
    /// Grayscale amount (0.0 = none, 1.0 = full grayscale)
    pub grayscale: f32,
    /// Invert amount (0.0 = none, 1.0 = fully inverted)
    pub invert: f32,
    /// Sepia amount (0.0 = none, 1.0 = full sepia)
    pub sepia: f32,
    /// Hue rotation in degrees
    pub hue_rotate: f32,
    /// Brightness multiplier (1.0 = normal)
    pub brightness: f32,
    /// Contrast multiplier (1.0 = normal)
    pub contrast: f32,
    /// Saturation multiplier (1.0 = normal)
    pub saturate: f32,
    /// Blur radius in pixels (0.0 = no blur)
    pub blur: f32,
    /// Drop shadow (offset, blur, color) — rendered as LayerEffect
    pub drop_shadow: Option<Shadow>,
}

impl Default for CssFilter {
    fn default() -> Self {
        Self {
            grayscale: 0.0,
            invert: 0.0,
            sepia: 0.0,
            hue_rotate: 0.0,
            brightness: 1.0,
            contrast: 1.0,
            saturate: 1.0,
            blur: 0.0,
            drop_shadow: None,
        }
    }
}

impl CssFilter {
    /// Returns true if all filter values are at identity (no effect)
    pub fn is_identity(&self) -> bool {
        self.grayscale == 0.0
            && self.invert == 0.0
            && self.sepia == 0.0
            && self.hue_rotate == 0.0
            && self.brightness == 1.0
            && self.contrast == 1.0
            && self.saturate == 1.0
            && self.blur == 0.0
            && self.drop_shadow.is_none()
    }
}
use blinc_theme::{ShadowTokens, ThemeState};

/// Look up a theme shadow stack and convert it to a `Vec<blinc_core::Shadow>`.
fn theme_shadow_stack<F>(pick: F) -> Vec<Shadow>
where
    F: Fn(&ShadowTokens) -> &[blinc_theme::Shadow],
{
    let shadows = ThemeState::get().shadows();
    pick(&shadows).iter().map(Shadow::from).collect()
}

use crate::material::{GlassMaterial, Material, MetallicMaterial, RenderLayer, WoodMaterial};
use crate::parser::{CssAnimation, CssTransitionSet};

/// Text decoration line types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDecoration {
    /// No decoration
    None,
    /// Underline
    Underline,
    /// Line through the middle of the text
    LineThrough,
}

/// CSS font-style
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontStyle {
    /// Upright
    Normal,
    /// Slanted
    Italic,
}

impl FontStyle {
    /// Whether this style renders italic
    pub fn is_italic(self) -> bool {
        matches!(self, Self::Italic)
    }
}

/// CSS text-overflow behavior
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOverflow {
    /// Clip overflowing text (default)
    Clip,
    /// Show ellipsis (...) when text overflows
    Ellipsis,
}

/// CSS white-space property
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhiteSpace {
    /// Normal whitespace handling (collapse + wrap)
    Normal,
    /// No wrapping (single line)
    Nowrap,
    /// Preserve whitespace and newlines (no wrap)
    Pre,
    /// Preserve whitespace but allow wrapping
    PreWrap,
}

/// CSS scrollbar-width values
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollbarWidth {
    /// Default scrollbar width
    Auto,
    /// Thin scrollbar
    Thin,
    /// Hidden scrollbar (no space taken)
    None,
}

// ============================================================================
// Layout Style Types
// ============================================================================

/// Spacing values for padding and margin (all in pixels)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpacingRect {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl SpacingRect {
    /// All sides equal
    pub fn uniform(px: f32) -> Self {
        Self {
            top: px,
            right: px,
            bottom: px,
            left: px,
        }
    }

    /// Horizontal and vertical
    pub fn xy(x: f32, y: f32) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }

    /// Individual sides
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// Flex direction
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleFlexDirection {
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

/// Display mode
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleDisplay {
    Flex,
    Block,
    None,
}

/// Alignment for align-items and align-self
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleAlign {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}

/// Justify content values
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleJustify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Overflow behavior
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleOverflow {
    Visible,
    Clip,
    Scroll,
}

/// CSS visibility property
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleVisibility {
    Visible,
    Hidden,
}

/// CSS position property
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StylePosition {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// CSS dimension value (length, auto, or keyword)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StyleDimension {
    /// Fixed length in pixels
    Length(f32),
    /// Percentage of parent (0.0-1.0)
    Percent(f32),
    /// Auto sizing (shrink to content)
    Auto,
}

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

    // =========================================================================
    // Transform
    // =========================================================================

    /// Set transform
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }

    /// Scale uniformly
    pub fn scale(self, factor: f32) -> Self {
        self.transform(Transform::scale(factor, factor))
    }

    /// Scale with different x and y factors
    pub fn scale_xy(self, sx: f32, sy: f32) -> Self {
        self.transform(Transform::scale(sx, sy))
    }

    /// Translate by x and y offset
    pub fn translate(self, x: f32, y: f32) -> Self {
        self.transform(Transform::translate(x, y))
    }

    /// Rotate by angle in radians
    pub fn rotate(self, angle: f32) -> Self {
        self.transform(Transform::rotate(angle))
    }

    /// Rotate by angle in degrees
    pub fn rotate_deg(self, degrees: f32) -> Self {
        self.rotate(degrees * std::f32::consts::PI / 180.0)
    }

    // =========================================================================
    // 3D Transform
    // =========================================================================

    /// Set X-axis rotation in degrees (3D tilt)
    pub fn rotate_x_deg(mut self, degrees: f32) -> Self {
        self.rotate_x = Some(degrees);
        self
    }

    /// Set Y-axis rotation in degrees (3D turn)
    pub fn rotate_y_deg(mut self, degrees: f32) -> Self {
        self.rotate_y = Some(degrees);
        self
    }

    /// Set perspective distance in pixels
    pub fn perspective_px(mut self, px: f32) -> Self {
        self.perspective = Some(px);
        self
    }

    /// Set 3D shape type
    pub fn shape_3d(mut self, shape: impl Into<String>) -> Self {
        self.shape_3d = Some(shape.into());
        self
    }

    /// Set 3D extrusion depth in pixels
    pub fn depth_px(mut self, px: f32) -> Self {
        self.depth = Some(px);
        self
    }

    /// Set light direction
    pub fn light_direction(mut self, x: f32, y: f32, z: f32) -> Self {
        self.light_direction = Some([x, y, z]);
        self
    }

    /// Set light intensity
    pub fn light_intensity(mut self, intensity: f32) -> Self {
        self.light_intensity = Some(intensity);
        self
    }

    /// Set ambient light level
    pub fn ambient_light(mut self, level: f32) -> Self {
        self.ambient = Some(level);
        self
    }

    /// Set specular power
    pub fn specular_power(mut self, power: f32) -> Self {
        self.specular = Some(power);
        self
    }

    /// Set translate-z offset in pixels (positive = toward viewer)
    pub fn translate_z_px(mut self, px: f32) -> Self {
        self.translate_z = Some(px);
        self
    }

    /// Set 3D boolean operation type
    pub fn op_3d_type(mut self, op: &str) -> Self {
        self.op_3d = Some(op.to_string());
        self
    }

    /// Set blend radius for smooth boolean operations
    pub fn blend_3d_px(mut self, px: f32) -> Self {
        self.blend_3d = Some(px);
        self
    }

    // =========================================================================
    // Clip-Path
    // =========================================================================

    /// Set CSS clip-path shape function
    pub fn clip_path(mut self, path: ClipPath) -> Self {
        self.clip_path = Some(path);
        self
    }

    // =========================================================================
    // Material
    // =========================================================================

    /// Set material effect
    pub fn material(mut self, material: Material) -> Self {
        // Glass materials also set the render layer to Glass
        if matches!(material, Material::Glass(_)) {
            self.render_layer = Some(RenderLayer::Glass);
        }
        self.material = Some(material);
        self
    }

    /// Apply a visual effect
    pub fn effect(self, effect: impl Into<Material>) -> Self {
        self.material(effect.into())
    }

    /// Apply glass material with default settings
    pub fn glass(self) -> Self {
        self.material(Material::Glass(GlassMaterial::new()))
    }

    /// Apply glass material with custom settings
    pub fn glass_custom(self, glass: GlassMaterial) -> Self {
        self.material(Material::Glass(glass))
    }

    /// Apply metallic material with default settings
    pub fn metallic(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::new()))
    }

    /// Apply chrome metallic preset
    pub fn chrome(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::chrome()))
    }

    /// Apply gold metallic preset
    pub fn gold(self) -> Self {
        self.material(Material::Metallic(MetallicMaterial::gold()))
    }

    /// Apply wood material with default settings
    pub fn wood(self) -> Self {
        self.material(Material::Wood(WoodMaterial::new()))
    }

    // =========================================================================
    // Layer
    // =========================================================================

    /// Set render layer
    pub fn layer(mut self, layer: RenderLayer) -> Self {
        self.render_layer = Some(layer);
        self
    }

    /// Render in foreground layer
    pub fn foreground(self) -> Self {
        self.layer(RenderLayer::Foreground)
    }

    /// Render in background layer
    pub fn layer_background(self) -> Self {
        self.layer(RenderLayer::Background)
    }

    // =========================================================================
    // Opacity
    // =========================================================================

    /// Set opacity (0.0 = transparent, 1.0 = opaque)
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity.clamp(0.0, 1.0));
        self
    }

    /// Fully opaque
    pub fn opaque(self) -> Self {
        self.opacity(1.0)
    }

    /// Semi-transparent (50% opacity)
    pub fn translucent(self) -> Self {
        self.opacity(0.5)
    }

    /// Fully transparent
    pub fn transparent(self) -> Self {
        self.opacity(0.0)
    }

    // =========================================================================
    // Layout: Sizing
    // =========================================================================

    /// Set width in pixels
    pub fn w(mut self, px: f32) -> Self {
        self.width = Some(StyleDimension::Length(px));
        self
    }

    /// Set height in pixels
    pub fn h(mut self, px: f32) -> Self {
        self.height = Some(StyleDimension::Length(px));
        self
    }

    /// Set minimum width in pixels
    pub fn min_w(mut self, px: f32) -> Self {
        self.min_width = Some(px);
        self
    }

    /// Set minimum height in pixels
    pub fn min_h(mut self, px: f32) -> Self {
        self.min_height = Some(px);
        self
    }

    /// Set maximum width in pixels
    pub fn max_w(mut self, px: f32) -> Self {
        self.max_width = Some(px);
        self
    }

    /// Set maximum height in pixels
    pub fn max_h(mut self, px: f32) -> Self {
        self.max_height = Some(px);
        self
    }

    // =========================================================================
    // Layout: Flex Direction & Display
    // =========================================================================

    /// Set display to flex with row direction
    pub fn flex_row(mut self) -> Self {
        self.display = Some(StyleDisplay::Flex);
        self.flex_direction = Some(StyleFlexDirection::Row);
        self
    }

    /// Set display to flex with column direction
    pub fn flex_col(mut self) -> Self {
        self.display = Some(StyleDisplay::Flex);
        self.flex_direction = Some(StyleFlexDirection::Column);
        self
    }

    /// Set display to flex with row-reverse direction
    pub fn flex_row_reverse(mut self) -> Self {
        self.display = Some(StyleDisplay::Flex);
        self.flex_direction = Some(StyleFlexDirection::RowReverse);
        self
    }

    /// Set display to flex with column-reverse direction
    pub fn flex_col_reverse(mut self) -> Self {
        self.display = Some(StyleDisplay::Flex);
        self.flex_direction = Some(StyleFlexDirection::ColumnReverse);
        self
    }

    /// Enable flex wrapping
    pub fn flex_wrap(mut self) -> Self {
        self.flex_wrap = Some(true);
        self
    }

    /// Set display to none (hidden)
    pub fn display_none(mut self) -> Self {
        self.display = Some(StyleDisplay::None);
        self
    }

    // =========================================================================
    // Layout: Flex Properties
    // =========================================================================

    /// Set flex-grow to 1
    pub fn flex_grow(mut self) -> Self {
        self.flex_grow = Some(1.0);
        self
    }

    /// Set flex-grow to a specific value
    pub fn flex_grow_value(mut self, value: f32) -> Self {
        self.flex_grow = Some(value);
        self
    }

    /// Set flex-shrink to 0 (prevent shrinking)
    pub fn flex_shrink_0(mut self) -> Self {
        self.flex_shrink = Some(0.0);
        self
    }

    // =========================================================================
    // Layout: Alignment
    // =========================================================================

    /// Align items to center on cross axis
    pub fn items_center(mut self) -> Self {
        self.align_items = Some(StyleAlign::Center);
        self
    }

    /// Align items to start on cross axis
    pub fn items_start(mut self) -> Self {
        self.align_items = Some(StyleAlign::Start);
        self
    }

    /// Align items to end on cross axis
    pub fn items_end(mut self) -> Self {
        self.align_items = Some(StyleAlign::End);
        self
    }

    /// Stretch items on cross axis
    pub fn items_stretch(mut self) -> Self {
        self.align_items = Some(StyleAlign::Stretch);
        self
    }

    /// Justify content to center on main axis
    pub fn justify_center(mut self) -> Self {
        self.justify_content = Some(StyleJustify::Center);
        self
    }

    /// Justify content to start on main axis
    pub fn justify_start(mut self) -> Self {
        self.justify_content = Some(StyleJustify::Start);
        self
    }

    /// Justify content to end on main axis
    pub fn justify_end(mut self) -> Self {
        self.justify_content = Some(StyleJustify::End);
        self
    }

    /// Space between items on main axis
    pub fn justify_between(mut self) -> Self {
        self.justify_content = Some(StyleJustify::SpaceBetween);
        self
    }

    /// Space around items on main axis
    pub fn justify_around(mut self) -> Self {
        self.justify_content = Some(StyleJustify::SpaceAround);
        self
    }

    /// Space evenly on main axis
    pub fn justify_evenly(mut self) -> Self {
        self.justify_content = Some(StyleJustify::SpaceEvenly);
        self
    }

    /// Align self to center (override parent's align-items)
    pub fn self_center(mut self) -> Self {
        self.align_self = Some(StyleAlign::Center);
        self
    }

    /// Align self to start (override parent's align-items)
    pub fn self_start(mut self) -> Self {
        self.align_self = Some(StyleAlign::Start);
        self
    }

    /// Align self to end (override parent's align-items)
    pub fn self_end(mut self) -> Self {
        self.align_self = Some(StyleAlign::End);
        self
    }

    // =========================================================================
    // Layout: Spacing
    // =========================================================================

    /// Set uniform padding in pixels
    pub fn p(mut self, px: f32) -> Self {
        self.padding = Some(SpacingRect::uniform(px));
        self
    }

    /// Set horizontal and vertical padding in pixels
    pub fn p_xy(mut self, x: f32, y: f32) -> Self {
        self.padding = Some(SpacingRect::xy(x, y));
        self
    }

    /// Set per-side padding in pixels (top, right, bottom, left)
    pub fn p_trbl(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.padding = Some(SpacingRect::new(top, right, bottom, left));
        self
    }

    /// Set uniform margin in pixels
    pub fn m(mut self, px: f32) -> Self {
        self.margin = Some(SpacingRect::uniform(px));
        self
    }

    /// Set horizontal and vertical margin in pixels
    pub fn m_xy(mut self, x: f32, y: f32) -> Self {
        self.margin = Some(SpacingRect::xy(x, y));
        self
    }

    /// Set per-side margin in pixels (top, right, bottom, left)
    pub fn m_trbl(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.margin = Some(SpacingRect::new(top, right, bottom, left));
        self
    }

    /// Set uniform gap between children in pixels
    pub fn gap(mut self, px: f32) -> Self {
        self.gap = Some(px);
        self
    }

    // =========================================================================
    // Layout: Overflow
    // =========================================================================

    /// Clip overflow
    pub fn overflow_clip(mut self) -> Self {
        self.overflow = Some(StyleOverflow::Clip);
        self
    }

    /// Allow visible overflow
    pub fn overflow_visible(mut self) -> Self {
        self.overflow = Some(StyleOverflow::Visible);
        self
    }

    /// Enable scroll overflow
    pub fn overflow_scroll(mut self) -> Self {
        self.overflow = Some(StyleOverflow::Scroll);
        self
    }

    // =========================================================================
    // Overflow Fade
    // =========================================================================

    /// Set uniform overflow fade distance (in pixels)
    pub fn overflow_fade(mut self, distance: f32) -> Self {
        self.overflow_fade = Some(OverflowFade::uniform(distance));
        self
    }

    /// Set per-edge overflow fade (top, right, bottom, left)
    pub fn overflow_fade_edges(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.overflow_fade = Some(OverflowFade::new(top, right, bottom, left));
        self
    }

    /// Set vertical overflow fade only (top + bottom)
    pub fn overflow_fade_y(mut self, distance: f32) -> Self {
        self.overflow_fade = Some(OverflowFade::vertical(distance));
        self
    }

    /// Set horizontal overflow fade only (left + right)
    pub fn overflow_fade_x(mut self, distance: f32) -> Self {
        self.overflow_fade = Some(OverflowFade::horizontal(distance));
        self
    }

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

    // =========================================================================
    // Interaction Properties
    // =========================================================================

    /// Set pointer-events behavior
    pub fn pointer_events(mut self, pe: PointerEvents) -> Self {
        self.pointer_events = Some(pe);
        self
    }

    /// Set pointer-events: none
    pub fn pointer_events_none(mut self) -> Self {
        self.pointer_events = Some(PointerEvents::None);
        self
    }

    /// Set cursor style
    pub fn cursor(mut self, cursor: CursorStyle) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Set mix-blend-mode
    pub fn mix_blend_mode(mut self, mode: BlendMode) -> Self {
        self.mix_blend_mode = Some(mode);
        self
    }

    /// Set text-decoration-color
    pub fn text_decoration_color(mut self, color: Color) -> Self {
        self.text_decoration_color = Some(color);
        self
    }

    /// Set text-decoration-thickness
    pub fn text_decoration_thickness(mut self, thickness: f32) -> Self {
        self.text_decoration_thickness = Some(thickness);
        self
    }

    /// Set text-overflow
    pub fn text_overflow(mut self, overflow: TextOverflow) -> Self {
        self.text_overflow = Some(overflow);
        self
    }

    /// Set white-space
    pub fn white_space(mut self, ws: WhiteSpace) -> Self {
        self.white_space = Some(ws);
        self
    }

    /// Set mask-image URL
    pub fn mask_image(mut self, url: impl Into<String>) -> Self {
        self.mask_image = Some(blinc_core::MaskImage::Url(url.into()));
        self
    }

    /// Set mask-image gradient
    pub fn mask_gradient(mut self, gradient: blinc_core::Gradient) -> Self {
        self.mask_image = Some(blinc_core::MaskImage::Gradient(gradient));
        self
    }

    /// Set mask-mode
    pub fn mask_mode(mut self, mode: blinc_core::MaskMode) -> Self {
        self.mask_mode = Some(mode);
        self
    }

    /// Set @flow shader reference by name
    pub fn flow(mut self, name: impl Into<String>) -> Self {
        self.flow = Some(name.into());
        self
    }

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
    // Transform Extras
    // =========================================================================

    /// Set skew X angle in degrees
    pub fn skew_x(mut self, deg: f32) -> Self {
        self.skew_x = Some(deg);
        self
    }

    /// Set skew Y angle in degrees
    pub fn skew_y(mut self, deg: f32) -> Self {
        self.skew_y = Some(deg);
        self
    }

    /// Set transform origin as percentages [x%, y%] (50, 50 = center)
    pub fn transform_origin(mut self, x: f32, y: f32) -> Self {
        self.transform_origin = Some([x, y]);
        self
    }

    // =========================================================================
    // Transition
    // =========================================================================

    /// Set CSS transition configuration
    pub fn transition(mut self, t: CssTransitionSet) -> Self {
        self.transition = Some(t);
        self
    }

    // =========================================================================
    // Filter
    // =========================================================================

    /// Set CSS filter
    pub fn filter(mut self, f: CssFilter) -> Self {
        self.filter = Some(f);
        self
    }

    // =========================================================================
    // Overflow per-axis
    // =========================================================================

    /// Set overflow-x behavior
    pub fn overflow_x(mut self, o: StyleOverflow) -> Self {
        self.overflow_x = Some(o);
        self
    }

    /// Set overflow-y behavior
    pub fn overflow_y(mut self, o: StyleOverflow) -> Self {
        self.overflow_y = Some(o);
        self
    }

    // =========================================================================
    // Position & Inset
    // =========================================================================

    /// Set CSS position
    pub fn position(mut self, pos: StylePosition) -> Self {
        self.position = Some(pos);
        self
    }

    /// Set top inset in pixels
    pub fn top(mut self, px: f32) -> Self {
        self.top = Some(px);
        self
    }

    /// Set right inset in pixels
    pub fn right(mut self, px: f32) -> Self {
        self.right = Some(px);
        self
    }

    /// Set bottom inset in pixels
    pub fn bottom(mut self, px: f32) -> Self {
        self.bottom = Some(px);
        self
    }

    /// Set left inset in pixels
    pub fn left(mut self, px: f32) -> Self {
        self.left = Some(px);
        self
    }

    /// Set inset for all sides
    pub fn inset(mut self, px: f32) -> Self {
        self.top = Some(px);
        self.right = Some(px);
        self.bottom = Some(px);
        self.left = Some(px);
        self
    }

    /// Set z-index
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = Some(z);
        self
    }

    /// Set visibility
    pub fn visibility(mut self, vis: StyleVisibility) -> Self {
        self.visibility = Some(vis);
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

    // =========================================================================
    // Flex shrink with value
    // =========================================================================

    /// Set flex-shrink to a specific value
    pub fn flex_shrink(mut self, value: f32) -> Self {
        self.flex_shrink = Some(value);
        self
    }

    // =========================================================================
    // Merging
    // =========================================================================

    /// Merge another style on top of this one
    ///
    /// Properties from `other` will override properties in `self` if they are set.
    /// Unset properties in `other` will not override.
    pub fn merge(&self, other: &ElementStyle) -> ElementStyle {
        ElementStyle {
            // Visual
            background: other.background.clone().or_else(|| self.background.clone()),
            corner_radius: other.corner_radius.or(self.corner_radius),
            corner_shape: other.corner_shape.or(self.corner_shape),
            shadow: if !other.shadow.is_empty() {
                other.shadow.clone()
            } else {
                self.shadow.clone()
            },
            transform: other.transform.clone().or_else(|| self.transform.clone()),
            material: other.material.clone().or_else(|| self.material.clone()),
            render_layer: other.render_layer.or(self.render_layer),
            opacity: other.opacity.or(self.opacity),
            text_color: other.text_color.or(self.text_color),
            font_size: other.font_size.or(self.font_size),
            text_shadow: other.text_shadow.or(self.text_shadow),
            font_weight: other.font_weight.or(self.font_weight),
            font_style: other.font_style.or(self.font_style),
            text_decoration: other.text_decoration.or(self.text_decoration),
            line_height: other.line_height.or(self.line_height),
            text_align: other.text_align.or(self.text_align),
            letter_spacing: other.letter_spacing.or(self.letter_spacing),
            rotate: other.rotate.or(self.rotate),
            scale_x: other.scale_x.or(self.scale_x),
            scale_y: other.scale_y.or(self.scale_y),
            skew_x: other.skew_x.or(self.skew_x),
            skew_y: other.skew_y.or(self.skew_y),
            transform_origin: other.transform_origin.or(self.transform_origin),
            animation: other.animation.clone().or_else(|| self.animation.clone()),
            transition: other.transition.clone().or_else(|| self.transition.clone()),
            // 3D
            rotate_x: other.rotate_x.or(self.rotate_x),
            rotate_y: other.rotate_y.or(self.rotate_y),
            perspective: other.perspective.or(self.perspective),
            shape_3d: other.shape_3d.clone().or_else(|| self.shape_3d.clone()),
            depth: other.depth.or(self.depth),
            light_direction: other.light_direction.or(self.light_direction),
            light_intensity: other.light_intensity.or(self.light_intensity),
            ambient: other.ambient.or(self.ambient),
            specular: other.specular.or(self.specular),
            translate_z: other.translate_z.or(self.translate_z),
            op_3d: other.op_3d.clone().or_else(|| self.op_3d.clone()),
            blend_3d: other.blend_3d.or(self.blend_3d),
            // Clip-path
            clip_path: other.clip_path.clone().or_else(|| self.clip_path.clone()),
            filter: other.filter.or(self.filter),
            // Layout
            width: other.width.or(self.width),
            height: other.height.or(self.height),
            min_width: other.min_width.or(self.min_width),
            min_height: other.min_height.or(self.min_height),
            max_width: other.max_width.or(self.max_width),
            max_height: other.max_height.or(self.max_height),
            display: other.display.or(self.display),
            flex_direction: other.flex_direction.or(self.flex_direction),
            flex_wrap: other.flex_wrap.or(self.flex_wrap),
            flex_grow: other.flex_grow.or(self.flex_grow),
            flex_shrink: other.flex_shrink.or(self.flex_shrink),
            align_items: other.align_items.or(self.align_items),
            justify_content: other.justify_content.or(self.justify_content),
            align_self: other.align_self.or(self.align_self),
            padding: other.padding.or(self.padding),
            margin: other.margin.or(self.margin),
            gap: other.gap.or(self.gap),
            overflow: other.overflow.or(self.overflow),
            overflow_x: other.overflow_x.or(self.overflow_x),
            overflow_y: other.overflow_y.or(self.overflow_y),
            overflow_fade: other.overflow_fade.or(self.overflow_fade),
            border_width: other.border_width.or(self.border_width),
            border_color: other.border_color.or(self.border_color),
            outline_width: other.outline_width.or(self.outline_width),
            outline_color: other.outline_color.or(self.outline_color),
            outline_offset: other.outline_offset.or(self.outline_offset),
            // Form element properties
            caret_color: other.caret_color.or(self.caret_color),
            selection_color: other.selection_color.or(self.selection_color),
            placeholder_color: other.placeholder_color.or(self.placeholder_color),
            accent_color: other.accent_color.or(self.accent_color),
            // Scrollbar
            scrollbar_color: other.scrollbar_color.or(self.scrollbar_color),
            scrollbar_width: other.scrollbar_width.or(self.scrollbar_width),
            // SVG
            fill: other.fill.or(self.fill),
            stroke: other.stroke.or(self.stroke),
            stroke_width: other.stroke_width.or(self.stroke_width),
            stroke_dasharray: other
                .stroke_dasharray
                .clone()
                .or(self.stroke_dasharray.clone()),
            stroke_dashoffset: other.stroke_dashoffset.or(self.stroke_dashoffset),
            svg_path_data: other.svg_path_data.clone().or(self.svg_path_data.clone()),
            position: other.position.or(self.position),
            top: other.top.or(self.top),
            right: other.right.or(self.right),
            bottom: other.bottom.or(self.bottom),
            left: other.left.or(self.left),
            z_index: other.z_index.or(self.z_index),
            visibility: other.visibility.or(self.visibility),
            // Image
            object_fit: other.object_fit.or(self.object_fit),
            object_position: other.object_position.or(self.object_position),
            loading_strategy: other.loading_strategy.or(self.loading_strategy),
            image_placeholder_type: other.image_placeholder_type.or(self.image_placeholder_type),
            image_placeholder_color: other
                .image_placeholder_color
                .or(self.image_placeholder_color),
            image_placeholder_image: other
                .image_placeholder_image
                .clone()
                .or_else(|| self.image_placeholder_image.clone()),
            fade_duration_ms: other.fade_duration_ms.or(self.fade_duration_ms),
            // Interaction
            pointer_events: other.pointer_events.or(self.pointer_events),
            cursor: other.cursor.or(self.cursor),
            // Blend mode
            mix_blend_mode: other.mix_blend_mode.or(self.mix_blend_mode),
            // Text decoration enhancements
            text_decoration_color: other.text_decoration_color.or(self.text_decoration_color),
            text_decoration_thickness: other
                .text_decoration_thickness
                .or(self.text_decoration_thickness),
            // Text overflow
            text_overflow: other.text_overflow.or(self.text_overflow),
            white_space: other.white_space.or(self.white_space),
            // Mask
            mask_image: other
                .mask_image
                .as_ref()
                .or(self.mask_image.as_ref())
                .cloned(),
            mask_mode: other.mask_mode.clone().or(self.mask_mode.clone()),
            // Flow DAG
            flow: other.flow.clone().or_else(|| self.flow.clone()),
            // Pointer query
            pointer_space: other
                .pointer_space
                .clone()
                .or_else(|| self.pointer_space.clone()),
            // Dynamic properties (merge: other's override self's for same property type)
            dynamic_properties: match (&self.dynamic_properties, &other.dynamic_properties) {
                (None, None) => None,
                (Some(a), None) => Some(a.clone()),
                (None, Some(b)) => Some(b.clone()),
                (Some(a), Some(b)) => {
                    let mut merged = a.clone();
                    merged.extend(b.iter().cloned());
                    Some(merged)
                }
            },
        }
    }

    /// Check if any visual property is set
    pub fn has_visual_props(&self) -> bool {
        self.background.is_some()
            || self.corner_radius.is_some()
            || self.corner_shape.is_some()
            || !self.shadow.is_empty()
            || self.transform.is_some()
            || self.material.is_some()
            || self.render_layer.is_some()
            || self.opacity.is_some()
            || self.animation.is_some()
            || self.z_index.is_some()
            || self.visibility.is_some()
            || self.overflow_fade.is_some()
    }

    /// Check if any layout property is set
    pub fn has_layout_props(&self) -> bool {
        self.width.is_some()
            || self.height.is_some()
            || self.min_width.is_some()
            || self.min_height.is_some()
            || self.max_width.is_some()
            || self.max_height.is_some()
            || self.display.is_some()
            || self.flex_direction.is_some()
            || self.flex_wrap.is_some()
            || self.flex_grow.is_some()
            || self.flex_shrink.is_some()
            || self.align_items.is_some()
            || self.justify_content.is_some()
            || self.align_self.is_some()
            || self.padding.is_some()
            || self.margin.is_some()
            || self.gap.is_some()
            || self.overflow.is_some()
            || self.overflow_x.is_some()
            || self.overflow_y.is_some()
            || self.border_width.is_some()
            || self.border_color.is_some()
            || self.position.is_some()
            || self.top.is_some()
            || self.right.is_some()
            || self.bottom.is_some()
            || self.left.is_some()
            || self.visibility.is_some()
    }

    /// Check if no property is set
    pub fn is_empty(&self) -> bool {
        !self.has_visual_props() && !self.has_layout_props()
    }

    // =========================================================================
    // Animation
    // =========================================================================

    /// Set CSS animation
    pub fn animation(mut self, animation: CssAnimation) -> Self {
        self.animation = Some(animation);
        self
    }

    /// Set animation by name (requires stylesheet lookup later)
    pub fn animation_name(mut self, name: impl Into<String>) -> Self {
        let mut anim = self.animation.take().unwrap_or_default();
        anim.name = name.into();
        self.animation = Some(anim);
        self
    }

    /// Set animation duration in milliseconds
    pub fn animation_duration(mut self, duration_ms: u32) -> Self {
        let mut anim = self.animation.take().unwrap_or_default();
        anim.duration_ms = duration_ms;
        self.animation = Some(anim);
        self
    }
}

/// Create a new element style
pub fn style() -> ElementStyle {
    ElementStyle::new()
}

/// CSS-like macro for creating ElementStyle with CSS property names
///
/// Uses CSS property naming conventions (with hyphens parsed as separate tokens).
/// Provides a familiar syntax for developers coming from CSS/web development.
///
/// # Examples
///
/// ```ignore
/// use blinc_layout::prelude::*;
/// use blinc_core::Color;
///
/// // CSS-style properties (note: use spaces around hyphens)
/// let card = css! {
///     background: Color::WHITE;
///     border-radius: 8.0;
///     box-shadow: Shadow::new(0.0, 4.0, 8.0, Color::BLACK.with_alpha(0.2));
///     opacity: 0.9;
/// };
///
/// // Transform properties
/// let hover = css! {
///     transform: Transform::scale(1.05, 1.05);
///     opacity: 1.0;
/// };
///
/// // Material effects (Blinc extensions)
/// let glass_panel = css! {
///     background: Color::WHITE.with_alpha(0.1);
///     border-radius: 16.0;
///     backdrop-filter: glass;
/// };
///
/// // Animation
/// let animated = css! {
///     animation-name: "fade-in";
///     animation-duration: 300;
/// };
/// ```
///
/// # Supported Properties
///
/// ## Visual
/// - `background`: Color or Brush
/// - `border-radius`: f32 or CornerRadius
/// - `box-shadow`: sm | md | lg | xl | none | Shadow
/// - `opacity`: f32 (0.0-1.0)
/// - `transform`: Transform | `scale(f)` | `scale(x,y)` | `translate(x,y)` | `rotate(deg)` | `skewX(deg)` | `skewY(deg)`
/// - `transform-origin`: (x%, y%)
/// - `clip-path`: ClipPath
/// - `filter`: CssFilter
/// - `mask-image`: MaskImage
/// - `mask-mode`: MaskMode
/// - `mix-blend-mode`: BlendMode
///
/// ## Text
/// - `color`: Color (text color)
/// - `font-size`: f32 (pixels)
/// - `font-weight`: FontWeight
/// - `text-decoration`: TextDecoration
/// - `text-decoration-color`: Color
/// - `text-decoration-thickness`: f32
/// - `line-height`: f32
/// - `text-align`: left | center | right | TextAlign
/// - `letter-spacing`: f32
/// - `text-shadow`: Shadow
/// - `text-overflow`: clip | ellipsis | TextOverflow
/// - `white-space`: normal | nowrap | pre | WhiteSpace
///
/// ## Layout
/// - `width`, `height`, `min-width`, `min-height`, `max-width`, `max-height`: f32
/// - `display`: flex | block | none
/// - `flex-direction`: row | column | row-reverse | column-reverse
/// - `flex-wrap`: wrap | nowrap
/// - `flex-grow`, `flex-shrink`: f32
/// - `align-items`: center | start | end | stretch | baseline
/// - `justify-content`: center | start | end | space-between | space-around | space-evenly
/// - `align-self`: center | start | end | stretch | baseline
/// - `padding`, `margin`: f32 (uniform)
/// - `gap`: f32
/// - `overflow`: clip | hidden | visible | scroll
/// - `overflow-x`, `overflow-y`: clip | hidden | visible | scroll
/// - `position`: static | relative | absolute | fixed | sticky
/// - `top`, `right`, `bottom`, `left`: f32
/// - `inset`: f32 (all sides)
/// - `z-index`: i32
/// - `visibility`: visible | hidden
///
/// ## Border & Outline
/// - `border`: (width, color)
/// - `border-width`: f32
/// - `border-color`: Color
/// - `outline`: (width, color)
/// - `outline-width`, `outline-color`, `outline-offset`: f32 / Color
///
/// ## 3D
/// - `rotate-x`, `rotate-y`: f32 (degrees)
/// - `perspective`: f32, `translate-z`: f32
/// - `shape-3d`: "box" | "sphere" | "cylinder" | "torus" | "capsule" | "group"
/// - `depth`: f32, `light-direction`: (x,y,z), `light-intensity`, `ambient`, `specular`: f32
/// - `3d-op`: "union" | "subtract" | "intersect" | smooth variants
/// - `3d-blend`: f32
///
/// ## Materials
/// - `backdrop-filter`: glass | metallic | chrome | gold | wood | Material
/// - `render-layer`: foreground | background | RenderLayer
///
/// ## Animation & Transition
/// - `animation`: CssAnimation
/// - `animation-name`: String, `animation-duration`: u32 (ms)
/// - `animation-delay`: u32, `animation-timing-function`, `animation-iteration-count`: u32
/// - `animation-direction`, `animation-fill-mode`
/// - `transition`: CssTransitionSet
///
/// ## SVG
/// - `fill`, `stroke`: Color
/// - `stroke-width`: f32, `stroke-dasharray`: `Vec<f32>`, `stroke-dashoffset`: f32
///
/// ## Form Controls
/// - `caret-color`, `selection-color`, `placeholder-color`, `accent-color`: Color
/// - `scrollbar-color`: (thumb, track), `scrollbar-width`: auto | thin | none
///
/// ## Interaction
/// - `pointer-events`: auto | none
/// - `cursor`: CursorStyle
///
/// ## Image
/// - `object-fit`: u8, `object-position`: (x, y)
#[macro_export]
macro_rules! css {
    // Empty style
    () => {
        $crate::element_style::ElementStyle::new()
    };

    // Main entry point - parse CSS properties (semicolon separated)
    ($($tokens:tt)*) => {{
        let mut __style = $crate::element_style::ElementStyle::new();
        $crate::css_impl!(__style; $($tokens)*);
        __style
    }};
}

/// Internal macro for parsing CSS properties
#[macro_export]
#[doc(hidden)]
macro_rules! css_impl {
    // Base case - no more tokens
    ($style:ident;) => {};

    // =========================================================================
    // Background (CSS: background)
    // =========================================================================
    ($style:ident; background: $value:expr; $($rest:tt)*) => {
        $style = $style.bg($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; background: $value:expr) => {
        $style = $style.bg($value);
    };

    // =========================================================================
    // Border Radius (CSS: border-radius)
    // =========================================================================
    ($style:ident; border-radius: $value:expr; $($rest:tt)*) => {
        $style = $style.rounded($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; border-radius: $value:expr) => {
        $style = $style.rounded($value);
    };

    // =========================================================================
    // Corner Shape (CSS: corner-shape)
    // =========================================================================
    // corner-shape keyword values
    ($style:ident; corner-shape: round; $($rest:tt)*) => {
        $style = $style.corner_shape(1.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: round) => {
        $style = $style.corner_shape(1.0);
    };
    ($style:ident; corner-shape: bevel; $($rest:tt)*) => {
        $style = $style.corner_shape(0.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: bevel) => {
        $style = $style.corner_shape(0.0);
    };
    ($style:ident; corner-shape: squircle; $($rest:tt)*) => {
        $style = $style.corner_shape(2.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: squircle) => {
        $style = $style.corner_shape(2.0);
    };
    ($style:ident; corner-shape: scoop; $($rest:tt)*) => {
        $style = $style.corner_shape(-1.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: scoop) => {
        $style = $style.corner_shape(-1.0);
    };
    ($style:ident; corner-shape: notch; $($rest:tt)*) => {
        $style = $style.corner_shape(-100.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: notch) => {
        $style = $style.corner_shape(-100.0);
    };
    ($style:ident; corner-shape: square; $($rest:tt)*) => {
        $style = $style.corner_shape(100.0);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: square) => {
        $style = $style.corner_shape(100.0);
    };
    ($style:ident; corner-shape: $value:expr; $($rest:tt)*) => {
        $style = $style.corner_shape($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; corner-shape: $value:expr) => {
        $style = $style.corner_shape($value);
    };

    // =========================================================================
    // Overflow Fade (CSS: overflow-fade)
    // =========================================================================
    ($style:ident; overflow-fade: $value:expr; $($rest:tt)*) => {
        $style = $style.overflow_fade($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-fade: $value:expr) => {
        $style = $style.overflow_fade($value);
    };

    // =========================================================================
    // Box Shadow (CSS: box-shadow)
    // Shadow presets must come BEFORE generic expr to match correctly
    // =========================================================================
    ($style:ident; box-shadow: sm; $($rest:tt)*) => {
        $style = $style.shadow_sm();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: sm) => {
        $style = $style.shadow_sm();
    };
    ($style:ident; box-shadow: md; $($rest:tt)*) => {
        $style = $style.shadow_md();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: md) => {
        $style = $style.shadow_md();
    };
    ($style:ident; box-shadow: lg; $($rest:tt)*) => {
        $style = $style.shadow_lg();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: lg) => {
        $style = $style.shadow_lg();
    };
    ($style:ident; box-shadow: xl; $($rest:tt)*) => {
        $style = $style.shadow_xl();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: xl) => {
        $style = $style.shadow_xl();
    };
    ($style:ident; box-shadow: none; $($rest:tt)*) => {
        $style = $style.shadow_none();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: none) => {
        $style = $style.shadow_none();
    };
    // Generic expression (must come after presets)
    ($style:ident; box-shadow: $value:expr; $($rest:tt)*) => {
        $style = $style.shadow($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; box-shadow: $value:expr) => {
        $style = $style.shadow($value);
    };

    // =========================================================================
    // Opacity (CSS: opacity)
    // =========================================================================
    ($style:ident; opacity: $value:expr; $($rest:tt)*) => {
        $style = $style.opacity($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; opacity: $value:expr) => {
        $style = $style.opacity($value);
    };

    // =========================================================================
    // Flow (CSS: flow)
    // =========================================================================
    ($style:ident; flow: $value:expr; $($rest:tt)*) => {
        $style = $style.flow($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flow: $value:expr) => {
        $style = $style.flow($value);
    };

    // =========================================================================
    // Transform (CSS: transform)
    // =========================================================================
    ($style:ident; transform: $value:expr; $($rest:tt)*) => {
        $style = $style.transform($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; transform: $value:expr) => {
        $style = $style.transform($value);
    };
    // Scale shorthand
    ($style:ident; transform: scale($value:expr); $($rest:tt)*) => {
        $style = $style.scale($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; transform: scale($sx:expr, $sy:expr); $($rest:tt)*) => {
        $style = $style.scale_xy($sx, $sy);
        $crate::css_impl!($style; $($rest)*);
    };
    // Translate shorthand
    ($style:ident; transform: translate($x:expr, $y:expr); $($rest:tt)*) => {
        $style = $style.translate($x, $y);
        $crate::css_impl!($style; $($rest)*);
    };
    // Rotate shorthand (degrees)
    ($style:ident; transform: rotate($deg:expr); $($rest:tt)*) => {
        $style = $style.rotate_deg($deg);
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // 3D Transform Properties
    // =========================================================================
    ($style:ident; rotate-x: $value:expr; $($rest:tt)*) => {
        $style = $style.rotate_x_deg($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; rotate-x: $value:expr) => {
        $style = $style.rotate_x_deg($value);
    };
    ($style:ident; rotate-y: $value:expr; $($rest:tt)*) => {
        $style = $style.rotate_y_deg($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; rotate-y: $value:expr) => {
        $style = $style.rotate_y_deg($value);
    };
    ($style:ident; perspective: $value:expr; $($rest:tt)*) => {
        $style = $style.perspective_px($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; perspective: $value:expr) => {
        $style = $style.perspective_px($value);
    };
    ($style:ident; translate-z: $value:expr; $($rest:tt)*) => {
        $style = $style.translate_z_px($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; translate-z: $value:expr) => {
        $style = $style.translate_z_px($value);
    };

    // =========================================================================
    // 3D SDF Shape Properties
    // =========================================================================
    ($style:ident; shape-3d: $value:expr; $($rest:tt)*) => {
        $style = $style.shape_3d($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; shape-3d: $value:expr) => {
        $style = $style.shape_3d($value);
    };
    ($style:ident; depth: $value:expr; $($rest:tt)*) => {
        $style = $style.depth_px($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; depth: $value:expr) => {
        $style = $style.depth_px($value);
    };

    // =========================================================================
    // 3D Lighting Properties
    // =========================================================================
    ($style:ident; light-direction: ($x:expr, $y:expr, $z:expr); $($rest:tt)*) => {
        $style = $style.light_direction($x, $y, $z);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; light-direction: ($x:expr, $y:expr, $z:expr)) => {
        $style = $style.light_direction($x, $y, $z);
    };
    ($style:ident; light-intensity: $value:expr; $($rest:tt)*) => {
        $style = $style.light_intensity($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; light-intensity: $value:expr) => {
        $style = $style.light_intensity($value);
    };
    ($style:ident; ambient: $value:expr; $($rest:tt)*) => {
        $style = $style.ambient_light($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; ambient: $value:expr) => {
        $style = $style.ambient_light($value);
    };
    ($style:ident; specular: $value:expr; $($rest:tt)*) => {
        $style = $style.specular_power($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; specular: $value:expr) => {
        $style = $style.specular_power($value);
    };

    // =========================================================================
    // 3D Boolean Operation Properties
    // =========================================================================
    ($style:ident; 3d-op: $value:expr; $($rest:tt)*) => {
        $style = $style.op_3d_type($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; 3d-op: $value:expr) => {
        $style = $style.op_3d_type($value);
    };
    ($style:ident; 3d-blend: $value:expr; $($rest:tt)*) => {
        $style = $style.blend_3d_px($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; 3d-blend: $value:expr) => {
        $style = $style.blend_3d_px($value);
    };

    // =========================================================================
    // Clip-Path
    // =========================================================================
    ($style:ident; clip-path: $value:expr; $($rest:tt)*) => {
        $style = $style.clip_path($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; clip-path: $value:expr) => {
        $style = $style.clip_path($value);
    };

    // =========================================================================
    // Backdrop Filter (Blinc extension for materials)
    // =========================================================================
    ($style:ident; backdrop-filter: glass; $($rest:tt)*) => {
        $style = $style.glass();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; backdrop-filter: glass) => {
        $style = $style.glass();
    };
    ($style:ident; backdrop-filter: metallic; $($rest:tt)*) => {
        $style = $style.metallic();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; backdrop-filter: chrome; $($rest:tt)*) => {
        $style = $style.chrome();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; backdrop-filter: gold; $($rest:tt)*) => {
        $style = $style.gold();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; backdrop-filter: wood; $($rest:tt)*) => {
        $style = $style.wood();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; backdrop-filter: $value:expr; $($rest:tt)*) => {
        $style = $style.material($value);
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Render Layer (Blinc extension)
    // =========================================================================
    ($style:ident; render-layer: foreground; $($rest:tt)*) => {
        $style = $style.foreground();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; render-layer: background; $($rest:tt)*) => {
        $style = $style.layer_background();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; render-layer: $value:expr; $($rest:tt)*) => {
        $style = $style.layer($value);
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Animation Properties
    // =========================================================================
    ($style:ident; animation: $value:expr; $($rest:tt)*) => {
        $style = $style.animation($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation: $value:expr) => {
        $style = $style.animation($value);
    };
    ($style:ident; animation-name: $value:expr; $($rest:tt)*) => {
        $style = $style.animation_name($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-name: $value:expr) => {
        $style = $style.animation_name($value);
    };
    ($style:ident; animation-duration: $value:expr; $($rest:tt)*) => {
        $style = $style.animation_duration($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-duration: $value:expr) => {
        $style = $style.animation_duration($value);
    };

    // =========================================================================
    // Layout: Sizing (CSS: width, height, min-width, etc.)
    // =========================================================================
    ($style:ident; width: $value:expr; $($rest:tt)*) => {
        $style = $style.w($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; width: $value:expr) => {
        $style = $style.w($value);
    };
    ($style:ident; height: $value:expr; $($rest:tt)*) => {
        $style = $style.h($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; height: $value:expr) => {
        $style = $style.h($value);
    };
    ($style:ident; min-width: $value:expr; $($rest:tt)*) => {
        $style = $style.min_w($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; min-width: $value:expr) => {
        $style = $style.min_w($value);
    };
    ($style:ident; min-height: $value:expr; $($rest:tt)*) => {
        $style = $style.min_h($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; min-height: $value:expr) => {
        $style = $style.min_h($value);
    };
    ($style:ident; max-width: $value:expr; $($rest:tt)*) => {
        $style = $style.max_w($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; max-width: $value:expr) => {
        $style = $style.max_w($value);
    };
    ($style:ident; max-height: $value:expr; $($rest:tt)*) => {
        $style = $style.max_h($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; max-height: $value:expr) => {
        $style = $style.max_h($value);
    };

    // =========================================================================
    // Layout: Flex Direction (CSS: display, flex-direction, flex-wrap)
    // =========================================================================
    ($style:ident; display: flex; $($rest:tt)*) => {
        $style.display = Some($crate::element_style::StyleDisplay::Flex);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; display: none; $($rest:tt)*) => {
        $style = $style.display_none();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-direction: row; $($rest:tt)*) => {
        $style = $style.flex_row();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-direction: column; $($rest:tt)*) => {
        $style = $style.flex_col();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-direction: row-reverse; $($rest:tt)*) => {
        $style = $style.flex_row_reverse();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-direction: column-reverse; $($rest:tt)*) => {
        $style = $style.flex_col_reverse();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-wrap: wrap; $($rest:tt)*) => {
        $style = $style.flex_wrap();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-grow: $value:expr; $($rest:tt)*) => {
        $style = $style.flex_grow_value($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-grow: $value:expr) => {
        $style = $style.flex_grow_value($value);
    };
    ($style:ident; flex-shrink: $value:expr; $($rest:tt)*) => {
        $style.flex_shrink = Some($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-shrink: $value:expr) => {
        $style.flex_shrink = Some($value);
    };

    // =========================================================================
    // Layout: Alignment (CSS: align-items, justify-content, align-self)
    // =========================================================================
    ($style:ident; align-items: center; $($rest:tt)*) => {
        $style = $style.items_center();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-items: start; $($rest:tt)*) => {
        $style = $style.items_start();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-items: end; $($rest:tt)*) => {
        $style = $style.items_end();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-items: stretch; $($rest:tt)*) => {
        $style = $style.items_stretch();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: center; $($rest:tt)*) => {
        $style = $style.justify_center();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: start; $($rest:tt)*) => {
        $style = $style.justify_start();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: end; $($rest:tt)*) => {
        $style = $style.justify_end();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: space-between; $($rest:tt)*) => {
        $style = $style.justify_between();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: space-around; $($rest:tt)*) => {
        $style = $style.justify_around();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; justify-content: space-evenly; $($rest:tt)*) => {
        $style = $style.justify_evenly();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-self: center; $($rest:tt)*) => {
        $style = $style.self_center();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-self: start; $($rest:tt)*) => {
        $style = $style.self_start();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-self: end; $($rest:tt)*) => {
        $style = $style.self_end();
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Layout: Spacing (CSS: padding, margin, gap)
    // =========================================================================
    ($style:ident; padding: $value:expr; $($rest:tt)*) => {
        $style = $style.p($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; padding: $value:expr) => {
        $style = $style.p($value);
    };
    ($style:ident; margin: $value:expr; $($rest:tt)*) => {
        $style = $style.m($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; margin: $value:expr) => {
        $style = $style.m($value);
    };
    ($style:ident; gap: $value:expr; $($rest:tt)*) => {
        $style = $style.gap($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; gap: $value:expr) => {
        $style = $style.gap($value);
    };

    // =========================================================================
    // Layout: Overflow (CSS: overflow)
    // =========================================================================
    ($style:ident; overflow: clip; $($rest:tt)*) => {
        $style = $style.overflow_clip();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow: visible; $($rest:tt)*) => {
        $style = $style.overflow_visible();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow: scroll; $($rest:tt)*) => {
        $style = $style.overflow_scroll();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow: hidden; $($rest:tt)*) => {
        $style = $style.overflow_clip();
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Layout: Border (CSS: border-width, border-color)
    // =========================================================================
    ($style:ident; border-width: $value:expr; $($rest:tt)*) => {
        $style = $style.border_w($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; border-width: $value:expr) => {
        $style = $style.border_w($value);
    };
    ($style:ident; border-color: $value:expr; $($rest:tt)*) => {
        $style.border_color = Some($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; border-color: $value:expr) => {
        $style.border_color = Some($value);
    };

    // =========================================================================
    // Text Properties (CSS: color, font-size, font-weight, etc.)
    // =========================================================================
    ($style:ident; color: $value:expr; $($rest:tt)*) => {
        $style = $style.text_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; color: $value:expr) => {
        $style = $style.text_color($value);
    };
    ($style:ident; font-size: $value:expr; $($rest:tt)*) => {
        $style = $style.font_size($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; font-size: $value:expr) => {
        $style = $style.font_size($value);
    };
    ($style:ident; font-weight: $value:expr; $($rest:tt)*) => {
        $style = $style.font_weight($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; font-weight: $value:expr) => {
        $style = $style.font_weight($value);
    };
    ($style:ident; font-style: $value:expr; $($rest:tt)*) => {
        $style = $style.font_style($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; font-style: $value:expr) => {
        $style = $style.font_style($value);
    };
    ($style:ident; text-decoration: $value:expr; $($rest:tt)*) => {
        $style = $style.text_decoration($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-decoration: $value:expr) => {
        $style = $style.text_decoration($value);
    };
    ($style:ident; text-decoration-color: $value:expr; $($rest:tt)*) => {
        $style = $style.text_decoration_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-decoration-color: $value:expr) => {
        $style = $style.text_decoration_color($value);
    };
    ($style:ident; text-decoration-thickness: $value:expr; $($rest:tt)*) => {
        $style = $style.text_decoration_thickness($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-decoration-thickness: $value:expr) => {
        $style = $style.text_decoration_thickness($value);
    };
    ($style:ident; line-height: $value:expr; $($rest:tt)*) => {
        $style = $style.line_height($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; line-height: $value:expr) => {
        $style = $style.line_height($value);
    };
    ($style:ident; text-align: center; $($rest:tt)*) => {
        $style = $style.text_align($crate::text::TextAlign::Center);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-align: right; $($rest:tt)*) => {
        $style = $style.text_align($crate::text::TextAlign::Right);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-align: left; $($rest:tt)*) => {
        $style = $style.text_align($crate::text::TextAlign::Left);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-align: $value:expr; $($rest:tt)*) => {
        $style = $style.text_align($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-align: $value:expr) => {
        $style = $style.text_align($value);
    };
    ($style:ident; letter-spacing: $value:expr; $($rest:tt)*) => {
        $style = $style.letter_spacing($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; letter-spacing: $value:expr) => {
        $style = $style.letter_spacing($value);
    };
    ($style:ident; text-shadow: $value:expr; $($rest:tt)*) => {
        $style = $style.text_shadow($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-shadow: $value:expr) => {
        $style = $style.text_shadow($value);
    };
    ($style:ident; text-overflow: clip; $($rest:tt)*) => {
        $style = $style.text_overflow($crate::element_style::TextOverflow::Clip);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-overflow: ellipsis; $($rest:tt)*) => {
        $style = $style.text_overflow($crate::element_style::TextOverflow::Ellipsis);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-overflow: $value:expr; $($rest:tt)*) => {
        $style = $style.text_overflow($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; text-overflow: $value:expr) => {
        $style = $style.text_overflow($value);
    };
    ($style:ident; white-space: normal; $($rest:tt)*) => {
        $style = $style.white_space($crate::element_style::WhiteSpace::Normal);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; white-space: nowrap; $($rest:tt)*) => {
        $style = $style.white_space($crate::element_style::WhiteSpace::Nowrap);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; white-space: pre; $($rest:tt)*) => {
        $style = $style.white_space($crate::element_style::WhiteSpace::Pre);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; white-space: $value:expr; $($rest:tt)*) => {
        $style = $style.white_space($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; white-space: $value:expr) => {
        $style = $style.white_space($value);
    };

    // =========================================================================
    // Transform Extras (CSS: transform-origin, skew)
    // =========================================================================
    ($style:ident; transform-origin: ($x:expr, $y:expr); $($rest:tt)*) => {
        $style = $style.transform_origin($x, $y);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; transform-origin: ($x:expr, $y:expr)) => {
        $style = $style.transform_origin($x, $y);
    };
    ($style:ident; transform: skewX($deg:expr); $($rest:tt)*) => {
        $style = $style.skew_x($deg);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; transform: skewY($deg:expr); $($rest:tt)*) => {
        $style = $style.skew_y($deg);
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Transition (CSS: transition)
    // =========================================================================
    ($style:ident; transition: $value:expr; $($rest:tt)*) => {
        $style = $style.transition($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; transition: $value:expr) => {
        $style = $style.transition($value);
    };

    // =========================================================================
    // Filter (CSS: filter)
    // =========================================================================
    ($style:ident; filter: $value:expr; $($rest:tt)*) => {
        $style = $style.filter($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; filter: $value:expr) => {
        $style = $style.filter($value);
    };

    // =========================================================================
    // Mask Properties (CSS: mask-image, mask-mode)
    // =========================================================================
    ($style:ident; mask-image: $value:expr; $($rest:tt)*) => {
        $style.mask_image = Some($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; mask-image: $value:expr) => {
        $style.mask_image = Some($value);
    };
    ($style:ident; mask-mode: $value:expr; $($rest:tt)*) => {
        $style = $style.mask_mode($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; mask-mode: $value:expr) => {
        $style = $style.mask_mode($value);
    };

    // =========================================================================
    // Mix Blend Mode (CSS: mix-blend-mode)
    // =========================================================================
    ($style:ident; mix-blend-mode: $value:expr; $($rest:tt)*) => {
        $style = $style.mix_blend_mode($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; mix-blend-mode: $value:expr) => {
        $style = $style.mix_blend_mode($value);
    };

    // =========================================================================
    // Outline (CSS: outline, outline-width, outline-color, outline-offset)
    // =========================================================================
    ($style:ident; outline: ($width:expr, $color:expr); $($rest:tt)*) => {
        $style = $style.outline($width, $color);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; outline: ($width:expr, $color:expr)) => {
        $style = $style.outline($width, $color);
    };
    ($style:ident; outline-width: $value:expr; $($rest:tt)*) => {
        $style = $style.outline_w($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; outline-width: $value:expr) => {
        $style = $style.outline_w($value);
    };
    ($style:ident; outline-color: $value:expr; $($rest:tt)*) => {
        $style.outline_color = Some($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; outline-color: $value:expr) => {
        $style.outline_color = Some($value);
    };
    ($style:ident; outline-offset: $value:expr; $($rest:tt)*) => {
        $style = $style.outline_offset($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; outline-offset: $value:expr) => {
        $style = $style.outline_offset($value);
    };

    // =========================================================================
    // Border shorthand (CSS: border)
    // =========================================================================
    ($style:ident; border: ($width:expr, $color:expr); $($rest:tt)*) => {
        $style = $style.border($width, $color);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; border: ($width:expr, $color:expr)) => {
        $style = $style.border($width, $color);
    };

    // =========================================================================
    // Overflow per-axis (CSS: overflow-x, overflow-y)
    // =========================================================================
    ($style:ident; overflow-x: clip; $($rest:tt)*) => {
        $style = $style.overflow_x($crate::element_style::StyleOverflow::Clip);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-x: hidden; $($rest:tt)*) => {
        $style = $style.overflow_x($crate::element_style::StyleOverflow::Clip);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-x: visible; $($rest:tt)*) => {
        $style = $style.overflow_x($crate::element_style::StyleOverflow::Visible);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-x: scroll; $($rest:tt)*) => {
        $style = $style.overflow_x($crate::element_style::StyleOverflow::Scroll);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-y: clip; $($rest:tt)*) => {
        $style = $style.overflow_y($crate::element_style::StyleOverflow::Clip);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-y: hidden; $($rest:tt)*) => {
        $style = $style.overflow_y($crate::element_style::StyleOverflow::Clip);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-y: visible; $($rest:tt)*) => {
        $style = $style.overflow_y($crate::element_style::StyleOverflow::Visible);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; overflow-y: scroll; $($rest:tt)*) => {
        $style = $style.overflow_y($crate::element_style::StyleOverflow::Scroll);
        $crate::css_impl!($style; $($rest)*);
    };

    // =========================================================================
    // Position & Inset (CSS: position, top, right, bottom, left, inset)
    // =========================================================================
    ($style:ident; position: static; $($rest:tt)*) => {
        $style = $style.position($crate::element_style::StylePosition::Static);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: relative; $($rest:tt)*) => {
        $style = $style.position($crate::element_style::StylePosition::Relative);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: absolute; $($rest:tt)*) => {
        $style = $style.position($crate::element_style::StylePosition::Absolute);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: fixed; $($rest:tt)*) => {
        $style = $style.position($crate::element_style::StylePosition::Fixed);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: sticky; $($rest:tt)*) => {
        $style = $style.position($crate::element_style::StylePosition::Sticky);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: $value:expr; $($rest:tt)*) => {
        $style = $style.position($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; position: $value:expr) => {
        $style = $style.position($value);
    };
    ($style:ident; top: $value:expr; $($rest:tt)*) => {
        $style = $style.top($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; top: $value:expr) => {
        $style = $style.top($value);
    };
    ($style:ident; right: $value:expr; $($rest:tt)*) => {
        $style = $style.right($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; right: $value:expr) => {
        $style = $style.right($value);
    };
    ($style:ident; bottom: $value:expr; $($rest:tt)*) => {
        $style = $style.bottom($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; bottom: $value:expr) => {
        $style = $style.bottom($value);
    };
    ($style:ident; left: $value:expr; $($rest:tt)*) => {
        $style = $style.left($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; left: $value:expr) => {
        $style = $style.left($value);
    };
    ($style:ident; inset: $value:expr; $($rest:tt)*) => {
        $style = $style.inset($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; inset: $value:expr) => {
        $style = $style.inset($value);
    };
    ($style:ident; z-index: $value:expr; $($rest:tt)*) => {
        $style = $style.z_index($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; z-index: $value:expr) => {
        $style = $style.z_index($value);
    };
    ($style:ident; visibility: visible; $($rest:tt)*) => {
        $style = $style.visibility($crate::element_style::StyleVisibility::Visible);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; visibility: hidden; $($rest:tt)*) => {
        $style = $style.visibility($crate::element_style::StyleVisibility::Hidden);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; visibility: $value:expr; $($rest:tt)*) => {
        $style = $style.visibility($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; visibility: $value:expr) => {
        $style = $style.visibility($value);
    };

    // =========================================================================
    // Form Element Colors
    // =========================================================================
    ($style:ident; caret-color: $value:expr; $($rest:tt)*) => {
        $style = $style.caret_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; caret-color: $value:expr) => {
        $style = $style.caret_color($value);
    };
    ($style:ident; selection-color: $value:expr; $($rest:tt)*) => {
        $style = $style.selection_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; selection-color: $value:expr) => {
        $style = $style.selection_color($value);
    };
    ($style:ident; placeholder-color: $value:expr; $($rest:tt)*) => {
        $style = $style.placeholder_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; placeholder-color: $value:expr) => {
        $style = $style.placeholder_color($value);
    };
    ($style:ident; accent-color: $value:expr; $($rest:tt)*) => {
        $style = $style.accent_color($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; accent-color: $value:expr) => {
        $style = $style.accent_color($value);
    };

    // =========================================================================
    // Scrollbar Properties
    // =========================================================================
    ($style:ident; scrollbar-color: ($thumb:expr, $track:expr); $($rest:tt)*) => {
        $style = $style.scrollbar_color($thumb, $track);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; scrollbar-color: ($thumb:expr, $track:expr)) => {
        $style = $style.scrollbar_color($thumb, $track);
    };
    ($style:ident; scrollbar-width: auto; $($rest:tt)*) => {
        $style = $style.scrollbar_width($crate::element_style::ScrollbarWidth::Auto);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; scrollbar-width: thin; $($rest:tt)*) => {
        $style = $style.scrollbar_width($crate::element_style::ScrollbarWidth::Thin);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; scrollbar-width: none; $($rest:tt)*) => {
        $style = $style.scrollbar_width($crate::element_style::ScrollbarWidth::None);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; scrollbar-width: $value:expr; $($rest:tt)*) => {
        $style = $style.scrollbar_width($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; scrollbar-width: $value:expr) => {
        $style = $style.scrollbar_width($value);
    };

    // =========================================================================
    // SVG Properties (CSS: fill, stroke, etc.)
    // =========================================================================
    ($style:ident; fill: $value:expr; $($rest:tt)*) => {
        $style = $style.fill($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; fill: $value:expr) => {
        $style = $style.fill($value);
    };
    ($style:ident; stroke: $value:expr; $($rest:tt)*) => {
        $style = $style.stroke($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; stroke: $value:expr) => {
        $style = $style.stroke($value);
    };
    ($style:ident; stroke-width: $value:expr; $($rest:tt)*) => {
        $style = $style.stroke_width($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; stroke-width: $value:expr) => {
        $style = $style.stroke_width($value);
    };
    ($style:ident; stroke-dasharray: $value:expr; $($rest:tt)*) => {
        $style = $style.stroke_dasharray($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; stroke-dasharray: $value:expr) => {
        $style = $style.stroke_dasharray($value);
    };
    ($style:ident; stroke-dashoffset: $value:expr; $($rest:tt)*) => {
        $style = $style.stroke_dashoffset($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; stroke-dashoffset: $value:expr) => {
        $style = $style.stroke_dashoffset($value);
    };

    // =========================================================================
    // Image Properties (CSS: object-fit, object-position)
    // =========================================================================
    ($style:ident; object-fit: $value:expr; $($rest:tt)*) => {
        $style = $style.object_fit($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; object-fit: $value:expr) => {
        $style = $style.object_fit($value);
    };
    ($style:ident; object-position: ($x:expr, $y:expr); $($rest:tt)*) => {
        $style = $style.object_position($x, $y);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; object-position: ($x:expr, $y:expr)) => {
        $style = $style.object_position($x, $y);
    };

    // =========================================================================
    // Interaction Properties (CSS: pointer-events, cursor)
    // =========================================================================
    ($style:ident; pointer-events: none; $($rest:tt)*) => {
        $style = $style.pointer_events_none();
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; pointer-events: auto; $($rest:tt)*) => {
        $style = $style.pointer_events(blinc_core::PointerEvents::Auto);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; pointer-events: $value:expr; $($rest:tt)*) => {
        $style = $style.pointer_events($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; pointer-events: $value:expr) => {
        $style = $style.pointer_events($value);
    };
    ($style:ident; cursor: $value:expr; $($rest:tt)*) => {
        $style = $style.cursor($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; cursor: $value:expr) => {
        $style = $style.cursor($value);
    };

    // =========================================================================
    // Animation sub-properties
    // =========================================================================
    ($style:ident; animation-delay: $value:expr; $($rest:tt)*) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.delay_ms = $value;
            $style.animation = Some(anim);
        }
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-delay: $value:expr) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.delay_ms = $value;
            $style.animation = Some(anim);
        }
    };
    ($style:ident; animation-timing-function: $value:expr; $($rest:tt)*) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.timing = $value;
            $style.animation = Some(anim);
        }
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-timing-function: $value:expr) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.timing = $value;
            $style.animation = Some(anim);
        }
    };
    ($style:ident; animation-iteration-count: $value:expr; $($rest:tt)*) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.iteration_count = $value;
            $style.animation = Some(anim);
        }
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-iteration-count: $value:expr) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.iteration_count = $value;
            $style.animation = Some(anim);
        }
    };
    ($style:ident; animation-direction: $value:expr; $($rest:tt)*) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.direction = $value;
            $style.animation = Some(anim);
        }
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-direction: $value:expr) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.direction = $value;
            $style.animation = Some(anim);
        }
    };
    ($style:ident; animation-fill-mode: $value:expr; $($rest:tt)*) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.fill_mode = $value;
            $style.animation = Some(anim);
        }
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; animation-fill-mode: $value:expr) => {
        {
            let mut anim = $style.animation.clone().unwrap_or_default();
            anim.fill_mode = $value;
            $style.animation = Some(anim);
        }
    };

    // =========================================================================
    // Display: block
    // =========================================================================
    ($style:ident; display: block; $($rest:tt)*) => {
        $style.display = Some($crate::element_style::StyleDisplay::Block);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-wrap: nowrap; $($rest:tt)*) => {
        $style.flex_wrap = Some(false);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-shrink: $value:expr; $($rest:tt)*) => {
        $style = $style.flex_shrink($value);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; flex-shrink: $value:expr) => {
        $style = $style.flex_shrink($value);
    };
    ($style:ident; align-items: baseline; $($rest:tt)*) => {
        $style.align_items = Some($crate::element_style::StyleAlign::Baseline);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-self: stretch; $($rest:tt)*) => {
        $style.align_self = Some($crate::element_style::StyleAlign::Stretch);
        $crate::css_impl!($style; $($rest)*);
    };
    ($style:ident; align-self: baseline; $($rest:tt)*) => {
        $style.align_self = Some($crate::element_style::StyleAlign::Baseline);
        $crate::css_impl!($style; $($rest)*);
    };
}

/// Rust-friendly macro for creating ElementStyle with builder-like syntax
///
/// Uses Rust naming conventions (underscores instead of hyphens).
/// Comma-separated properties with colon syntax.
///
/// # Examples
///
/// ```ignore
/// use blinc_layout::prelude::*;
/// use blinc_core::Color;
///
/// // Basic usage with property: value syntax
/// let s = style! {
///     bg: Color::BLUE,
///     rounded: 8.0,
///     opacity: 0.9,
/// };
///
/// // Preset methods (no value needed)
/// let card = style! {
///     bg: Color::WHITE,
///     rounded_lg,
///     shadow_md,
/// };
///
/// // Transform shortcuts
/// let hover = style! {
///     scale: 1.05,
///     rotate_deg: 15.0,
///     translate: (10.0, 5.0),
/// };
///
/// // Material effects
/// let glass_panel = style! {
///     glass,
///     rounded: 16.0,
/// };
/// ```
#[macro_export]
macro_rules! style {
    // Empty style
    () => {
        $crate::element_style::ElementStyle::new()
    };

    // Main entry point - parse properties
    ($($tokens:tt)*) => {{
        let mut __style = $crate::element_style::ElementStyle::new();
        $crate::style_impl!(__style; $($tokens)*);
        __style
    }};
}

/// Internal macro for parsing style properties (Rust-style)
#[macro_export]
#[doc(hidden)]
macro_rules! style_impl {
    // Base case - no more tokens
    ($style:ident;) => {};

    // =========================================================================
    // Background properties
    // =========================================================================
    ($style:ident; bg: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.bg($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; background: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.background($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Corner radius properties
    // =========================================================================
    ($style:ident; rounded: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.rounded($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_corners: ($tl:expr, $tr:expr, $br:expr, $bl:expr) $(, $($rest:tt)*)?) => {
        $style = $style.rounded_corners($tl, $tr, $br, $bl);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    // Preset corner radii
    ($style:ident; rounded_sm $(, $($rest:tt)*)?) => {
        $style = $style.rounded_sm();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_md $(, $($rest:tt)*)?) => {
        $style = $style.rounded_md();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_lg $(, $($rest:tt)*)?) => {
        $style = $style.rounded_lg();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_xl $(, $($rest:tt)*)?) => {
        $style = $style.rounded_xl();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_2xl $(, $($rest:tt)*)?) => {
        $style = $style.rounded_2xl();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_none $(, $($rest:tt)*)?) => {
        $style = $style.rounded_none();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rounded_full $(, $($rest:tt)*)?) => {
        $style = $style.rounded_full();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Corner Shape properties
    // =========================================================================
    ($style:ident; corner_shape: round $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(1.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: bevel $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(0.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: squircle $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(2.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: scoop $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(-1.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: notch $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(-100.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: square $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape(100.0);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; corner_shape: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.corner_shape($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Overflow Fade properties
    // =========================================================================
    ($style:ident; overflow_fade: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.overflow_fade($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Shadow properties
    // =========================================================================
    ($style:ident; shadow: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.shadow($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; shadow_sm $(, $($rest:tt)*)?) => {
        $style = $style.shadow_sm();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; shadow_md $(, $($rest:tt)*)?) => {
        $style = $style.shadow_md();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; shadow_lg $(, $($rest:tt)*)?) => {
        $style = $style.shadow_lg();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; shadow_xl $(, $($rest:tt)*)?) => {
        $style = $style.shadow_xl();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; shadow_none $(, $($rest:tt)*)?) => {
        $style = $style.shadow_none();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Transform properties
    // =========================================================================
    ($style:ident; transform: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.transform($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; scale: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.scale($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; scale_xy: ($sx:expr, $sy:expr) $(, $($rest:tt)*)?) => {
        $style = $style.scale_xy($sx, $sy);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; translate: ($x:expr, $y:expr) $(, $($rest:tt)*)?) => {
        $style = $style.translate($x, $y);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rotate: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.rotate($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rotate_deg: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.rotate_deg($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // 3D Transform properties
    // =========================================================================
    ($style:ident; rotate_x: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.rotate_x_deg($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; rotate_y: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.rotate_y_deg($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; perspective: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.perspective_px($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; translate_z: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.translate_z_px($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // 3D SDF Shape properties
    // =========================================================================
    ($style:ident; shape_3d: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.shape_3d($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; depth: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.depth_px($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // 3D Lighting properties
    // =========================================================================
    ($style:ident; light_direction: ($x:expr, $y:expr, $z:expr) $(, $($rest:tt)*)?) => {
        $style = $style.light_direction($x, $y, $z);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; light_intensity: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.light_intensity($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; ambient: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.ambient_light($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; specular: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.specular_power($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // 3D Boolean Operation properties
    // =========================================================================
    ($style:ident; op_3d: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.op_3d_type($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; blend_3d: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.blend_3d_px($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Clip-Path
    // =========================================================================
    ($style:ident; clip_path: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.clip_path($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Opacity properties
    // =========================================================================
    ($style:ident; opacity: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.opacity($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Flow shader
    // =========================================================================
    ($style:ident; flow: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.flow($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; opaque $(, $($rest:tt)*)?) => {
        $style = $style.opaque();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; translucent $(, $($rest:tt)*)?) => {
        $style = $style.translucent();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; transparent $(, $($rest:tt)*)?) => {
        $style = $style.transparent();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Material properties
    // =========================================================================
    ($style:ident; material: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.material($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; glass $(, $($rest:tt)*)?) => {
        $style = $style.glass();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; metallic $(, $($rest:tt)*)?) => {
        $style = $style.metallic();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; chrome $(, $($rest:tt)*)?) => {
        $style = $style.chrome();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; gold $(, $($rest:tt)*)?) => {
        $style = $style.gold();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; wood $(, $($rest:tt)*)?) => {
        $style = $style.wood();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layer properties
    // =========================================================================
    ($style:ident; layer: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.layer($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; foreground $(, $($rest:tt)*)?) => {
        $style = $style.foreground();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; layer_background $(, $($rest:tt)*)?) => {
        $style = $style.layer_background();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Animation properties
    // =========================================================================
    ($style:ident; animation: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.animation($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; animation_name: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.animation_name($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; animation_duration: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.animation_duration($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Sizing
    // =========================================================================
    ($style:ident; w: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; h: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.h($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; min_w: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.min_w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; min_h: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.min_h($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; max_w: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.max_w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; max_h: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.max_h($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Flex Direction & Display
    // =========================================================================
    ($style:ident; flex_row $(, $($rest:tt)*)?) => {
        $style = $style.flex_row();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_col $(, $($rest:tt)*)?) => {
        $style = $style.flex_col();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_row_reverse $(, $($rest:tt)*)?) => {
        $style = $style.flex_row_reverse();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_col_reverse $(, $($rest:tt)*)?) => {
        $style = $style.flex_col_reverse();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_wrap $(, $($rest:tt)*)?) => {
        $style = $style.flex_wrap();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; display_none $(, $($rest:tt)*)?) => {
        $style = $style.display_none();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Flex Properties
    // =========================================================================
    ($style:ident; flex_grow $(, $($rest:tt)*)?) => {
        $style = $style.flex_grow();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_grow_value: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.flex_grow_value($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; flex_shrink_0 $(, $($rest:tt)*)?) => {
        $style = $style.flex_shrink_0();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Alignment
    // =========================================================================
    ($style:ident; items_center $(, $($rest:tt)*)?) => {
        $style = $style.items_center();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; items_start $(, $($rest:tt)*)?) => {
        $style = $style.items_start();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; items_end $(, $($rest:tt)*)?) => {
        $style = $style.items_end();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; items_stretch $(, $($rest:tt)*)?) => {
        $style = $style.items_stretch();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_center $(, $($rest:tt)*)?) => {
        $style = $style.justify_center();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_start $(, $($rest:tt)*)?) => {
        $style = $style.justify_start();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_end $(, $($rest:tt)*)?) => {
        $style = $style.justify_end();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_between $(, $($rest:tt)*)?) => {
        $style = $style.justify_between();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_around $(, $($rest:tt)*)?) => {
        $style = $style.justify_around();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; justify_evenly $(, $($rest:tt)*)?) => {
        $style = $style.justify_evenly();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; self_center $(, $($rest:tt)*)?) => {
        $style = $style.self_center();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; self_start $(, $($rest:tt)*)?) => {
        $style = $style.self_start();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; self_end $(, $($rest:tt)*)?) => {
        $style = $style.self_end();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Spacing
    // =========================================================================
    ($style:ident; p: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.p($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; p_xy: ($x:expr, $y:expr) $(, $($rest:tt)*)?) => {
        $style = $style.p_xy($x, $y);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; m: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.m($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; m_xy: ($x:expr, $y:expr) $(, $($rest:tt)*)?) => {
        $style = $style.m_xy($x, $y);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; gap: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.gap($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Overflow
    // =========================================================================
    ($style:ident; overflow_clip $(, $($rest:tt)*)?) => {
        $style = $style.overflow_clip();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; overflow_visible $(, $($rest:tt)*)?) => {
        $style = $style.overflow_visible();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; overflow_scroll $(, $($rest:tt)*)?) => {
        $style = $style.overflow_scroll();
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Layout: Border
    // =========================================================================
    ($style:ident; border: ($width:expr, $color:expr) $(, $($rest:tt)*)?) => {
        $style = $style.border($width, $color);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; border_width: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.border_w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; border_color: $value:expr $(, $($rest:tt)*)?) => {
        $style.border_color = Some($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Text Properties
    // =========================================================================
    ($style:ident; text_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; font_size: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.font_size($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; font_weight: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.font_weight($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_decoration: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_decoration($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_decoration_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_decoration_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_decoration_thickness: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_decoration_thickness($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; line_height: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.line_height($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_align: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_align($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; letter_spacing: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.letter_spacing($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_shadow: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_shadow($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; text_overflow: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.text_overflow($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; white_space: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.white_space($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Transform Extras
    // =========================================================================
    ($style:ident; skew_x: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.skew_x($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; skew_y: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.skew_y($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; transform_origin: ($x:expr, $y:expr) $(, $($rest:tt)*)?) => {
        $style = $style.transform_origin($x, $y);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Transition
    // =========================================================================
    ($style:ident; transition: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.transition($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Filter
    // =========================================================================
    ($style:ident; filter: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.filter($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Mask Properties
    // =========================================================================
    ($style:ident; mask_image: $value:expr $(, $($rest:tt)*)?) => {
        $style.mask_image = Some($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; mask_gradient: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.mask_gradient($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; mask_mode: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.mask_mode($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Mix Blend Mode
    // =========================================================================
    ($style:ident; mix_blend_mode: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.mix_blend_mode($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Outline
    // =========================================================================
    ($style:ident; outline: ($width:expr, $color:expr) $(, $($rest:tt)*)?) => {
        $style = $style.outline($width, $color);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; outline_width: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.outline_w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; outline_color: $value:expr $(, $($rest:tt)*)?) => {
        $style.outline_color = Some($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; outline_offset: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.outline_offset($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Overflow per-axis
    // =========================================================================
    ($style:ident; overflow_x: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.overflow_x($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; overflow_y: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.overflow_y($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Position & Inset
    // =========================================================================
    ($style:ident; position: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.position($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; top: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.top($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; right: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.right($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; bottom: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.bottom($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; left: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.left($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; inset: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.inset($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; z_index: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.z_index($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; visibility: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.visibility($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Form Element Colors
    // =========================================================================
    ($style:ident; caret_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.caret_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; selection_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.selection_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; placeholder_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.placeholder_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; accent_color: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.accent_color($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Scrollbar Properties
    // =========================================================================
    ($style:ident; scrollbar_color: ($thumb:expr, $track:expr) $(, $($rest:tt)*)?) => {
        $style = $style.scrollbar_color($thumb, $track);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; scrollbar_width: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.scrollbar_width($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // SVG Properties
    // =========================================================================
    ($style:ident; fill: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.fill($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; stroke: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.stroke($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; stroke_width: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.stroke_width($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; stroke_dasharray: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.stroke_dasharray($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; stroke_dashoffset: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.stroke_dashoffset($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; svg_path_data: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.svg_path_data($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Image Properties
    // =========================================================================
    ($style:ident; object_fit: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.object_fit($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; object_position: ($x:expr, $y:expr) $(, $($rest:tt)*)?) => {
        $style = $style.object_position($x, $y);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Interaction Properties
    // =========================================================================
    ($style:ident; pointer_events: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.pointer_events($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; pointer_events_none $(, $($rest:tt)*)?) => {
        $style = $style.pointer_events_none();
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; cursor: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.cursor($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Flex shrink with value
    // =========================================================================
    ($style:ident; flex_shrink: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.flex_shrink($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Display block
    // =========================================================================
    ($style:ident; display_block $(, $($rest:tt)*)?) => {
        $style.display = Some($crate::element_style::StyleDisplay::Block);
        $crate::style_impl!($style; $($($rest)*)?);
    };

    // =========================================================================
    // Width/Height aliases for CSS-style naming
    // =========================================================================
    ($style:ident; width: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.w($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
    ($style:ident; height: $value:expr $(, $($rest:tt)*)?) => {
        $style = $style.h($value);
        $crate::style_impl!($style; $($($rest)*)?);
    };
}

// =============================================================================
// flow! macro — define @flow shaders using Rust identifiers and primitives
// =============================================================================

/// Define a `@flow` shader using Rust-like syntax that compiles to a [`FlowGraph`](blinc_core::FlowGraph).
///
/// The macro accepts Rust identifiers and primitives — no raw strings needed.
/// Produces a `blinc_core::FlowGraph` that can be passed directly to `div().flow(graph)`.
///
/// # Syntax
///
/// ```rust,ignore
/// let graph = flow!(shader_name, fragment, {
///     input uv: builtin(uv);
///     input time: builtin(time);
///     step mist: pattern_noise { scale: 3.0; detail: 5; };
///     node wave = sin(uv.x * 10.0 + time);
///     output color = vec4(wave, wave, wave, 1.0);
/// });
/// ```
///
/// # Usage
///
/// ```rust,ignore
/// // Define and apply directly to an element
/// div().flow(flow!(ripple, fragment, {
///     input uv: builtin(uv);
///     input time: builtin(time);
///     node d = distance(uv, vec2(0.5, 0.5));
///     node wave = sin(d * 20.0 - time * 4.0);
///     output color = vec4(wave, wave, wave, 1.0);
/// }))
/// ```
#[macro_export]
macro_rules! flow {
    ($name:ident, $target:ident, { $($body:tt)* }) => {{
        $crate::parser::parse_flow_string(concat!(
            "@flow ", stringify!($name), " {\n  target: ", stringify!($target), ";\n  ",
            stringify!($($body)*),
            "\n}"
        )).expect(concat!("flow!: failed to parse flow '", stringify!($name), "'"))
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_builder() {
        // Initialize theme (required for shadow_md which uses theme)
        ThemeState::init_default();

        let s = style().bg(Color::BLUE).rounded(8.0).shadow_md().scale(1.05);

        assert!(s.background.is_some());
        assert!(s.corner_radius.is_some());
        assert!(!s.shadow.is_empty());
        assert!(s.transform.is_some());
    }

    #[test]
    fn test_style_merge() {
        // Initialize theme (required for shadow_sm which uses theme)
        ThemeState::init_default();

        let base = style().bg(Color::BLUE).rounded(8.0).shadow_sm();

        let hover = style().bg(Color::GREEN).scale(1.02);

        let merged = base.merge(&hover);

        // Background should be overridden
        assert!(matches!(merged.background, Some(Brush::Solid(c)) if c == Color::GREEN));
        // Corner radius should be preserved from base
        assert!(merged.corner_radius.is_some());
        // Shadow should be preserved from base
        assert!(!merged.shadow.is_empty());
        // Transform should come from hover
        assert!(merged.transform.is_some());
    }

    #[test]
    fn test_style_empty() {
        let empty = ElementStyle::new();
        assert!(empty.is_empty());

        let non_empty = style().bg(Color::RED);
        assert!(!non_empty.is_empty());
    }

    // =========================================================================
    // style! macro tests
    // =========================================================================

    #[test]
    fn test_style_macro_empty() {
        let s = style!();
        assert!(s.is_empty());
    }

    #[test]
    fn test_style_macro_basic() {
        ThemeState::init_default();

        let s = style! {
            bg: Color::BLUE,
            rounded: 8.0,
            opacity: 0.9,
        };

        assert!(matches!(s.background, Some(Brush::Solid(c)) if c == Color::BLUE));
        assert!(s.corner_radius.is_some());
        assert_eq!(s.opacity, Some(0.9));
    }

    #[test]
    fn test_style_macro_presets() {
        ThemeState::init_default();

        let s = style! {
            bg: Color::WHITE,
            rounded_lg,
            shadow_md,
        };

        assert!(s.background.is_some());
        assert!(s.corner_radius.is_some());
        assert!(!s.shadow.is_empty());
    }

    #[test]
    fn test_style_macro_transforms() {
        let s = style! {
            scale: 1.05,
        };
        assert!(s.transform.is_some());

        let s2 = style! {
            translate: (10.0, 20.0),
        };
        assert!(s2.transform.is_some());

        let s3 = style! {
            rotate_deg: 45.0,
        };
        assert!(s3.transform.is_some());

        let s4 = style! {
            scale_xy: (1.1, 0.9),
        };
        assert!(s4.transform.is_some());
    }

    #[test]
    fn test_style_macro_materials() {
        let s = style! {
            glass,
            rounded: 16.0,
        };

        assert!(s.material.is_some());
        assert!(s.corner_radius.is_some());
        // Glass sets render layer to Glass
        assert!(s.render_layer.is_some());
    }

    #[test]
    fn test_style_macro_opacity_presets() {
        let s1 = style! { opaque };
        assert_eq!(s1.opacity, Some(1.0));

        let s2 = style! { translucent };
        assert_eq!(s2.opacity, Some(0.5));

        let s3 = style! { transparent };
        assert_eq!(s3.opacity, Some(0.0));
    }

    #[test]
    fn test_style_macro_combined() {
        ThemeState::init_default();

        // Test combining multiple properties
        let card_style = style! {
            bg: Color::WHITE,
            rounded_lg,
            shadow_md,
            opacity: 0.95,
            scale: 1.0,
        };

        assert!(card_style.background.is_some());
        assert!(card_style.corner_radius.is_some());
        assert!(!card_style.shadow.is_empty());
        assert_eq!(card_style.opacity, Some(0.95));
        assert!(card_style.transform.is_some());
    }

    #[test]
    fn test_style_macro_rounded_variants() {
        ThemeState::init_default();

        let s1 = style! { rounded_sm };
        assert!(s1.corner_radius.is_some());

        let s2 = style! { rounded_md };
        assert!(s2.corner_radius.is_some());

        let s3 = style! { rounded_xl };
        assert!(s3.corner_radius.is_some());

        let s4 = style! { rounded_full };
        assert!(s4.corner_radius.is_some());

        let s5 = style! { rounded_none };
        assert!(s5.corner_radius.is_some());
    }

    #[test]
    fn test_style_macro_shadow_variants() {
        ThemeState::init_default();

        let s1 = style! { shadow_sm };
        assert!(!s1.shadow.is_empty());

        let s2 = style! { shadow_lg };
        assert!(!s2.shadow.is_empty());

        let s3 = style! { shadow_xl };
        assert!(!s3.shadow.is_empty());

        let s4 = style! { shadow_none };
        assert!(!s4.shadow.is_empty()); // shadow_none sets a transparent shadow
    }

    #[test]
    fn test_style_macro_material_variants() {
        let s1 = style! { metallic };
        assert!(s1.material.is_some());

        let s2 = style! { chrome };
        assert!(s2.material.is_some());

        let s3 = style! { gold };
        assert!(s3.material.is_some());

        let s4 = style! { wood };
        assert!(s4.material.is_some());
    }

    #[test]
    fn test_style_macro_layer() {
        let s1 = style! { foreground };
        assert!(s1.render_layer.is_some());

        let s2 = style! { layer_background };
        assert!(s2.render_layer.is_some());
    }

    #[test]
    fn test_style_macro_rounded_corners() {
        let s = style! {
            rounded_corners: (8.0, 8.0, 0.0, 0.0),
        };
        assert!(s.corner_radius.is_some());
        let cr = s.corner_radius.unwrap();
        assert_eq!(cr.top_left, 8.0);
        assert_eq!(cr.top_right, 8.0);
        assert_eq!(cr.bottom_right, 0.0);
        assert_eq!(cr.bottom_left, 0.0);
    }

    // =========================================================================
    // css! macro tests - CSS property name compatibility
    // =========================================================================

    #[test]
    fn test_css_macro_empty() {
        let s = css!();
        assert!(s.is_empty());
    }

    #[test]
    fn test_css_macro_basic() {
        // Uses CSS property names with semicolon separators
        let s = css! {
            background: Color::BLUE;
            border-radius: 8.0;
            opacity: 0.9;
        };

        assert!(matches!(s.background, Some(Brush::Solid(c)) if c == Color::BLUE));
        assert!(s.corner_radius.is_some());
        assert_eq!(s.opacity, Some(0.9));
    }

    #[test]
    fn test_css_macro_shadow() {
        ThemeState::init_default();

        let s = css! {
            box-shadow: md;
        };
        assert!(!s.shadow.is_empty());

        let s2 = css! {
            box-shadow: Shadow::new(0.0, 4.0, 8.0, Color::BLACK);
        };
        assert!(!s2.shadow.is_empty());
    }

    #[test]
    fn test_css_macro_transform() {
        let s = css! {
            transform: Transform::scale(1.05, 1.05);
        };
        assert!(s.transform.is_some());
    }

    #[test]
    fn test_css_macro_backdrop_filter() {
        // Blinc extension for materials
        let s = css! {
            backdrop-filter: glass;
        };
        assert!(s.material.is_some());
        assert!(s.render_layer.is_some()); // Glass sets render layer
    }

    #[test]
    fn test_css_macro_combined() {
        ThemeState::init_default();

        // Full CSS-like card style
        let card = css! {
            background: Color::WHITE;
            border-radius: 12.0;
            box-shadow: lg;
            opacity: 0.95;
        };

        assert!(card.background.is_some());
        assert!(card.corner_radius.is_some());
        assert!(!card.shadow.is_empty());
        assert_eq!(card.opacity, Some(0.95));
    }

    #[test]
    fn test_css_macro_animation() {
        let s = css! {
            animation-name: "fade-in";
            animation-duration: 300;
        };

        assert!(s.animation.is_some());
        let anim = s.animation.unwrap();
        assert_eq!(anim.name, "fade-in");
        assert_eq!(anim.duration_ms, 300);
    }

    #[test]
    fn test_css_and_style_macros_produce_same_result() {
        // Both macros should produce equivalent ElementStyle for same properties
        let from_css = css! {
            background: Color::RED;
            border-radius: 10.0;
            opacity: 0.8;
        };

        let from_style = style! {
            bg: Color::RED,
            rounded: 10.0,
            opacity: 0.8,
        };

        // Same background
        assert!(matches!(from_css.background, Some(Brush::Solid(c)) if c == Color::RED));
        assert!(matches!(from_style.background, Some(Brush::Solid(c)) if c == Color::RED));

        // Same corner radius
        assert_eq!(from_css.corner_radius, from_style.corner_radius);

        // Same opacity
        assert_eq!(from_css.opacity, from_style.opacity);
    }

    #[test]
    fn test_css_macro_text_properties() {
        let s = css! {
            color: Color::RED;
            font-size: 16.0;
            line-height: 1.5;
            letter-spacing: 0.5;
        };
        assert_eq!(s.text_color, Some(Color::RED));
        assert_eq!(s.font_size, Some(16.0));
        assert_eq!(s.line_height, Some(1.5));
        assert_eq!(s.letter_spacing, Some(0.5));
    }

    #[test]
    fn test_style_macro_text_properties() {
        let s = style! {
            text_color: Color::BLUE,
            font_size: 14.0,
            line_height: 1.2,
            letter_spacing: 1.0,
        };
        assert_eq!(s.text_color, Some(Color::BLUE));
        assert_eq!(s.font_size, Some(14.0));
        assert_eq!(s.line_height, Some(1.2));
        assert_eq!(s.letter_spacing, Some(1.0));
    }

    #[test]
    fn test_css_macro_text_decoration() {
        let s = css! {
            text-decoration: TextDecoration::Underline;
            text-decoration-color: Color::RED;
            text-decoration-thickness: 2.0;
        };
        assert_eq!(s.text_decoration, Some(TextDecoration::Underline));
        assert_eq!(s.text_decoration_color, Some(Color::RED));
        assert_eq!(s.text_decoration_thickness, Some(2.0));
    }

    #[test]
    fn test_css_macro_text_overflow() {
        let s = css! {
            text-overflow: ellipsis;
            white-space: nowrap;
        };
        assert_eq!(s.text_overflow, Some(TextOverflow::Ellipsis));
        assert_eq!(s.white_space, Some(WhiteSpace::Nowrap));
    }

    #[test]
    fn test_css_macro_position_inset() {
        let s = css! {
            position: absolute;
            top: 10.0;
            right: 20.0;
            bottom: 30.0;
            left: 40.0;
            z-index: 5;
        };
        assert_eq!(s.position, Some(StylePosition::Absolute));
        assert_eq!(s.top, Some(10.0));
        assert_eq!(s.right, Some(20.0));
        assert_eq!(s.bottom, Some(30.0));
        assert_eq!(s.left, Some(40.0));
        assert_eq!(s.z_index, Some(5));
    }

    #[test]
    fn test_style_macro_position_inset() {
        let s = style! {
            position: StylePosition::Relative,
            top: 5.0,
            inset: 0.0,
            z_index: 10,
        };
        assert_eq!(s.position, Some(StylePosition::Relative));
        // inset overrides top
        assert_eq!(s.top, Some(0.0));
        assert_eq!(s.right, Some(0.0));
        assert_eq!(s.bottom, Some(0.0));
        assert_eq!(s.left, Some(0.0));
        assert_eq!(s.z_index, Some(10));
    }

    #[test]
    fn test_css_macro_visibility() {
        let s = css! {
            visibility: hidden;
        };
        assert_eq!(s.visibility, Some(StyleVisibility::Hidden));
    }

    #[test]
    fn test_css_macro_overflow_axes() {
        let s = css! {
            overflow-x: scroll;
            overflow-y: hidden;
        };
        assert_eq!(s.overflow_x, Some(StyleOverflow::Scroll));
        assert_eq!(s.overflow_y, Some(StyleOverflow::Clip));
    }

    #[test]
    fn test_css_macro_outline() {
        let s = css! {
            outline: (2.0, Color::RED);
            outline-offset: 4.0;
        };
        assert_eq!(s.outline_width, Some(2.0));
        assert_eq!(s.outline_color, Some(Color::RED));
        assert_eq!(s.outline_offset, Some(4.0));
    }

    #[test]
    fn test_style_macro_outline() {
        let s = style! {
            outline: (3.0, Color::BLUE),
            outline_offset: 2.0,
        };
        assert_eq!(s.outline_width, Some(3.0));
        assert_eq!(s.outline_color, Some(Color::BLUE));
        assert_eq!(s.outline_offset, Some(2.0));
    }

    #[test]
    fn test_css_macro_form_colors() {
        let s = css! {
            caret-color: Color::RED;
            selection-color: Color::BLUE;
            placeholder-color: Color::rgba(0.5, 0.5, 0.5, 1.0);
            accent-color: Color::GREEN;
        };
        assert_eq!(s.caret_color, Some(Color::RED));
        assert_eq!(s.selection_color, Some(Color::BLUE));
        assert!(s.placeholder_color.is_some());
        assert_eq!(s.accent_color, Some(Color::GREEN));
    }

    #[test]
    fn test_style_macro_form_colors() {
        let s = style! {
            caret_color: Color::RED,
            accent_color: Color::GREEN,
        };
        assert_eq!(s.caret_color, Some(Color::RED));
        assert_eq!(s.accent_color, Some(Color::GREEN));
    }

    #[test]
    fn test_css_macro_svg_properties() {
        let s = css! {
            fill: Color::RED;
            stroke: Color::BLUE;
            stroke-width: 2.0;
            stroke-dashoffset: 10.0;
        };
        assert_eq!(s.fill, Some(Color::RED));
        assert_eq!(s.stroke, Some(Color::BLUE));
        assert_eq!(s.stroke_width, Some(2.0));
        assert_eq!(s.stroke_dashoffset, Some(10.0));
    }

    #[test]
    fn test_style_macro_svg_properties() {
        let s = style! {
            fill: Color::RED,
            stroke: Color::BLUE,
            stroke_width: 3.0,
            stroke_dasharray: vec![5.0, 3.0],
            stroke_dashoffset: 0.0,
        };
        assert_eq!(s.fill, Some(Color::RED));
        assert_eq!(s.stroke, Some(Color::BLUE));
        assert_eq!(s.stroke_width, Some(3.0));
        assert_eq!(s.stroke_dasharray, Some(vec![5.0, 3.0]));
        assert_eq!(s.stroke_dashoffset, Some(0.0));
    }

    #[test]
    fn test_css_macro_transform_extras() {
        let s = css! {
            transform-origin: (0.0, 100.0);
        };
        assert_eq!(s.transform_origin, Some([0.0, 100.0]));
    }

    #[test]
    fn test_style_macro_transform_extras() {
        let s = style! {
            skew_x: 15.0,
            skew_y: 10.0,
            transform_origin: (50.0, 50.0),
        };
        assert_eq!(s.skew_x, Some(15.0));
        assert_eq!(s.skew_y, Some(10.0));
        assert_eq!(s.transform_origin, Some([50.0, 50.0]));
    }

    #[test]
    fn test_css_macro_scrollbar() {
        let s = css! {
            scrollbar-color: (Color::RED, Color::WHITE);
            scrollbar-width: thin;
        };
        assert_eq!(s.scrollbar_color, Some((Color::RED, Color::WHITE)));
        assert_eq!(s.scrollbar_width, Some(ScrollbarWidth::Thin));
    }

    #[test]
    fn test_css_macro_image_properties() {
        let s = css! {
            object-fit: 1;
            object-position: (0.5, 0.0);
        };
        assert_eq!(s.object_fit, Some(1));
        assert_eq!(s.object_position, Some([0.5, 0.0]));
    }

    #[test]
    fn test_style_macro_image_properties() {
        let s = style! {
            object_fit: 0,
            object_position: (0.0, 1.0),
        };
        assert_eq!(s.object_fit, Some(0));
        assert_eq!(s.object_position, Some([0.0, 1.0]));
    }

    #[test]
    fn test_css_macro_filter() {
        let f = CssFilter {
            grayscale: 1.0,
            ..Default::default()
        };
        let s = css! {
            filter: f;
        };
        assert!(s.filter.is_some());
        assert_eq!(s.filter.unwrap().grayscale, 1.0);
    }

    #[test]
    fn test_style_macro_filter() {
        let f = CssFilter {
            brightness: 1.5,
            ..Default::default()
        };
        let s = style! {
            filter: f,
        };
        assert!(s.filter.is_some());
        assert_eq!(s.filter.unwrap().brightness, 1.5);
    }

    #[test]
    fn test_css_macro_border_shorthand() {
        let s = css! {
            border: (2.0, Color::RED);
        };
        assert_eq!(s.border_width, Some(2.0));
        assert_eq!(s.border_color, Some(Color::RED));
    }

    #[test]
    fn test_css_macro_display_block() {
        let s = css! {
            display: block;
        };
        assert_eq!(s.display, Some(StyleDisplay::Block));
    }

    #[test]
    fn test_css_macro_pointer_events() {
        let s = css! {
            pointer-events: none;
        };
        assert_eq!(s.pointer_events, Some(PointerEvents::None));
    }

    #[test]
    fn test_style_macro_pointer_events() {
        let s = style! {
            pointer_events_none,
        };
        assert_eq!(s.pointer_events, Some(PointerEvents::None));
    }

    #[test]
    fn test_css_macro_inset() {
        let s = css! {
            inset: 10.0;
        };
        assert_eq!(s.top, Some(10.0));
        assert_eq!(s.right, Some(10.0));
        assert_eq!(s.bottom, Some(10.0));
        assert_eq!(s.left, Some(10.0));
    }

    #[test]
    fn test_css_macro_flex_extras() {
        let s = css! {
            flex-wrap: nowrap;
            align-items: baseline;
            align-self: stretch;
        };
        assert_eq!(s.flex_wrap, Some(false));
        assert_eq!(s.align_items, Some(StyleAlign::Baseline));
        assert_eq!(s.align_self, Some(StyleAlign::Stretch));
    }

    #[test]
    fn test_css_style_parity_text() {
        let from_css = css! {
            color: Color::RED;
            font-size: 16.0;
            letter-spacing: 2.0;
        };
        let from_style = style! {
            text_color: Color::RED,
            font_size: 16.0,
            letter_spacing: 2.0,
        };
        assert_eq!(from_css.text_color, from_style.text_color);
        assert_eq!(from_css.font_size, from_style.font_size);
        assert_eq!(from_css.letter_spacing, from_style.letter_spacing);
    }

    #[test]
    fn test_css_style_parity_position() {
        let from_css = css! {
            position: absolute;
            top: 10.0;
            z-index: 5;
        };
        let from_style = style! {
            position: StylePosition::Absolute,
            top: 10.0,
            z_index: 5,
        };
        assert_eq!(from_css.position, from_style.position);
        assert_eq!(from_css.top, from_style.top);
        assert_eq!(from_css.z_index, from_style.z_index);
    }
}
