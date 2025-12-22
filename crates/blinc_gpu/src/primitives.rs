//! GPU primitive batching
//!
//! Defines GPU-ready data structures that match the shader uniform layouts.
//! All structures use `#[repr(C)]` and implement `bytemuck::Pod` for safe
//! GPU buffer copies.

/// Primitive types (must match shader constants)
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrimitiveType {
    #[default]
    Rect = 0,
    Circle = 1,
    Ellipse = 2,
    Shadow = 3,
    InnerShadow = 4,
    CircleShadow = 5,
    CircleInnerShadow = 6,
}

/// Fill types (must match shader constants)
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillType {
    #[default]
    Solid = 0,
    LinearGradient = 1,
    RadialGradient = 2,
}

/// Glass material types (must match shader constants)
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlassType {
    UltraThin = 0,
    Thin = 1,
    #[default]
    Regular = 2,
    Thick = 3,
    Chrome = 4,
}

/// Clip types for primitives
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipType {
    /// No clipping
    #[default]
    None = 0,
    /// Rectangular clip (with optional rounded corners)
    Rect = 1,
    /// Circular clip
    Circle = 2,
    /// Elliptical clip
    Ellipse = 3,
}

/// A GPU primitive ready for rendering (matches shader `Primitive` struct)
///
/// Memory layout:
/// - bounds: `vec4<f32>`        (16 bytes)
/// - corner_radius: `vec4<f32>` (16 bytes)
/// - color: `vec4<f32>`         (16 bytes)
/// - color2: `vec4<f32>`        (16 bytes)
/// - border: `vec4<f32>`        (16 bytes)
/// - border_color: `vec4<f32>`  (16 bytes)
/// - shadow: `vec4<f32>`        (16 bytes)
/// - shadow_color: `vec4<f32>`  (16 bytes)
/// - clip_bounds: `vec4<f32>`   (16 bytes) - clip region (x, y, width, height)
/// - clip_radius: `vec4<f32>`   (16 bytes) - clip corner radii or circle/ellipse radii
/// - type_info: `vec4<u32>`     (16 bytes) - (primitive_type, fill_type, clip_type, 0)
/// Total: 176 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPrimitive {
    /// Bounds (x, y, width, height)
    pub bounds: [f32; 4],
    /// Corner radii (top-left, top-right, bottom-right, bottom-left)
    pub corner_radius: [f32; 4],
    /// Fill color (or gradient start color)
    pub color: [f32; 4],
    /// Gradient end color (for gradients)
    pub color2: [f32; 4],
    /// Border (width, 0, 0, 0)
    pub border: [f32; 4],
    /// Border color
    pub border_color: [f32; 4],
    /// Shadow (offset_x, offset_y, blur, spread)
    pub shadow: [f32; 4],
    /// Shadow color
    pub shadow_color: [f32; 4],
    /// Clip bounds (x, y, width, height) - set to large values for no clip
    pub clip_bounds: [f32; 4],
    /// Clip corner radii (for rounded rect) or (radius_x, radius_y, 0, 0) for ellipse
    pub clip_radius: [f32; 4],
    /// Type info (primitive_type, fill_type, clip_type, 0)
    pub type_info: [u32; 4],
}

impl Default for GpuPrimitive {
    fn default() -> Self {
        Self {
            bounds: [0.0, 0.0, 100.0, 100.0],
            corner_radius: [0.0; 4],
            color: [1.0, 1.0, 1.0, 1.0],
            color2: [1.0, 1.0, 1.0, 1.0],
            border: [0.0; 4],
            border_color: [0.0, 0.0, 0.0, 1.0],
            shadow: [0.0; 4],
            shadow_color: [0.0, 0.0, 0.0, 0.0],
            // Default: no clip (large bounds that won't clip anything)
            clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
            clip_radius: [0.0; 4],
            type_info: [0; 4], // clip_type defaults to None (0)
        }
    }
}

impl GpuPrimitive {
    /// Create a new rectangle primitive
    pub fn rect(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            bounds: [x, y, width, height],
            type_info: [PrimitiveType::Rect as u32, FillType::Solid as u32, 0, 0],
            ..Default::default()
        }
    }

    /// Create a new circle primitive
    pub fn circle(cx: f32, cy: f32, radius: f32) -> Self {
        Self {
            bounds: [cx - radius, cy - radius, radius * 2.0, radius * 2.0],
            type_info: [PrimitiveType::Circle as u32, FillType::Solid as u32, 0, 0],
            ..Default::default()
        }
    }

    /// Set the fill color
    pub fn with_color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.color = [r, g, b, a];
        self
    }

    /// Set uniform corner radius
    pub fn with_corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = [radius; 4];
        self
    }

    /// Set per-corner radius (top-left, top-right, bottom-right, bottom-left)
    pub fn with_corner_radii(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.corner_radius = [tl, tr, br, bl];
        self
    }

    /// Set border
    pub fn with_border(mut self, width: f32, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.border = [width, 0.0, 0.0, 0.0];
        self.border_color = [r, g, b, a];
        self
    }

    /// Set shadow
    pub fn with_shadow(
        mut self,
        offset_x: f32,
        offset_y: f32,
        blur: f32,
        spread: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) -> Self {
        self.shadow = [offset_x, offset_y, blur, spread];
        self.shadow_color = [r, g, b, a];
        self
    }

    /// Set linear gradient fill
    pub fn with_linear_gradient(
        mut self,
        r1: f32,
        g1: f32,
        b1: f32,
        a1: f32,
        r2: f32,
        g2: f32,
        b2: f32,
        a2: f32,
    ) -> Self {
        self.color = [r1, g1, b1, a1];
        self.color2 = [r2, g2, b2, a2];
        self.type_info[1] = FillType::LinearGradient as u32;
        self
    }

    /// Set radial gradient fill
    pub fn with_radial_gradient(
        mut self,
        r1: f32,
        g1: f32,
        b1: f32,
        a1: f32,
        r2: f32,
        g2: f32,
        b2: f32,
        a2: f32,
    ) -> Self {
        self.color = [r1, g1, b1, a1];
        self.color2 = [r2, g2, b2, a2];
        self.type_info[1] = FillType::RadialGradient as u32;
        self
    }

    /// Set rectangular clip region
    pub fn with_clip_rect(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.clip_bounds = [x, y, width, height];
        self.clip_radius = [0.0; 4];
        self.type_info[2] = ClipType::Rect as u32;
        self
    }

    /// Set rounded rectangular clip region
    pub fn with_clip_rounded_rect(
        mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        tl: f32,
        tr: f32,
        br: f32,
        bl: f32,
    ) -> Self {
        self.clip_bounds = [x, y, width, height];
        self.clip_radius = [tl, tr, br, bl];
        self.type_info[2] = ClipType::Rect as u32;
        self
    }

    /// Set circular clip region
    pub fn with_clip_circle(mut self, cx: f32, cy: f32, radius: f32) -> Self {
        self.clip_bounds = [cx, cy, radius, radius];
        self.clip_radius = [radius, radius, 0.0, 0.0];
        self.type_info[2] = ClipType::Circle as u32;
        self
    }

    /// Set elliptical clip region
    pub fn with_clip_ellipse(mut self, cx: f32, cy: f32, rx: f32, ry: f32) -> Self {
        self.clip_bounds = [cx, cy, rx, ry];
        self.clip_radius = [rx, ry, 0.0, 0.0];
        self.type_info[2] = ClipType::Ellipse as u32;
        self
    }

    /// Clear clip region
    pub fn with_no_clip(mut self) -> Self {
        self.clip_bounds = [-10000.0, -10000.0, 100000.0, 100000.0];
        self.clip_radius = [0.0; 4];
        self.type_info[2] = ClipType::None as u32;
        self
    }
}

/// A GPU glass primitive for vibrancy/blur effects (matches shader `GlassPrimitive` struct)
///
/// Memory layout:
/// - bounds: `vec4<f32>`        (16 bytes)
/// - corner_radius: `vec4<f32>` (16 bytes)
/// - tint_color: `vec4<f32>`    (16 bytes)
/// - params: `vec4<f32>`        (16 bytes)
/// - params2: `vec4<f32>`       (16 bytes)
/// - type_info: `vec4<u32>`     (16 bytes)
/// Total: 96 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuGlassPrimitive {
    /// Bounds (x, y, width, height)
    pub bounds: [f32; 4],
    /// Corner radii (top-left, top-right, bottom-right, bottom-left)
    pub corner_radius: [f32; 4],
    /// Tint color (RGBA)
    pub tint_color: [f32; 4],
    /// Glass parameters (blur_radius, saturation, brightness, noise_amount)
    pub params: [f32; 4],
    /// Glass parameters 2 (border_thickness, light_angle, shadow_blur, shadow_opacity)
    /// - border_thickness: thickness of edge highlight in pixels (default 0.8)
    /// - light_angle: angle of simulated light source in radians (default -PI/4 = top-left)
    /// - shadow_blur: blur radius for drop shadow (default 0 = no shadow)
    /// - shadow_opacity: opacity of the drop shadow (default 0 = no shadow)
    pub params2: [f32; 4],
    /// Type info (glass_type, 0, 0, 0)
    pub type_info: [u32; 4],
}

impl Default for GpuGlassPrimitive {
    fn default() -> Self {
        Self {
            bounds: [0.0, 0.0, 100.0, 100.0],
            corner_radius: [0.0; 4],
            tint_color: [1.0, 1.0, 1.0, 0.1], // Subtle white tint
            params: [20.0, 1.0, 1.0, 0.0],    // blur=20, saturation=1, brightness=1, noise=0
            // border_thickness=0.8, light_angle=-0.785 (top-left, -45 degrees)
            params2: [0.8, -0.785398, 0.0, 0.0],
            type_info: [GlassType::Regular as u32, 0, 0, 0],
        }
    }
}

impl GpuGlassPrimitive {
    /// Create a new glass rectangle
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            bounds: [x, y, width, height],
            ..Default::default()
        }
    }

    /// Set uniform corner radius
    pub fn with_corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = [radius; 4];
        self
    }

    /// Set tint color
    pub fn with_tint(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.tint_color = [r, g, b, a];
        self
    }

    /// Set blur radius
    pub fn with_blur(mut self, radius: f32) -> Self {
        self.params[0] = radius;
        self
    }

    /// Set saturation (1.0 = normal, 0.0 = grayscale, >1.0 = oversaturated)
    pub fn with_saturation(mut self, saturation: f32) -> Self {
        self.params[1] = saturation;
        self
    }

    /// Set brightness multiplier
    pub fn with_brightness(mut self, brightness: f32) -> Self {
        self.params[2] = brightness;
        self
    }

    /// Set noise amount for frosted texture
    pub fn with_noise(mut self, amount: f32) -> Self {
        self.params[3] = amount;
        self
    }

    /// Set glass type/style
    pub fn with_glass_type(mut self, glass_type: GlassType) -> Self {
        self.type_info[0] = glass_type as u32;
        self
    }

    /// Ultra-thin glass preset (very subtle blur)
    pub fn ultra_thin(mut self) -> Self {
        self.type_info[0] = GlassType::UltraThin as u32;
        self.params[0] = 10.0; // Less blur
        self
    }

    /// Thin glass preset
    pub fn thin(mut self) -> Self {
        self.type_info[0] = GlassType::Thin as u32;
        self.params[0] = 15.0;
        self
    }

    /// Regular glass preset (default)
    pub fn regular(mut self) -> Self {
        self.type_info[0] = GlassType::Regular as u32;
        self.params[0] = 20.0;
        self
    }

    /// Thick glass preset (stronger effect)
    pub fn thick(mut self) -> Self {
        self.type_info[0] = GlassType::Thick as u32;
        self.params[0] = 30.0;
        self
    }

    /// Chrome/metallic glass preset
    pub fn chrome(mut self) -> Self {
        self.type_info[0] = GlassType::Chrome as u32;
        self.params[0] = 25.0;
        self.params[1] = 0.8; // Slightly desaturated
        self
    }

    /// Set border/edge highlight thickness in pixels
    pub fn with_border_thickness(mut self, thickness: f32) -> Self {
        self.params2[0] = thickness;
        self
    }

    /// Set light angle for edge reflection effect in radians
    /// 0 = light from right, PI/2 = from bottom, PI = from left, -PI/2 = from top
    /// Default is -PI/4 (-45 degrees, top-left)
    pub fn with_light_angle(mut self, angle_radians: f32) -> Self {
        self.params2[1] = angle_radians;
        self
    }

    /// Set light angle in degrees (convenience method)
    pub fn with_light_angle_degrees(mut self, angle_degrees: f32) -> Self {
        self.params2[1] = angle_degrees * std::f32::consts::PI / 180.0;
        self
    }

    /// Set drop shadow for the glass panel
    /// - blur: blur radius in pixels (0 = no shadow)
    /// - opacity: shadow opacity (0.0 - 1.0)
    pub fn with_shadow(mut self, blur: f32, opacity: f32) -> Self {
        self.params2[2] = blur;
        self.params2[3] = opacity;
        self
    }

    /// Set drop shadow with offset and color
    /// For more advanced shadow control, use the full shadow parameters
    /// Note: Offset is stored in type_info[1] and type_info[2] as bits
    pub fn with_shadow_offset(mut self, blur: f32, opacity: f32, offset_x: f32, offset_y: f32) -> Self {
        self.params2[2] = blur;
        self.params2[3] = opacity;
        // Store offset in type_info (as f32 bits reinterpreted as u32)
        self.type_info[1] = offset_x.to_bits();
        self.type_info[2] = offset_y.to_bits();
        self
    }
}

/// A GPU text glyph instance (matches shader `GlyphInstance` struct)
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuGlyph {
    /// Position and size (x, y, width, height)
    pub bounds: [f32; 4],
    /// UV coordinates in atlas (u_min, v_min, u_max, v_max)
    pub uv_bounds: [f32; 4],
    /// Text color (RGBA)
    pub color: [f32; 4],
}

impl Default for GpuGlyph {
    fn default() -> Self {
        Self {
            bounds: [0.0; 4],
            uv_bounds: [0.0, 0.0, 1.0, 1.0],
            color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// Uniform buffer for viewport information
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub viewport_size: [f32; 2],
    pub _padding: [f32; 2],
}

/// Uniform buffer for glass shader
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlassUniforms {
    pub viewport_size: [f32; 2],
    pub time: f32,
    pub _padding: f32,
}

/// Uniform buffer for compositor
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompositeUniforms {
    pub opacity: f32,
    pub blend_mode: u32,
    pub _padding: [f32; 2],
}

/// Uniform buffer for path rendering
/// Layout matches shader struct exactly:
/// - viewport_size: `vec2<f32>` (8 bytes)
/// - opacity: f32 (4 bytes)
/// - _pad0: f32 (4 bytes)
/// - transform_row0: `vec4<f32>` (16 bytes)
/// - transform_row1: `vec4<f32>` (16 bytes)
/// - transform_row2: `vec4<f32>` (16 bytes)
/// Total: 64 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PathUniforms {
    pub viewport_size: [f32; 2],
    pub opacity: f32,
    pub _pad0: f32,
    /// 3x3 transform matrix stored as 3 vec4s (xyz used, w is padding)
    pub transform: [[f32; 4]; 3],
}

impl Default for PathUniforms {
    fn default() -> Self {
        Self {
            viewport_size: [800.0, 600.0],
            opacity: 1.0,
            _pad0: 0.0,
            // Identity matrix (row-major: row0 = [1,0,0], row1 = [0,1,0], row2 = [0,0,1])
            transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
            ],
        }
    }
}

/// A batch of tessellated path geometry
#[derive(Clone, Default)]
pub struct PathBatch {
    /// Vertices for all paths in this batch
    pub vertices: Vec<crate::path::PathVertex>,
    /// Indices for all paths in this batch
    pub indices: Vec<u32>,
}

/// Batch of GPU primitives for efficient rendering
pub struct PrimitiveBatch {
    pub primitives: Vec<GpuPrimitive>,
    pub glass_primitives: Vec<GpuGlassPrimitive>,
    pub glyphs: Vec<GpuGlyph>,
    /// Tessellated path geometry
    pub paths: PathBatch,
}

impl PrimitiveBatch {
    pub fn new() -> Self {
        Self {
            primitives: Vec::new(),
            glass_primitives: Vec::new(),
            glyphs: Vec::new(),
            paths: PathBatch::default(),
        }
    }

    pub fn clear(&mut self) {
        self.primitives.clear();
        self.glass_primitives.clear();
        self.glyphs.clear();
        self.paths.vertices.clear();
        self.paths.indices.clear();
    }

    pub fn push(&mut self, primitive: GpuPrimitive) {
        self.primitives.push(primitive);
    }

    pub fn push_glass(&mut self, glass: GpuGlassPrimitive) {
        self.glass_primitives.push(glass);
    }

    pub fn push_glyph(&mut self, glyph: GpuGlyph) {
        self.glyphs.push(glyph);
    }

    /// Add tessellated path geometry to the batch
    pub fn push_path(&mut self, tessellated: crate::path::TessellatedPath) {
        if tessellated.is_empty() {
            return;
        }
        // Offset indices by current vertex count
        let base_vertex = self.paths.vertices.len() as u32;
        self.paths.vertices.extend(tessellated.vertices);
        self.paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    pub fn is_empty(&self) -> bool {
        self.primitives.is_empty()
            && self.glass_primitives.is_empty()
            && self.glyphs.is_empty()
            && self.paths.vertices.is_empty()
    }

    pub fn path_vertex_count(&self) -> usize {
        self.paths.vertices.len()
    }

    pub fn path_index_count(&self) -> usize {
        self.paths.indices.len()
    }

    pub fn primitive_count(&self) -> usize {
        self.primitives.len()
    }

    pub fn glass_count(&self) -> usize {
        self.glass_primitives.len()
    }

    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }
}

impl Default for PrimitiveBatch {
    fn default() -> Self {
        Self::new()
    }
}

// Keep the old GpuRect for backwards compatibility during transition
/// Legacy rectangle primitive (deprecated - use GpuPrimitive instead)
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[deprecated(note = "Use GpuPrimitive instead")]
pub struct GpuRect {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub corner_radius: [f32; 4],
    pub border_width: f32,
    pub border_color: [f32; 4],
    pub _padding: [f32; 3],
}
