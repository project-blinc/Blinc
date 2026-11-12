//! GPU primitive batching
//!
//! Defines GPU-ready data structures that match the shader uniform layouts.
//! All structures use `#[repr(C)]` and implement `bytemuck::Pod` for safe
//! GPU buffer copies.

mod aux_data;
mod batch;
pub mod cross_stream;
mod dynamic_image;
mod effect_uniforms;
mod foreground;
mod glass_primitive;
mod glyph;
mod gpu_primitive;
mod kinds;
mod layer_command;
mod uniforms;
mod viewport3d;

pub use batch::{DynamicImage, ImagePaintOrder, PathBatch, PrimitiveBatch, RetainedImage};
pub use effect_uniforms::{
    BlurUniforms, ColorMatrixUniforms, DropShadowUniforms, GlowUniforms, MaskImageUniforms,
};
pub use glass_primitive::{GpuGlassPrimitive, MAX_GROUP_SHAPES, ShapeDesc};
pub use glyph::GpuGlyph;
pub use gpu_primitive::SdfVertexInstance;
pub use kinds::{
    ClipType, FillType, GlassType, NOTCH_CORNER_CONCAVE, NOTCH_CORNER_SHARP_OR_CONVEX,
    NOTCH_MOD_BULGE, NOTCH_MOD_CUT, NOTCH_MOD_NONE, NOTCH_MOD_PEAK, NOTCH_MOD_SCOOP, PrimitiveType,
    SdfPipelineCategory,
};
pub use layer_command::{LayerCommand, LayerCommandEntry};
pub use uniforms::{
    CompositeUniforms, GlassUniforms, LayerCompositeUniforms, PathUniforms, Uniforms,
};
pub use viewport3d::{GpuRect, ParticleViewport3D, Sdf3DUniform, Viewport3D};

// Declared here: `tests/primitive_layout.rs` parses these fields from this file's source.
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
///   Total: 544 bytes (34 x vec4). Asserted against the WGSL
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
    /// Text fill modes: native mask 0 and colour 1; ABI colour shape 2,
    /// colour with record alpha 3 and mask 4. ABI modes clip their atlas rect.
    pub type_info: [u32; 4],
    /// Border colour of the top, right, bottom and left sides when the
    /// sides differ. Alphas that sum to zero mean unset: the border is
    /// `border_color` all round.
    pub side_color_top: [f32; 4],
    pub side_color_right: [f32; 4],
    pub side_color_bottom: [f32; 4],
    pub side_color_left: [f32; 4],
    /// A gradient's stops, when it has more than two or its first and last are not
    /// 0 and 1, or it spreads past them: `x` is one more than the offset in
    /// `aux_data` of its table, and zero means none, the fill being `color` to
    /// `color2`. The table is a header of the stop count and spread (0 pad, 1
    /// repeat, 2 reflect), a row of the average colour, then for each stop a row of
    /// its colour and a row of its offset.
    pub gradient_table: [f32; 4],
    /// A clip in the primitive's own frame: the box's corner is the origin and a
    /// transform of the box has been taken back, so a turned parent clips its
    /// children to its turned, rounded box. A width of zero means none. Its corners
    /// are `clip_corner_shape`'s.
    pub local_clip_bounds: [f32; 4],
    pub local_clip_radius: [f32; 4],
    /// Screen rectangle for `clip_fade`. A zero width uses `clip_bounds`, as
    /// native primitives do; a separate rectangle keeps intersecting clips from
    /// moving the fade's edges.
    pub clip_fade_bounds: [f32; 4],
    /// ABI notch coordinates: x is device pixels per logical pixel (zero uses
    /// native notch shading), y is 0 for fill, 1 for outer shadow, 2 for inset.
    /// Geometry occupies the native notch slots in `perspective`, `sdf_3d`,
    /// `light` and `corner_radius`.
    pub notch_info: [f32; 4],
    /// One more than the offset in `aux_data` of a shape clip, zero for
    /// none. Three rows hold its inverse screen frame, translation/kind/radius,
    /// and shape parameters, followed by polygon points two per row. A polygon
    /// offset is relative to that header, so moving a run only rebases this pointer.
    pub clip_shape: [f32; 4],
}

#[cfg(test)]
mod simple_core_tests {
    use super::*;

    fn simple() -> GpuPrimitive {
        GpuPrimitive::default()
    }

    /// The simple pipeline compiles the core shader without the optional blocks, so a
    /// primitive that reaches one of them has to stay on the full pipeline.
    #[test]
    fn a_plain_primitive_is_simple_and_each_optional_feature_is_not() {
        assert_eq!(simple().pipeline_category(), SdfPipelineCategory::CorePlain);

        // Features the simple pipeline keeps.
        let mut bordered = simple();
        bordered.border = [1.0, 0.0, 0.0, 0.0];
        bordered.corner_radius = [8.0; 4];
        bordered.type_info[2] = 1; // a rect clip
        assert!(bordered.is_simple_core());

        type Change = fn(&mut GpuPrimitive);
        let cases: [(&str, Change); 9] = [
            ("gradient", |p| p.type_info[1] = 1),
            ("polygon clip", |p| p.type_info[2] = 4),
            ("shape clip", |p| p.clip_shape[0] = 3.0),
            ("local clip", |p| p.local_clip_bounds[2] = 10.0),
            ("mask", |p| p.mask_info[0] = 1.0),
            ("filter", |p| p.filter_a[0] = 0.5),
            ("brightness", |p| p.filter_b[0] = 1.2),
            ("inline shadow", |p| p.shadow[2] = 8.0),
            ("tilt", |p| p.perspective[0] = 0.2),
        ];
        for (name, change) in cases {
            let mut p = simple();
            change(&mut p);
            assert!(!p.is_simple_core(), "{name} needs the full shader");
            assert_eq!(p.pipeline_category(), SdfPipelineCategory::Core, "{name}");
        }

        // Simple primitives the compact shader cannot draw take the simple core pipeline: a
        // corner shape, an ellipse, a circle clip, per-side borders, an edge fade, a transform.
        let not_plain: [(&str, Change); 9] = [
            ("bevel", |p| p.corner_shape = [0.0; 4]),
            ("scoop", |p| p.corner_shape = [-1.0; 4]),
            ("ellipse", |p| p.type_info[0] = 2),
            ("circle clip", |p| p.type_info[2] = 2),
            ("clip corner shape", |p| p.clip_corner_shape = [2.0; 4]),
            ("per-side border", |p| p.border = [1.0, 2.0, 1.0, 2.0]),
            ("side colour", |p| p.side_color_top[3] = 1.0),
            ("clip fade", |p| p.clip_fade[0] = 4.0),
            ("rotation", |p| p.local_affine = [0.9, 0.1, -0.1, 0.9]),
        ];
        for (name, change) in not_plain {
            let mut p = simple();
            change(&mut p);
            assert!(!p.is_plain_core(), "{name} is not plain");
            assert_eq!(
                p.pipeline_category(),
                SdfPipelineCategory::CoreSimple,
                "{name}"
            );
        }
        // A squircle corner is plain; the compact shader draws its curve.
        let mut squircle = simple();
        squircle.corner_shape = [1.084; 4];
        assert!(squircle.is_plain_core());
        // The same width on every side is uniform, and a circle is plain.
        let mut uniform = simple();
        uniform.border = [2.0; 4];
        assert!(uniform.is_plain_core());
        let mut circle = simple();
        circle.type_info[0] = 1;
        assert!(circle.is_plain_core());

        // Other categories are untouched.
        let mut text = simple();
        text.type_info[0] = 7;
        assert_eq!(text.pipeline_category(), SdfPipelineCategory::Text);
        let mut mesh = simple();
        mesh.type_info[0] = 9;
        assert_eq!(mesh.pipeline_category(), SdfPipelineCategory::Core);
        let mut boxed = simple();
        boxed.perspective[3] = 1.0;
        assert_eq!(boxed.pipeline_category(), SdfPipelineCategory::Sdf3D);

        // A page background is the clear colour; anything that changes the pixels is not.
        let mut backdrop = simple();
        backdrop.bounds = [0.0, 0.0, 400.0, 800.0];
        backdrop.color = [0.1, 0.2, 0.3, 1.0];
        let view = (400.0, 800.0);
        assert_eq!(
            backdrop.opaque_backdrop(view),
            Some([0.1_f32 as f64, 0.2_f32 as f64, 0.3_f32 as f64, 1.0])
        );
        let breaks: [(&str, Change); 7] = [
            ("translucent", |p| p.color[3] = 0.9),
            ("rounded", |p| p.corner_radius = [8.0; 4]),
            ("bordered", |p| p.border = [1.0, 0.0, 0.0, 0.0]),
            ("clipped", |p| p.type_info[2] = 1),
            ("short", |p| p.bounds[3] = 700.0),
            ("offset", |p| p.bounds[0] = 5.0),
            ("gradient", |p| p.type_info[1] = 1),
        ];
        for (name, change) in breaks {
            let mut p = backdrop;
            change(&mut p);
            assert_eq!(p.opaque_backdrop(view), None, "{name} is not a backdrop");
        }

        // A NaN fails the tests and takes the full shader.
        let mut nan = simple();
        nan.mask_info[0] = f32::NAN;
        assert!(!nan.is_simple_core());
    }
}
