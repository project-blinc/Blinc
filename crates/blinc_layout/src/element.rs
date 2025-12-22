//! Element types and traits for layout-driven UI
//!
//! Provides the core abstractions for building layout trees that can be
//! rendered via the DrawContext API.

use blinc_core::{Brush, Color, CornerRadius, Rect, Shadow, Transform};
use taffy::Layout;

use crate::tree::LayoutNodeId;

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

/// Visual properties for rendering an element
#[derive(Clone, Default)]
pub struct RenderProps {
    /// Background fill (solid color or gradient)
    pub background: Option<Brush>,
    /// Corner radius for rounded rectangles
    pub border_radius: CornerRadius,
    /// Which layer this element renders in
    pub layer: RenderLayer,
    /// Material applied to this element (glass, metallic, etc.)
    pub material: Option<Material>,
    /// Node ID for looking up children
    pub node_id: Option<LayoutNodeId>,
    /// Drop shadow applied to this element
    pub shadow: Option<Shadow>,
    /// Transform applied to this element (translate, scale, rotate)
    pub transform: Option<Transform>,
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

    /// Set drop shadow
    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Set transform
    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }
}
