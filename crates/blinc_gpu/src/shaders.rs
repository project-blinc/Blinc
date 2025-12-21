//! GPU shaders for SDF primitives
//!
//! These shaders render:
//! - Rounded rectangles with borders
//! - Circles and ellipses
//! - Gaussian blur shadows (via error function approximation)
//! - Gradients (linear, radial, conic)
//! - Glass/vibrancy effects (backdrop blur, tint)

/// Main SDF primitive shader
///
/// Renders all basic UI primitives using signed distance fields:
/// - Rounded rectangles with per-corner radius
/// - Circles and ellipses
/// - Shadows with Gaussian blur
/// - Solid colors and gradients
pub const SDF_SHADER: &str = r#"
// ============================================================================
// Blinc SDF Primitive Shader
// ============================================================================

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
}

struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
}

// Primitive types
const PRIM_RECT: u32 = 0u;
const PRIM_CIRCLE: u32 = 1u;
const PRIM_ELLIPSE: u32 = 2u;
const PRIM_SHADOW: u32 = 3u;
const PRIM_INNER_SHADOW: u32 = 4u;
const PRIM_CIRCLE_SHADOW: u32 = 5u;
const PRIM_CIRCLE_INNER_SHADOW: u32 = 6u;

// Fill types
const FILL_SOLID: u32 = 0u;
const FILL_LINEAR_GRADIENT: u32 = 1u;
const FILL_RADIAL_GRADIENT: u32 = 2u;

// Clip types
const CLIP_NONE: u32 = 0u;
const CLIP_RECT: u32 = 1u;
const CLIP_CIRCLE: u32 = 2u;
const CLIP_ELLIPSE: u32 = 3u;

struct Primitive {
    // Bounds (x, y, width, height)
    bounds: vec4<f32>,
    // Corner radii (top-left, top-right, bottom-right, bottom-left)
    corner_radius: vec4<f32>,
    // Fill color (or gradient start color)
    color: vec4<f32>,
    // Gradient end color (for gradients)
    color2: vec4<f32>,
    // Border (width, 0, 0, 0)
    border: vec4<f32>,
    // Border color
    border_color: vec4<f32>,
    // Shadow (offset_x, offset_y, blur, spread)
    shadow: vec4<f32>,
    // Shadow color
    shadow_color: vec4<f32>,
    // Clip bounds (x, y, width, height) for rect clips, (cx, cy, rx, ry) for circle/ellipse
    clip_bounds: vec4<f32>,
    // Clip corner radii (for rounded rect) or (radius_x, radius_y, 0, 0) for ellipse
    clip_radius: vec4<f32>,
    // Type info (primitive_type, fill_type, clip_type, 0)
    type_info: vec4<u32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> primitives: array<Primitive>;

// ============================================================================
// Vertex Shader
// ============================================================================

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let prim = primitives[instance_index];

    // Expand bounds for shadow blur
    let blur_expand = prim.shadow.z * 3.0 + abs(prim.shadow.x) + abs(prim.shadow.y);
    let bounds = vec4<f32>(
        prim.bounds.x - blur_expand,
        prim.bounds.y - blur_expand,
        prim.bounds.z + blur_expand * 2.0,
        prim.bounds.w + blur_expand * 2.0
    );

    // Generate quad vertices (two triangles)
    // 0--1
    // |\ |
    // | \|
    // 3--2
    let quad_verts = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), // 0
        vec2<f32>(1.0, 0.0), // 1
        vec2<f32>(1.0, 1.0), // 2
        vec2<f32>(0.0, 0.0), // 0
        vec2<f32>(1.0, 1.0), // 2
        vec2<f32>(0.0, 1.0), // 3
    );

    let uv = quad_verts[vertex_index];
    let pos = vec2<f32>(
        bounds.x + uv.x * bounds.z,
        bounds.y + uv.y * bounds.w
    );

    // Convert to clip space (-1 to 1)
    let clip_pos = vec2<f32>(
        (pos.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pos.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.uv = pos; // Pass world position for SDF calculation
    out.instance_index = instance_index;

    return out;
}

// ============================================================================
// SDF Functions
// ============================================================================

// Rounded rectangle SDF
fn sd_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let rel = p - center;  // Relative position from center (signed)
    let q = abs(rel) - half_size;

    // Select corner radius based on quadrant
    // radius: (top-left, top-right, bottom-right, bottom-left)
    // In screen coords: Y increases downward, so rel.y < 0 means top half
    var r: f32;
    if rel.y < 0.0 {
        // Top half (y is above center)
        if rel.x > 0.0 {
            r = radius.y; // top-right
        } else {
            r = radius.x; // top-left
        }
    } else {
        // Bottom half (y is below center)
        if rel.x > 0.0 {
            r = radius.z; // bottom-right
        } else {
            r = radius.w; // bottom-left
        }
    }

    // Clamp radius to half the minimum dimension
    r = min(r, min(half_size.x, half_size.y));

    let q_adjusted = q + vec2<f32>(r);
    return length(max(q_adjusted, vec2<f32>(0.0))) + min(max(q_adjusted.x, q_adjusted.y), 0.0) - r;
}

// Circle SDF
fn sd_circle(p: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    return length(p - center) - radius;
}

// Ellipse SDF (approximation)
fn sd_ellipse(p: vec2<f32>, center: vec2<f32>, radii: vec2<f32>) -> f32 {
    let p_centered = p - center;
    let p_norm = p_centered / radii;
    let dist = length(p_norm);
    return (dist - 1.0) * min(radii.x, radii.y);
}

// Error function approximation for Gaussian blur
fn erf(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
    return s * y;
}

// Gaussian shadow for rectangle
fn shadow_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, sigma: f32) -> f32 {
    if sigma < 0.001 {
        // No blur - use hard edge
        let d = sd_rounded_rect(p, origin, size, vec4<f32>(0.0));
        return select(0.0, 1.0, d < 0.0);
    }

    let d = 0.5 * sqrt(2.0) * sigma;
    let half = size * 0.5;
    let center = origin + half;
    let rel = p - center;

    let x = 0.5 * (erf((half.x - rel.x) / d) + erf((half.x + rel.x) / d));
    let y = 0.5 * (erf((half.y - rel.y) / d) + erf((half.y + rel.y) / d));

    return x * y;
}

// Gaussian shadow for circle - radially symmetric blur
fn shadow_circle(p: vec2<f32>, center: vec2<f32>, radius: f32, sigma: f32) -> f32 {
    let dist = length(p - center);

    if sigma < 0.001 {
        // No blur - hard edge
        return select(0.0, 1.0, dist < radius);
    }

    // Gaussian falloff from circle edge
    // erf gives cumulative distribution, we want shadow inside and fading outside
    let d = 0.5 * sqrt(2.0) * sigma;
    return 0.5 * (1.0 + erf((radius - dist) / d));
}

// Calculate clip alpha (1.0 = inside clip, 0.0 = outside)
fn calculate_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>, clip_radius: vec4<f32>, clip_type: u32) -> f32 {
    // If no clip, return 1.0 (fully visible)
    if clip_type == CLIP_NONE {
        return 1.0;
    }

    var clip_d: f32;

    switch clip_type {
        case CLIP_RECT: {
            // Rectangular clip with optional rounded corners
            let clip_origin = clip_bounds.xy;
            let clip_size = clip_bounds.zw;
            clip_d = sd_rounded_rect(p, clip_origin, clip_size, clip_radius);
        }
        case CLIP_CIRCLE: {
            // Circle clip: clip_bounds = (cx, cy, radius, radius)
            let center = clip_bounds.xy;
            let radius = clip_radius.x;
            clip_d = sd_circle(p, center, radius);
        }
        case CLIP_ELLIPSE: {
            // Ellipse clip: clip_bounds = (cx, cy, rx, ry)
            let center = clip_bounds.xy;
            let radii = clip_radius.xy;
            clip_d = sd_ellipse(p, center, radii);
        }
        default: {
            return 1.0;
        }
    }

    // Anti-aliased clip edge
    let aa_width = fwidth(clip_d) * 0.5;
    return 1.0 - smoothstep(-aa_width, aa_width, clip_d);
}

// ============================================================================
// Fragment Shader
// ============================================================================

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let prim = primitives[in.instance_index];
    let p = in.uv;

    let prim_type = prim.type_info.x;
    let fill_type = prim.type_info.y;
    let clip_type = prim.type_info.z;

    // Early clip test - discard if completely outside clip region
    let clip_alpha = calculate_clip_alpha(p, prim.clip_bounds, prim.clip_radius, clip_type);
    if clip_alpha < 0.001 {
        discard;
    }

    let origin = prim.bounds.xy;
    let size = prim.bounds.zw;
    let center = origin + size * 0.5;

    var result = vec4<f32>(0.0);

    // Calculate shadow first (rendered behind) - but NOT for inner shadow primitives
    // InnerShadow primitives handle their own shadow rendering differently
    if (prim.shadow.z > 0.0 || prim.shadow.w != 0.0) && prim_type != PRIM_INNER_SHADOW {
        let shadow_offset = prim.shadow.xy;
        let blur = prim.shadow.z;
        let spread = prim.shadow.w;

        let shadow_origin = origin + shadow_offset - vec2<f32>(spread);
        let shadow_size = size + vec2<f32>(spread * 2.0);

        let shadow_alpha = shadow_rect(p, shadow_origin, shadow_size, blur);
        let shadow_color = prim.shadow_color * shadow_alpha;

        // Premultiply and blend
        result = shadow_color;
    }

    // Calculate main shape SDF
    var d: f32;
    switch prim_type {
        case PRIM_RECT: {
            d = sd_rounded_rect(p, origin, size, prim.corner_radius);
        }
        case PRIM_CIRCLE: {
            let radius = min(size.x, size.y) * 0.5;
            d = sd_circle(p, center, radius);
        }
        case PRIM_ELLIPSE: {
            d = sd_ellipse(p, center, size * 0.5);
        }
        case PRIM_SHADOW: {
            // Shadow-only primitive - mask out the shape area so shadow doesn't render under it
            let shape_d = sd_rounded_rect(p, origin, size, prim.corner_radius);
            let aa_width = fwidth(shape_d) * 0.5;
            let shape_mask = smoothstep(-aa_width, aa_width, shape_d); // 0 inside shape, 1 outside
            result.a *= shape_mask;
            result.a *= clip_alpha;
            return result;
        }
        case PRIM_INNER_SHADOW: {
            // Inner shadow - renders INSIDE the shape only
            let shape_d = sd_rounded_rect(p, origin, size, prim.corner_radius);

            // Hard clip at shape boundary - only render where d < 0 (inside)
            if shape_d > 0.0 {
                discard;
            }

            let blur = max(prim.shadow.z, 0.1);
            let spread = prim.shadow.w;
            let offset = prim.shadow.xy;

            // Inner shadow effect: shadow darkens from outer edge inward
            // Use distance from edge (negative shape_d = distance inside)
            let edge_dist = -shape_d;  // Positive value = how far inside the shape

            // Create shadow falloff from edge toward center
            // At edge (edge_dist ≈ 0): full shadow
            // Further inside (edge_dist > blur + spread): no shadow
            let shadow_range = blur + spread;
            let shadow_alpha = 1.0 - smoothstep(0.0, shadow_range, edge_dist - spread);

            // Apply offset by shifting the shadow calculation
            // Offset shifts which "edge" the shadow appears from
            let offset_effect = dot(normalize(offset + vec2<f32>(0.001)), p - center);
            let offset_bias = clamp(offset_effect / (length(size) * 0.5), -1.0, 1.0) * length(offset);
            let biased_alpha = shadow_alpha * (1.0 + offset_bias * 0.5);

            var inner_result = prim.shadow_color;
            inner_result.a *= clamp(biased_alpha, 0.0, 1.0) * clip_alpha;
            return inner_result;
        }
        case PRIM_CIRCLE_SHADOW: {
            // Circle shadow - radially symmetric Gaussian blur
            let radius = min(size.x, size.y) * 0.5;
            let blur = prim.shadow.z;
            let spread = prim.shadow.w;
            let shadow_offset = prim.shadow.xy;

            let shadow_center = center + shadow_offset;
            let shadow_radius = radius + spread;

            let shadow_alpha = shadow_circle(p, shadow_center, shadow_radius, blur);

            // Mask out the circle area so shadow doesn't render under it
            let circle_d = sd_circle(p, center, radius);
            let aa_width = fwidth(circle_d) * 0.5;
            let shape_mask = smoothstep(-aa_width, aa_width, circle_d); // 0 inside, 1 outside

            var circle_result = prim.shadow_color * shadow_alpha;
            circle_result.a *= shape_mask * clip_alpha;
            return circle_result;
        }
        case PRIM_CIRCLE_INNER_SHADOW: {
            // Circle inner shadow - renders INSIDE the circle only
            let radius = min(size.x, size.y) * 0.5;
            let circle_d = sd_circle(p, center, radius);

            // Hard clip at circle boundary
            if circle_d > 0.0 {
                discard;
            }

            let blur = max(prim.shadow.z, 0.1);
            let spread = prim.shadow.w;
            let offset = prim.shadow.xy;

            // Inner shadow effect: shadow darkens from outer edge inward
            let edge_dist = -circle_d;  // How far inside the circle

            // Create shadow falloff from edge toward center
            let shadow_range = blur + spread;
            let shadow_alpha = 1.0 - smoothstep(0.0, shadow_range, edge_dist - spread);

            // Apply offset
            let offset_effect = dot(normalize(offset + vec2<f32>(0.001)), p - center);
            let offset_bias = clamp(offset_effect / radius, -1.0, 1.0) * length(offset);
            let biased_alpha = shadow_alpha * (1.0 + offset_bias * 0.5);

            var inner_result = prim.shadow_color;
            inner_result.a *= clamp(biased_alpha, 0.0, 1.0) * clip_alpha;
            return inner_result;
        }
        default: {
            d = sd_rounded_rect(p, origin, size, prim.corner_radius);
        }
    }

    // Anti-aliasing: smooth transition at edge
    let aa_width = fwidth(d) * 0.5;
    let fill_alpha = 1.0 - smoothstep(-aa_width, aa_width, d);

    if fill_alpha < 0.001 {
        return result;
    }

    // Determine fill color
    var fill_color: vec4<f32>;
    switch fill_type {
        case FILL_SOLID: {
            fill_color = prim.color;
        }
        case FILL_LINEAR_GRADIENT: {
            // Linear gradient along x-axis of bounds
            let t = clamp((p.x - origin.x) / size.x, 0.0, 1.0);
            fill_color = mix(prim.color, prim.color2, t);
        }
        case FILL_RADIAL_GRADIENT: {
            // Radial gradient from center
            let dist = length(p - center);
            let max_dist = length(size * 0.5);
            let t = clamp(dist / max_dist, 0.0, 1.0);
            fill_color = mix(prim.color, prim.color2, t);
        }
        default: {
            fill_color = prim.color;
        }
    }

    // Handle border
    let border_width = prim.border.x;
    if border_width > 0.0 {
        let inner_d = d + border_width;
        let border_alpha = 1.0 - smoothstep(-aa_width, aa_width, inner_d);

        // Border is the area between outer and inner edges
        let in_border = fill_alpha - border_alpha;

        // Blend: background color inside, border color in border region
        fill_color = mix(fill_color, prim.border_color, clamp(in_border / fill_alpha, 0.0, 1.0));
    }

    fill_color.a *= fill_alpha;

    // Apply clip alpha to both shadow and fill
    fill_color.a *= clip_alpha;
    result.a *= clip_alpha;

    // Blend over shadow (assuming premultiplied alpha)
    result = fill_color + result * (1.0 - fill_color.a);

    return result;
}
"#;

/// Shader for text rendering with SDF glyphs
pub const TEXT_SHADER: &str = r#"
// ============================================================================
// Blinc SDF Text Shader
// ============================================================================

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct TextUniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
}

struct GlyphInstance {
    // Position and size (x, y, width, height)
    bounds: vec4<f32>,
    // UV coordinates in atlas (u_min, v_min, u_max, v_max)
    uv_bounds: vec4<f32>,
    // Text color
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: TextUniforms;
@group(0) @binding(1) var<storage, read> glyphs: array<GlyphInstance>;
@group(0) @binding(2) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(3) var glyph_sampler: sampler;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let glyph = glyphs[instance_index];

    // Generate quad vertices
    let quad_verts = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );

    let local_uv = quad_verts[vertex_index];

    // Position in screen space
    let pos = vec2<f32>(
        glyph.bounds.x + local_uv.x * glyph.bounds.z,
        glyph.bounds.y + local_uv.y * glyph.bounds.w
    );

    // UV in atlas
    let uv = vec2<f32>(
        glyph.uv_bounds.x + local_uv.x * (glyph.uv_bounds.z - glyph.uv_bounds.x),
        glyph.uv_bounds.y + local_uv.y * (glyph.uv_bounds.w - glyph.uv_bounds.y)
    );

    // Convert to clip space
    let clip_pos = vec2<f32>(
        (pos.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pos.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.uv = uv;
    out.color = glyph.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample SDF value from atlas
    let sdf = textureSample(glyph_atlas, glyph_sampler, in.uv).r;

    // SDF threshold (0.5 is the edge)
    let threshold = 0.5;

    // Anti-aliasing width based on screen-space derivatives
    let aa_width = length(vec2<f32>(dpdx(sdf), dpdy(sdf))) * 0.75;

    // Smooth alpha transition
    let alpha = smoothstep(threshold - aa_width, threshold + aa_width, sdf);

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

/// Shader for glass/vibrancy effects (Apple Glass UI style)
///
/// This shader creates frosted glass effects by:
/// 1. Sampling and blurring the backdrop
/// 2. Applying a tint color
/// 3. Adding optional noise for texture
/// 4. Compositing with the shape mask
pub const GLASS_SHADER: &str = r#"
// ============================================================================
// Blinc Glass/Vibrancy Shader
// ============================================================================
// Creates frosted glass effects similar to Apple's vibrancy system

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) screen_uv: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
}

struct GlassUniforms {
    viewport_size: vec2<f32>,
    time: f32,
    _padding: f32,
}

// Glass material types (matching Apple's vibrancy styles)
const GLASS_ULTRA_THIN: u32 = 0u;
const GLASS_THIN: u32 = 1u;
const GLASS_REGULAR: u32 = 2u;
const GLASS_THICK: u32 = 3u;
const GLASS_CHROME: u32 = 4u;

struct GlassPrimitive {
    // Bounds (x, y, width, height)
    bounds: vec4<f32>,
    // Corner radii (top-left, top-right, bottom-right, bottom-left)
    corner_radius: vec4<f32>,
    // Tint color (RGBA)
    tint_color: vec4<f32>,
    // Glass parameters (blur_radius, saturation, brightness, noise_amount)
    params: vec4<f32>,
    // Type info (glass_type, 0, 0, 0)
    type_info: vec4<u32>,
}

@group(0) @binding(0) var<uniform> uniforms: GlassUniforms;
@group(0) @binding(1) var<storage, read> primitives: array<GlassPrimitive>;
@group(0) @binding(2) var backdrop_texture: texture_2d<f32>;
@group(0) @binding(3) var backdrop_sampler: sampler;

// ============================================================================
// Vertex Shader
// ============================================================================

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let prim = primitives[instance_index];
    let bounds = prim.bounds;

    // Generate quad vertices
    let quad_verts = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );

    let local_uv = quad_verts[vertex_index];
    let pos = vec2<f32>(
        bounds.x + local_uv.x * bounds.z,
        bounds.y + local_uv.y * bounds.w
    );

    // Convert to clip space
    let clip_pos = vec2<f32>(
        (pos.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pos.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.uv = pos;
    out.screen_uv = pos / uniforms.viewport_size;
    out.instance_index = instance_index;

    return out;
}

// ============================================================================
// SDF and Blur Functions
// ============================================================================

fn sd_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let rel = p - center;
    let q = abs(rel) - half_size;

    // Select corner radius based on quadrant
    // radius: (top-left, top-right, bottom-right, bottom-left)
    // In screen coords: Y increases downward, so rel.y < 0 means top half
    var r: f32;
    if rel.y < 0.0 {
        if rel.x > 0.0 {
            r = radius.y; // top-right
        } else {
            r = radius.x; // top-left
        }
    } else {
        if rel.x > 0.0 {
            r = radius.z; // bottom-right
        } else {
            r = radius.w; // bottom-left
        }
    }

    r = min(r, min(half_size.x, half_size.y));
    let q_adjusted = q + vec2<f32>(r);
    return length(max(q_adjusted, vec2<f32>(0.0))) + min(max(q_adjusted.x, q_adjusted.y), 0.0) - r;
}

// Hash function for noise
fn hash(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

// Smooth noise
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    return mix(
        mix(hash(i + vec2<f32>(0.0, 0.0)), hash(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y
    );
}

// Gaussian weight function
fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

// High quality blur using a proper Gaussian kernel with rotated samples
// This avoids the checkered pattern by using more samples with proper weighting
fn blur_backdrop(uv: vec2<f32>, blur_radius: f32) -> vec4<f32> {
    if blur_radius < 0.5 {
        return textureSample(backdrop_texture, backdrop_sampler, uv);
    }

    let texel_size = 1.0 / uniforms.viewport_size;
    let sigma = blur_radius * 0.5;

    // Start with center sample (highest weight)
    var color = textureSample(backdrop_texture, backdrop_sampler, uv);
    var total_weight = 1.0;

    // Use rotated sample pattern to avoid axis-aligned artifacts
    // Golden angle rotation provides good distribution
    let golden_angle = 2.39996323; // radians

    // Number of samples scales with blur radius for quality
    let num_rings = 3;
    let samples_per_ring = 8;

    for (var ring = 1; ring <= num_rings; ring++) {
        let ring_radius = blur_radius * f32(ring) / f32(num_rings);
        let ring_offset = ring_radius * texel_size;

        for (var i = 0; i < samples_per_ring; i++) {
            // Rotate each sample by golden angle offset
            let angle = f32(i) * (6.283185 / f32(samples_per_ring)) + f32(ring) * golden_angle;
            let offset = vec2<f32>(cos(angle), sin(angle)) * ring_offset;

            let sample_pos = uv + offset;
            let weight = gaussian_weight(ring_radius, sigma);

            color += textureSample(backdrop_texture, backdrop_sampler, sample_pos) * weight;
            total_weight += weight;
        }
    }

    return color / total_weight;
}

// Apply saturation adjustment
fn adjust_saturation(color: vec3<f32>, saturation: f32) -> vec3<f32> {
    let luminance = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    return mix(vec3<f32>(luminance), color, saturation);
}

// Calculate SDF gradient (normal direction pointing outward from shape)
fn sdf_gradient(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> vec2<f32> {
    let eps = 0.5;
    let d = sd_rounded_rect(p, origin, size, radius);
    let dx = sd_rounded_rect(p + vec2<f32>(eps, 0.0), origin, size, radius) - d;
    let dy = sd_rounded_rect(p + vec2<f32>(0.0, eps), origin, size, radius) - d;
    let g = vec2<f32>(dx, dy);
    let len = length(g);
    if len < 0.001 {
        return vec2<f32>(0.0, -1.0);
    }
    return g / len;
}

// Fresnel effect for glass edge highlights
fn fresnel(normal: vec2<f32>, view_dir: vec2<f32>, power: f32) -> f32 {
    let ndotv = max(dot(normal, view_dir), 0.0);
    return pow(1.0 - ndotv, power);
}

// ============================================================================
// Fragment Shader - Liquid Glass Effect
// ============================================================================

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let prim = primitives[in.instance_index];
    let p = in.uv;

    let origin = prim.bounds.xy;
    let size = prim.bounds.zw;
    let center = origin + size * 0.5;

    // Calculate shape mask
    let d = sd_rounded_rect(p, origin, size, prim.corner_radius);
    let aa_width = fwidth(d) * 0.5;
    let mask = 1.0 - smoothstep(-aa_width, aa_width, d);

    if mask < 0.001 {
        discard;
    }

    // Glass parameters
    let blur_radius = prim.params.x;
    let saturation = prim.params.y;
    let brightness = prim.params.z;
    let noise_amount = prim.params.w;

    // ========================================================================
    // LIQUID GLASS EFFECT
    // ========================================================================

    // Edge zone calculations
    let edge_zone = max(8.0, blur_radius * 0.6);
    let edge_factor = 1.0 - smoothstep(0.0, edge_zone, abs(d));
    let inner_factor = smoothstep(0.0, edge_zone * 2.0, abs(d)); // 1.0 in center

    // Get surface normal (gradient of SDF)
    let normal = sdf_gradient(p, origin, size, prim.corner_radius);

    // Simulated view direction (from top-left light source)
    let light_dir = normalize(vec2<f32>(-0.6, -0.8));
    let view_dir = normalize(center - p);

    // Fresnel for edge brightness (glass reflects more at glancing angles)
    let edge_fresnel = fresnel(normal, view_dir, 2.5);

    // ========================================================================
    // REFRACTION - Light bends through the glass surface
    // ========================================================================
    let refraction_strength = 0.02 * blur_radius;
    let texel_size = 1.0 / uniforms.viewport_size;

    // Refraction offset based on surface normal and edge proximity
    let refraction_offset = normal * edge_factor * refraction_strength;
    let refracted_uv = in.screen_uv + refraction_offset * texel_size * uniforms.viewport_size.y;

    // Sample blurred backdrop with refraction
    var backdrop = blur_backdrop(refracted_uv, blur_radius);

    // Apply saturation
    backdrop = vec4<f32>(adjust_saturation(backdrop.rgb, saturation), backdrop.a);

    // Apply brightness
    backdrop = vec4<f32>(backdrop.rgb * brightness, backdrop.a);

    // ========================================================================
    // DEPTH SHADING - Glass appears thicker at edges
    // ========================================================================
    // Darken edges to simulate light absorption through thicker glass
    let depth_darkening = mix(0.92, 1.0, inner_factor);
    backdrop = vec4<f32>(backdrop.rgb * depth_darkening, backdrop.a);

    // ========================================================================
    // SPECULAR HIGHLIGHTS - Bright reflections on edges
    // ========================================================================
    // Edge highlight (rim lighting effect)
    let rim_intensity = edge_fresnel * 0.4;

    // Specular highlight from light direction
    let reflect_dir = reflect(light_dir, normal);
    let spec_angle = max(dot(reflect_dir, view_dir), 0.0);
    let specular = pow(spec_angle, 16.0) * edge_factor * 0.6;

    // Top edge highlight (stronger on upper edges facing light)
    let top_highlight = max(-normal.y, 0.0) * edge_factor * 0.25;

    // Combine highlights
    let total_highlight = rim_intensity + specular + top_highlight;
    let highlight_color = vec3<f32>(1.0, 1.0, 1.0);

    // ========================================================================
    // INNER GLOW - Subtle brightness in center
    // ========================================================================
    let inner_glow = inner_factor * 0.05;

    // ========================================================================
    // COMBINE EFFECTS
    // ========================================================================

    // Add subtle noise for texture
    var noise_contrib = 0.0;
    if noise_amount > 0.0 {
        let n = noise(p * 0.5 + vec2<f32>(uniforms.time * 0.1, 0.0));
        noise_contrib = (n - 0.5) * noise_amount * 0.08;
    }

    // Apply tint color
    let tint = prim.tint_color;
    var result = vec3<f32>(
        mix(backdrop.rgb, tint.rgb, tint.a)
    );

    // Add highlights
    result = result + highlight_color * total_highlight;

    // Add inner glow
    result = result + vec3<f32>(inner_glow);

    // Add noise
    result = result + vec3<f32>(noise_contrib);

    // Apply glass type specific adjustments
    let glass_type = prim.type_info.x;
    switch glass_type {
        case GLASS_ULTRA_THIN: {
            // Very subtle effect - reduce highlights
            result = mix(backdrop.rgb, result, 0.4);
        }
        case GLASS_THIN: {
            result = mix(backdrop.rgb, result, 0.6);
        }
        case GLASS_REGULAR: {
            // Default - full liquid glass effect
        }
        case GLASS_THICK: {
            // Enhance depth and highlights
            result = mix(backdrop.rgb, result, 1.0);
            result = result + highlight_color * total_highlight * 0.3; // Extra highlights
        }
        case GLASS_CHROME: {
            // Metallic liquid glass
            let chrome_tint = vec3<f32>(0.92, 0.92, 0.96);
            result = mix(result, chrome_tint, 0.15);
            result = result + highlight_color * total_highlight * 0.5; // Strong highlights
        }
        default: {
            // Regular glass
        }
    }

    // Clamp to valid range
    result = clamp(result, vec3<f32>(0.0), vec3<f32>(1.0));

    return vec4<f32>(result, mask);
}
"#;

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
pub const PATH_SHADER: &str = r#"
// ============================================================================
// Path Rendering Shader
// ============================================================================
//
// Renders tessellated vector paths as colored triangles.
// Uses per-vertex colors for fills and strokes.

struct Uniforms {
    // viewport_size (vec2) + padding (vec2) = 16 bytes, offset 0
    viewport_size: vec2<f32>,
    opacity: f32,
    _pad0: f32,
    // 3x3 transform stored as 3 vec4s (xyz used, w is padding) = 48 bytes, offset 16
    transform_row0: vec4<f32>,
    transform_row1: vec4<f32>,
    transform_row2: vec4<f32>,
}
// Total: 64 bytes

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Reconstruct transform matrix and apply
    let p = vec3<f32>(in.position, 1.0);
    let transformed = vec3<f32>(
        dot(uniforms.transform_row0.xyz, p),
        dot(uniforms.transform_row1.xyz, p),
        dot(uniforms.transform_row2.xyz, p)
    );

    // Convert to clip space (-1 to 1)
    let clip_pos = vec2<f32>(
        (transformed.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (transformed.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.color = in.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;
    color.a *= uniforms.opacity;
    return color;
}
"#;
