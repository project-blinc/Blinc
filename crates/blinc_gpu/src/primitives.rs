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
    /// Notch: rounded rect with concave corners and optional top/bottom edge
    /// modifiers (scoop, bulge, v-cut, v-peak). SDF-composed in the fragment
    /// shader so notches inherit free AA, transforms, layer clipping, and
    /// shadow from the primary pipeline — no tessellation required.
    ///
    /// Repurposed fields (only valid when prim_type == Notch):
    ///
    /// - `perspective` → top modifier [type, width, height_or_depth, corner_radius]
    /// - `sdf_3d`      → bottom modifier [type, width, height_or_depth, corner_radius]
    /// - `light`       → per-corner type flags [TL, TR, BR, BL]
    ///   (0.0 = sharp/convex, 1.0 = concave)
    ///
    /// Modifier type codes (stored in the `type` slot as f32):
    ///   0 = none   1 = scoop   2 = bulge   3 = v-cut   4 = v-peak
    ///
    /// Notches cannot use the 3D pipeline (they reuse those slots); if you
    /// need 3D rotation on a notched shape, fall back to `Path` for that
    /// draw call.
    Notch = 8,
    /// Mesh triangle. Used to route tessellated path fills through
    /// the SDF primitive stream so their submission order interleaves
    /// naturally with text and other SDF primitives — text rendered
    /// after a shape fill paints on top, matching the caller's draw
    /// order instead of ending up behind every path draw (which is
    /// what the separate tessellated-path pipeline does on its own).
    ///
    /// Each primitive represents ONE triangle. The three corners live
    /// in the shared `aux_data` storage buffer starting at
    /// `border[2]` (the same `aux_offset` slot the 3D compound shape
    /// primitives reuse), packed as:
    ///
    /// - `aux_data[offset + 0] = (v0.x, v0.y, v1.x, v1.y)`
    /// - `aux_data[offset + 1] = (v2.x, v2.y, 0, 0)`
    ///
    /// `bounds` is pre-computed to the triangle AABB so the vertex
    /// shader still emits a tight quad and the fragment shader's
    /// point-in-triangle test only runs over covered pixels.
    Mesh = 9,
}

/// Corner style for notch primitives — matches `blinc_layout::notch::CornerStyle`
/// but lives here so the GPU side can encode it without depending on layout.
pub const NOTCH_CORNER_SHARP_OR_CONVEX: f32 = 0.0;
pub const NOTCH_CORNER_CONCAVE: f32 = 1.0;

/// Top/bottom edge modifier codes for notch primitives.
pub const NOTCH_MOD_NONE: f32 = 0.0;
pub const NOTCH_MOD_SCOOP: f32 = 1.0;
pub const NOTCH_MOD_BULGE: f32 = 2.0;
pub const NOTCH_MOD_CUT: f32 = 3.0;
pub const NOTCH_MOD_PEAK: f32 = 4.0;

/// Pipeline category for multi-pipeline SDF dispatch.
///
/// Primitives are sorted by category before GPU upload so each
/// specialized pipeline draws a contiguous range of instances.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SdfPipelineCategory {
    /// Rect, Circle, Ellipse — 2D shapes with borders, gradients, filters
    Core = 0,
    /// Shadow, InnerShadow, CircleShadow, CircleInnerShadow
    Shadow = 1,
    /// 3D raymarched shapes (shape_type > 0 in perspective.w)
    Sdf3D = 2,
    /// Notch (prim_type 8) — concave corners + edge modifiers
    Notch = 3,
    /// Text (prim_type 7) — handled by TEXT_SHADER pipeline
    Text = 4,
}

impl GpuPrimitive {
    /// Determine which SDF pipeline should render this primitive.
    /// 3D is detected by shape_type in `perspective[3]`, but only promotes
    /// *core* prim types (rect/circle/ellipse). Shadows, text, and notches
    /// stay on their own pipelines even when `perspective[3]` is non-zero —
    /// `current_perspective_params()` bleeds the active element's
    /// `set_3d_shape` state onto every primitive built during that element's
    /// render, and we don't want a shadow to be raymarched as a solid 3D
    /// shape or have sdf_core overwrite it.
    pub fn pipeline_category(&self) -> SdfPipelineCategory {
        let prim_type = self.type_info[0];
        let shape_3d_type = self.perspective[3] as u32;
        match prim_type {
            0..=2 => {
                if shape_3d_type > 0 {
                    SdfPipelineCategory::Sdf3D
                } else {
                    SdfPipelineCategory::Core
                }
            }
            3..=6 => SdfPipelineCategory::Shadow,
            7 => SdfPipelineCategory::Text,
            8 => SdfPipelineCategory::Notch,
            // Mesh triangles share the Core pipeline — they reuse the
            // same fragment-shader branching as rects / circles, just
            // with a point-in-triangle check instead of an SDF.
            9 => SdfPipelineCategory::Core,
            _ => SdfPipelineCategory::Core, // fallback
        }
    }
}

/// Vertex-shader-required fields for the SDF pipeline, packed into an
/// instance-stepped vertex buffer. Used as a fallback when the GPU
/// adapter does not support `VERTEX_STORAGE` (storage buffers in vertex
/// shaders). The fragment shader still reads the full `GpuPrimitive`
/// from the storage buffer — only the vertex stage needs this.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SdfVertexInstance {
    pub bounds: [f32; 4],
    pub shadow: [f32; 4],
    pub rotation: [f32; 4],
    pub perspective: [f32; 4],
    pub sdf_3d: [f32; 4],
    pub local_affine: [f32; 4],
    pub corner_radius: [f32; 4],
    pub light: [f32; 4],
}

impl SdfVertexInstance {
    /// Extract vertex-shader-needed fields from a full primitive.
    pub fn from_primitive(p: &GpuPrimitive) -> Self {
        Self {
            bounds: p.bounds,
            shadow: p.shadow,
            rotation: p.rotation,
            perspective: p.perspective,
            sdf_3d: p.sdf_3d,
            local_affine: p.local_affine,
            corner_radius: p.corner_radius,
            light: p.light,
        }
    }

    /// Vertex buffer layout for instance-stepped SDF vertex data.
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<SdfVertexInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            // @location(0) bounds
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            // @location(1) shadow
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 1,
            },
            // @location(2) rotation
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 2,
            },
            // @location(3) perspective
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 3,
            },
            // @location(4) sdf_3d
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 64,
                shader_location: 4,
            },
            // @location(5) local_affine
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 80,
                shader_location: 5,
            },
            // @location(6) corner_radius
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 96,
                shader_location: 6,
            },
            // @location(7) light
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 112,
                shader_location: 7,
            },
        ],
    };
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
    /// Simple frosted glass - pure blur + tint, no liquid glass effects
    Simple = 5,
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
    /// Polygon clip (winding number test via aux_data vertices)
    Polygon = 4,
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
/// - rotation: `vec4<f32>`        (16 bytes) - (sin_rz, cos_rz, sin_ry, cos_ry)
/// - perspective: `vec4<f32>`     (16 bytes) - (sin_rx, cos_rx, perspective_d, shape_3d_type)
/// - sdf_3d: `vec4<f32>`          (16 bytes) - (depth, ambient, specular_power, translate_z)
/// - light: `vec4<f32>`           (16 bytes) - (dir_x, dir_y, dir_z, intensity)
/// - filter_a: `vec4<f32>`        (16 bytes) - (grayscale, invert, sepia, hue_rotate_rad)
/// - filter_b: `vec4<f32>`        (16 bytes) - (brightness, contrast, saturate, 0)
/// - mask_params: `vec4<f32>`     (16 bytes) - mask gradient geometry (x1,y1,x2,y2 or cx,cy,r,0)
/// - mask_info: `vec4<f32>`       (16 bytes) - (mask_type, start_alpha, end_alpha, 0)
/// - corner_shape: `vec4<f32>`    (16 bytes) - superellipse n per corner (1.0=round, 0.0=bevel, 2.0=squircle)
/// - clip_fade: `vec4<f32>`       (16 bytes) - overflow fade distances (top, right, bottom, left) in pixels
/// - type_info: `vec4<u32>`       (16 bytes) - (primitive_type, fill_type, clip_type, z_layer)
///   Total: 384 bytes (24 x vec4). Asserted against the WGSL
///   declaration in `tests/primitive_layout.rs`.
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
    /// Per-corner clip superellipse `n` parameter (round=1.0, squircle=2.0,
    /// bevel=0.0, scoop=-1.0, square=>=100, notch=<=-100). Default
    /// `[1.0; 4]` keeps the historical circular rounded-rect clip;
    /// non-default values let the SDF clip evaluator follow a
    /// squircle / scoop / bevel curve that matches the parent fill,
    /// so overflow:clip on a squircle parent doesn't leak its bg
    /// through at the diagonal corner where a circular clip would
    /// cut children off short.
    pub clip_corner_shape: [f32; 4],
    /// Gradient parameters: linear (x1, y1, x2, y2), radial (cx, cy, r, 0)
    pub gradient_params: [f32; 4],
    /// Rotation (sin_rz, cos_rz, sin_ry, cos_ry) - pre-computed for GPU efficiency
    pub rotation: [f32; 4],
    /// Local 2x2 affine transform (a, b, c, d) - normalized (DPI-scale removed).
    /// Maps from local rect space to screen space. Used by the fragment shader
    /// to apply inverse transform (supports rotation, scale, AND skew).
    /// Identity = [1, 0, 0, 1].
    pub local_affine: [f32; 4],
    /// Perspective (sin_rx, cos_rx, perspective_d, shape_3d_type)
    /// shape_3d_type: 0=none, 1=box, 2=sphere, 3=cylinder, 4=torus, 5=capsule, 6=group
    pub perspective: [f32; 4],
    /// SDF 3D params (depth, ambient, specular_power, translate_z)
    pub sdf_3d: [f32; 4],
    /// Light params (dir_x, dir_y, dir_z, intensity)
    pub light: [f32; 4],
    /// CSS filter A (grayscale, invert, sepia, hue_rotate_rad)
    pub filter_a: [f32; 4],
    /// CSS filter B (brightness, contrast, saturate, 0)
    pub filter_b: [f32; 4],
    /// Mask gradient params: linear=(x1,y1,x2,y2), radial=(cx,cy,r,0) in pixel coords
    pub mask_params: [f32; 4],
    /// Mask info: [mask_type, start_alpha, end_alpha, 0]
    /// mask_type: 0=none, 1=linear, 2=radial
    pub mask_info: [f32; 4],
    /// Corner shape (superellipse n parameter per corner)
    /// n=1.0 = round (default), n=0.0 = bevel, n=2.0 = squircle, n=-1.0 = scoop
    /// n>=100.0 = square, n<=-100.0 = notch
    pub corner_shape: [f32; 4],
    /// Overflow fade distances (top, right, bottom, left) in pixels.
    /// Non-zero values create a smooth alpha ramp at clip edges.
    pub clip_fade: [f32; 4],
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
            // Default clip corner shape: round (matches the historical
            // circular rounded clip — squircle/scoop/bevel only kicks
            // in when the clip is pushed with a non-round CornerShape).
            clip_corner_shape: [1.0; 4],
            // Default gradient: horizontal (0,0) to (1,0)
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            // No rotation: sin_rz=0, cos_rz=1, sin_ry=0, cos_ry=1
            rotation: [0.0, 1.0, 0.0, 1.0],
            // Identity local affine: a=1, b=0, c=0, d=1
            local_affine: [1.0, 0.0, 0.0, 1.0],
            // No perspective: sin_rx=0, cos_rx=1, persp_d=0, shape=none
            perspective: [0.0, 1.0, 0.0, 0.0],
            // No 3D: depth=0, ambient=0.3, specular=32, unused=0
            sdf_3d: [0.0, 0.3, 32.0, 0.0],
            // Default light: top direction, intensity 0.8
            light: [0.0, -1.0, 0.5, 0.8],
            // Default filter: identity (no effect)
            filter_a: [0.0, 0.0, 0.0, 0.0], // grayscale=0, invert=0, sepia=0, hue_rotate=0
            filter_b: [1.0, 1.0, 1.0, 0.0], // brightness=1, contrast=1, saturate=1, unused=0
            // Default mask: none
            mask_params: [0.0; 4],
            mask_info: [0.0; 4], // mask_type=0 (none)
            // Default corner shape: round (n=1.0)
            corner_shape: [1.0; 4],
            // Default clip fade: none (hard clip)
            clip_fade: [0.0; 4],
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

    /// Set 2D rotation angle (Z-axis) in radians
    pub fn with_rotation(mut self, angle_rad: f32) -> Self {
        self.rotation[0] = angle_rad.sin();
        self.rotation[1] = angle_rad.cos();
        self
    }

    /// Set 3D rotation (X and Y axes) in radians + perspective distance
    pub fn with_3d_rotation(mut self, rx_rad: f32, ry_rad: f32, perspective_d: f32) -> Self {
        self.rotation[2] = ry_rad.sin();
        self.rotation[3] = ry_rad.cos();
        self.perspective[0] = rx_rad.sin();
        self.perspective[1] = rx_rad.cos();
        self.perspective[2] = perspective_d;
        self
    }

    /// Set 3D shape type and depth for SDF raymarching
    /// shape_type: 0=none, 1=box, 2=sphere, 3=cylinder, 4=torus, 5=capsule
    pub fn with_3d_shape(mut self, shape_type: f32, depth: f32) -> Self {
        self.perspective[3] = shape_type;
        self.sdf_3d[0] = depth;
        self
    }

    /// Set 3D lighting parameters
    pub fn with_light(mut self, dir: [f32; 3], intensity: f32) -> Self {
        self.light = [dir[0], dir[1], dir[2], intensity];
        self
    }

    /// Set ambient and specular for 3D rendering
    pub fn with_3d_material(mut self, ambient: f32, specular_power: f32) -> Self {
        self.sdf_3d[1] = ambient;
        self.sdf_3d[2] = specular_power;
        self
    }

    /// Set translate-z offset for 3D elements (positive = toward viewer)
    pub fn with_3d_translate_z(mut self, z: f32) -> Self {
        self.sdf_3d[3] = z;
        self
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
    #[allow(clippy::too_many_arguments)]
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
    #[allow(clippy::too_many_arguments)]
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
    #[allow(clippy::too_many_arguments)]
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
    #[allow(clippy::too_many_arguments)]
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
            clip_corner_shape: [1.0; 4],
            gradient_params: glyph.uv_bounds,
            rotation: [0.0, 1.0, 0.0, 1.0],
            local_affine: [1.0, 0.0, 0.0, 1.0],
            perspective: [0.0, 1.0, 0.0, 0.0],
            sdf_3d: [0.0, 0.3, 32.0, 0.0],
            light: [0.0, -1.0, 0.5, 0.8],
            filter_a: [0.0, 0.0, 0.0, 0.0],
            filter_b: [1.0, 1.0, 1.0, 0.0],
            mask_params: [0.0; 4],
            mask_info: [0.0; 4],
            corner_shape: [1.0; 4],
            clip_fade: glyph.clip_fade,
            type_info: [
                PrimitiveType::Text as u32,
                is_color_flag,
                ClipType::None as u32,
                0,
            ],
        }
    }

    /// Create a text glyph primitive with explicit parameters
    #[allow(clippy::too_many_arguments)]
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
            clip_corner_shape: [1.0; 4],
            gradient_params: [uv_min_x, uv_min_y, uv_max_x, uv_max_y],
            rotation: [0.0, 1.0, 0.0, 1.0],
            local_affine: [1.0, 0.0, 0.0, 1.0],
            perspective: [0.0, 1.0, 0.0, 0.0],
            sdf_3d: [0.0, 0.3, 32.0, 0.0],
            light: [0.0, -1.0, 0.5, 0.8],
            filter_a: [0.0, 0.0, 0.0, 0.0],
            filter_b: [1.0, 1.0, 1.0, 0.0],
            mask_params: [0.0; 4],
            mask_info: [0.0; 4],
            corner_shape: [1.0; 4],
            clip_fade: [0.0; 4],
            type_info: [PrimitiveType::Text as u32, 0, ClipType::None as u32, 0],
        }
    }
}

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
    /// Clip corner radii (for rounded rect clips)
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

/// A GPU text glyph instance (matches shader `GlyphInstance` struct)
///
/// Memory layout:
/// - bounds: `vec4<f32>`       (16 bytes) - position and size
/// - uv_bounds: `vec4<f32>`    (16 bytes) - UV coordinates in atlas
/// - color: `vec4<f32>`        (16 bytes) - text color
/// - clip_bounds: `vec4<f32>`  (16 bytes) - clip region (x, y, width, height)
/// - clip_fade: `vec4<f32>`   (16 bytes) - overflow fade distances (top, right, bottom, left)
///   Total: 96 bytes
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
    /// Overflow fade distances (top, right, bottom, left) in pixels
    pub clip_fade: [f32; 4],
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
            clip_fade: [0.0; 4], // No fade (hard clip)
            flags: [0.0; 4],     // Not a color glyph by default
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
/// - perspective_d: f32 (4 bytes) - Perspective distance (0 = no 3D)
/// - sin_rx: f32 (4 bytes) - sin(rotate-x angle)
/// - cos_rx: f32 (4 bytes) - cos(rotate-x angle)
/// - sin_ry: f32 (4 bytes) - sin(rotate-y angle)
/// - cos_ry: f32 (4 bytes) - cos(rotate-y angle)
/// - sin_rz: f32 (4 bytes) - sin(rotate-z angle), used by the flat
///   composite path for in-plane rotation (motion-bound bake-and-blit)
/// - cos_rz: f32 (4 bytes) - cos(rotate-z angle)
///   Total: 112 bytes
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
    /// Perspective distance in physical pixels (0 = no 3D transform)
    pub perspective_d: f32,
    /// sin(rotate-x angle)
    pub sin_rx: f32,
    /// cos(rotate-x angle)
    pub cos_rx: f32,
    /// sin(rotate-y angle)
    pub sin_ry: f32,
    /// cos(rotate-y angle)
    pub cos_ry: f32,
    /// sin(rotate-z angle) — in-plane rotation applied to the flat
    /// (non-perspective) composite path. Lets motion-bound subtrees
    /// (e.g. cn::spinner's rotate_timeline) rotate the cached layer
    /// texture per-frame without re-baking. Identity = (0.0, 1.0).
    pub sin_rz: f32,
    /// cos(rotate-z angle) — see [`Self::sin_rz`].
    pub cos_rz: f32,
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
            perspective_d: 0.0,
            sin_rx: 0.0,
            cos_rx: 1.0,
            sin_ry: 0.0,
            cos_ry: 1.0,
            sin_rz: 0.0,
            cos_rz: 1.0,
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
            perspective_d: 0.0,
            sin_rx: 0.0,
            cos_rx: 1.0,
            sin_ry: 0.0,
            cos_ry: 1.0,
            sin_rz: 0.0,
            cos_rz: 1.0,
        }
    }

    /// Set clip region with optional rounded corners
    pub fn with_clip(mut self, bounds: [f32; 4], radius: [f32; 4]) -> Self {
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
///   Total: 160 bytes
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

/// Uniforms for the glow effect shader
///
/// Memory layout:
/// - color: `vec4<f32>` (16 bytes) - glow color RGBA
/// - blur: `f32` (4 bytes) - blur softness
/// - range: `f32` (4 bytes) - glow range
/// - opacity: `f32` (4 bytes) - glow opacity
/// - _pad0: `f32` (4 bytes) - padding
/// - texel_size: `vec2<f32>` (8 bytes) - inverse texture size
/// - _pad1: `vec2<f32>` (8 bytes) - padding for alignment
///   Total: 48 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlowUniforms {
    /// Glow color (RGBA)
    pub color: [f32; 4],
    /// Blur softness (affects falloff smoothness)
    pub blur: f32,
    /// Glow range (how far the glow extends)
    pub range: f32,
    /// Glow opacity (0-1)
    pub opacity: f32,
    /// Padding for alignment
    pub _pad0: f32,
    /// Inverse texture size (1/width, 1/height)
    pub texel_size: [f32; 2],
    /// Padding for 16-byte alignment
    pub _pad1: [f32; 2],
}

impl Default for GlowUniforms {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0, 1.0], // White glow
            blur: 8.0,
            range: 4.0,
            opacity: 1.0,
            _pad0: 0.0,
            texel_size: [1.0 / 800.0, 1.0 / 600.0],
            _pad1: [0.0, 0.0],
        }
    }
}

/// Uniforms for the mask image effect shader
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaskImageUniforms {
    /// 0 = alpha, 1 = luminance
    pub mask_mode: u32,
    pub _pad: [f32; 3],
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
    /// The foreground primitive index when this command was recorded.
    /// Needed so an effect layer composed of foreground primitives
    /// can be ranged the same way background primitives are.
    pub foreground_primitive_index: usize,
    /// The path-vertex index when this command was recorded. Each
    /// tessellated `fill_path` / `stroke_path` appends to
    /// `PathBatch.vertices`; ranging by this index lets the effect
    /// pipeline pull the exact path chunk that belongs to the
    /// current layer into its offscreen texture. Without it, path
    /// content (all of Lottie's shapes) bypassed the offscreen
    /// composite and rendered directly onto the final framebuffer
    /// — so layer-level opacity on a Lottie layer was visually
    /// indistinguishable from per-fill alpha multiplication.
    pub path_vertex_index: usize,
    /// The path-index count (triangle-list index buffer position)
    /// when this command was recorded. Paired with
    /// `path_vertex_index` so the renderer can pull both the vertex
    /// range and the index range for an effect layer without
    /// scanning the index buffer to find which triangles reference
    /// a given vertex window.
    pub path_index_count: usize,
    /// Foreground path-vertex index.
    pub foreground_path_vertex_index: usize,
    /// Foreground path-index count.
    pub foreground_path_index_count: usize,
    /// The layer command
    pub command: LayerCommand,
}

/// Batch of GPU primitives for efficient rendering
#[derive(Clone)]
pub struct PrimitiveBatch {
    /// Background primitives (rendered before glass)
    pub primitives: Vec<GpuPrimitive>,
    /// Foreground primitives (rendered after glass)
    pub foreground_primitives: Vec<GpuPrimitive>,
    pub glass_primitives: Vec<GpuGlassPrimitive>,
    /// Nested glass primitives (glass inside another glass element, rendered in a second pass)
    pub nested_glass_primitives: Vec<GpuGlassPrimitive>,
    pub glyphs: Vec<GpuGlyph>,
    /// Tessellated path geometry
    pub paths: PathBatch,
    /// Foreground tessellated path geometry
    pub foreground_paths: PathBatch,
    /// Layer commands for offscreen rendering and composition
    pub layer_commands: Vec<LayerCommandEntry>,
    /// 3D viewports to render via raymarching
    pub viewports_3d: Vec<Viewport3D>,
    /// GPU particle viewports to render
    pub particle_viewports: Vec<ParticleViewport3D>,
    /// Auxiliary data buffer (`vec4<f32>` array) for variable-length per-primitive data.
    /// Used for 3D group shape descriptors and polygon clip vertices.
    pub aux_data: Vec<[f32; 4]>,
    /// Dynamic RGBA images to upload and render this frame
    /// (video frames, camera preview, procedural textures)
    pub dynamic_images: Vec<DynamicImage>,
}

/// An RGBA image to upload to the GPU and render in a single frame
#[derive(Clone)]
pub struct DynamicImage {
    /// RGBA pixel data (4 bytes per pixel)
    pub data: Vec<u8>,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Destination rectangle (layout coordinates)
    pub dest: blinc_core::Rect,
    /// Opacity
    pub opacity: f32,
    /// Corner radius
    pub corner_radius: f32,
    /// Active clip rect at emit time, in transformed/physical space,
    /// as `[min_x, min_y, max_x, max_y]` (the form `get_clip_data`
    /// returns). Fed to the image shader's per-instance clip at render
    /// (the same in-shader clip SDF primitives use) so a canvas-emitted
    /// image stays inside its node body / scroll container. The
    /// no-clip sentinel (`[-10000, -10000, 100000, 100000]`) means
    /// full-coverage.
    pub clip_bounds: [f32; 4],
    /// Per-corner clip radius `[tl, tr, br, bl]` from `get_clip_data`,
    /// so a rounded node-body clip rounds the image too.
    pub clip_radius: [f32; 4],
}

impl PrimitiveBatch {
    pub fn new() -> Self {
        Self {
            primitives: Vec::new(),
            foreground_primitives: Vec::new(),
            glass_primitives: Vec::new(),
            nested_glass_primitives: Vec::new(),
            glyphs: Vec::new(),
            paths: PathBatch::default(),
            foreground_paths: PathBatch::default(),
            layer_commands: Vec::new(),
            viewports_3d: Vec::new(),
            particle_viewports: Vec::new(),
            aux_data: Vec::new(),
            dynamic_images: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.primitives.clear();
        self.foreground_primitives.clear();
        self.glass_primitives.clear();
        self.nested_glass_primitives.clear();
        self.glyphs.clear();
        self.paths = PathBatch::default();
        self.foreground_paths = PathBatch::default();
        self.layer_commands.clear();
        self.viewports_3d.clear();
        self.particle_viewports.clear();
        self.aux_data.clear();
        self.dynamic_images.clear();
    }

    /// Push a 3D viewport for SDF raymarching
    pub fn push_viewport_3d(&mut self, viewport: Viewport3D) {
        self.viewports_3d.push(viewport);
    }

    /// Check if there are any 3D viewports to render
    pub fn has_3d_viewports(&self) -> bool {
        !self.viewports_3d.is_empty()
    }

    /// Push a particle viewport for GPU particle rendering
    pub fn push_particle_viewport(&mut self, viewport: ParticleViewport3D) {
        self.particle_viewports.push(viewport);
    }

    /// Check if there are any particle viewports to render
    pub fn has_particle_viewports(&self) -> bool {
        !self.particle_viewports.is_empty()
    }

    /// Record a layer command at the current primitive index
    pub fn push_layer_command(&mut self, command: LayerCommand) {
        self.layer_commands.push(LayerCommandEntry {
            primitive_index: self.primitives.len(),
            foreground_primitive_index: self.foreground_primitives.len(),
            path_vertex_index: self.paths.vertices.len(),
            path_index_count: self.paths.indices.len(),
            foreground_path_vertex_index: self.foreground_paths.vertices.len(),
            foreground_path_index_count: self.foreground_paths.indices.len(),
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
                !config.effects.is_empty() || config.blend_mode != blinc_core::BlendMode::Normal
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

    pub fn push_nested_glass(&mut self, glass: GpuGlassPrimitive) {
        self.nested_glass_primitives.push(glass);
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
        mut tessellated: crate::path::TessellatedPath,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_type: ClipType,
        brush_info: &crate::path::PathBrushInfo,
    ) {
        if tessellated.is_empty() {
            return;
        }

        // Stamp clip data onto each new vertex. Path primitives share a single
        // VBO + uniform buffer, so the legacy "batch-level" clip would get
        // clobbered every time a new path with a different clip was submitted —
        // making the LAST notch's clip apply to ALL paths in the batch.
        // Carrying clip per-vertex lets multiple canvases coexist in one draw.
        let clip_type_u = clip_type as u32;
        for v in &mut tessellated.vertices {
            v.clip_bounds = clip_bounds;
            v.clip_radius = clip_radius;
            v.clip_type = clip_type_u;
        }

        // Update batch-level clip too (kept for legacy uniform binding fallback,
        // though the shader now reads from the per-vertex attributes).
        self.paths.clip_bounds = clip_bounds;
        self.paths.clip_radius = clip_radius;
        self.paths.clip_type = clip_type_u;

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
        mut tessellated: crate::path::TessellatedPath,
        clip_bounds: [f32; 4],
        clip_radius: [f32; 4],
        clip_type: ClipType,
        brush_info: &crate::path::PathBrushInfo,
    ) {
        if tessellated.is_empty() {
            return;
        }

        // Stamp clip data onto each new vertex (see push_path_with_brush_info
        // for the rationale — same shared-batch problem applies here).
        let clip_type_u = clip_type as u32;
        for v in &mut tessellated.vertices {
            v.clip_bounds = clip_bounds;
            v.clip_radius = clip_radius;
            v.clip_type = clip_type_u;
        }

        // Update clip data
        self.foreground_paths.clip_bounds = clip_bounds;
        self.foreground_paths.clip_radius = clip_radius;
        self.foreground_paths.clip_type = clip_type_u;

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
            && self.viewports_3d.is_empty()
    }

    /// Check if the batch contains any tessellated path geometry
    pub fn has_paths(&self) -> bool {
        !self.paths.vertices.is_empty() || !self.foreground_paths.vertices.is_empty()
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

    /// Filter primitives by z_layer, excluding those in effect/blend layers
    pub fn primitives_for_layer_excluding_effects(
        &self,
        z_layer: u32,
        effect_indices: &std::collections::HashSet<usize>,
    ) -> Vec<GpuPrimitive> {
        self.primitives
            .iter()
            .enumerate()
            .filter(|(i, p)| p.z_layer() == z_layer && !effect_indices.contains(i))
            .map(|(_, p)| *p)
            .collect()
    }

    /// Get the set of primitive indices that belong to effect/blend layers
    pub fn effect_layer_indices(&self) -> std::collections::HashSet<usize> {
        let mut indices = std::collections::HashSet::new();
        let mut stack: Vec<(usize, &blinc_core::LayerConfig)> = Vec::new();
        for entry in &self.layer_commands {
            match &entry.command {
                LayerCommand::Push { config } => {
                    stack.push((entry.primitive_index, config));
                }
                LayerCommand::Pop => {
                    if let Some((start_idx, config)) = stack.pop() {
                        if !config.effects.is_empty()
                            || config.blend_mode != blinc_core::BlendMode::Normal
                        {
                            for i in start_idx..entry.primitive_index {
                                indices.insert(i);
                            }
                        }
                    }
                }
                LayerCommand::Sample { .. } => {}
            }
        }
        indices
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

        // Merge 3D viewports
        self.viewports_3d.extend(other.viewports_3d);

        // Merge aux_data (offsets in primitives already point to correct positions
        // because aux_data offsets are absolute within a batch — callers must rebind)
        self.aux_data.extend(other.aux_data);
    }
}

impl Default for PrimitiveBatch {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3D Viewport Types
// ─────────────────────────────────────────────────────────────────────────────

/// Uniform data for 3D SDF raymarching
///
/// Must match the shader layout exactly:
/// ```wgsl
/// struct SdfUniform {
///     camera_pos: vec4<f32>,
///     camera_dir: vec4<f32>,
///     camera_up: vec4<f32>,
///     camera_right: vec4<f32>,
///     resolution: vec2<f32>,
///     time: f32,
///     fov: f32,
///     max_steps: u32,
///     max_distance: f32,
///     epsilon: f32,
///     _padding: f32,
///     uv_offset: vec2<f32>,
///     uv_scale: vec2<f32>,
/// }
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Sdf3DUniform {
    /// Camera position (vec4, w unused)
    pub camera_pos: [f32; 4],
    /// Camera direction (normalized, vec4, w unused)
    pub camera_dir: [f32; 4],
    /// Camera up vector (normalized, vec4, w unused)
    pub camera_up: [f32; 4],
    /// Camera right vector (normalized, vec4, w unused)
    pub camera_right: [f32; 4],
    /// Resolution of the original (unclipped) viewport (width, height)
    pub resolution: [f32; 2],
    /// Time for animation
    pub time: f32,
    /// Field of view in radians
    pub fov: f32,
    /// Maximum raymarch steps
    pub max_steps: u32,
    /// Maximum ray distance
    pub max_distance: f32,
    /// Surface hit epsilon
    pub epsilon: f32,
    /// Padding for alignment
    pub _padding: f32,
    /// UV offset for clipped viewports (how much is clipped from top-left, normalized 0-1)
    pub uv_offset: [f32; 2],
    /// UV scale for clipped viewports (visible portion size / original size)
    pub uv_scale: [f32; 2],
}

impl Default for Sdf3DUniform {
    fn default() -> Self {
        Self {
            camera_pos: [0.0, 2.0, 5.0, 1.0],
            camera_dir: [0.0, 0.0, -1.0, 0.0],
            camera_up: [0.0, 1.0, 0.0, 0.0],
            camera_right: [1.0, 0.0, 0.0, 0.0],
            resolution: [800.0, 600.0],
            time: 0.0,
            fov: 0.8,
            max_steps: 128,
            max_distance: 100.0,
            epsilon: 0.001,
            _padding: 0.0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
        }
    }
}

/// A 3D viewport to be rendered via raymarching
#[derive(Clone, Debug)]
pub struct Viewport3D {
    /// The generated WGSL shader code for this scene
    pub shader_wgsl: String,
    /// Uniform data for the shader
    pub uniforms: Sdf3DUniform,
    /// Viewport bounds in screen coordinates
    pub bounds: [f32; 4],
    /// Lights in the scene
    pub lights: Vec<blinc_core::Light>,
}

/// GPU particle viewport for rendering particle systems
#[derive(Clone, Debug)]
pub struct ParticleViewport3D {
    /// Emitter configuration
    pub emitter: crate::particles::GpuEmitter,
    /// Force affectors
    pub forces: Vec<crate::particles::GpuForce>,
    /// Maximum particles in this system
    pub max_particles: u32,
    /// Viewport bounds in screen coordinates (x, y, width, height)
    pub bounds: [f32; 4],
    /// Camera position
    pub camera_pos: [f32; 3],
    /// Camera target
    pub camera_target: [f32; 3],
    /// Camera up vector
    pub camera_up: [f32; 3],
    /// Field of view
    pub fov: f32,
    /// Current time
    pub time: f32,
    /// Delta time for this frame
    pub delta_time: f32,
    /// Blend mode (0=alpha, 1=additive)
    pub blend_mode: u32,
    /// Whether system is playing
    pub playing: bool,
}

impl Default for ParticleViewport3D {
    fn default() -> Self {
        Self {
            emitter: crate::particles::GpuEmitter::default(),
            forces: Vec::new(),
            max_particles: 10000,
            bounds: [0.0, 0.0, 800.0, 600.0],
            camera_pos: [0.0, 2.0, 5.0],
            camera_target: [0.0, 0.0, 0.0],
            camera_up: [0.0, 1.0, 0.0],
            fov: 0.8,
            time: 0.0,
            delta_time: 0.016,
            blend_mode: 0,
            playing: true,
        }
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
