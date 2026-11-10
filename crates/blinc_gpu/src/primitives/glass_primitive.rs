//! Glass instances and the 3D group shape descriptor.

use super::{ClipType, GlassType};

/// Max shapes per 3D group (practical limit for uniform/storage buffer)
pub const MAX_GROUP_SHAPES: usize = 16;

/// Per-shape descriptor for 3D group composition (64 bytes, 4 vec4s)
///
/// Used when `shape-3d: group` combines multiple child shapes into a single
/// compound SDF via boolean operations.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShapeDesc {
    /// (offset_x, offset_y, offset_z, corner_radius) — relative to group center
    pub offset: [f32; 4],
    /// (shape_type, depth, op_type, blend_radius)
    /// shape_type: 1=box, 2=sphere, 3=cylinder, 4=torus, 5=capsule
    /// op_type: 0=union, 1=subtract, 2=intersect, 3=smooth-union, 4=smooth-subtract, 5=smooth-intersect
    pub params: [f32; 4],
    /// (half_w, half_h, half_d, 0)
    pub half_ext: [f32; 4],
    /// (r, g, b, a) — per-shape color
    pub color: [f32; 4],
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
/// - border_color: `vec4<f32>`  (16 bytes)
///   Total: 144 bytes
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
    /// Rounded clip radii, or [path_header, 0, 0, -1] for a prepared shape clip.
    pub clip_radius: [f32; 4],
    /// Border color (RGBA) - when alpha > 0, renders a solid border instead of light-based highlights
    pub border_color: [f32; 4],
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
            border_color: [0.0, 0.0, 0.0, 0.0], // Transparent = use light-based highlights
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
    #[allow(clippy::too_many_arguments)]
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

    /// Set border color (RGBA). When alpha > 0, a solid border is rendered
    /// using this color instead of the default light-based edge highlights.
    pub fn with_border_color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.border_color = [r, g, b, a];
        self
    }
}
