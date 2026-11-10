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
pub const CLEAR_QUAD_SHADER: &str = include_str!("shaders/clear_quad.wgsl");

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
pub const SDF_SHADOW_SHADER: &str = crate::sdf_russl::storage::sdf_shadow::WGSL;

/// Split SDF shader: 3D raymarched shapes
///
/// Handles primitives with shape_type > 0 (3D box, sphere, cylinder,
/// torus, capsule, group). 32-step raymarching with Blinn-Phong lighting.
pub const SDF_3D_SHADER: &str = crate::sdf_russl::storage::sdf_3d::WGSL;

/// Split SDF shader: Notch primitives (prim_type 8)
///
/// Handles concave corners with edge modifiers (scoop, bulge, cut, peak).
/// Full border, gradient, filter, and mask support.
pub const SDF_NOTCH_SHADER: &str = crate::sdf_russl::storage::sdf_notch::WGSL;

/// Vertex-buffer fallback: Core shapes (no VERTEX_STORAGE needed)
///
/// Same as SDF_CORE_SHADER but vs_main reads instance attributes
/// from a vertex buffer instead of the storage buffer.
pub const SDF_CORE_VB_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_core_vb.wgsl"));

/// Vertex-buffer fallback: Shadow primitives
pub const SDF_SHADOW_VB_SHADER: &str = crate::sdf_russl::vertex_buffer::sdf_shadow::WGSL;

/// Vertex-buffer fallback: 3D raymarched shapes
pub const SDF_3D_VB_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_3d_vb.wgsl"));

/// Vertex-buffer fallback: Notch primitives
pub const SDF_NOTCH_VB_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_notch_vb.wgsl"));

/// Data-texture fallback: Core shapes (no storage buffers needed — WebGL2)
///
/// Replaces storage buffer reads with textureLoad from RGBA32F data textures.
/// Also uses vertex buffer instance attributes (inherits from VB variant).
pub const SDF_CORE_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_core_dt.wgsl"));

/// Data-texture fallback: Shadow primitives
pub const SDF_SHADOW_DT_SHADER: &str = crate::sdf_russl::data_texture::sdf_shadow::WGSL;

/// Data-texture fallback: 3D raymarched shapes
pub const SDF_3D_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_3d_dt.wgsl"));

/// Data-texture fallback: Notch primitives
pub const SDF_NOTCH_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_notch_dt.wgsl"));

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
pub const COMPOSITE_SHADER: &str = r#"
// ============================================================================
// Blinc Compositor Shader
// ============================================================================

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct CompositeUniforms {
    opacity: f32,
    blend_mode: u32,
    _padding: vec2<f32>,
}

// Blend modes
const BLEND_NORMAL: u32 = 0u;
const BLEND_MULTIPLY: u32 = 1u;
const BLEND_SCREEN: u32 = 2u;
const BLEND_OVERLAY: u32 = 3u;
const BLEND_DARKEN: u32 = 4u;
const BLEND_LIGHTEN: u32 = 5u;

@group(0) @binding(0) var<uniform> uniforms: CompositeUniforms;
@group(0) @binding(1) var source_texture: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Fullscreen triangle
    let uv = vec2<f32>(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u)
    );

    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);

    return out;
}

fn blend_overlay(base: vec3<f32>, blend: vec3<f32>) -> vec3<f32> {
    return select(
        2.0 * base * blend,
        1.0 - 2.0 * (1.0 - base) * (1.0 - blend),
        base > vec3<f32>(0.5)
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(source_texture, source_sampler, in.uv);

    // Apply opacity
    var result = color;
    result.a *= uniforms.opacity;

    // Note: actual blending with destination happens in the blend state
    // This shader just prepares the source color

    return result;
}
"#;

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
pub const LAYER_COMPOSITE_SHADER: &str = r#"
// ============================================================================
// Layer Composition Shader
// ============================================================================
//
// Composites a layer texture onto a destination with blend modes and opacity.

// Blend mode constants (matching blinc_core::BlendMode)
const BLEND_NORMAL: u32 = 0u;
const BLEND_MULTIPLY: u32 = 1u;
const BLEND_SCREEN: u32 = 2u;
const BLEND_OVERLAY: u32 = 3u;
const BLEND_DARKEN: u32 = 4u;
const BLEND_LIGHTEN: u32 = 5u;
const BLEND_COLOR_DODGE: u32 = 6u;
const BLEND_COLOR_BURN: u32 = 7u;
const BLEND_HARD_LIGHT: u32 = 8u;
const BLEND_SOFT_LIGHT: u32 = 9u;
const BLEND_DIFFERENCE: u32 = 10u;
const BLEND_EXCLUSION: u32 = 11u;

struct LayerUniforms {
    // Source rectangle in layer texture (normalized 0-1)
    source_rect: vec4<f32>,  // x, y, width, height
    // Destination rectangle in viewport (pixels)
    dest_rect: vec4<f32>,    // x, y, width, height
    // Viewport size for coordinate conversion
    viewport_size: vec2<f32>,
    // Layer opacity (0.0 - 1.0)
    opacity: f32,
    // Blend mode (see constants above)
    blend_mode: u32,
    // Clip bounds (x, y, width, height) in pixels
    clip_bounds: vec4<f32>,
    // Clip corner radii (top-left, top-right, bottom-right, bottom-left)
    clip_radius: vec4<f32>,
    // Clip type: 0=none, 1=rect with optional rounded corners
    clip_type: u32,
    // 3D perspective transform (0 = disabled)
    perspective_d: f32,
    sin_rx: f32,
    cos_rx: f32,
    sin_ry: f32,
    cos_ry: f32,
    // In-plane (Z-axis) rotation, applied to the flat composite path.
    // Identity = (0.0, 1.0). Used by motion-bound subtrees whose
    // cached texture must rotate per frame (e.g. cn::spinner's
    // rotate_timeline) without re-baking.
    sin_rz: f32,
    cos_rz: f32,
}

@group(0) @binding(0) var<uniform> uniforms: LayerUniforms;
@group(0) @binding(1) var layer_texture: texture_2d<f32>;
@group(0) @binding(2) var layer_sampler: sampler;
@group(0) @binding(3) var dest_texture: texture_2d<f32>;
@group(0) @binding(4) var dest_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) frag_pos: vec2<f32>,  // Fragment position in viewport pixels
}

// SDF for rounded rectangle clipping
fn sd_rounded_rect_clip(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    // rect: x, y, width, height
    // radii: top-left, top-right, bottom-right, bottom-left
    let center = rect.xy + rect.zw * 0.5;
    let half_size = rect.zw * 0.5;
    let q = abs(p - center) - half_size;

    // Select corner radius based on quadrant
    var r: f32;
    if (p.x < center.x) {
        if (p.y < center.y) {
            r = radii.x;  // top-left
        } else {
            r = radii.w;  // bottom-left
        }
    } else {
        if (p.y < center.y) {
            r = radii.y;  // top-right
        } else {
            r = radii.z;  // bottom-right
        }
    }

    // Clamp radius to half the minimum dimension (CSS spec)
    r = min(r, min(half_size.x, half_size.y));

    let adjusted_q = q + r;
    return length(max(adjusted_q, vec2<f32>(0.0))) + min(max(adjusted_q.x, adjusted_q.y), 0.0) - r;
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Generate quad vertices from vertex index (0-5 for two triangles)
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),  // Top-left
        vec2<f32>(1.0, 0.0),  // Top-right
        vec2<f32>(0.0, 1.0),  // Bottom-left
        vec2<f32>(1.0, 0.0),  // Top-right
        vec2<f32>(1.0, 1.0),  // Bottom-right
        vec2<f32>(0.0, 1.0),  // Bottom-left
    );

    let local_pos = positions[vertex_index];

    // Map to source rectangle UV
    let uv = uniforms.source_rect.xy + local_pos * uniforms.source_rect.zw;

    var out: VertexOutput;

    if (uniforms.perspective_d > 0.0) {
        // 3D perspective compositing: distort quad with perspective projection.
        // The layer was rendered flat; we now apply rotate-x/rotate-y to the composite.
        let center = uniforms.dest_rect.xy + uniforms.dest_rect.zw * 0.5;
        let half = uniforms.dest_rect.zw * 0.5;

        // Local position relative to center (in pixels)
        var p = vec3<f32>(
            (local_pos.x - 0.5) * 2.0 * half.x,
            (local_pos.y - 0.5) * 2.0 * half.y,
            0.0
        );

        // Rotate around Y axis
        let ry_x = p.x * uniforms.cos_ry - p.z * uniforms.sin_ry;
        let ry_z = p.x * uniforms.sin_ry + p.z * uniforms.cos_ry;
        p.x = ry_x;
        p.z = ry_z;

        // Rotate around X axis
        let rx_y = p.y * uniforms.cos_rx - p.z * uniforms.sin_rx;
        let rx_z = p.y * uniforms.sin_rx + p.z * uniforms.cos_rx;
        p.y = rx_y;
        p.z = rx_z;

        // Perspective projection
        let d = uniforms.perspective_d;
        let w = (d + p.z) / d;
        let screen = center + p.xy / w;

        // Convert to NDC
        let ndc = (screen / uniforms.viewport_size) * 2.0 - 1.0;

        // Output with perspective w for correct UV interpolation
        out.position = vec4<f32>(ndc.x * w, -ndc.y * w, 0.0, w);
        out.frag_pos = screen;
    } else {
        // Standard flat compositing (no perspective). Apply in-plane
        // Z rotation around the dest rect's center: shift local_pos
        // into a centered (-0.5..0.5) frame, rotate by (sin_rz,
        // cos_rz), then scale by dest_size and shift back to the
        // destination origin. Identity rotation (sin=0, cos=1) leaves
        // local_pos unchanged, so the existing non-rotating paths see
        // no behavioural change.
        let centered = local_pos - vec2<f32>(0.5, 0.5);
        let half_size = uniforms.dest_rect.zw * 0.5;
        let centered_px = centered * uniforms.dest_rect.zw;
        let rotated_px = vec2<f32>(
            centered_px.x * uniforms.cos_rz - centered_px.y * uniforms.sin_rz,
            centered_px.x * uniforms.sin_rz + centered_px.y * uniforms.cos_rz,
        );
        let dest_pos = uniforms.dest_rect.xy + half_size + rotated_px;
        let ndc = (dest_pos / uniforms.viewport_size) * 2.0 - 1.0;
        out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
        out.frag_pos = dest_pos;
    }

    out.uv = uv;
    return out;
}

// ============================================================================
// Blend Mode Functions
// ============================================================================

fn blend_normal(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return src;
}

fn blend_multiply(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return src * dst;
}

fn blend_screen(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return 1.0 - (1.0 - src) * (1.0 - dst);
}

fn blend_overlay_channel(s: f32, d: f32) -> f32 {
    if (d < 0.5) {
        return 2.0 * s * d;
    } else {
        return 1.0 - 2.0 * (1.0 - s) * (1.0 - d);
    }
}

fn blend_overlay(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        blend_overlay_channel(src.r, dst.r),
        blend_overlay_channel(src.g, dst.g),
        blend_overlay_channel(src.b, dst.b)
    );
}

fn blend_darken(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return min(src, dst);
}

fn blend_lighten(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return max(src, dst);
}

fn blend_color_dodge_channel(s: f32, d: f32) -> f32 {
    if (s >= 1.0) {
        return 1.0;
    }
    return min(1.0, d / (1.0 - s));
}

fn blend_color_dodge(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        blend_color_dodge_channel(src.r, dst.r),
        blend_color_dodge_channel(src.g, dst.g),
        blend_color_dodge_channel(src.b, dst.b)
    );
}

fn blend_color_burn_channel(s: f32, d: f32) -> f32 {
    if (s <= 0.0) {
        return 0.0;
    }
    return 1.0 - min(1.0, (1.0 - d) / s);
}

fn blend_color_burn(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        blend_color_burn_channel(src.r, dst.r),
        blend_color_burn_channel(src.g, dst.g),
        blend_color_burn_channel(src.b, dst.b)
    );
}

fn blend_hard_light(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    // Hard light is overlay with src/dst swapped
    return vec3<f32>(
        blend_overlay_channel(dst.r, src.r),
        blend_overlay_channel(dst.g, src.g),
        blend_overlay_channel(dst.b, src.b)
    );
}

fn blend_soft_light_channel(s: f32, d: f32) -> f32 {
    if (s <= 0.5) {
        return d - (1.0 - 2.0 * s) * d * (1.0 - d);
    } else {
        var g: f32;
        if (d <= 0.25) {
            g = ((16.0 * d - 12.0) * d + 4.0) * d;
        } else {
            g = sqrt(d);
        }
        return d + (2.0 * s - 1.0) * (g - d);
    }
}

fn blend_soft_light(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        blend_soft_light_channel(src.r, dst.r),
        blend_soft_light_channel(src.g, dst.g),
        blend_soft_light_channel(src.b, dst.b)
    );
}

fn blend_difference(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return abs(src - dst);
}

fn blend_exclusion(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return src + dst - 2.0 * src * dst;
}

// Apply blend mode to colors
fn apply_blend_mode(src: vec3<f32>, dst: vec3<f32>, mode: u32) -> vec3<f32> {
    switch (mode) {
        case BLEND_MULTIPLY: { return blend_multiply(src, dst); }
        case BLEND_SCREEN: { return blend_screen(src, dst); }
        case BLEND_OVERLAY: { return blend_overlay(src, dst); }
        case BLEND_DARKEN: { return blend_darken(src, dst); }
        case BLEND_LIGHTEN: { return blend_lighten(src, dst); }
        case BLEND_COLOR_DODGE: { return blend_color_dodge(src, dst); }
        case BLEND_COLOR_BURN: { return blend_color_burn(src, dst); }
        case BLEND_HARD_LIGHT: { return blend_hard_light(src, dst); }
        case BLEND_SOFT_LIGHT: { return blend_soft_light(src, dst); }
        case BLEND_DIFFERENCE: { return blend_difference(src, dst); }
        case BLEND_EXCLUSION: { return blend_exclusion(src, dst); }
        default: { return blend_normal(src, dst); }  // BLEND_NORMAL
    }
}

// ============================================================================
// Fragment Shader
// ============================================================================

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Apply clip if enabled
    if (uniforms.clip_type == 1u) {
        let dist = sd_rounded_rect_clip(in.frag_pos, uniforms.clip_bounds, uniforms.clip_radius);
        if (dist > 0.5) {
            discard;
        }
    }

    // Sample layer texture (LOD 0 — non-uniform flow from discard above)
    let src = textureSampleLevel(layer_texture, layer_sampler, in.uv, 0.0);

    // Apply opacity
    var src_alpha = src.a * uniforms.opacity;

    // Apply anti-aliased clip edge
    if (uniforms.clip_type == 1u) {
        let dist = sd_rounded_rect_clip(in.frag_pos, uniforms.clip_bounds, uniforms.clip_radius);
        let clip_alpha = 1.0 - smoothstep(-0.5, 0.5, dist);
        src_alpha *= clip_alpha;
    }

    // Early out for fully transparent pixels
    if (src_alpha < 0.001) {
        discard;
    }

    // For non-Normal blend modes, sample the destination texture (pre-copied snapshot)
    // and apply the CSS blend function. The result is output as premultiplied alpha
    // so hardware blending (src + dst * (1-srcA)) produces the correct composite:
    //   final = blended * srcA + dst * (1-srcA) = mix(dst, blended, srcA)
    if (uniforms.blend_mode != BLEND_NORMAL) {
        // Compute screen UV from fragment position
        let screen_uv = in.frag_pos / uniforms.viewport_size;
        let dst = textureSampleLevel(dest_texture, dest_sampler, screen_uv, 0.0);

        // Unpremultiply source (src.a > 0 since src_alpha > 0.001 and opacity <= 1.0)
        let src_c = src.rgb / src.a;
        // Unpremultiply destination
        let dst_c = select(dst.rgb / dst.a, vec3<f32>(0.0, 0.0, 0.0), dst.a < 0.001);

        // Apply CSS blend mode
        let blended = apply_blend_mode(src_c, dst_c, uniforms.blend_mode);

        // Output premultiplied for hardware alpha compositing
        return vec4<f32>(blended * src_alpha, src_alpha);
    }

    // Normal blend: premultiplied alpha for hardware blending
    let premultiplied = vec4<f32>(src.rgb * src_alpha, src_alpha);
    return premultiplied;
}
"#;

/// Kawase blur shader for layer effects
///
/// Implements multi-pass Kawase blur which approximates Gaussian blur
/// with better performance. Each pass samples 5 points in an X pattern.
pub const BLUR_SHADER: &str = r#"
// ============================================================================
// Kawase Blur Shader (Layer Effects)
// ============================================================================
//
// Multi-pass blur using Kawase algorithm for efficient Gaussian approximation.
// Run multiple passes with increasing iteration values for stronger blur.

struct BlurUniforms {
    // Inverse texture size (1/width, 1/height)
    texel_size: vec2<f32>,
    // Base blur radius
    radius: f32,
    // Current iteration (0, 1, 2, ...) - affects sample offset
    iteration: u32,
    // Whether to blur alpha (1) or preserve original alpha (0)
    blur_alpha: u32,
    // Padding for 16-byte alignment
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
}

@group(0) @binding(0) var<uniform> uniforms: BlurUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var input_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@fragment
fn fs_kawase_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    // Kawase blur: each pass samples at increasing offsets scaled by the radius
    // More passes spread the blur further; radius controls per-pass spread
    let base_offset = f32(uniforms.iteration) + 0.5;
    let spread = max(uniforms.radius * 0.4, 1.0);
    let offset = base_offset * spread;
    let pixel_offset = offset * uniforms.texel_size;

    // Sample in + pattern (up, down, left, right) instead of X pattern
    let uv_up = clamp(in.uv + vec2<f32>(0.0, -pixel_offset.y), vec2<f32>(0.0), vec2<f32>(1.0));
    let uv_down = clamp(in.uv + vec2<f32>(0.0, pixel_offset.y), vec2<f32>(0.0), vec2<f32>(1.0));
    let uv_left = clamp(in.uv + vec2<f32>(-pixel_offset.x, 0.0), vec2<f32>(0.0), vec2<f32>(1.0));
    let uv_right = clamp(in.uv + vec2<f32>(pixel_offset.x, 0.0), vec2<f32>(0.0), vec2<f32>(1.0));

    // Sample 5 points in + pattern (center, up, down, left, right)
    let s0 = textureSample(input_texture, input_sampler, in.uv);
    let s1 = textureSample(input_texture, input_sampler, uv_up);
    let s2 = textureSample(input_texture, input_sampler, uv_down);
    let s3 = textureSample(input_texture, input_sampler, uv_left);
    let s4 = textureSample(input_texture, input_sampler, uv_right);

    if (uniforms.blur_alpha == 0u) {
        // CSS filter blur mode: blur all RGBA for visible effect on solid-color elements.
        // Alpha-weighted RGB averaging prevents dark fringing at transparent edges.
        // The alpha-restore pass (mode 2) will fix corner softening after all blur passes.
        let total_alpha = s0.a + s1.a + s2.a + s3.a + s4.a;
        let avg_alpha = total_alpha / 5.0;

        if (avg_alpha < 0.001) {
            return vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }

        let weighted_rgb = s0.rgb * s0.a + s1.rgb * s1.a + s2.rgb * s2.a + s3.rgb * s3.a + s4.rgb * s4.a;
        let avg_rgb = weighted_rgb / total_alpha;

        return vec4<f32>(avg_rgb, avg_alpha);
    } else if (uniforms.blur_alpha == 1u) {
        // Shadow blur mode: only blur alpha for shadow shape
        // Output white RGB since drop shadow shader uses uniform color, not texture RGB
        let total_alpha = s0.a + s1.a + s2.a + s3.a + s4.a;
        let avg_alpha = total_alpha / 5.0;

        return vec4<f32>(1.0, 1.0, 1.0, avg_alpha);
    } else {
        // Mode 2: passthrough — sample center pixel only (used for alpha restore pass)
        return s0;
    }
}

// Single-pass box blur for low quality mode
@fragment
fn fs_box_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = i32(uniforms.radius);
    let center = textureSample(input_texture, input_sampler, in.uv);

    if (uniforms.blur_alpha == 0u) {
        // Element blur mode: preserve alpha, blur RGB with alpha weighting
        var weighted_rgb = vec3<f32>(0.0);
        var total_alpha = 0.0;

        for (var x = -radius; x <= radius; x++) {
            for (var y = -radius; y <= radius; y++) {
                let offset = vec2<f32>(f32(x), f32(y)) * uniforms.texel_size;
                let sample_uv = clamp(in.uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));
                let s = textureSample(input_texture, input_sampler, sample_uv);
                weighted_rgb += s.rgb * s.a;
                total_alpha += s.a;
            }
        }

        if (total_alpha < 0.001) {
            return vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }

        let avg_rgb = weighted_rgb / total_alpha;
        return vec4<f32>(avg_rgb, center.a);
    } else {
        // Shadow blur mode: only blur alpha for shadow shape
        // Output white RGB since drop shadow shader uses uniform color, not texture RGB
        var total_alpha = 0.0;
        var samples = 0.0;

        for (var x = -radius; x <= radius; x++) {
            for (var y = -radius; y <= radius; y++) {
                let offset = vec2<f32>(f32(x), f32(y)) * uniforms.texel_size;
                let sample_uv = clamp(in.uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));
                let s = textureSample(input_texture, input_sampler, sample_uv);
                total_alpha += s.a;
                samples += 1.0;
            }
        }

        let avg_alpha = total_alpha / samples;
        return vec4<f32>(1.0, 1.0, 1.0, avg_alpha);
    }
}
"#;

/// Color matrix shader for layer effects
///
/// Applies a 4x5 color transformation matrix to achieve effects like:
/// grayscale, sepia, brightness, contrast, saturation adjustments.
pub const COLOR_MATRIX_SHADER: &str = r#"
// ============================================================================
// Color Matrix Shader (Layer Effects)
// ============================================================================
//
// Applies a 4x5 color transformation matrix:
// [R']   [m0  m1  m2  m3  m4 ]   [R]
// [G'] = [m5  m6  m7  m8  m9 ] * [G]
// [B']   [m10 m11 m12 m13 m14]   [B]
// [A']   [m15 m16 m17 m18 m19]   [A]
//                                [1]

struct ColorMatrixUniforms {
    // 4x5 matrix stored as 5 vec4s (rows)
    row0: vec4<f32>,  // [m0,  m1,  m2,  m3 ]
    row1: vec4<f32>,  // [m5,  m6,  m7,  m8 ]
    row2: vec4<f32>,  // [m10, m11, m12, m13]
    row3: vec4<f32>,  // [m15, m16, m17, m18]
    // Offset column (m4, m9, m14, m19)
    offset: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: ColorMatrixUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var input_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@fragment
fn fs_color_matrix(in: VertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(input_texture, input_sampler, in.uv);

    // Apply 4x4 matrix multiplication + offset
    var result: vec4<f32>;
    result.r = dot(uniforms.row0, src) + uniforms.offset.r;
    result.g = dot(uniforms.row1, src) + uniforms.offset.g;
    result.b = dot(uniforms.row2, src) + uniforms.offset.b;
    result.a = dot(uniforms.row3, src) + uniforms.offset.a;

    // Clamp to valid range
    return clamp(result, vec4<f32>(0.0), vec4<f32>(1.0));
}
"#;

/// Mask image shader for CSS mask-image support
///
/// Multiplies the layer's alpha by the mask image value.
/// Supports alpha mode (use mask alpha) and luminance mode (use mask luminance as alpha).
pub const MASK_IMAGE_SHADER: &str = r#"
// ============================================================================
// Mask Image Shader (Layer Effects)
// ============================================================================
//
// Applies a mask image to a layer: output.a = input.a * mask_value
// mask_mode: 0 = alpha (use mask.a), 1 = luminance (use dot(mask.rgb, luma))

struct MaskUniforms {
    // 0 = alpha, 1 = luminance
    mask_mode: u32,
    _pad: vec3<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: MaskUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var input_sampler: sampler;
@group(0) @binding(3) var mask_texture: texture_2d<f32>;
@group(0) @binding(4) var mask_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@fragment
fn fs_mask(in: VertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(input_texture, input_sampler, in.uv);
    let mask = textureSample(mask_texture, mask_sampler, in.uv);

    // Compute mask value based on mode
    var mask_alpha: f32;
    if (uniforms.mask_mode == 1u) {
        // Luminance mode: use weighted RGB as alpha
        mask_alpha = dot(mask.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)) * mask.a;
    } else {
        // Alpha mode: use mask alpha channel directly
        mask_alpha = mask.a;
    }

    // Multiply source by mask value (premultiplied alpha)
    return vec4<f32>(src.rgb * mask_alpha, src.a * mask_alpha);
}
"#;

/// Shadow colorize shader for layer effects
///
/// Takes a pre-blurred texture and colorizes its alpha channel to create shadow.
/// This is used after Kawase blur for smooth shadows at any radius.
pub const DROP_SHADOW_SHADER: &str = r#"
// ============================================================================
// Shadow Colorize Shader (Layer Effects)
// ============================================================================
//
// Takes a pre-blurred texture and:
// 1. Samples the blurred alpha at offset position for shadow shape
// 2. Colorizes with shadow color
// 3. Composites shadow behind original content
//
// This shader expects the input to already be blurred using Kawase blur passes.

struct DropShadowUniforms {
    // Shadow offset in pixels
    offset: vec2<f32>,
    // Blur radius (stored but not used - blur is pre-applied)
    blur_radius: f32,
    // Spread (expand/contract)
    spread: f32,
    // Shadow color (RGBA)
    color: vec4<f32>,
    // Texture size for offset calculation
    texel_size: vec2<f32>,
    // Padding
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: DropShadowUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var input_sampler: sampler;
// Original (unblurred) texture for compositing
@group(0) @binding(3) var original_texture: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

// Calculate minimum distance to an opaque pixel by sampling in a pattern
// This preserves the shape (including rounded corners) unlike blur-based approaches
fn sample_min_distance(uv: vec2<f32>, radius: f32, texel_size: vec2<f32>) -> f32 {
    // Check center first - if opaque, distance is 0
    let center = textureSampleLevel(original_texture, input_sampler, uv, 0.0);
    if (center.a > 0.5) {
        return 0.0;
    }

    // Sample in concentric rings to find nearest opaque pixel
    // Start with small radius and expand - this gives good quality with fewer samples
    var min_dist = radius + 1.0;

    // Sample at multiple distances and angles
    // Balanced for performance (8x8 = 64 samples max, with early exit)
    let num_angles = 8;
    let num_rings = 8;

    for (var ring = 1; ring <= num_rings; ring++) {
        let dist = (f32(ring) / f32(num_rings)) * radius;
        let pixel_dist = dist;

        for (var i = 0; i < num_angles; i++) {
            let angle = f32(i) * 6.28318530718 / f32(num_angles);
            let offset = vec2<f32>(cos(angle), sin(angle)) * pixel_dist * texel_size;
            let sample_uv = clamp(uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));
            let s = textureSampleLevel(original_texture, input_sampler, sample_uv, 0.0);

            if (s.a > 0.5) {
                min_dist = min(min_dist, dist);
            }
        }

        // Early exit if we found an opaque pixel in this ring
        if (min_dist <= dist) {
            break;
        }
    }

    return min_dist;
}

@fragment
fn fs_drop_shadow(in: VertexOutput) -> @location(0) vec4<f32> {
    // Calculate shadow UV with offset
    let shadow_uv = clamp(in.uv - uniforms.offset * uniforms.texel_size, vec2<f32>(0.0), vec2<f32>(1.0));

    // Find minimum distance to the original shape
    let dist = sample_min_distance(shadow_uv, uniforms.blur_radius, uniforms.texel_size);

    // Convert distance to alpha using smooth falloff
    // At distance 0, alpha = 1. At distance = blur_radius, alpha ≈ 0
    var alpha = 1.0 - smoothstep(0.0, uniforms.blur_radius, dist);

    // Apply spread (expand/contract the shape)
    if (uniforms.spread != 0.0) {
        // Positive spread = larger shadow, negative = smaller
        let adjusted_dist = dist - uniforms.spread;
        alpha = 1.0 - smoothstep(0.0, uniforms.blur_radius, max(adjusted_dist, 0.0));
    }

    // Shadow color with computed alpha
    let shadow_a = uniforms.color.a * alpha;
    let shadow_rgb = uniforms.color.rgb;

    // Sample original (unblurred) content at current position
    let original = textureSampleLevel(original_texture, input_sampler, in.uv, 0.0);

    // Composite shadow behind original using porter-duff "over" for non-premultiplied colors
    let result_a = original.a + shadow_a * (1.0 - original.a);

    if (result_a < 0.001) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let result_rgb = (original.rgb * original.a + shadow_rgb * shadow_a * (1.0 - original.a)) / result_a;

    return vec4<f32>(result_rgb, result_a);
}
"#;

/// Glow effect shader for layer effects
///
/// Creates a radial glow around the shape by:
/// 1. Finding distance to nearest opaque pixel
/// 2. Applying smooth radial falloff based on blur + range
/// 3. Compositing glow behind original content
pub const GLOW_SHADER: &str = r#"
// ============================================================================
// Glow Effect Shader (Layer Effects)
// ============================================================================
//
// Creates an outer glow around shapes by:
// 1. Sampling to find distance to nearest opaque pixel
// 2. Applying Gaussian-like falloff from the shape edge
// 3. Compositing glow behind the original content

struct GlowUniforms {
    // Glow color (RGBA)
    color: vec4<f32>,
    // Blur softness (affects falloff smoothness)
    blur: f32,
    // Glow range (how far the glow extends)
    range: f32,
    // Glow opacity (0-1)
    opacity: f32,
    // Padding for alignment
    _pad0: f32,
    // Texture size for distance calculation
    texel_size: vec2<f32>,
    // Padding
    _pad1: vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: GlowUniforms;
@group(0) @binding(1) var source_texture: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

// Find minimum distance to an opaque pixel within search_radius
fn find_edge_distance(uv: vec2<f32>, search_radius: f32, texel_size: vec2<f32>) -> f32 {
    // Check center first - if opaque, distance is 0
    let center = textureSampleLevel(source_texture, source_sampler, uv, 0.0);
    if (center.a > 0.5) {
        return 0.0;
    }

    // Sample in concentric rings to find nearest opaque pixel
    var min_dist = search_radius + 1.0;  // Start with "not found" value

    // Sample at multiple distances and angles
    // Balanced for performance (8x8 = 64 samples max, with early exit)
    let num_angles = 8;
    let num_rings = 8;

    for (var ring = 1; ring <= num_rings; ring++) {
        let dist = (f32(ring) / f32(num_rings)) * search_radius;
        let pixel_dist = dist;

        for (var i = 0; i < num_angles; i++) {
            let angle = f32(i) * 6.28318530718 / f32(num_angles);
            let offset = vec2<f32>(cos(angle), sin(angle)) * pixel_dist * texel_size;
            let sample_uv = clamp(uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));
            let s = textureSampleLevel(source_texture, source_sampler, sample_uv, 0.0);

            if (s.a > 0.5) {
                min_dist = min(min_dist, dist);
            }
        }

        // Early exit if we found an opaque pixel in this ring
        if (min_dist <= dist) {
            break;
        }
    }

    return min_dist;
}

@fragment
fn fs_glow(in: VertexOutput) -> @location(0) vec4<f32> {
    // Total search distance = blur + range
    let search_radius = uniforms.blur + uniforms.range;

    // Find distance to nearest opaque pixel
    let dist = find_edge_distance(in.uv, search_radius, uniforms.texel_size);

    // Calculate glow alpha with Gaussian-like falloff
    // - At distance 0: we're inside the shape, no glow needed (original shows)
    // - At distance <= range: full glow intensity
    // - At distance > range: fade out over 'blur' distance
    var glow_alpha = 0.0;

    if (dist > 0.0 && dist <= search_radius) {
        // Distance from the extended glow edge
        // If dist <= range, we're in the "full glow" zone
        // If dist > range, we're in the "fade" zone
        if (dist <= uniforms.range) {
            // Inside the glow range - full intensity
            glow_alpha = 1.0;
        } else {
            // Fade zone: distance beyond range, fading over 'blur' distance
            let fade_dist = dist - uniforms.range;
            // Smooth Gaussian-like falloff
            let sigma = uniforms.blur * 0.5;
            glow_alpha = exp(-(fade_dist * fade_dist) / (2.0 * sigma * sigma));
        }
    }

    // Apply opacity
    glow_alpha *= uniforms.opacity * uniforms.color.a;

    // Sample original content
    let original = textureSampleLevel(source_texture, source_sampler, in.uv, 0.0);

    // Glow color (premultiplied)
    let glow_rgb = uniforms.color.rgb;

    // Composite glow behind original using porter-duff "over"
    // Result = Original + Glow * (1 - Original.a)
    let result_a = original.a + glow_alpha * (1.0 - original.a);

    if (result_a < 0.001) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let result_rgb = (original.rgb * original.a + glow_rgb * glow_alpha * (1.0 - original.a)) / result_a;

    return vec4<f32>(result_rgb, result_a);
}
"#;

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
