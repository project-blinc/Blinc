//! `GpuPrimitive` methods and the vertex-stage subset of its fields.

use super::{ClipType, FillType, GpuGlyph, GpuPrimitive, PrimitiveType, SdfPipelineCategory};

impl GpuPrimitive {
    /// RGBA32Float texels in one data-texture row for this primitive.
    pub const DATA_TEXTURE_TEXELS: u32 =
        (std::mem::size_of::<Self>() / std::mem::size_of::<[f32; 4]>()) as u32;

    /// Determine which SDF pipeline should render this primitive.
    /// 3D is detected by shape_type in `perspective[3]`, but only promotes
    /// *core* prim types (rect/circle/ellipse). Shadows, text, and notches
    /// stay on their own pipelines even when `perspective[3]` is non-zero —
    /// `current_perspective_params()` bleeds the active element's
    /// `set_3d_shape` state onto every primitive built during that element's
    /// render, and we don't want a shadow to be raymarched as a solid 3D
    /// shape or have sdf_core overwrite it.
    /// Whether a rect, circle or ellipse needs only the core shader's basic path: a solid fill
    /// with an optional border, corner shape and rect or rounded clip.
    ///
    /// Each test mirrors the condition that enables the matching block in `sdf_core.wgsl`, so a
    /// primitive that passes draws the same pixels in the simple pipeline. A NaN fails every
    /// test and takes the full shader.
    pub fn is_simple_core(&self) -> bool {
        let [prim_type, fill_type, clip_type, _] = self.type_info;
        let [fa_x, fa_y, fa_z, fa_w] = self.filter_a;
        let [fb_x, fb_y, fb_z, _] = self.filter_b;
        prim_type <= 2
            && fill_type == 0
            && clip_type != 4
            && self.clip_shape[0] < 0.5
            && self.local_clip_bounds[2] <= 0.0
            && self.mask_info[0] <= 0.5
            && fa_x == 0.0
            && fa_y == 0.0
            && fa_z == 0.0
            && fa_w.abs() <= 0.001
            && fb_x == 1.0
            && fb_y == 1.0
            && fb_z == 1.0
            && self.shadow[2] <= 0.0
            && self.shadow[3] == 0.0
            && self.rotation[2].abs() <= 0.0001
            && self.perspective[0].abs() <= 0.0001
            && self.perspective[2] <= 0.001
    }

    /// Whether the compact shader draws this primitive exactly as the core shader would: a
    /// simple rect or circle, circular or convex superellipse corners, no clip or a rounded rect
    /// clip, no edge fade,
    /// no local transform, and a border that is the same width and colour on every side.
    pub fn is_plain_core(&self) -> bool {
        let [bt, br, bb, bl] = self.border;
        let uniform_border =
            (br == 0.0 && bb == 0.0 && bl == 0.0) || (bt == br && br == bb && bb == bl);
        let [a, b, c, d] = self.local_affine;
        let side_alphas = self.side_color_top[3]
            + self.side_color_right[3]
            + self.side_color_bottom[3]
            + self.side_color_left[3];
        self.is_simple_core()
            && self.type_info[0] <= 1
            && self.type_info[2] <= 1
            // Circular or convex superellipse corners: not a bevel, scoop, square or notch.
            && self.corner_shape.iter().all(|n| *n > 0.01 && *n < 100.0)
            && self.clip_corner_shape == [1.0; 4]
            && self.clip_fade == [0.0; 4]
            && uniform_border
            && side_alphas == 0.0
            && (a - 1.0).abs() < 0.0001
            && b.abs() < 0.0001
            && c.abs() < 0.0001
            && (d - 1.0).abs() < 0.0001
    }

    /// The colour this primitive paints when it is an opaque, square, unclipped rect that fills
    /// a `viewport`-sized target by itself: a page background. Drawing it is the same as clearing
    /// the target to that colour, which costs no shading.
    pub fn opaque_backdrop(&self, viewport: (f32, f32)) -> Option<[f64; 4]> {
        let [x, y, w, h] = self.bounds;
        let identity = self.local_affine == [1.0, 0.0, 0.0, 1.0];
        (self.is_simple_core()
            && self.type_info[0] == 0
            && self.type_info[2] == 0
            && identity
            && self.color[3] >= 1.0
            && self.border == [0.0; 4]
            && self.corner_radius == [0.0; 4]
            && self.clip_fade == [0.0; 4]
            && self.shadow == [0.0; 4]
            && x <= 0.0
            && y <= 0.0
            && x + w >= viewport.0
            && y + h >= viewport.1)
            .then(|| {
                let [r, g, b, _] = self.color;
                [r as f64, g as f64, b as f64, 1.0]
            })
    }

    pub fn pipeline_category(&self) -> SdfPipelineCategory {
        let prim_type = self.type_info[0];
        let shape_3d_type = self.perspective[3] as u32;
        match prim_type {
            0..=2 => {
                if shape_3d_type > 0 {
                    SdfPipelineCategory::Sdf3D
                } else if self.is_plain_core() {
                    SdfPipelineCategory::CorePlain
                } else if self.is_simple_core() {
                    SdfPipelineCategory::CoreSimple
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
    pub notch_info: [f32; 4],
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
            notch_info: p.notch_info,
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
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 128,
                shader_location: 8,
            },
        ],
    };
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
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
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
    /// Path headers retain their batch-relative offsets; keep the owning auxiliary
    /// rows with the converted primitive.
    pub fn from_glyph(glyph: &GpuGlyph) -> Self {
        // Use type_info[1] to store is_color flag (1 = color emoji, 0 = grayscale)
        let is_color_flag = if glyph.flags[0] > 0.5 { 1u32 } else { 0u32 };
        // A path clip names itself in `flags[1]`; otherwise a glyph is
        // clipped by its rect unless that is the unbounded default.
        let clip_type = if glyph.flags[1] > 0.0 || glyph.clip_bounds[2] < 50000.0 {
            ClipType::Rect
        } else {
            ClipType::None
        };
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
                clip_type as u32,
                0,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [glyph.flags[1], 0.0, 0.0, 0.0],
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
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        }
    }
}
