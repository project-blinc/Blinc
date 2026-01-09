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
    /// Text glyph - samples from atlas texture using gradient_params as UV bounds
    Text = 7,
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
/// - bounds: `vec4<f32>`          (16 bytes)
/// - corner_radius: `vec4<f32>`   (16 bytes)
/// - color: `vec4<f32>`           (16 bytes)
/// - color2: `vec4<f32>`          (16 bytes)
/// - border: `vec4<f32>`          (16 bytes)
/// - border_color: `vec4<f32>`    (16 bytes)
/// - shadow: `vec4<f32>`          (16 bytes)
/// - shadow_color: `vec4<f32>`    (16 bytes)
/// - clip_bounds: `vec4<f32>`     (16 bytes) - clip region (x, y, width, height)
/// - clip_radius: `vec4<f32>`     (16 bytes) - clip corner radii or circle/ellipse radii
/// - gradient_params: `vec4<f32>` (16 bytes) - gradient direction (x1, y1, x2, y2) or (cx, cy, r, 0)
/// - type_info: `vec4<u32>`       (16 bytes) - (primitive_type, fill_type, clip_type, 0)
/// Total: 192 bytes
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
    /// Gradient parameters: linear (x1, y1, x2, y2), radial (cx, cy, r, 0)
    pub gradient_params: [f32; 4],
    /// Type info (primitive_type, fill_type, clip_type, z_layer)
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
            // Default gradient: horizontal (0,0) to (1,0)
            gradient_params: [0.0, 0.0, 1.0, 0.0],
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

    /// Set the z-layer for interleaved rendering
    ///
    /// Z-layers control the order in which primitives and text are rendered
    /// together. All primitives and text with the same z-layer are rendered
    /// before moving to the next z-layer.
    pub fn with_z_layer(mut self, layer: u32) -> Self {
        self.type_info[3] = layer;
        self
    }

    /// Get the z-layer of this primitive
    pub fn z_layer(&self) -> u32 {
        self.type_info[3]
    }

    /// Set the z-layer in place
    pub fn set_z_layer(&mut self, layer: u32) {
        self.type_info[3] = layer;
    }

    /// Create a text glyph primitive from a GpuGlyph
    ///
    /// This converts a text glyph into a unified primitive that can be rendered
    /// in the same pass as shapes, enabling proper z-ordering.
    ///
    /// The glyph's UV bounds are stored in `gradient_params` and the color in `color`.
    /// For color emoji, `flags[0]` is 1.0 (stored in `type_info[1]`).
    pub fn from_glyph(glyph: &GpuGlyph) -> Self {
        // Use type_info[1] to store is_color flag (1 = color emoji, 0 = grayscale)
        let is_color_flag = if glyph.flags[0] > 0.5 { 1u32 } else { 0u32 };
        Self {
            bounds: glyph.bounds,
            corner_radius: [0.0; 4],
            color: glyph.color,
            color2: [0.0; 4],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds: glyph.clip_bounds,
            clip_radius: [0.0; 4],
            // Store UV bounds (u_min, v_min, u_max, v_max) in gradient_params
            gradient_params: glyph.uv_bounds,
            type_info: [
                PrimitiveType::Text as u32,
                is_color_flag,
                ClipType::None as u32,
                0,
            ],
        }
    }

    /// Create a text glyph primitive with explicit parameters
    pub fn text_glyph(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        uv_min_x: f32,
        uv_min_y: f32,
        uv_max_x: f32,
        uv_max_y: f32,
        color: [f32; 4],
    ) -> Self {
        Self {
            bounds: [x, y, width, height],
            corner_radius: [0.0; 4],
            color,
            color2: [0.0; 4],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
            clip_radius: [0.0; 4],
            // Store UV bounds in gradient_params
            gradient_params: [uv_min_x, uv_min_y, uv_max_x, uv_max_y],
            type_info: [PrimitiveType::Text as u32, 0, ClipType::None as u32, 0],
        }
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
/// - clip_bounds: `vec4<f32>`   (16 bytes)
/// - clip_radius: `vec4<f32>`   (16 bytes)
/// Total: 128 bytes
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
    /// Type info (glass_type, shadow_offset_x_bits, shadow_offset_y_bits, clip_type)
    pub type_info: [u32; 4],
    /// Clip bounds (x, y, width, height) for clipping blur samples
    pub clip_bounds: [f32; 4],
    /// Clip corner radii (for rounded rect clips)
    pub clip_radius: [f32; 4],
}

impl Default for GpuGlassPrimitive {
    fn default() -> Self {
        Self {
            bounds: [0.0, 0.0, 100.0, 100.0],
            corner_radius: [0.0; 4],
            tint_color: [1.0, 1.0, 1.0, 0.1], // Subtle white tint
            params: [20.0, 1.0, 1.0, 0.0],    // blur=20, saturation=1, brightness=1, noise=0
            // border_thickness=0.8, light_angle=-PI/4 (top-left, -45 degrees)
            params2: [0.8, -std::f32::consts::FRAC_PI_4, 0.0, 0.0],
            type_info: [GlassType::Regular as u32, 0, 0, ClipType::None as u32],
            // No clip by default (very large bounds)
            clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
            clip_radius: [0.0; 4],
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

    /// Create a glass circle (uses rounded rect with max radius)
    pub fn circle(center_x: f32, center_y: f32, radius: f32) -> Self {
        let diameter = radius * 2.0;
        Self {
            bounds: [center_x - radius, center_y - radius, diameter, diameter],
            corner_radius: [radius; 4],
            ..Default::default()
        }
    }

    /// Set uniform corner radius
    pub fn with_corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = [radius; 4];
        self
    }

    /// Set per-corner radius (top-left, top-right, bottom-right, bottom-left)
    pub fn with_corner_radius_per_corner(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.corner_radius = [tl, tr, br, bl];
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
    /// Note: Offset is stored in type_info\[1\] and type_info\[2\] as bits
    pub fn with_shadow_offset(
        mut self,
        blur: f32,
        opacity: f32,
        offset_x: f32,
        offset_y: f32,
    ) -> Self {
        self.params2[2] = blur;
        self.params2[3] = opacity;
        // Store offset in type_info (as f32 bits reinterpreted as u32)
        self.type_info[1] = offset_x.to_bits();
        self.type_info[2] = offset_y.to_bits();
        self
    }

    /// Set refraction strength multiplier (0.0 = no refraction/flat, 1.0 = full refraction)
    /// Use this to create flat blurred backgrounds without the edge bending effect
    pub fn with_refraction(mut self, strength: f32) -> Self {
        // Store as negative to signal "explicitly set" (shader checks sign bit)
        // The shader will use abs() to get the actual value
        self.type_info[3] = (-strength).to_bits();
        self
    }

    /// Disable refraction for a flat blurred background (no edge bending)
    pub fn flat(mut self) -> Self {
        // Store -0.0 which has sign bit set but value 0
        self.type_info[3] = (-0.0_f32).to_bits();
        self
    }

    /// Set rectangular clip region for blur sampling
    ///
    /// Blur samples outside this region will be clamped to the edge,
    /// preventing blur from bleeding outside scroll containers.
    pub fn with_clip_rect(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.clip_bounds = [x, y, width, height];
        self.clip_radius = [0.0; 4];
        self
    }

    /// Set rounded rectangular clip region
    pub fn with_clip_rounded_rect(
        mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
    ) -> Self {
        self.clip_bounds = [x, y, width, height];
        self.clip_radius = [radius; 4];
        self
    }

    /// Set rounded rectangular clip region with per-corner radii
    pub fn with_clip_rounded_rect_per_corner(
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
        self
    }

    /// Clear clip region (no clipping)
    pub fn with_no_clip(mut self) -> Self {
        self.clip_bounds = [-10000.0, -10000.0, 100000.0, 100000.0];
        self.clip_radius = [0.0; 4];
        self
    }
}

/// Convert a layout GlassPanel to GPU primitive
///
/// This bridges the layout system's material definitions to the GPU rendering system.
impl From<&blinc_layout::GlassPanel> for GpuGlassPrimitive {
    fn from(panel: &blinc_layout::GlassPanel) -> Self {
        let mat = &panel.material;
        let bounds = &panel.bounds;
        let cr = &panel.corner_radius;

        let mut glass = GpuGlassPrimitive::new(bounds.x, bounds.y, bounds.width, bounds.height)
            .with_corner_radius_per_corner(
                cr.top_left,
                cr.top_right,
                cr.bottom_right,
                cr.bottom_left,
            )
            .with_blur(mat.blur)
            .with_tint(mat.tint.r, mat.tint.g, mat.tint.b, mat.tint.a)
            .with_saturation(mat.saturation)
            .with_brightness(mat.brightness)
            .with_noise(mat.noise)
            .with_border_thickness(mat.border_thickness);

        // Apply shadow if present
        if let Some(ref shadow) = mat.shadow {
            glass = glass.with_shadow_offset(
                shadow.blur,
                shadow.opacity,
                shadow.offset.0,
                shadow.offset.1,
            );
        }

        glass
    }
}

/// A GPU text glyph instance (matches shader `GlyphInstance` struct)
///
/// Memory layout:
/// - bounds: `vec4<f32>`       (16 bytes) - position and size
/// - uv_bounds: `vec4<f32>`    (16 bytes) - UV coordinates in atlas
/// - color: `vec4<f32>`        (16 bytes) - text color
/// - clip_bounds: `vec4<f32>`  (16 bytes) - clip region (x, y, width, height)
/// Total: 80 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuGlyph {
    /// Position and size (x, y, width, height)
    pub bounds: [f32; 4],
    /// UV coordinates in atlas (u_min, v_min, u_max, v_max)
    pub uv_bounds: [f32; 4],
    /// Text color (RGBA)
    pub color: [f32; 4],
    /// Clip bounds (x, y, width, height) - set to large values for no clip
    pub clip_bounds: [f32; 4],
    /// Flags: [is_color, unused, unused, unused]
    /// is_color: 1.0 for color emoji (use color atlas), 0.0 for grayscale (use main atlas)
    pub flags: [f32; 4],
}

impl Default for GpuGlyph {
    fn default() -> Self {
        Self {
            bounds: [0.0; 4],
            uv_bounds: [0.0, 0.0, 1.0, 1.0],
            color: [0.0, 0.0, 0.0, 1.0],
            // Default: no clip (large bounds that won't clip anything)
            clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
            flags: [0.0; 4], // Not a color glyph by default
        }
    }
}

impl GpuGlyph {
    /// Set rectangular clip bounds for this glyph
    pub fn with_clip_rect(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.clip_bounds = [x, y, width, height];
        self
    }

    /// Clear clip bounds (no clipping)
    pub fn with_no_clip(mut self) -> Self {
        self.clip_bounds = [-10000.0, -10000.0, 100000.0, 100000.0];
        self
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

/// Uniform buffer for layer composition
///
/// Layout matches LAYER_COMPOSITE_SHADER uniforms:
/// - source_rect: `vec4<f32>` (16 bytes) - Source rectangle in layer texture (normalized 0-1)
/// - dest_rect: `vec4<f32>` (16 bytes) - Destination rectangle in viewport (pixels)
/// - viewport_size: `vec2<f32>` (8 bytes)
/// - opacity: f32 (4 bytes)
/// - blend_mode: u32 (4 bytes)
/// - clip_bounds: `vec4<f32>` (16 bytes) - Clip region (x, y, width, height)
/// - clip_radius: `vec4<f32>` (16 bytes) - Corner radii (tl, tr, br, bl)
/// - clip_type: u32 (4 bytes) - 0=none, 1=rect
/// - _pad: 28 bytes (12 bytes alignment + 16 bytes for vec3 stored as vec4)
/// Total: 112 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LayerCompositeUniforms {
    /// Source rectangle in layer texture (x, y, width, height) - normalized 0-1
    pub source_rect: [f32; 4],
    /// Destination rectangle in viewport (x, y, width, height) - pixels
    pub dest_rect: [f32; 4],
    /// Viewport size for coordinate conversion
    pub viewport_size: [f32; 2],
    /// Layer opacity (0.0 - 1.0)
    pub opacity: f32,
    /// Blend mode (see BlendMode enum)
    pub blend_mode: u32,
    /// Clip bounds (x, y, width, height) in pixels
    pub clip_bounds: [f32; 4],
    /// Clip corner radii (top-left, top-right, bottom-right, bottom-left)
    pub clip_radius: [f32; 4],
    /// Clip type: 0=none, 1=rect with optional rounded corners
    pub clip_type: u32,
    /// Padding (12 bytes to align vec3 to 16, then 16 bytes for vec3 stored as vec4)
    pub _pad: [f32; 7],
}

impl LayerCompositeUniforms {
    /// Create uniforms for full-layer composition at a specific position
    pub fn new(
        layer_size: (u32, u32),
        dest_x: f32,
        dest_y: f32,
        viewport_size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) -> Self {
        Self {
            source_rect: [0.0, 0.0, 1.0, 1.0], // Full texture
            dest_rect: [dest_x, dest_y, layer_size.0 as f32, layer_size.1 as f32],
            viewport_size: [viewport_size.0, viewport_size.1],
            opacity,
            blend_mode: blend_mode as u32,
            clip_bounds: [0.0, 0.0, viewport_size.0, viewport_size.1], // No clipping
            clip_radius: [0.0, 0.0, 0.0, 0.0],
            clip_type: 0, // No clip
            _pad: [0.0; 7],
        }
    }

    /// Create uniforms for sub-region composition
    pub fn with_source_rect(
        source_rect: [f32; 4],
        dest_rect: [f32; 4],
        viewport_size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) -> Self {
        Self {
            source_rect,
            dest_rect,
            viewport_size: [viewport_size.0, viewport_size.1],
            opacity,
            blend_mode: blend_mode as u32,
            clip_bounds: [0.0, 0.0, viewport_size.0, viewport_size.1], // No clipping
            clip_radius: [0.0, 0.0, 0.0, 0.0],
            clip_type: 0, // No clip
            _pad: [0.0; 7],
        }
    }

    /// Set clip region with optional rounded corners
    pub fn with_clip(
        mut self,
        bounds: [f32; 4],
        radius: [f32; 4],
    ) -> Self {
        self.clip_bounds = bounds;
        self.clip_radius = radius;
        self.clip_type = 1;
        self
    }
}

/// Uniform buffer for path rendering
/// Layout matches shader struct exactly:
/// - viewport_size: `vec2<f32>` (8 bytes)
/// - opacity: f32 (4 bytes)
/// - _pad0: f32 (4 bytes)
/// - transform_row0: `vec4<f32>` (16 bytes)
/// - transform_row1: `vec4<f32>` (16 bytes)
/// - transform_row2: `vec4<f32>` (16 bytes)
/// - clip_bounds: `vec4<f32>` (16 bytes) - (x, y, width, height) or (cx, cy, rx, ry)
/// - clip_radius: `vec4<f32>` (16 bytes) - corner radii (tl, tr, br, bl) or (rx, ry, 0, 0)
/// - clip_type: `u32` (4 bytes) - 0=none, 1=rect, 2=circle, 3=ellipse
/// - use_gradient_texture: `u32` (4 bytes) - 0=use vertex colors, 1=sample gradient texture
/// - use_image_texture: `u32` (4 bytes) - 0=no image, 1=sample image texture
/// - use_glass_effect: `u32` (4 bytes) - 0=no glass, 1=glass effect on path
/// - image_uv_bounds: `vec4<f32>` (16 bytes) - (u_min, v_min, u_max, v_max) for image UV mapping
/// - glass_params: `vec4<f32>` (16 bytes) - (blur_radius, saturation, tint_strength, opacity)
/// - glass_tint: `vec4<f32>` (16 bytes) - glass tint color RGBA
/// Total: 160 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PathUniforms {
    pub viewport_size: [f32; 2],
    pub opacity: f32,
    pub _pad0: f32,
    /// 3x3 transform matrix stored as 3 vec4s (xyz used, w is padding)
    pub transform: [[f32; 4]; 3],
    /// Clip bounds: (x, y, width, height) for rects, (cx, cy, rx, ry) for circles/ellipses
    pub clip_bounds: [f32; 4],
    /// Clip corner radii for rounded rects: (top-left, top-right, bottom-right, bottom-left)
    /// For circles/ellipses: (rx, ry, 0, 0)
    pub clip_radius: [f32; 4],
    /// Clip type: 0=none, 1=rect, 2=circle, 3=ellipse
    pub clip_type: u32,
    /// Use gradient texture: 0=use vertex colors (2-stop fast path), 1=sample gradient texture
    pub use_gradient_texture: u32,
    /// Use image texture: 0=no image, 1=sample image texture for brush
    pub use_image_texture: u32,
    /// Use glass effect: 0=no glass, 1=glass effect on path (requires mask texture)
    pub use_glass_effect: u32,
    /// Image UV bounds for mapping: (u_min, v_min, u_max, v_max)
    pub image_uv_bounds: [f32; 4],
    /// Glass parameters: (blur_radius, saturation, tint_strength, opacity)
    pub glass_params: [f32; 4],
    /// Glass tint color (RGBA)
    pub glass_tint: [f32; 4],
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
            // Default: no clipping (huge bounds)
            clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
            clip_radius: [0.0; 4],
            clip_type: ClipType::None as u32,
            use_gradient_texture: 0, // Default: use vertex colors (2-stop fast path)
            use_image_texture: 0,    // Default: no image texture
            use_glass_effect: 0,     // Default: no glass effect
            image_uv_bounds: [0.0, 0.0, 1.0, 1.0], // Default: full UV range
            glass_params: [20.0, 1.0, 0.5, 0.9], // Default: blur=20, sat=1, tint=0.5, opacity=0.9
            glass_tint: [1.0, 1.0, 1.0, 0.3], // Default: white with 30% alpha
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layer Effect Uniforms
// ─────────────────────────────────────────────────────────────────────────────

/// Uniforms for Kawase blur shader
///
/// Memory layout (32 bytes total, padded to 16-byte alignment):
/// - texel_size: `vec2<f32>` (8 bytes) - inverse texture size (1/width, 1/height)
/// - radius: `f32` (4 bytes) - blur radius
/// - iteration: `u32` (4 bytes) - current pass iteration
/// - blur_alpha: `u32` (4 bytes) - whether to blur alpha (1) or preserve it (0)
/// - _pad1, _pad2, _pad3: `f32` (12 bytes) - padding to 32 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlurUniforms {
    /// Inverse texture size (1/width, 1/height)
    pub texel_size: [f32; 2],
    /// Blur radius
    pub radius: f32,
    /// Current iteration (0, 1, 2, ...) for multi-pass blur
    pub iteration: u32,
    /// Whether to blur alpha channel (1 = blur alpha, 0 = preserve original alpha)
    /// Use blur_alpha=0 for element blur (preserves corner radius)
    /// Use blur_alpha=1 for shadow blur (creates soft edges)
    pub blur_alpha: u32,
    /// Padding for 16-byte alignment
    pub _pad1: f32,
    pub _pad2: f32,
    pub _pad3: f32,
}

/// Uniforms for color matrix shader
///
/// Memory layout (80 bytes total):
/// - row0-row3: 4 x `vec4<f32>` (64 bytes) - 4x4 matrix rows
/// - offset: `vec4<f32>` (16 bytes) - offset/bias for each channel
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorMatrixUniforms {
    /// Row 0: [m0, m1, m2, m3] - R coefficients
    pub row0: [f32; 4],
    /// Row 1: [m5, m6, m7, m8] - G coefficients
    pub row1: [f32; 4],
    /// Row 2: [m10, m11, m12, m13] - B coefficients
    pub row2: [f32; 4],
    /// Row 3: [m15, m16, m17, m18] - A coefficients
    pub row3: [f32; 4],
    /// Offset: [m4, m9, m14, m19] - bias for R, G, B, A
    pub offset: [f32; 4],
}

impl Default for ColorMatrixUniforms {
    fn default() -> Self {
        // Identity matrix (no color transformation)
        Self {
            row0: [1.0, 0.0, 0.0, 0.0],
            row1: [0.0, 1.0, 0.0, 0.0],
            row2: [0.0, 0.0, 1.0, 0.0],
            row3: [0.0, 0.0, 0.0, 1.0],
            offset: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

impl ColorMatrixUniforms {
    /// Create from a 4x5 matrix (row-major, 20 elements)
    pub fn from_matrix(matrix: &[f32; 20]) -> Self {
        Self {
            row0: [matrix[0], matrix[1], matrix[2], matrix[3]],
            row1: [matrix[5], matrix[6], matrix[7], matrix[8]],
            row2: [matrix[10], matrix[11], matrix[12], matrix[13]],
            row3: [matrix[15], matrix[16], matrix[17], matrix[18]],
            offset: [matrix[4], matrix[9], matrix[14], matrix[19]],
        }
    }
}

/// Uniforms for drop shadow shader
///
/// Memory layout (48 bytes total):
/// - offset: `vec2<f32>` (8 bytes) - shadow offset in pixels
/// - blur_radius: `f32` (4 bytes) - blur radius
/// - spread: `f32` (4 bytes) - spread (expand/contract)
/// - color: `vec4<f32>` (16 bytes) - shadow color RGBA
/// - texel_size: `vec2<f32>` (8 bytes) - inverse texture size
/// - _pad: `vec2<f32>` (8 bytes) - padding for alignment
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DropShadowUniforms {
    /// Shadow offset in pixels (x, y)
    pub offset: [f32; 2],
    /// Blur radius
    pub blur_radius: f32,
    /// Spread (positive expands, negative contracts)
    pub spread: f32,
    /// Shadow color (RGBA)
    pub color: [f32; 4],
    /// Inverse texture size (1/width, 1/height)
    pub texel_size: [f32; 2],
    /// Padding for 16-byte alignment
    pub _pad: [f32; 2],
}

impl Default for DropShadowUniforms {
    fn default() -> Self {
        Self {
            offset: [4.0, 4.0],
            blur_radius: 8.0,
            spread: 0.0,
            color: [0.0, 0.0, 0.0, 0.5], // 50% black
            texel_size: [1.0 / 800.0, 1.0 / 600.0],
            _pad: [0.0, 0.0],
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
    /// Clip bounds for this batch: (x, y, width, height) or (cx, cy, rx, ry)
    pub clip_bounds: [f32; 4],
    /// Clip corner radii for this batch
    pub clip_radius: [f32; 4],
    /// Clip type for this batch: 0=none, 1=rect, 2=circle, 3=ellipse
    pub clip_type: u32,
    /// Whether to use gradient texture (for >2 stop gradients)
    pub use_gradient_texture: bool,
    /// Gradient stops for texture rasterization (when use_gradient_texture is true)
    pub gradient_stops: Option<Vec<blinc_core::GradientStop>>,
    /// Whether to use image texture
    pub use_image_texture: bool,
    /// Image source path for image brush (None if not using image)
    pub image_source: Option<String>,
    /// Image UV bounds: (u_min, v_min, u_max, v_max)
    pub image_uv_bounds: [f32; 4],
    /// Whether to use glass effect
    pub use_glass_effect: bool,
    /// Glass parameters: (blur_radius, saturation, tint_strength, opacity)
    pub glass_params: [f32; 4],
    /// Glass tint color (RGBA)
    pub glass_tint: [f32; 4],
}

/// Commands for layer operations during rendering
///
/// These commands are recorded during painting and executed by the renderer
/// to manage offscreen render targets and layer composition.
#[derive(Clone, Debug)]
pub enum LayerCommand {
    /// Push a new layer - begins rendering to an offscreen target
    Push {
        /// Layer configuration
        config: blinc_core::LayerConfig,
    },
    /// Pop the current layer - composite it back to the parent
    Pop,
    /// Sample from a named layer's texture into the current target
    Sample {
        /// ID of the layer to sample from
        id: blinc_core::LayerId,
        /// Source rectangle in the layer's texture (in pixels)
        source: blinc_core::Rect,
        /// Destination rectangle in the current target (in pixels)
        dest: blinc_core::Rect,
    },
}

/// A recorded layer command with its primitive index
#[derive(Clone, Debug)]
pub struct LayerCommandEntry {
    /// The primitive index when this command was recorded
    pub primitive_index: usize,
    /// The layer command
    pub command: LayerCommand,
}

/// Batch of GPU primitives for efficient rendering
pub struct PrimitiveBatch {
    /// Background primitives (rendered before glass)
    pub primitives: Vec<GpuPrimitive>,
    /// Foreground primitives (rendered after glass)
    pub foreground_primitives: Vec<GpuPrimitive>,
    pub glass_primitives: Vec<GpuGlassPrimitive>,
    pub glyphs: Vec<GpuGlyph>,
    /// Tessellated path geometry
    pub paths: PathBatch,
    /// Foreground tessellated path geometry
    pub foreground_paths: PathBatch,
    /// Layer commands for offscreen rendering and composition
    pub layer_commands: Vec<LayerCommandEntry>,
}

impl PrimitiveBatch {
    pub fn new() -> Self {
        Self {
            primitives: Vec::new(),
            foreground_primitives: Vec::new(),
            glass_primitives: Vec::new(),
            glyphs: Vec::new(),
            paths: PathBatch::default(),
            foreground_paths: PathBatch::default(),
            layer_commands: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.primitives.clear();
        self.foreground_primitives.clear();
        self.glass_primitives.clear();
        self.glyphs.clear();
        self.paths = PathBatch::default();
        self.foreground_paths = PathBatch::default();
        self.layer_commands.clear();
    }

    /// Record a layer command at the current primitive index
    pub fn push_layer_command(&mut self, command: LayerCommand) {
        self.layer_commands.push(LayerCommandEntry {
            primitive_index: self.primitives.len(),
            command,
        });
    }

    /// Check if there are any layer commands recorded
    pub fn has_layer_commands(&self) -> bool {
        !self.layer_commands.is_empty()
    }

    /// Check if there are any layer commands with effects
    pub fn has_layer_effects(&self) -> bool {
        self.layer_commands.iter().any(|entry| {
            if let LayerCommand::Push { config } = &entry.command {
                !config.effects.is_empty()
            } else {
                false
            }
        })
    }

    pub fn push(&mut self, primitive: GpuPrimitive) {
        self.primitives.push(primitive);
    }

    /// Push a primitive to the foreground layer (rendered after glass)
    pub fn push_foreground(&mut self, primitive: GpuPrimitive) {
        self.foreground_primitives.push(primitive);
    }

    pub fn push_glass(&mut self, glass: GpuGlassPrimitive) {
        self.glass_primitives.push(glass);
    }

    pub fn push_glyph(&mut self, glyph: GpuGlyph) {
        self.glyphs.push(glyph);
    }

    /// Convert a glyph to a primitive and add it to the foreground primitives
    ///
    /// This enables unified rendering of text and SDF shapes in the same pass,
    /// ensuring transforms are applied consistently during animations.
    pub fn push_glyph_as_primitive(&mut self, glyph: GpuGlyph) {
        self.foreground_primitives
            .push(GpuPrimitive::from_glyph(&glyph));
    }

    /// Convert all accumulated glyphs to foreground primitives
    ///
    /// This should be called before rendering to enable unified text/SDF rendering.
    /// After calling this, the glyphs vector will be empty and all text will be
    /// rendered as SDF primitives.
    pub fn convert_glyphs_to_primitives(&mut self) {
        for glyph in self.glyphs.drain(..) {
            self.foreground_primitives
                .push(GpuPrimitive::from_glyph(&glyph));
        }
    }

    /// Get the combined primitives including converted glyphs for unified rendering
    ///
    /// Returns a vector of all foreground primitives plus glyphs converted to primitives.
    /// This is useful for unified rendering without modifying the batch state.
    pub fn get_unified_foreground_primitives(&self) -> Vec<GpuPrimitive> {
        let mut result = self.foreground_primitives.clone();
        for glyph in &self.glyphs {
            result.push(GpuPrimitive::from_glyph(glyph));
        }
        result
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

    /// Add tessellated path geometry to the foreground batch
    pub fn push_foreground_path(&mut self, tessellated: crate::path::TessellatedPath) {
        if tessellated.is_empty() {
            return;
        }
        let base_vertex = self.foreground_paths.vertices.len() as u32;
        self.foreground_paths.vertices.extend(tessellated.vertices);
        self.foreground_paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    /// Add tessellated path geometry with clip data to the batch
    pub fn push_path_with_clip(
        &mut self,
        tessellated: crate::path::TessellatedPath,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_type: ClipType,
    ) {
        if tessellated.is_empty() {
            return;
        }
        // Update clip data on the batch (last path's clip wins)
        self.paths.clip_bounds = clip_bounds;
        self.paths.clip_radius = clip_radius;
        self.paths.clip_type = clip_type as u32;

        // Offset indices by current vertex count
        let base_vertex = self.paths.vertices.len() as u32;
        self.paths.vertices.extend(tessellated.vertices);
        self.paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    /// Add tessellated path geometry with clip data to the foreground batch
    pub fn push_foreground_path_with_clip(
        &mut self,
        tessellated: crate::path::TessellatedPath,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_type: ClipType,
    ) {
        if tessellated.is_empty() {
            return;
        }
        // Update clip data on the batch (last path's clip wins)
        self.foreground_paths.clip_bounds = clip_bounds;
        self.foreground_paths.clip_radius = clip_radius;
        self.foreground_paths.clip_type = clip_type as u32;

        let base_vertex = self.foreground_paths.vertices.len() as u32;
        self.foreground_paths.vertices.extend(tessellated.vertices);
        self.foreground_paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    /// Add tessellated path geometry with clip data and brush info to the batch
    pub fn push_path_with_brush_info(
        &mut self,
        tessellated: crate::path::TessellatedPath,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_type: ClipType,
        brush_info: &crate::path::PathBrushInfo,
    ) {
        if tessellated.is_empty() {
            return;
        }

        // Update clip data
        self.paths.clip_bounds = clip_bounds;
        self.paths.clip_radius = clip_radius;
        self.paths.clip_type = clip_type as u32;

        // Update brush metadata
        self.paths.use_gradient_texture = brush_info.needs_gradient_texture;
        self.paths.gradient_stops = brush_info.gradient_stops.clone();
        self.paths.use_image_texture =
            matches!(brush_info.brush_type, crate::path::PathBrushType::Image);
        self.paths.image_source = brush_info.image_source.clone();
        self.paths.image_uv_bounds = [0.0, 0.0, 1.0, 1.0]; // Default full UV range
        self.paths.use_glass_effect =
            matches!(brush_info.brush_type, crate::path::PathBrushType::Glass);
        self.paths.glass_params = brush_info.glass_params;
        self.paths.glass_tint = [
            brush_info.glass_tint.r,
            brush_info.glass_tint.g,
            brush_info.glass_tint.b,
            brush_info.glass_tint.a,
        ];

        // Offset indices by current vertex count
        let base_vertex = self.paths.vertices.len() as u32;
        self.paths.vertices.extend(tessellated.vertices);
        self.paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    /// Add tessellated path geometry with clip data and brush info to the foreground batch
    pub fn push_foreground_path_with_brush_info(
        &mut self,
        tessellated: crate::path::TessellatedPath,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_type: ClipType,
        brush_info: &crate::path::PathBrushInfo,
    ) {
        if tessellated.is_empty() {
            return;
        }

        // Update clip data
        self.foreground_paths.clip_bounds = clip_bounds;
        self.foreground_paths.clip_radius = clip_radius;
        self.foreground_paths.clip_type = clip_type as u32;

        // Update brush metadata
        self.foreground_paths.use_gradient_texture = brush_info.needs_gradient_texture;
        self.foreground_paths.gradient_stops = brush_info.gradient_stops.clone();
        self.foreground_paths.use_image_texture =
            matches!(brush_info.brush_type, crate::path::PathBrushType::Image);
        self.foreground_paths.image_source = brush_info.image_source.clone();
        self.foreground_paths.image_uv_bounds = [0.0, 0.0, 1.0, 1.0];
        self.foreground_paths.use_glass_effect =
            matches!(brush_info.brush_type, crate::path::PathBrushType::Glass);
        self.foreground_paths.glass_params = brush_info.glass_params;
        self.foreground_paths.glass_tint = [
            brush_info.glass_tint.r,
            brush_info.glass_tint.g,
            brush_info.glass_tint.b,
            brush_info.glass_tint.a,
        ];

        let base_vertex = self.foreground_paths.vertices.len() as u32;
        self.foreground_paths.vertices.extend(tessellated.vertices);
        self.foreground_paths
            .indices
            .extend(tessellated.indices.iter().map(|i| i + base_vertex));
    }

    pub fn is_empty(&self) -> bool {
        self.primitives.is_empty()
            && self.foreground_primitives.is_empty()
            && self.glass_primitives.is_empty()
            && self.glyphs.is_empty()
            && self.paths.vertices.is_empty()
            && self.foreground_paths.vertices.is_empty()
    }

    pub fn path_vertex_count(&self) -> usize {
        self.paths.vertices.len()
    }

    pub fn path_index_count(&self) -> usize {
        self.paths.indices.len()
    }

    pub fn foreground_path_vertex_count(&self) -> usize {
        self.foreground_paths.vertices.len()
    }

    pub fn foreground_path_index_count(&self) -> usize {
        self.foreground_paths.indices.len()
    }

    pub fn primitive_count(&self) -> usize {
        self.primitives.len()
    }

    pub fn foreground_primitive_count(&self) -> usize {
        self.foreground_primitives.len()
    }

    pub fn glass_count(&self) -> usize {
        self.glass_primitives.len()
    }

    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// Get the maximum z_layer used by primitives in this batch
    pub fn max_z_layer(&self) -> u32 {
        self.primitives
            .iter()
            .chain(self.foreground_primitives.iter())
            .map(|p| p.z_layer())
            .max()
            .unwrap_or(0)
    }

    /// Filter primitives by z_layer, returning a new batch with only matching primitives
    pub fn primitives_for_layer(&self, z_layer: u32) -> Vec<GpuPrimitive> {
        self.primitives
            .iter()
            .filter(|p| p.z_layer() == z_layer)
            .cloned()
            .collect()
    }

    /// Filter foreground primitives by z_layer
    pub fn foreground_primitives_for_layer(&self, z_layer: u32) -> Vec<GpuPrimitive> {
        self.foreground_primitives
            .iter()
            .filter(|p| p.z_layer() == z_layer)
            .cloned()
            .collect()
    }

    /// Check if any primitives use z_layer (i.e., max_z_layer > 0)
    pub fn has_z_layers(&self) -> bool {
        self.max_z_layer() > 0
    }

    /// Merge another batch into this one
    ///
    /// Useful for combining batches from different paint contexts.
    pub fn merge(&mut self, other: PrimitiveBatch) {
        // Record the current primitive count for offsetting layer commands
        let primitive_offset = self.primitives.len();

        self.primitives.extend(other.primitives);
        self.foreground_primitives
            .extend(other.foreground_primitives);
        self.glass_primitives.extend(other.glass_primitives);
        self.glyphs.extend(other.glyphs);

        // Merge paths with index offset
        let base_vertex = self.paths.vertices.len() as u32;
        self.paths.vertices.extend(other.paths.vertices);
        self.paths
            .indices
            .extend(other.paths.indices.iter().map(|i| i + base_vertex));

        // Merge foreground paths
        let fg_base_vertex = self.foreground_paths.vertices.len() as u32;
        self.foreground_paths
            .vertices
            .extend(other.foreground_paths.vertices);
        self.foreground_paths.indices.extend(
            other
                .foreground_paths
                .indices
                .iter()
                .map(|i| i + fg_base_vertex),
        );

        // Merge layer commands with offset primitive indices
        for mut entry in other.layer_commands {
            entry.primitive_index += primitive_offset;
            self.layer_commands.push(entry);
        }
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
