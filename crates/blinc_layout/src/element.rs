//! Element types and traits for layout-driven UI
//!
//! Provides the core abstractions for building layout trees that can be
//! rendered via the DrawContext API.

use std::collections::HashMap;
use std::sync::Arc;

use blinc_core::{
    BlurQuality, Brush, ClipPath, Color, CornerRadius, CornerShape, DynFloat, DynValue, FlowGraph,
    LayerEffect, OverflowFade, Rect, Shadow, Transform, ValueContext,
};
use taffy::Layout;

use crate::tree::LayoutNodeId;

// =============================================================================
// FlowRef — polymorphic flow reference (name string or direct FlowGraph)
// =============================================================================

/// A reference to a `@flow` shader — either by name (for CSS-defined flows) or
/// by direct `FlowGraph` (for `flow!` macro-defined flows).
#[derive(Clone, Debug)]
pub enum FlowRef {
    /// Name of a @flow DAG defined in a CSS stylesheet
    Name(String),
    /// Direct FlowGraph (e.g. from `flow!` macro), auto-persisted by name
    Graph(Arc<FlowGraph>),
}

impl From<&str> for FlowRef {
    fn from(s: &str) -> Self {
        FlowRef::Name(s.to_string())
    }
}

impl From<String> for FlowRef {
    fn from(s: String) -> Self {
        FlowRef::Name(s)
    }
}

impl From<FlowGraph> for FlowRef {
    fn from(g: FlowGraph) -> Self {
        FlowRef::Graph(Arc::new(g))
    }
}

/// Per-tag SVG style overrides for CSS tag-name selectors (e.g., `path { fill: red; }`)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SvgTagStyle {
    pub fill: Option<[f32; 4]>,
    pub stroke: Option<[f32; 4]>,
    pub stroke_width: Option<f32>,
    pub stroke_dasharray: Option<Vec<f32>>,
    pub stroke_dashoffset: Option<f32>,
    pub opacity: Option<f32>,
}

// ============================================================================
// Cursor Style
// ============================================================================

/// Mouse cursor style for an element
///
/// When the cursor hovers over an element with a cursor style set,
/// the window cursor will change to this style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CursorStyle {
    /// Default arrow cursor
    #[default]
    Default,
    /// Pointer/hand cursor (for clickable elements like links, buttons)
    Pointer,
    /// Text/I-beam cursor (for text input)
    Text,
    /// Crosshair cursor
    Crosshair,
    /// Move cursor (for dragging)
    Move,
    /// Not allowed cursor
    NotAllowed,
    /// North-South resize cursor
    ResizeNS,
    /// East-West resize cursor
    ResizeEW,
    /// Northeast-Southwest resize cursor
    ResizeNESW,
    /// Northwest-Southeast resize cursor
    ResizeNWSE,
    /// Grab cursor (open hand)
    Grab,
    /// Grabbing cursor (closed hand)
    Grabbing,
    /// Wait/loading cursor
    Wait,
    /// Progress cursor (arrow with spinner)
    Progress,
    /// Hidden cursor
    None,
}

// ============================================================================
// Material System
// ============================================================================

/// Material types that can be applied to elements
///
/// Materials define how an element appears and interacts with its background.
/// This is similar to a physical material system where each material has
/// unique visual properties.
#[derive(Clone, Debug)]
pub enum Material {
    /// Transparent glass/vibrancy effect that blurs content behind it
    Glass(GlassMaterial),
    /// Metallic/reflective surface
    Metallic(MetallicMaterial),
    /// Wood grain texture (placeholder for future implementation)
    Wood(WoodMaterial),
    /// Solid opaque material (default)
    Solid(SolidMaterial),
}

impl Default for Material {
    fn default() -> Self {
        Material::Solid(SolidMaterial::default())
    }
}

// ============================================================================
// Into<Material> implementations for ergonomic effect() API
// ============================================================================

impl From<GlassMaterial> for Material {
    fn from(glass: GlassMaterial) -> Self {
        Material::Glass(glass)
    }
}

impl From<MetallicMaterial> for Material {
    fn from(metal: MetallicMaterial) -> Self {
        Material::Metallic(metal)
    }
}

impl From<WoodMaterial> for Material {
    fn from(wood: WoodMaterial) -> Self {
        Material::Wood(wood)
    }
}

impl From<SolidMaterial> for Material {
    fn from(solid: SolidMaterial) -> Self {
        Material::Solid(solid)
    }
}

// ============================================================================
// Glass Material
// ============================================================================

/// Glass/vibrancy material that blurs content behind it
///
/// Creates a frosted glass effect similar to macOS vibrancy or iOS blur.
#[derive(Clone, Debug)]
pub struct GlassMaterial {
    /// Blur intensity (0-50, default 20)
    pub blur: f32,
    /// Tint color applied over the blur
    pub tint: Color,
    /// Color saturation (1.0 = normal, 0.0 = grayscale, >1.0 = vibrant)
    pub saturation: f32,
    /// Brightness multiplier (1.0 = normal)
    pub brightness: f32,
    /// Noise/grain amount for frosted texture (0.0-0.1)
    pub noise: f32,
    /// Border highlight thickness
    pub border_thickness: f32,
    /// Optional drop shadow
    pub shadow: Option<MaterialShadow>,
    /// Use simple frosted glass mode (no liquid glass effects)
    pub simple: bool,
}

impl Default for GlassMaterial {
    fn default() -> Self {
        Self {
            blur: 20.0,
            tint: Color::rgba(1.0, 1.0, 1.0, 0.1),
            saturation: 1.0,
            brightness: 1.0,
            noise: 0.0,
            border_thickness: 0.8,
            shadow: None,
            simple: false,
        }
    }
}

impl GlassMaterial {
    /// Create a new glass material with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set blur intensity
    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }

    /// Set tint color
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = color;
        self
    }

    /// Set tint from RGBA components
    pub fn tint_rgba(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.tint = Color::rgba(r, g, b, a);
        self
    }

    /// Set saturation
    pub fn saturation(mut self, saturation: f32) -> Self {
        self.saturation = saturation;
        self
    }

    /// Set brightness
    pub fn brightness(mut self, brightness: f32) -> Self {
        self.brightness = brightness;
        self
    }

    /// Add noise/grain for frosted effect
    pub fn noise(mut self, amount: f32) -> Self {
        self.noise = amount;
        self
    }

    /// Set border highlight thickness
    pub fn border(mut self, thickness: f32) -> Self {
        self.border_thickness = thickness;
        self
    }

    /// Add drop shadow
    pub fn shadow(mut self, shadow: MaterialShadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    // Presets

    /// Ultra-thin glass (very subtle blur)
    pub fn ultra_thin() -> Self {
        Self::new().blur(10.0)
    }

    /// Thin glass
    pub fn thin() -> Self {
        Self::new().blur(15.0)
    }

    /// Regular glass (default blur)
    pub fn regular() -> Self {
        Self::new()
    }

    /// Thick glass (heavy blur)
    pub fn thick() -> Self {
        Self::new().blur(30.0)
    }

    /// Frosted glass with grain texture
    pub fn frosted() -> Self {
        Self::new().noise(0.03)
    }

    /// Card style with border and shadow
    pub fn card() -> Self {
        Self::new().border(1.0).shadow(MaterialShadow::md())
    }

    /// Simple frosted glass - pure blur without liquid glass effects
    ///
    /// Creates a clean backdrop blur effect without edge refraction,
    /// light reflections, or bevel effects. More performant and ideal
    /// for subtle UI backgrounds where you want pure frosted glass.
    pub fn simple() -> Self {
        Self {
            blur: 15.0,
            tint: Color::rgba(1.0, 1.0, 1.0, 0.15),
            saturation: 1.1,
            brightness: 1.0,
            noise: 0.0,
            border_thickness: 0.0,
            shadow: None,
            simple: true,
        }
    }

    /// Enable/disable simple mode (no liquid glass effects)
    pub fn with_simple(mut self, simple: bool) -> Self {
        self.simple = simple;
        self
    }
}

// ============================================================================
// Metallic Material
// ============================================================================

/// Metallic/reflective material
///
/// Creates a metallic appearance with highlights and reflections.
#[derive(Clone, Debug)]
pub struct MetallicMaterial {
    /// Base metal color
    pub color: Color,
    /// Roughness (0.0 = mirror, 1.0 = matte)
    pub roughness: f32,
    /// Metallic intensity (0.0 = dielectric, 1.0 = full metal)
    pub metallic: f32,
    /// Reflection intensity
    pub reflection: f32,
    /// Optional drop shadow
    pub shadow: Option<MaterialShadow>,
}

impl Default for MetallicMaterial {
    fn default() -> Self {
        Self {
            color: Color::rgba(0.8, 0.8, 0.85, 1.0),
            roughness: 0.3,
            metallic: 1.0,
            reflection: 0.5,
            shadow: None,
        }
    }
}

impl MetallicMaterial {
    /// Create a new metallic material
    pub fn new() -> Self {
        Self::default()
    }

    /// Set base color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set roughness (0 = mirror, 1 = matte)
    pub fn roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness;
        self
    }

    /// Set metallic intensity
    pub fn metallic(mut self, metallic: f32) -> Self {
        self.metallic = metallic;
        self
    }

    /// Set reflection intensity
    pub fn reflection(mut self, reflection: f32) -> Self {
        self.reflection = reflection;
        self
    }

    /// Add drop shadow
    pub fn shadow(mut self, shadow: MaterialShadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    // Presets

    /// Chrome/mirror finish
    pub fn chrome() -> Self {
        Self::new().roughness(0.1).reflection(0.8)
    }

    /// Brushed metal
    pub fn brushed() -> Self {
        Self::new().roughness(0.5).reflection(0.3)
    }

    /// Gold finish
    pub fn gold() -> Self {
        Self::new()
            .color(Color::rgba(1.0, 0.84, 0.0, 1.0))
            .roughness(0.2)
    }

    /// Silver finish
    pub fn silver() -> Self {
        Self::new()
            .color(Color::rgba(0.75, 0.75, 0.75, 1.0))
            .roughness(0.2)
    }

    /// Copper finish
    pub fn copper() -> Self {
        Self::new()
            .color(Color::rgba(0.72, 0.45, 0.2, 1.0))
            .roughness(0.3)
    }
}

// ============================================================================
// Wood Material (placeholder)
// ============================================================================

/// Wood grain material (placeholder for future texture support)
#[derive(Clone, Debug)]
pub struct WoodMaterial {
    /// Base wood color
    pub color: Color,
    /// Grain intensity
    pub grain: f32,
    /// Glossiness (0 = matte, 1 = polished)
    pub gloss: f32,
    /// Optional drop shadow
    pub shadow: Option<MaterialShadow>,
}

impl Default for WoodMaterial {
    fn default() -> Self {
        Self {
            color: Color::rgba(0.55, 0.35, 0.2, 1.0),
            grain: 0.5,
            gloss: 0.2,
            shadow: None,
        }
    }
}

impl WoodMaterial {
    /// Create a new wood material
    pub fn new() -> Self {
        Self::default()
    }

    /// Set base color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set grain intensity
    pub fn grain(mut self, grain: f32) -> Self {
        self.grain = grain;
        self
    }

    /// Set glossiness
    pub fn gloss(mut self, gloss: f32) -> Self {
        self.gloss = gloss;
        self
    }

    /// Add drop shadow
    pub fn shadow(mut self, shadow: MaterialShadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    // Presets

    /// Oak wood
    pub fn oak() -> Self {
        Self::new().color(Color::rgba(0.6, 0.45, 0.25, 1.0))
    }

    /// Walnut wood
    pub fn walnut() -> Self {
        Self::new().color(Color::rgba(0.4, 0.25, 0.15, 1.0))
    }

    /// Cherry wood
    pub fn cherry() -> Self {
        Self::new().color(Color::rgba(0.6, 0.3, 0.2, 1.0))
    }

    /// Pine wood (lighter)
    pub fn pine() -> Self {
        Self::new().color(Color::rgba(0.8, 0.65, 0.45, 1.0))
    }
}

// ============================================================================
// Solid Material
// ============================================================================

/// Solid opaque material (the default)
#[derive(Clone, Debug, Default)]
pub struct SolidMaterial {
    /// Optional drop shadow
    pub shadow: Option<MaterialShadow>,
}

impl SolidMaterial {
    /// Create a new solid material
    pub fn new() -> Self {
        Self::default()
    }

    /// Add drop shadow
    pub fn shadow(mut self, shadow: MaterialShadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
}

// ============================================================================
// Material Shadow
// ============================================================================

/// Shadow configuration for materials
#[derive(Clone, Debug)]
pub struct MaterialShadow {
    /// Shadow color
    pub color: Color,
    /// Blur radius
    pub blur: f32,
    /// Offset (x, y)
    pub offset: (f32, f32),
    /// Opacity
    pub opacity: f32,
}

impl Default for MaterialShadow {
    fn default() -> Self {
        Self {
            color: Color::rgba(0.0, 0.0, 0.0, 1.0),
            blur: 10.0,
            offset: (0.0, 4.0),
            opacity: 0.25,
        }
    }
}

/// Convert from blinc_core::Shadow to MaterialShadow
impl From<Shadow> for MaterialShadow {
    fn from(shadow: Shadow) -> Self {
        Self {
            color: shadow.color,
            blur: shadow.blur,
            offset: (shadow.offset_x, shadow.offset_y),
            // Use the shadow color's alpha as opacity
            opacity: shadow.color.a,
        }
    }
}

/// Convert from &Shadow to MaterialShadow
impl From<&Shadow> for MaterialShadow {
    fn from(shadow: &Shadow) -> Self {
        Self {
            color: shadow.color,
            blur: shadow.blur,
            offset: (shadow.offset_x, shadow.offset_y),
            opacity: shadow.color.a,
        }
    }
}

impl MaterialShadow {
    /// Create a new shadow
    pub fn new() -> Self {
        Self::default()
    }

    /// Set shadow color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set blur radius
    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }

    /// Set offset
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    /// Set opacity
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    // Presets

    /// Small shadow
    pub fn sm() -> Self {
        Self::new().blur(4.0).offset(0.0, 2.0).opacity(0.2)
    }

    /// Medium shadow
    pub fn md() -> Self {
        Self::new().blur(10.0).offset(0.0, 4.0).opacity(0.25)
    }

    /// Large shadow
    pub fn lg() -> Self {
        Self::new().blur(20.0).offset(0.0, 8.0).opacity(0.3)
    }

    /// Extra large shadow
    pub fn xl() -> Self {
        Self::new().blur(30.0).offset(0.0, 12.0).opacity(0.35)
    }
}

/// Computed layout bounds for an element after layout computation
#[derive(Clone, Copy, Debug, Default)]
pub struct ElementBounds {
    /// X position relative to parent
    pub x: f32,
    /// Y position relative to parent
    pub y: f32,
    /// Computed width
    pub width: f32,
    /// Computed height
    pub height: f32,
}

impl ElementBounds {
    /// Create bounds from a Taffy Layout with parent offset
    pub fn from_layout(layout: &Layout, parent_offset: (f32, f32)) -> Self {
        Self {
            x: parent_offset.0 + layout.location.x,
            y: parent_offset.1 + layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        }
    }

    /// Create bounds at origin with given size
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Convert to a blinc_core Rect
    pub fn to_rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }

    /// Get bounds relative to self (origin at 0,0)
    pub fn local(&self) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: self.width,
            height: self.height,
        }
    }
}

/// Render layer for separating elements in glass-effect rendering
///
/// When using glass/vibrancy effects, elements need to be rendered in
/// different passes:
/// - Background elements are rendered first and get blurred behind glass
/// - Glass elements render the glass effect itself
/// - Foreground elements render on top without being blurred
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RenderLayer {
    /// Rendered behind glass (will be blurred)
    #[default]
    Background,
    /// Rendered as a glass element (blur effect applied)
    Glass,
    /// Rendered on top of glass (not blurred)
    Foreground,
}

/// Motion animation configuration for enter/exit animations
#[derive(Clone, Debug, Default)]
pub struct MotionAnimation {
    /// Enter animation properties (opacity, scale, translate at t=0)
    pub enter_from: Option<MotionKeyframe>,
    /// Enter animation duration in ms
    pub enter_duration_ms: u32,
    /// Enter animation delay in ms (for stagger)
    pub enter_delay_ms: u32,
    /// Exit animation properties (opacity, scale, translate at t=1)
    pub exit_to: Option<MotionKeyframe>,
    /// Exit animation duration in ms
    pub exit_duration_ms: u32,
}

/// A single keyframe of motion animation values
#[derive(Clone, Debug, Default)]
pub struct MotionKeyframe {
    /// Opacity (0.0 - 1.0)
    pub opacity: Option<f32>,
    /// Scale X
    pub scale_x: Option<f32>,
    /// Scale Y
    pub scale_y: Option<f32>,
    /// Translate X in pixels
    pub translate_x: Option<f32>,
    /// Translate Y in pixels
    pub translate_y: Option<f32>,
    /// Rotation in degrees
    pub rotate: Option<f32>,
}

impl MotionKeyframe {
    /// Create a new empty keyframe
    pub fn new() -> Self {
        Self::default()
    }

    /// Set opacity
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Set uniform scale
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale_x = Some(scale);
        self.scale_y = Some(scale);
        self
    }

    /// Set translation
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.translate_x = Some(x);
        self.translate_y = Some(y);
        self
    }

    /// Set rotation in degrees
    pub fn rotate(mut self, degrees: f32) -> Self {
        self.rotate = Some(degrees);
        self
    }

    /// Create from blinc_animation KeyframeProperties
    pub fn from_keyframe_properties(props: &blinc_animation::KeyframeProperties) -> Self {
        Self {
            opacity: props.opacity,
            scale_x: props.scale_x,
            scale_y: props.scale_y,
            translate_x: props.translate_x,
            translate_y: props.translate_y,
            rotate: props.rotate,
        }
    }

    /// Interpolate between two keyframes
    ///
    /// When a property is Some in one keyframe but None in the other,
    /// we use the "identity" value (1.0 for opacity/scale, 0.0 for translate/rotate).
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            // Opacity: identity is 1.0
            opacity: lerp_opt_with_default(self.opacity, other.opacity, t, 1.0),
            // Scale: identity is 1.0
            scale_x: lerp_opt_with_default(self.scale_x, other.scale_x, t, 1.0),
            scale_y: lerp_opt_with_default(self.scale_y, other.scale_y, t, 1.0),
            // Translation: identity is 0.0
            translate_x: lerp_opt_with_default(self.translate_x, other.translate_x, t, 0.0),
            translate_y: lerp_opt_with_default(self.translate_y, other.translate_y, t, 0.0),
            // Rotation: identity is 0.0
            rotate: lerp_opt_with_default(self.rotate, other.rotate, t, 0.0),
        }
    }

    /// Get the resolved opacity (defaults to 1.0 if not set)
    pub fn resolved_opacity(&self) -> f32 {
        self.opacity.unwrap_or(1.0)
    }

    /// Get the resolved scale (defaults to 1.0 if not set)
    pub fn resolved_scale(&self) -> (f32, f32) {
        (self.scale_x.unwrap_or(1.0), self.scale_y.unwrap_or(1.0))
    }

    /// Get the resolved translation (defaults to 0.0 if not set)
    pub fn resolved_translate(&self) -> (f32, f32) {
        (
            self.translate_x.unwrap_or(0.0),
            self.translate_y.unwrap_or(0.0),
        )
    }

    /// Get the resolved rotation (defaults to 0.0 if not set)
    pub fn resolved_rotate(&self) -> f32 {
        self.rotate.unwrap_or(0.0)
    }
}

/// Helper to interpolate optional values with a default for missing values
fn lerp_opt_with_default(a: Option<f32>, b: Option<f32>, t: f32, default: f32) -> Option<f32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a + (b - a) * t),
        (Some(a), None) => Some(a + (default - a) * t), // Lerp toward default
        (None, Some(b)) => Some(default + (b - default) * t), // Lerp from default
        (None, None) => None,
    }
}

impl MotionAnimation {
    /// Create a motion animation from a MultiKeyframeAnimation (enter animation)
    pub fn from_enter_animation(
        anim: &blinc_animation::MultiKeyframeAnimation,
        delay_ms: u32,
    ) -> Self {
        // Get the first keyframe's properties as the "from" state
        let enter_from = anim
            .first_keyframe()
            .map(|kf| MotionKeyframe::from_keyframe_properties(&kf.properties));

        Self {
            enter_from,
            enter_duration_ms: anim.duration_ms(),
            enter_delay_ms: delay_ms,
            exit_to: None,
            exit_duration_ms: 0,
        }
    }

    /// Add exit animation
    pub fn with_exit_animation(mut self, anim: &blinc_animation::MultiKeyframeAnimation) -> Self {
        // Get the last keyframe's properties as the "to" state
        self.exit_to = anim
            .last_keyframe()
            .map(|kf| MotionKeyframe::from_keyframe_properties(&kf.properties));
        self.exit_duration_ms = anim.duration_ms();
        self
    }
}

/// Individual border side configuration
#[derive(Clone, Copy, Debug, Default)]
pub struct BorderSide {
    /// Width in pixels (0 = no border)
    pub width: f32,
    /// Border color
    pub color: Color,
}

impl BorderSide {
    /// Create a new border side
    pub fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }

    /// Check if this border side is visible
    pub fn is_visible(&self) -> bool {
        self.width > 0.0 && self.color.a > 0.0
    }
}

/// Per-side border configuration for CSS-like border control
///
/// Allows setting borders independently for each side (top, right, bottom, left).
/// This is useful for blockquotes (left border only), dividers, etc.
#[derive(Clone, Copy, Debug, Default)]
pub struct BorderSides {
    /// Top border
    pub top: Option<BorderSide>,
    /// Right border
    pub right: Option<BorderSide>,
    /// Bottom border
    pub bottom: Option<BorderSide>,
    /// Left border
    pub left: Option<BorderSide>,
}

impl BorderSides {
    /// Create empty border sides (no borders)
    pub fn none() -> Self {
        Self::default()
    }

    /// Check if any border side is set
    pub fn has_any(&self) -> bool {
        self.top.as_ref().is_some_and(|b| b.is_visible())
            || self.right.as_ref().is_some_and(|b| b.is_visible())
            || self.bottom.as_ref().is_some_and(|b| b.is_visible())
            || self.left.as_ref().is_some_and(|b| b.is_visible())
    }
}

/// Builder for constructing per-side borders with a fluent API
///
/// # Example
///
/// ```ignore
/// use blinc_layout::element::BorderBuilder;
/// use blinc_core::Color;
///
/// // Create borders with different sides
/// let borders = BorderBuilder::new()
///     .left(4.0, Color::BLUE)
///     .bottom(1.0, Color::gray(0.3))
///     .build();
///
/// // Or use the shorthand on Div
/// div().borders(|b| b.left(4.0, Color::BLUE).bottom(1.0, Color::gray(0.3)))
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct BorderBuilder {
    sides: BorderSides,
}

impl BorderBuilder {
    /// Create a new border builder with no borders
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the left border
    pub fn left(mut self, width: f32, color: Color) -> Self {
        self.sides.left = Some(BorderSide::new(width, color));
        self
    }

    /// Set the right border
    pub fn right(mut self, width: f32, color: Color) -> Self {
        self.sides.right = Some(BorderSide::new(width, color));
        self
    }

    /// Set the top border
    pub fn top(mut self, width: f32, color: Color) -> Self {
        self.sides.top = Some(BorderSide::new(width, color));
        self
    }

    /// Set the bottom border
    pub fn bottom(mut self, width: f32, color: Color) -> Self {
        self.sides.bottom = Some(BorderSide::new(width, color));
        self
    }

    /// Set horizontal borders (left and right)
    pub fn x(mut self, width: f32, color: Color) -> Self {
        let side = BorderSide::new(width, color);
        self.sides.left = Some(side);
        self.sides.right = Some(side);
        self
    }

    /// Set vertical borders (top and bottom)
    pub fn y(mut self, width: f32, color: Color) -> Self {
        let side = BorderSide::new(width, color);
        self.sides.top = Some(side);
        self.sides.bottom = Some(side);
        self
    }

    /// Set all borders to the same value
    pub fn all(mut self, width: f32, color: Color) -> Self {
        let side = BorderSide::new(width, color);
        self.sides.top = Some(side);
        self.sides.right = Some(side);
        self.sides.bottom = Some(side);
        self.sides.left = Some(side);
        self
    }

    /// Build the BorderSides configuration
    pub fn build(self) -> BorderSides {
        self.sides
    }
}

/// Visual properties for rendering an element
#[derive(Clone)]
pub struct RenderProps {
    /// Background fill (solid color or gradient)
    pub background: Option<Brush>,
    /// Corner radius for rounded rectangles
    pub border_radius: CornerRadius,
    /// Tracks whether border_radius was explicitly set (distinguishes "set to 0" from "not set" in merges)
    pub border_radius_explicit: bool,
    /// Corner shape (superellipse n parameter per corner). Default is ROUND (n=1.0).
    pub corner_shape: CornerShape,
    /// When `true`, the active theme's [`ShapeTokens`](blinc_theme::ShapeTokens)
    /// must NOT substitute a squircle exponent onto this element's
    /// corners even if `corner_shape.is_round()`. Used by floating
    /// overlay widgets (popovers, dropdown panels, select menus,
    /// combobox lists) that want their chrome to match the surrounding
    /// UI's circular corners rather than picking up the theme's
    /// squircle aesthetic. Default `false`: the paint walker is free
    /// to substitute per the theme.
    pub corner_shape_locked: bool,
    /// Border color (None = no border) - used for uniform borders
    pub border_color: Option<Color>,
    /// Border width in pixels - used for uniform borders
    pub border_width: f32,
    /// Per-side borders (takes precedence over uniform border if set)
    pub border_sides: BorderSides,
    /// Outline color (None = no outline)
    pub outline_color: Option<Color>,
    /// Outline width in pixels
    pub outline_width: f32,
    /// Outline offset in pixels (gap between border and outline)
    pub outline_offset: f32,
    /// Which layer this element renders in
    pub layer: RenderLayer,
    /// Material applied to this element (glass, metallic, etc.)
    pub material: Option<Material>,
    /// Node ID for looking up children
    pub node_id: Option<LayoutNodeId>,
    /// Drop shadow stack (empty = none, 1 = single, 2-3 = compound layered shadows)
    pub shadow: Vec<Shadow>,
    /// Transform applied to this element (translate, scale, rotate)
    pub transform: Option<Transform>,
    /// Opacity (0.0 = transparent, 1.0 = opaque)
    pub opacity: f32,
    /// Whether this element clips its children (for scroll containers)
    pub clips_content: bool,
    /// Overflow fade distances (smooth alpha fade at clip edges). Applied when clips_content is true.
    pub overflow_fade: OverflowFade,
    /// Motion animation configuration (enter/exit animations)
    pub motion: Option<MotionAnimation>,
    /// Stable ID for motion animation tracking across tree rebuilds
    /// Used by overlays where the tree is rebuilt every frame
    pub motion_stable_id: Option<String>,
    /// Whether the motion animation should replay from the beginning
    /// Used with motion_stable_id to force animation restart on content change
    pub motion_should_replay: bool,
    /// Whether the motion animation should start in suspended state
    /// When true, the motion starts with opacity 0 and waits for explicit start
    pub motion_is_suspended: bool,
    /// Callback to invoke when the motion is laid out and ready
    /// Used with suspended animations to start the animation after content is mounted
    pub motion_on_ready_callback:
        Option<std::sync::Arc<dyn Fn(ElementBounds) + Send + Sync + 'static>>,
    /// Whether this is a Stack layer that increments z_layer for proper z-ordering
    /// When true, entering this node increments the DrawContext's z_layer
    pub is_stack_layer: bool,
    /// Whether this node is the root of an overlay panel (`overlay_stack` layer
    /// or `toast_tray` layer) that should be routed to the dynamic batch so
    /// the panel's SDF lands in `composite_frame`'s overlay pass — AFTER the
    /// static cache (and the static SVG dispatch) is blitted.
    ///
    /// Distinct from `is_stack_layer` because the generic `Stack` widget
    /// (used by e.g. `cn::avatar` for layering a status dot over the avatar
    /// circle) sets `is_stack_layer: true` for z-counter bumping but is NOT
    /// an overlay — routing those to the dynamic batch broke the avatar's
    /// inner SDF / image rendering.
    pub is_overlay_root: bool,
    /// Cursor style when hovering over this element (None = inherit from parent)
    pub cursor: Option<CursorStyle>,
    /// Pointer targets within this element, addressed by byte range.
    /// Consulted before [`Self::cursor`], so a single node can offer a
    /// pointer over part of itself.
    ///
    /// Exists for text that is one node but not uniformly interactive:
    /// a `RichText` paragraph containing a link is one node, so a
    /// node-level cursor would either lie about the whole paragraph or
    /// deny the link its pointer. Byte ranges rather than rects because
    /// where a link lands depends on where the text wrapped, which
    /// depends on a width this element does not have until layout runs.
    pub text_hit_spans: Option<std::sync::Arc<crate::text_hit::TextHitSpans>>,
    /// Whether this element is transparent to hit-testing (pointer-events: none)
    /// When true, this element will not capture clicks/hovers - only its children can.
    /// Used by Stack layers to allow clicks to pass through to siblings.
    pub pointer_events_none: bool,
    /// Whether this element has `position: fixed` behavior.
    /// Fixed elements are immune to scroll transforms from ancestor scroll containers.
    pub is_fixed: bool,
    /// Whether this element has `position: sticky` behavior.
    /// Sticky elements clamp their position when scrolling past thresholds.
    pub is_sticky: bool,
    /// Sticky scroll-lock threshold from top (in pixels).
    pub sticky_top: Option<f32>,
    /// Sticky scroll-lock threshold from bottom (in pixels).
    pub sticky_bottom: Option<f32>,
    /// CSS z-index for controlling render order within a layer
    pub z_index: i32,
    /// Text foreground color override (when set, overrides TextData.color during rendering)
    pub text_color: Option<[f32; 4]>,
    /// Font size override (when set, overrides TextData.font_size during rendering)
    pub font_size: Option<f32>,
    /// Text shadow (offset, blur, color)
    pub text_shadow: Option<Shadow>,
    /// Font weight override
    pub font_weight: Option<crate::div::FontWeight>,
    /// Font style override (upright or italic)
    pub font_style: Option<crate::element_style::FontStyle>,
    /// Text decoration override
    pub text_decoration: Option<crate::element_style::TextDecoration>,
    /// Line height multiplier override
    pub line_height: Option<f32>,
    /// Text alignment override
    pub text_align: Option<crate::div::TextAlign>,
    /// Letter spacing override in pixels
    pub letter_spacing: Option<f32>,
    /// SVG fill color override
    pub fill: Option<[f32; 4]>,
    /// SVG stroke color override
    pub stroke: Option<[f32; 4]>,
    /// SVG stroke width override
    pub stroke_width: Option<f32>,
    /// SVG stroke-dasharray pattern (alternating dash/gap lengths)
    pub stroke_dasharray: Option<Vec<f32>>,
    /// SVG stroke-dashoffset in pixels
    pub stroke_dashoffset: Option<f32>,
    /// SVG path `d` attribute data (for path morphing animations)
    pub svg_path_data: Option<String>,
    /// Transform origin as percentages [x%, y%] (default 50%, 50% = center)
    pub transform_origin: Option<[f32; 2]>,
    /// Layer effects applied to this element (blur, drop shadow, glow, color matrix)
    /// Effects are applied during layer composition when the element is rendered
    pub layer_effects: Vec<LayerEffect>,
    // 3D transform properties
    /// X-axis rotation in degrees
    pub rotate_x: Option<f32>,
    /// Y-axis rotation in degrees
    pub rotate_y: Option<f32>,
    /// Perspective distance in pixels
    pub perspective: Option<f32>,
    /// 3D shape type (0=none, 1=box, 2=sphere, 3=cylinder, 4=torus, 5=capsule)
    pub shape_3d: Option<f32>,
    /// 3D extrusion depth in pixels
    pub depth: Option<f32>,
    /// Light direction [x, y, z]
    pub light_direction: Option<[f32; 3]>,
    /// Light intensity (0.0 - 1.0+)
    pub light_intensity: Option<f32>,
    /// Ambient light level (0.0 - 1.0)
    pub ambient: Option<f32>,
    /// Specular power (higher = tighter highlights)
    pub specular: Option<f32>,
    /// Z-axis translation in pixels (positive = toward viewer)
    pub translate_z: Option<f32>,
    /// 3D boolean operation type (0=union, 1=subtract, 2=intersect, 3=smooth-union, 4=smooth-subtract, 5=smooth-intersect)
    pub op_3d: Option<f32>,
    /// Blend radius for smooth boolean operations (in pixels)
    pub blend_3d: Option<f32>,
    /// CSS clip-path shape function
    pub clip_path: Option<ClipPath>,
    /// CSS filter functions (grayscale, invert, sepia, brightness, contrast, saturate, hue-rotate)
    pub filter: Option<crate::element_style::CssFilter>,
    /// DEPRECATED: Whether the motion should start exiting
    ///
    /// This field is deprecated. Motion exit is now triggered explicitly via
    /// `MotionHandle.exit()` / `query_motion(key).exit()` instead of capturing
    /// the is_overlay_closing() flag at build time.
    ///
    /// The old mechanism was flawed because the flag reset after build_content(),
    /// breaking multi-frame exit animations.
    #[deprecated(
        since = "0.1.0",
        note = "Use query_motion(key).exit() to explicitly trigger motion exit"
    )]
    pub motion_is_exiting: bool,
    /// CSS visibility (false = hidden, keeps layout space but doesn't render)
    pub visible: bool,
    /// CSS object-fit override for images (0=cover, 1=contain, 2=fill, 3=scale-down, 4=none)
    pub object_fit: Option<u8>,
    /// CSS object-position override for images [x, y] in 0.0-1.0 range
    pub object_position: Option<[f32; 2]>,
    /// CSS loading strategy override (0=eager, 1=lazy)
    pub loading_strategy: Option<u8>,
    /// CSS placeholder type override (0=none, 1=color, 2=image, 3=skeleton)
    pub placeholder_type: Option<u8>,
    /// CSS placeholder color override
    pub placeholder_color: Option<[f32; 4]>,
    /// CSS placeholder image source override
    pub placeholder_image: Option<String>,
    /// CSS fade-in duration in milliseconds
    pub fade_duration_ms: Option<u32>,
    /// Per-SVG-tag style overrides from CSS tag-name selectors (e.g., `path { fill: red; }`)
    pub svg_tag_styles: HashMap<String, SvgTagStyle>,
    /// CSS mix-blend-mode for this element
    pub mix_blend_mode: Option<blinc_core::BlendMode>,
    /// Text decoration line color override
    pub text_decoration_color: Option<[f32; 4]>,
    /// Text decoration line thickness override
    pub text_decoration_thickness: Option<f32>,
    /// CSS text-overflow behavior
    pub text_overflow: Option<crate::element_style::TextOverflow>,
    /// CSS white-space behavior
    pub white_space: Option<crate::element_style::WhiteSpace>,
    /// CSS mask-image (URL or gradient)
    pub mask_image: Option<blinc_core::MaskImage>,
    /// CSS mask-mode
    pub mask_mode: Option<blinc_core::MaskMode>,
    /// @flow shader name (references a FlowGraph in the stylesheet)
    pub flow: Option<String>,
    /// Direct @flow graph (from `flow!` macro), bypasses stylesheet lookup
    pub flow_graph: Option<Arc<FlowGraph>>,
}

impl Default for RenderProps {
    #[allow(deprecated)]
    fn default() -> Self {
        Self {
            background: None,
            border_radius: CornerRadius::default(),
            border_radius_explicit: false,
            corner_shape: CornerShape::default(),
            corner_shape_locked: false,
            border_color: None,
            border_width: 0.0,
            border_sides: BorderSides::default(),
            outline_color: None,
            outline_width: 0.0,
            outline_offset: 0.0,
            layer: RenderLayer::default(),
            material: None,
            node_id: None,
            shadow: Vec::new(),
            transform: None,
            opacity: 1.0,
            clips_content: false,
            overflow_fade: OverflowFade::default(),
            motion: None,
            motion_stable_id: None,
            motion_should_replay: false,
            motion_is_suspended: false,
            motion_on_ready_callback: None,
            is_stack_layer: false,
            is_overlay_root: false,
            cursor: None,
            text_hit_spans: None,
            pointer_events_none: false,
            is_fixed: false,
            is_sticky: false,
            sticky_top: None,
            sticky_bottom: None,
            layer_effects: Vec::new(),
            rotate_x: None,
            rotate_y: None,
            perspective: None,
            shape_3d: None,
            depth: None,
            light_direction: None,
            light_intensity: None,
            ambient: None,
            specular: None,
            translate_z: None,
            op_3d: None,
            blend_3d: None,
            clip_path: None,
            filter: None,
            motion_is_exiting: false,
            z_index: 0,
            text_color: None,
            font_size: None,
            text_shadow: None,
            font_weight: None,
            font_style: None,
            text_decoration: None,
            line_height: None,
            text_align: None,
            letter_spacing: None,
            fill: None,
            stroke: None,
            stroke_width: None,
            stroke_dasharray: None,
            stroke_dashoffset: None,
            svg_path_data: None,
            transform_origin: None,
            visible: true,
            object_fit: None,
            object_position: None,
            loading_strategy: None,
            placeholder_type: None,
            placeholder_color: None,
            placeholder_image: None,
            fade_duration_ms: None,
            svg_tag_styles: HashMap::new(),
            mix_blend_mode: None,
            text_decoration_color: None,
            text_decoration_thickness: None,
            text_overflow: None,
            white_space: None,
            mask_image: None,
            mask_mode: None,
            flow: None,
            flow_graph: None,
        }
    }
}

impl RenderProps {
    /// Create new render properties
    pub fn new() -> Self {
        Self::default()
    }

    /// Set background brush
    pub fn with_background(mut self, brush: impl Into<Brush>) -> Self {
        self.background = Some(brush.into());
        self
    }

    /// Set background color
    pub fn with_bg_color(mut self, color: Color) -> Self {
        self.background = Some(Brush::Solid(color));
        self
    }

    /// Set corner radius
    pub fn with_border_radius(mut self, radius: CornerRadius) -> Self {
        self.border_radius = radius;
        self
    }

    /// Set uniform corner radius
    pub fn with_rounded(mut self, radius: f32) -> Self {
        self.border_radius = CornerRadius::uniform(radius);
        self
    }

    /// Set render layer
    pub fn with_layer(mut self, layer: RenderLayer) -> Self {
        self.layer = layer;
        self
    }

    /// Set material
    pub fn with_material(mut self, material: Material) -> Self {
        self.material = Some(material);
        self
    }

    /// Set node ID
    pub fn with_node_id(mut self, id: LayoutNodeId) -> Self {
        self.node_id = Some(id);
        self
    }

    /// Check if this element has a glass material
    pub fn is_glass(&self) -> bool {
        matches!(self.material, Some(Material::Glass(_)))
    }

    /// Get the glass material if present
    pub fn glass_material(&self) -> Option<&GlassMaterial> {
        match &self.material {
            Some(Material::Glass(glass)) => Some(glass),
            _ => None,
        }
    }

    /// Set drop shadow (replaces any existing layers).
    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = vec![shadow];
        self
    }

    /// Set a compound drop shadow stack.
    pub fn with_shadow_stack(mut self, shadows: Vec<Shadow>) -> Self {
        self.shadow = shadows;
        self
    }

    /// Set transform
    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }

    /// Set opacity (0.0 = transparent, 1.0 = opaque)
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Set whether this element clips its children
    pub fn with_clips_content(mut self, clips: bool) -> Self {
        self.clips_content = clips;
        self
    }

    /// Set cursor style for hover
    pub fn with_cursor(mut self, cursor: CursorStyle) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Merge changes from another RenderProps, only taking non-default values
    ///
    /// This allows stateful elements to overlay state-specific changes
    /// on top of base properties. Only properties that were explicitly
    /// set in `other` will override `self`.
    pub fn merge_from(&mut self, other: &RenderProps) {
        // Override background if set
        if other.background.is_some() {
            self.background = other.background.clone();
        }
        // Override border_radius if explicitly set or non-zero
        if other.border_radius_explicit || other.border_radius != CornerRadius::default() {
            self.border_radius = other.border_radius;
            self.border_radius_explicit = other.border_radius_explicit;
        }
        // Override border_color if set
        if other.border_color.is_some() {
            self.border_color = other.border_color;
        }
        // Override border_width if non-zero
        if other.border_width > 0.0 {
            self.border_width = other.border_width;
        }
        // Override layer if non-default
        if other.layer != RenderLayer::default() {
            self.layer = other.layer;
        }
        // Override material if set
        if other.material.is_some() {
            self.material = other.material.clone();
        }
        // node_id is not merged - keep the original
        // Override shadow if set
        if !other.shadow.is_empty() {
            self.shadow = other.shadow.clone();
        }
        // Override transform if set
        if other.transform.is_some() {
            self.transform = other.transform.clone();
        }
        // Override opacity if non-default
        if (other.opacity - 1.0).abs() > f32::EPSILON {
            self.opacity = other.opacity;
        }
        // Override clips_content if true
        if other.clips_content {
            self.clips_content = true;
        }
        // Override motion if set
        if other.motion.is_some() {
            self.motion = other.motion.clone();
        }
        // Override outline if set. Without this the Stateful state-callback's
        // queue_prop_update path wipes outline_* back to defaults on every
        // state transition (the callback rebuilds the inner Div with the
        // outline set, but merge_from doesn't carry it over), which both
        // dropped the focus ring entirely on focus and — worse — broke
        // transition replay on the second focus, because the in-flight
        // CSS transition's interpolated value gets overwritten with the
        // default (None) on every callback fire.
        if other.outline_color.is_some() {
            self.outline_color = other.outline_color;
        }
        if other.outline_width > 0.0 {
            self.outline_width = other.outline_width;
        }
        if other.outline_offset != 0.0 {
            self.outline_offset = other.outline_offset;
        }
        // Override cursor if set. Same rationale as outline above: without
        // it, a Stateful's `queue_prop_update` path wipes the cursor back to
        // the OS arrow on every state transition (the on_state callback sets
        // `.cursor(...)` on its Div, but merge_from didn't carry it over), so
        // a `stateful_button().cursor(Pointer)` reverted to Default the moment
        // the pointer moved and drove a hover transition. See issue #55.
        if other.cursor.is_some() {
            self.cursor = other.cursor;
        }
    }
}

// ============================================================================
// Dynamic Render Props (Value References)
// ============================================================================

/// Dynamic render props that can hold references to reactive values
///
/// Unlike `RenderProps` which stores resolved values directly, `DynRenderProps`
/// stores references (signal IDs, spring IDs) that are resolved at render time.
/// This enables visual property changes without tree rebuilds.
///
/// # Architecture
///
/// ```text
/// Build Time:                       Render Time:
/// ┌────────────┐                   ┌────────────────┐
/// │ ElementBuilder                │ ValueContext   │
/// │ .opacity(0.5)  ────────────►  │ .reactive      │
/// │ .bg_signal(id) ────────────►  │ .animations    │
/// └────────────┘                   └────────────────┘
///       │                                │
///       ▼                                ▼
/// ┌────────────┐                   ┌────────────────┐
/// │ DynRenderProps               │ Resolved Values │
/// │ opacity: DynFloat::Static    │ opacity: 0.5    │
/// │ background: DynValue::Signal │ background: Red │
/// └────────────┘                   └────────────────┘
/// ```
#[derive(Clone)]
pub struct DynRenderProps {
    /// Background fill (can be static or signal-driven)
    pub background: Option<DynValue<Brush>>,
    /// Corner radius (typically static)
    pub border_radius: CornerRadius,
    /// Corner shape (superellipse n parameter per corner)
    pub corner_shape: CornerShape,
    /// Which layer this element renders in
    pub layer: RenderLayer,
    /// Material applied to this element
    pub material: Option<Material>,
    /// Node ID for looking up children
    pub node_id: Option<LayoutNodeId>,
    /// Drop shadow stack (typically static, 1-3 layers)
    pub shadow: Vec<Shadow>,
    /// Transform (can be animated)
    pub transform: Option<Transform>,
    /// Opacity (can be static, signal, or spring animated)
    pub opacity: DynFloat,
    /// Whether this element clips its children
    pub clips_content: bool,
    /// Overflow fade distances
    pub overflow_fade: OverflowFade,
}

impl Default for DynRenderProps {
    fn default() -> Self {
        Self {
            background: None,
            border_radius: CornerRadius::default(),
            corner_shape: CornerShape::default(),
            layer: RenderLayer::default(),
            material: None,
            node_id: None,
            shadow: Vec::new(),
            transform: None,
            opacity: DynFloat::Static(1.0),
            clips_content: false,
            overflow_fade: OverflowFade::default(),
        }
    }
}

impl DynRenderProps {
    /// Create new dynamic render properties
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve all dynamic values using the provided context
    ///
    /// This is called at render time to get the actual values to draw with.
    pub fn resolve(&self, ctx: &ValueContext) -> ResolvedRenderProps {
        ResolvedRenderProps {
            background: self.background.as_ref().map(|v| v.get(ctx)),
            border_radius: self.border_radius,
            corner_shape: self.corner_shape,
            layer: self.layer,
            material: self.material.clone(),
            node_id: self.node_id,
            shadow: self.shadow.clone(),
            transform: self.transform.clone(),
            opacity: self.opacity.get(ctx),
            clips_content: self.clips_content,
            overflow_fade: self.overflow_fade,
        }
    }

    /// Convert from static RenderProps
    pub fn from_static(props: RenderProps) -> Self {
        Self {
            background: props.background.map(DynValue::Static),
            border_radius: props.border_radius,
            corner_shape: props.corner_shape,
            layer: props.layer,
            material: props.material,
            node_id: props.node_id,
            shadow: props.shadow,
            transform: props.transform,
            opacity: DynFloat::Static(props.opacity),
            clips_content: props.clips_content,
            overflow_fade: props.overflow_fade,
        }
    }

    /// Set static background
    pub fn with_background(mut self, brush: impl Into<Brush>) -> Self {
        self.background = Some(DynValue::Static(brush.into()));
        self
    }

    /// Set background from a signal reference
    pub fn with_background_signal(mut self, signal_id: u64, default: Brush) -> Self {
        self.background = Some(DynValue::Signal {
            id: signal_id,
            default,
        });
        self
    }

    /// Set static opacity
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = DynFloat::Static(opacity.clamp(0.0, 1.0));
        self
    }

    /// Set opacity from a signal reference
    pub fn with_opacity_signal(mut self, signal_id: u64, default: f32) -> Self {
        self.opacity = DynFloat::Signal {
            id: signal_id,
            default,
        };
        self
    }

    /// Set opacity from a spring animation
    pub fn with_opacity_spring(mut self, spring_id: u64, generation: u32, default: f32) -> Self {
        self.opacity = DynFloat::Spring {
            id: spring_id,
            generation,
            default,
        };
        self
    }
}

/// Resolved render props with concrete values
///
/// This is the result of resolving `DynRenderProps` at render time.
/// All dynamic references have been replaced with their current values.
#[derive(Clone)]
pub struct ResolvedRenderProps {
    /// Background fill (resolved)
    pub background: Option<Brush>,
    /// Corner radius
    pub border_radius: CornerRadius,
    /// Corner shape (superellipse n parameter per corner)
    pub corner_shape: CornerShape,
    /// Render layer
    pub layer: RenderLayer,
    /// Material
    pub material: Option<Material>,
    /// Node ID
    pub node_id: Option<LayoutNodeId>,
    /// Drop shadow stack
    pub shadow: Vec<Shadow>,
    /// Transform
    pub transform: Option<Transform>,
    /// Opacity (resolved)
    pub opacity: f32,
    /// Whether this element clips its children
    pub clips_content: bool,
    /// Overflow fade distances
    pub overflow_fade: OverflowFade,
}

impl Default for ResolvedRenderProps {
    fn default() -> Self {
        Self {
            background: None,
            border_radius: CornerRadius::default(),
            corner_shape: CornerShape::default(),
            layer: RenderLayer::default(),
            material: None,
            node_id: None,
            shadow: Vec::new(),
            transform: None,
            opacity: 1.0,
            clips_content: false,
            overflow_fade: OverflowFade::default(),
        }
    }
}

#[cfg(test)]
mod merge_from_tests {
    use super::{CursorStyle, RenderProps};

    // Regression for issue #55: a `stateful_button().cursor(Pointer)` lost its
    // cursor the moment the pointer moved. The state-callback path builds
    // `final_props = base_props; final_props.merge_from(&callback_props)` and
    // `merge_from` never carried `cursor`, so the queued prop update reset the
    // node's cursor to the OS arrow on every hover/press transition.
    #[test]
    fn merge_from_carries_callback_cursor() {
        let mut base = RenderProps::default();
        let callback = RenderProps {
            cursor: Some(CursorStyle::Pointer),
            ..Default::default()
        };
        base.merge_from(&callback);
        assert_eq!(base.cursor, Some(CursorStyle::Pointer));
    }

    // An unset callback cursor must not clobber a cursor already on the base
    // (mirrors how the other optional visual props merge).
    #[test]
    fn merge_from_keeps_base_cursor_when_callback_unset() {
        let mut base = RenderProps {
            cursor: Some(CursorStyle::Text),
            ..Default::default()
        };
        base.merge_from(&RenderProps::default());
        assert_eq!(base.cursor, Some(CursorStyle::Text));
    }
}
