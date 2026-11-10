//! GPU shaders for SDF primitives
//!
//! These shaders render:
//! - Rounded rectangles with borders
//! - Circles and ellipses
//! - Gaussian blur shadows (via error function approximation)
//! - Gradients (linear, radial, conic)
//! - Glass/vibrancy effects (backdrop blur, tint)

/// Compositor v2 damage-rect scissored-clear shader.
///
/// Draws a fullscreen triangle that outputs `(0, 0, 0, 0)`. Combined
/// with a REPLACE blend pipeline and an active `set_scissor_rect`,
/// it zeros the damaged region of the static cache so the
/// subsequent SDF dispatch can re-paint without ghosting on
/// anti-aliased / semi-transparent edges.
pub const CLEAR_QUAD_SHADER: &str = crate::shaders_russl::clear_quad::WGSL;

/// Split SDF shader: Core shapes (Rect, Circle, Ellipse)
///
/// Handles prim_type 0-2 with full features: borders, gradients,
/// CSS filters, mask gradients, perspective, clip regions.
/// Part of the multi-pipeline SDF split for driver compatibility.
pub const SDF_CORE_SHADER: &str = crate::sdf_russl::storage::sdf_core::WGSL;

/// Split SDF shader: plain Rect and Circle (solid fill, circular corners, uniform border,
/// rounded rect clip). Only for tiers with storage buffers in the vertex stage.
pub const SDF_SIMPLE_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_simple.wgsl"));

/// Split SDF shader: Shadow primitives
///
/// Handles prim_type 3-6 (Shadow, InnerShadow, CircleShadow,
/// CircleInnerShadow). Each case is self-contained with early return.
pub const SDF_SHADOW_SHADER: &str = crate::sdf_russl::sdf_shadow::WGSL;

/// Split SDF shader: 3D raymarched shapes
///
/// Handles primitives with shape_type > 0 (3D box, sphere, cylinder,
/// torus, capsule, group). 32-step raymarching with Blinn-Phong lighting.
pub const SDF_3D_SHADER: &str = crate::sdf_russl::sdf_3d::WGSL;

/// Split SDF shader: Notch primitives (prim_type 8)
///
/// Handles concave corners with edge modifiers (scoop, bulge, cut, peak).
/// Full border, gradient, filter, and mask support.
pub const SDF_NOTCH_SHADER: &str = crate::sdf_russl::sdf_notch::WGSL;

/// Vertex-buffer fallback: Core shapes (no VERTEX_STORAGE needed)
///
/// Same as SDF_CORE_SHADER but vs_main reads instance attributes
/// from a vertex buffer instead of the storage buffer.
pub const SDF_CORE_VB_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_core_vb.wgsl"));

/// Vertex-buffer fallback: Shadow primitives
pub const SDF_SHADOW_VB_SHADER: &str = crate::sdf_russl::sdf_shadow_vb::WGSL;

/// Vertex-buffer fallback: 3D raymarched shapes
pub const SDF_3D_VB_SHADER: &str = crate::sdf_russl::sdf_3d_vb::WGSL;

/// Vertex-buffer fallback: Notch primitives
pub const SDF_NOTCH_VB_SHADER: &str = crate::sdf_russl::sdf_notch_vb::WGSL;

/// Data-texture fallback: Core shapes (no storage buffers needed — WebGL2)
///
/// Replaces storage buffer reads with textureLoad from RGBA32F data textures.
/// Also uses vertex buffer instance attributes (inherits from VB variant).
pub const SDF_CORE_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_core_dt.wgsl"));

/// Data-texture fallback: Shadow primitives
pub const SDF_SHADOW_DT_SHADER: &str = crate::sdf_russl::sdf_shadow_dt::WGSL;

/// Data-texture fallback: 3D raymarched shapes
pub const SDF_3D_DT_SHADER: &str = crate::sdf_russl::sdf_3d_dt::WGSL;

/// Data-texture fallback: Notch primitives
pub const SDF_NOTCH_DT_SHADER: &str = crate::sdf_russl::sdf_notch_dt::WGSL;

/// Data-texture fallback: Text rendering (no storage buffers — WebGL2)
///
/// Glyph data packed into Rgba32Float texture (width=6, height=max_glyphs).
/// Same visual output as TEXT_SHADER.
pub const TEXT_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/text_dt.wgsl"));

/// Data-texture fallback: Glass effects (no storage buffers — WebGL2)
pub const GLASS_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/glass_dt.wgsl"));

/// Data-texture fallback: Simple glass effects (no storage buffers — WebGL2)
pub const SIMPLE_GLASS_DT_SHADER: &str =
    include_str!(concat!(env!("OUT_DIR"), "/simple_glass_dt.wgsl"));

/// Data-texture fallback: Mesh rendering with joint matrices as texture (WebGL2)
pub const MESH_DT_SHADER: &str = include_str!("shaders/mesh_dt.wgsl");

/// Shader for text rendering with SDF glyphs
///
/// Supports both grayscale text glyphs and color emoji:
/// - Grayscale: samples R channel from glyph_atlas, multiplies with color
/// - Color emoji: samples RGBA from color_atlas, uses texture color directly
pub const TEXT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/text.wgsl"));

/// Shader for glass/vibrancy effects (Apple Glass UI style)
///
/// This shader creates frosted glass effects by:
/// 1. Sampling and blurring the backdrop
/// 2. Applying a tint color
/// 3. Adding optional noise for texture
/// 4. Compositing with the shape mask
pub const GLASS_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/glass.wgsl"));

/// Simple frosted glass shader - pure backdrop blur without liquid glass effects
///
/// This shader provides:
/// - Backdrop blur (Gaussian approximation)
/// - Saturation/brightness adjustment
/// - Subtle tint overlay
/// - Drop shadows
///
/// Unlike GLASS_SHADER, this does NOT include:
/// - Edge bevels or refraction
/// - Light reflections
/// - Liquid glass distortion
pub const SIMPLE_GLASS_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/simple_glass.wgsl"));

/// Shader for compositing layers with blend modes
pub const COMPOSITE_SHADER: &str = crate::shaders_russl::composite::WGSL;

/// Shader for tessellated path rendering (triangles with per-vertex colors)
pub const PATH_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/path.wgsl"));

/// Path shader using a data texture for auxiliary clips.
pub const PATH_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/path_dt.wgsl"));

/// Shader for image rendering
///
/// Renders images with:
/// - UV cropping for box-fit modes
/// - Tinting and opacity
/// - Optional rounded corners
pub const IMAGE_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/image.wgsl"));

/// Image shader using a data texture for auxiliary clips.
pub const IMAGE_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/image_dt.wgsl"));

/// Shader for layer composition
///
/// Composites offscreen layer textures onto parent targets with:
/// - Blend mode support (Normal, Multiply, Screen, Overlay, etc.)
/// - Opacity application
/// - Source and destination rectangle mapping
pub const LAYER_COMPOSITE_SHADER: &str = crate::shaders_russl::layer_composite::WGSL;

/// Kawase blur shader for layer effects
///
/// Implements multi-pass Kawase blur which approximates Gaussian blur
/// with better performance. Each pass samples 5 points in an X pattern.
pub const BLUR_SHADER: &str = crate::shaders_russl::blur::WGSL;

/// Color matrix shader for layer effects
///
/// Applies a 4x5 color transformation matrix to achieve effects like:
/// grayscale, sepia, brightness, contrast, saturation adjustments.
pub const COLOR_MATRIX_SHADER: &str = crate::shaders_russl::color_matrix::WGSL;

/// Mask image shader for CSS mask-image support
///
/// Multiplies the layer's alpha by the mask image value.
/// Supports alpha mode (use mask alpha) and luminance mode (use mask luminance as alpha).
pub const MASK_IMAGE_SHADER: &str = crate::shaders_russl::mask_image::WGSL;

/// Shadow colorize shader for layer effects
///
/// Takes a pre-blurred texture and colorizes its alpha channel to create shadow.
/// This is used after Kawase blur for smooth shadows at any radius.
pub const DROP_SHADOW_SHADER: &str = crate::shaders_russl::drop_shadow::WGSL;

/// Glow effect shader for layer effects
///
/// Creates a radial glow around the shape by:
/// 1. Finding distance to nearest opaque pixel
/// 2. Applying smooth radial falloff based on blur + range
/// 3. Compositing glow behind original content
pub const GLOW_SHADER: &str = crate::shaders_russl::glow::WGSL;

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_wgsl(source: &str) -> Result<(), String> {
        naga::front::wgsl::parse_str(source).map_err(|e| e.emit_to_string(source))?;
        Ok(())
    }

    fn run_tool(program: &str, args: &[&std::ffi::OsStr]) -> std::process::Output {
        std::process::Command::new(program)
            .args(args)
            .output()
            .unwrap_or_else(|err| {
                panic!(
                    "failed to run `{program}`. Install `naga` and `dxc` to run this check: {err}"
                )
            })
    }

    fn assert_tool_success(program: &str, args: &[&std::ffi::OsStr]) {
        let output = run_tool(program, args);
        if !output.status.success() {
            panic!(
                "`{program}` failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
    }

    fn assert_wgsl_compiles_to_dxil(name: &str, source: &str) {
        let root =
            std::env::temp_dir().join(format!("blinc-hlsl-dxc-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temporary shader check directory");

        let wgsl_path = root.join(format!("{name}.wgsl"));
        let hlsl_path = root.join(format!("{name}.hlsl"));
        let vs_path = root.join(format!("{name}.vs.dxil"));
        let ps_path = root.join(format!("{name}.ps.dxil"));

        std::fs::write(&wgsl_path, source).expect("write WGSL source for HLSL check");

        assert_tool_success(
            "naga",
            &[
                std::ffi::OsStr::new("--input-kind"),
                std::ffi::OsStr::new("wgsl"),
                std::ffi::OsStr::new("--shader-model"),
                std::ffi::OsStr::new("60"),
                wgsl_path.as_os_str(),
                hlsl_path.as_os_str(),
            ],
        );

        assert_tool_success(
            "dxc",
            &[
                std::ffi::OsStr::new("-T"),
                std::ffi::OsStr::new("vs_6_0"),
                std::ffi::OsStr::new("-E"),
                std::ffi::OsStr::new("vs_main"),
                std::ffi::OsStr::new("-Fo"),
                vs_path.as_os_str(),
                hlsl_path.as_os_str(),
            ],
        );

        assert_tool_success(
            "dxc",
            &[
                std::ffi::OsStr::new("-T"),
                std::ffi::OsStr::new("ps_6_0"),
                std::ffi::OsStr::new("-E"),
                std::ffi::OsStr::new("fs_main"),
                std::ffi::OsStr::new("-Fo"),
                ps_path.as_os_str(),
                hlsl_path.as_os_str(),
            ],
        );
    }

    /// Runtime shader compilation is the only thing that parses these WGSL
    /// strings — if a case label or a const declaration is malformed, the
    /// error only surfaces when the app actually tries to build a pipeline
    /// on device. These tests invoke naga directly so the workspace test
    /// suite catches syntax regressions (e.g. a stray `// comment {`
    /// swallowing a case-body brace) before they ship.
    #[test]
    fn path_shader_parses() {
        parse_wgsl(PATH_SHADER).expect("PATH_SHADER");
        parse_wgsl(PATH_DT_SHADER).expect("PATH_DT_SHADER");
    }

    #[test]
    fn text_shader_parses() {
        parse_wgsl(TEXT_SHADER).expect("TEXT_SHADER");
    }

    #[test]
    fn glass_shader_parses() {
        parse_wgsl(GLASS_SHADER).expect("GLASS_SHADER");
    }

    #[test]
    fn simple_glass_shader_parses() {
        parse_wgsl(SIMPLE_GLASS_SHADER).expect("SIMPLE_GLASS_SHADER");
    }

    #[test]
    fn composite_shader_parses() {
        parse_wgsl(COMPOSITE_SHADER).expect("COMPOSITE_SHADER");
    }

    #[test]
    fn layer_composite_shader_parses() {
        parse_wgsl(LAYER_COMPOSITE_SHADER).expect("LAYER_COMPOSITE_SHADER");
    }

    #[test]
    fn blur_shader_parses() {
        parse_wgsl(BLUR_SHADER).expect("BLUR_SHADER");
    }

    #[test]
    fn drop_shadow_shader_parses() {
        parse_wgsl(DROP_SHADOW_SHADER).expect("DROP_SHADOW_SHADER");
    }

    #[test]
    fn glow_shader_parses() {
        parse_wgsl(GLOW_SHADER).expect("GLOW_SHADER");
    }

    #[test]
    fn color_matrix_shader_parses() {
        parse_wgsl(COLOR_MATRIX_SHADER).expect("COLOR_MATRIX_SHADER");
    }

    #[test]
    fn image_shaders_parse() {
        for source in [IMAGE_SHADER, IMAGE_DT_SHADER] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .expect("valid image shader");
        }
    }

    #[test]
    fn mask_image_shader_parses() {
        parse_wgsl(MASK_IMAGE_SHADER).expect("MASK_IMAGE_SHADER");
    }

    #[test]
    fn sdf_core_shader_parses() {
        parse_wgsl(SDF_CORE_SHADER).expect("SDF_CORE_SHADER");
    }

    #[test]
    fn sdf_simple_shader_parses() {
        parse_wgsl(SDF_SIMPLE_SHADER).expect("SDF_SIMPLE_SHADER");
    }

    #[test]
    fn sdf_shadow_shader_parses() {
        parse_wgsl(SDF_SHADOW_SHADER).expect("SDF_SHADOW_SHADER");
    }

    #[test]
    fn sdf_3d_shader_parses() {
        parse_wgsl(SDF_3D_SHADER).expect("SDF_3D_SHADER");
    }

    #[test]
    fn sdf_notch_shader_parses() {
        parse_wgsl(SDF_NOTCH_SHADER).expect("SDF_NOTCH_SHADER");
    }

    #[test]
    fn sdf_core_vb_shader_parses() {
        parse_wgsl(SDF_CORE_VB_SHADER).expect("SDF_CORE_VB_SHADER");
    }

    #[test]
    fn sdf_shadow_vb_shader_parses() {
        parse_wgsl(SDF_SHADOW_VB_SHADER).expect("SDF_SHADOW_VB_SHADER");
    }

    #[test]
    fn sdf_3d_vb_shader_parses() {
        parse_wgsl(SDF_3D_VB_SHADER).expect("SDF_3D_VB_SHADER");
    }

    #[test]
    fn sdf_notch_vb_shader_parses() {
        parse_wgsl(SDF_NOTCH_VB_SHADER).expect("SDF_NOTCH_VB_SHADER");
    }

    #[test]
    fn sdf_core_dt_shader_parses() {
        parse_wgsl(SDF_CORE_DT_SHADER).expect("SDF_CORE_DT_SHADER");
    }

    #[test]
    fn sdf_shadow_dt_shader_parses() {
        parse_wgsl(SDF_SHADOW_DT_SHADER).expect("SDF_SHADOW_DT_SHADER");
    }

    /// Host-side guard for Windows issue #53.
    ///
    /// CI runs this on a Windows runner with the SDK `dxc.exe`; local runs can
    /// use any `naga` + `dxc` install on PATH. It catches generated-HLSL
    /// syntax/control-flow regressions in the shadow pipelines before they
    /// reach D3D pipeline creation.
    #[test]
    #[ignore = "requires `naga` and `dxc` CLIs"]
    fn sdf_shadow_hlsl_compiles_with_dxc() {
        assert_wgsl_compiles_to_dxil("sdf_shadow", SDF_SHADOW_SHADER);
        assert_wgsl_compiles_to_dxil("sdf_shadow_vb", SDF_SHADOW_VB_SHADER);
        assert_wgsl_compiles_to_dxil("sdf_shadow_dt", SDF_SHADOW_DT_SHADER);
    }

    #[test]
    fn sdf_3d_dt_shader_parses() {
        parse_wgsl(SDF_3D_DT_SHADER).expect("SDF_3D_DT_SHADER");
    }

    #[test]
    fn sdf_notch_dt_shader_parses() {
        parse_wgsl(SDF_NOTCH_DT_SHADER).expect("SDF_NOTCH_DT_SHADER");
    }

    #[test]
    fn text_dt_shader_parses() {
        parse_wgsl(TEXT_DT_SHADER).expect("TEXT_DT_SHADER");
    }

    #[test]
    fn glass_dt_shader_parses() {
        parse_wgsl(GLASS_DT_SHADER).expect("GLASS_DT_SHADER");
    }

    #[test]
    fn simple_glass_dt_shader_parses() {
        parse_wgsl(SIMPLE_GLASS_DT_SHADER).expect("SIMPLE_GLASS_DT_SHADER");
    }

    #[test]
    fn mesh_dt_shader_parses() {
        parse_wgsl(MESH_DT_SHADER).expect("MESH_DT_SHADER");
    }
}
