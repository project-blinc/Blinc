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
const PRIM_TEXT: u32 = 7u;  // Text glyph - samples from atlas texture

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
    // Gradient parameters: linear (x1, y1, x2, y2), radial (cx, cy, r, 0) in user space
    gradient_params: vec4<f32>,
    // Type info (primitive_type, fill_type, clip_type, 0)
    type_info: vec4<u32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> primitives: array<Primitive>;
// Glyph atlas textures for unified text rendering
@group(0) @binding(2) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(3) var glyph_sampler: sampler;
@group(0) @binding(4) var color_glyph_atlas: texture_2d<f32>;

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
        case PRIM_TEXT: {
            // Text glyph - sample from glyph atlas
            // UV bounds are stored in gradient_params: (u_min, v_min, u_max, v_max)
            // fill_type stores is_color flag (1 = color emoji, 0 = grayscale)
            let uv_bounds = prim.gradient_params;
            let is_color = fill_type == 1u;

            // Calculate UV within the glyph quad
            // p is in screen coordinates, bounds defines the glyph quad
            let local_uv = (p - origin) / size;

            // Map to atlas UV coordinates
            let atlas_uv = uv_bounds.xy + local_uv * (uv_bounds.zw - uv_bounds.xy);

            var text_result: vec4<f32>;
            if is_color {
                // Color emoji - sample RGBA directly from color atlas
                text_result = textureSample(color_glyph_atlas, glyph_sampler, atlas_uv);
            } else {
                // Grayscale text - sample coverage from R channel, apply color tint
                let coverage = textureSample(glyph_atlas, glyph_sampler, atlas_uv).r;
                // Apply gamma correction for crisp text rendering
                let gamma_coverage = pow(coverage, 0.7);
                text_result = vec4<f32>(prim.color.rgb, prim.color.a * gamma_coverage);
            }

            // Apply clip alpha
            text_result.a *= clip_alpha;

            // Soft anti-aliased clipping at edges
            let edge_aa = 1.0;
            let clip_edge_alpha = smoothstep(0.0, edge_aa, min(
                min(p.x - prim.clip_bounds.x, prim.clip_bounds.x + prim.clip_bounds.z - p.x),
                min(p.y - prim.clip_bounds.y, prim.clip_bounds.y + prim.clip_bounds.w - p.y)
            ));
            text_result.a *= clip_edge_alpha;

            return text_result;
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
            // Linear gradient using gradient_params (x1, y1, x2, y2) in user space
            let g_start = prim.gradient_params.xy;
            let g_end = prim.gradient_params.zw;
            let g_dir = g_end - g_start;
            let g_len_sq = dot(g_dir, g_dir);

            var t: f32;
            if (g_len_sq > 0.0001) {
                // Project current position onto gradient line
                let proj = p - g_start;
                t = clamp(dot(proj, g_dir) / g_len_sq, 0.0, 1.0);
            } else {
                t = 0.0;
            }
            fill_color = mix(prim.color, prim.color2, t);
        }
        case FILL_RADIAL_GRADIENT: {
            // Radial gradient using gradient_params (cx, cy, radius, 0) in user space
            let g_center = prim.gradient_params.xy;
            let g_radius = prim.gradient_params.z;

            let dist = length(p - g_center);
            let t = clamp(dist / max(g_radius, 0.001), 0.0, 1.0);
            fill_color = mix(prim.color, prim.color2, t);
        }
        default: {
            fill_color = prim.color;
        }
    }

    // Handle border with stable anti-aliasing
    // The border is the ring between the outer shape edge and the inner edge (inset by border_width)
    let border_width = prim.border.x;
    if border_width > 0.0 {
        // Distance to inner edge (positive inside the inner area)
        let inner_d = d + border_width;

        // For thin borders, use a fixed aa_width to avoid fwidth instability.
        // The fwidth approach causes jitter on scroll because screen-space derivatives
        // change with subpixel position. A fixed 0.5-1.0 pixel aa provides stable edges.
        let stable_aa = 0.5;

        // Inner coverage using stable anti-aliasing (0 outside inner, 1 inside inner)
        let inner_coverage = 1.0 - smoothstep(-stable_aa, stable_aa, inner_d);

        // The border occupies the region between outer edge (d < 0) and inner edge (inner_d < 0).
        // We want to blend the border color in this ring.
        // border_blend = 1 when in the border ring, 0 when inside the inner area
        // Using the stable inner edge prevents jitter
        let border_blend = 1.0 - inner_coverage;

        // Only apply border color where we're actually inside the shape (fill_alpha > 0)
        // Use smoothstep clamping to avoid harsh transitions
        fill_color = mix(fill_color, prim.border_color, border_blend * step(0.001, fill_alpha));
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
///
/// Supports both grayscale text glyphs and color emoji:
/// - Grayscale: samples R channel from glyph_atlas, multiplies with color
/// - Color emoji: samples RGBA from color_atlas, uses texture color directly
pub const TEXT_SHADER: &str = r#"
// ============================================================================
// Blinc SDF Text Shader
// ============================================================================
// Supports grayscale text and color emoji via separate atlases

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_pos: vec2<f32>,
    @location(3) @interpolate(flat) clip_bounds: vec4<f32>,
    @location(4) @interpolate(flat) is_color: f32,
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
    // Clip bounds (x, y, width, height) - set to large values for no clip
    clip_bounds: vec4<f32>,
    // Flags: [is_color, unused, unused, unused]
    // is_color: 1.0 = color emoji (use color_atlas), 0.0 = grayscale (use glyph_atlas)
    flags: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: TextUniforms;
@group(0) @binding(1) var<storage, read> glyphs: array<GlyphInstance>;
@group(0) @binding(2) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(3) var glyph_sampler: sampler;
@group(0) @binding(4) var color_atlas: texture_2d<f32>;

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
    out.world_pos = pos;
    out.clip_bounds = glyph.clip_bounds;
    out.is_color = glyph.flags.x;

    return out;
}

// Calculate clip alpha for rectangular clip region
fn calculate_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>) -> f32 {
    // Check if clipping is active (default bounds are very large negative values)
    if clip_bounds.x < -5000.0 {
        return 1.0;
    }

    // Clip bounds are (x, y, width, height)
    let clip_min = clip_bounds.xy;
    let clip_max = clip_bounds.xy + clip_bounds.zw;

    // Calculate signed distance to clip rect edges
    let d_left = p.x - clip_min.x;
    let d_right = clip_max.x - p.x;
    let d_top = p.y - clip_min.y;
    let d_bottom = clip_max.y - p.y;

    // Minimum distance to any edge (negative = outside)
    let d = min(min(d_left, d_right), min(d_top, d_bottom));

    // Soft anti-aliased edge (1 pixel transition)
    return clamp(d + 0.5, 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Calculate clip alpha first - discard if completely outside
    let clip_alpha = calculate_clip_alpha(in.world_pos, in.clip_bounds);
    if clip_alpha < 0.001 {
        discard;
    }

    // Check if this is a color emoji glyph
    if in.is_color > 0.5 {
        // Color emoji: sample RGBA from color atlas, use texture color directly
        let emoji_color = textureSample(color_atlas, glyph_sampler, in.uv);
        // Apply clip alpha only - keep original emoji colors
        return vec4<f32>(emoji_color.rgb, emoji_color.a * clip_alpha);
    } else {
        // Grayscale text: sample coverage from glyph atlas, apply tint color
        let coverage = textureSample(glyph_atlas, glyph_sampler, in.uv).r;

        // Use coverage directly with slight gamma correction for cleaner edges
        // The rasterizer provides good coverage values - we just need to
        // apply a subtle curve to sharpen without losing anti-aliasing
        // pow(x, 0.7) brightens mid-tones, making strokes appear crisper
        let aa_alpha = pow(coverage, 0.7);

        // Apply both text alpha and clip alpha
        return vec4<f32>(in.color.rgb, in.color.a * aa_alpha * clip_alpha);
    }
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
    // Glass parameters 2 (border_thickness, light_angle, shadow_blur, shadow_opacity)
    params2: vec4<f32>,
    // Type info (glass_type, shadow_offset_x_bits, shadow_offset_y_bits, 0)
    type_info: vec4<u32>,
    // Clip bounds (x, y, width, height) for clamping blur samples
    clip_bounds: vec4<f32>,
    // Clip corner radii (for rounded rect clips)
    clip_radius: vec4<f32>,
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

    // Expand bounds for shadow blur
    let shadow_blur = prim.params2.z;
    let shadow_offset_x = bitcast<f32>(prim.type_info.y);
    let shadow_offset_y = bitcast<f32>(prim.type_info.z);
    let shadow_expand = shadow_blur * 3.0 + abs(shadow_offset_x) + abs(shadow_offset_y);

    let bounds = vec4<f32>(
        prim.bounds.x - shadow_expand,
        prim.bounds.y - shadow_expand,
        prim.bounds.z + shadow_expand * 2.0,
        prim.bounds.w + shadow_expand * 2.0
    );

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

// Error function approximation for Gaussian blur
fn erf(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
    return s * y;
}

// Gaussian shadow for rounded rectangle using SDF
// This properly respects corner radii for accurate rounded rect shadows
fn shadow_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>, sigma: f32) -> f32 {
    // Get SDF distance (negative inside, positive outside)
    let d = sd_rounded_rect(p, origin, size, radius);

    if sigma < 0.001 {
        // No blur - hard edge
        return select(0.0, 1.0, d < 0.0);
    }

    // Use SDF for Gaussian-like falloff
    // erf-based smooth transition from inside to outside
    // This creates a proper soft shadow that follows the rounded rect shape
    let blur_factor = 0.5 * sqrt(2.0) * sigma;
    return 0.5 * (1.0 - erf(d / blur_factor));
}

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

// Calculate clip alpha for rectangular clip region (for scroll containers)
fn calculate_glass_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>) -> f32 {
    // Check if clipping is active (default bounds are very large negative values)
    if clip_bounds.x < -5000.0 {
        return 1.0;
    }

    // Clip bounds are (x, y, width, height)
    let clip_min = clip_bounds.xy;
    let clip_max = clip_bounds.xy + clip_bounds.zw;

    // Calculate signed distance to clip rect edges
    let d_left = p.x - clip_min.x;
    let d_right = clip_max.x - p.x;
    let d_top = p.y - clip_min.y;
    let d_bottom = clip_max.y - p.y;

    // Minimum distance to any edge (negative = outside)
    let d = min(min(d_left, d_right), min(d_top, d_bottom));

    // Soft anti-aliased edge (1 pixel transition)
    return clamp(d + 0.5, 0.0, 1.0);
}

// High quality blur using spiral sampling pattern
// More samples and better distribution to eliminate checkered artifacts
fn blur_backdrop(uv: vec2<f32>, blur_radius: f32) -> vec4<f32> {
    if blur_radius < 0.5 {
        return textureSample(backdrop_texture, backdrop_sampler, uv);
    }

    let texel_size = 1.0 / uniforms.viewport_size;
    let sigma = blur_radius * 0.5;

    // Start with center sample (highest weight)
    var color = textureSample(backdrop_texture, backdrop_sampler, uv);
    var total_weight = 1.0;

    // Golden angle spiral for optimal sample distribution
    // This eliminates checkered patterns by avoiding regular grids
    let golden_angle = 2.39996323; // radians (137.5 degrees)

    // More samples for smoother blur - 5 rings with 12 samples each = 60 samples
    let num_rings = 5;
    let samples_per_ring = 12;

    for (var ring = 1; ring <= num_rings; ring++) {
        // Non-linear ring spacing - more samples near center
        let ring_t = f32(ring) / f32(num_rings);
        let ring_radius = blur_radius * ring_t * ring_t; // Quadratic falloff
        let ring_offset = ring_radius * texel_size;

        for (var i = 0; i < samples_per_ring; i++) {
            // Golden angle rotation + ring offset for spiral pattern
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

// High quality blur with clip bounds for scroll containers
// Samples are clamped to the clip region to prevent blur bleeding
fn blur_backdrop_clipped(uv: vec2<f32>, blur_radius: f32, clip_bounds: vec4<f32>) -> vec4<f32> {
    // Convert clip bounds from (x, y, width, height) to (min_x, min_y, max_x, max_y) in UV space
    let clip_min = clip_bounds.xy / uniforms.viewport_size;
    let clip_max = (clip_bounds.xy + clip_bounds.zw) / uniforms.viewport_size;

    // Check if clipping is active (default bounds are very large)
    let has_clip = clip_bounds.x > -5000.0;

    if blur_radius < 0.5 {
        let clamped_uv = select(uv, clamp(uv, clip_min, clip_max), has_clip);
        return textureSample(backdrop_texture, backdrop_sampler, clamped_uv);
    }

    let texel_size = 1.0 / uniforms.viewport_size;
    let sigma = blur_radius * 0.5;

    // Start with center sample (highest weight)
    let center_uv = select(uv, clamp(uv, clip_min, clip_max), has_clip);
    var color = textureSample(backdrop_texture, backdrop_sampler, center_uv);
    var total_weight = 1.0;

    // Golden angle spiral for optimal sample distribution
    let golden_angle = 2.39996323; // radians (137.5 degrees)

    // More samples for smoother blur - 5 rings with 12 samples each = 60 samples
    let num_rings = 5;
    let samples_per_ring = 12;

    for (var ring = 1; ring <= num_rings; ring++) {
        // Non-linear ring spacing - more samples near center
        let ring_t = f32(ring) / f32(num_rings);
        let ring_radius = blur_radius * ring_t * ring_t; // Quadratic falloff
        let ring_offset = ring_radius * texel_size;

        for (var i = 0; i < samples_per_ring; i++) {
            // Golden angle rotation + ring offset for spiral pattern
            let angle = f32(i) * (6.283185 / f32(samples_per_ring)) + f32(ring) * golden_angle;
            let offset = vec2<f32>(cos(angle), sin(angle)) * ring_offset;

            var sample_pos = uv + offset;

            // Clamp sample position to clip bounds if clipping is active
            if has_clip {
                sample_pos = clamp(sample_pos, clip_min, clip_max);
            }

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

// ============================================================================
// Fragment Shader - iOS 26 Liquid Glass Effect
// ============================================================================
// Liquid glass = smooth rounded bevel, NOT hard edge lines
// The "liquid" feel comes from wide, gentle transitions that look organic

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let prim = primitives[in.instance_index];
    let p = in.uv;

    // Calculate clip alpha first - discard if completely outside clip bounds
    let clip_alpha = calculate_glass_clip_alpha(p, prim.clip_bounds);
    if clip_alpha < 0.001 {
        discard;
    }

    let origin = prim.bounds.xy;
    let size = prim.bounds.zw;

    // Shadow parameters
    let shadow_blur = prim.params2.z;
    let shadow_opacity = prim.params2.w;
    let shadow_offset_x = bitcast<f32>(prim.type_info.y);
    let shadow_offset_y = bitcast<f32>(prim.type_info.z);

    // Calculate SDF with smooth anti-aliasing
    let d = sd_rounded_rect(p, origin, size, prim.corner_radius);
    let aa = fwidth(d) * 2.0; // Wide AA for smooth edges

    // Smooth mask - combine with clip alpha
    let mask = (1.0 - smoothstep(-aa, aa, d)) * clip_alpha;

    // ========================================================================
    // DROP SHADOW (rendered as pure shadow, no glass effects)
    // ========================================================================
    // Shadow is a simple soft rectangle behind the glass - no bevel, no refraction
    let has_shadow = shadow_opacity > 0.001 && shadow_blur > 0.001;
    var shadow_color_premult = vec4<f32>(0.0);

    if has_shadow {
        let shadow_origin = origin + vec2<f32>(shadow_offset_x, shadow_offset_y);
        let shadow_alpha = shadow_rounded_rect(p, shadow_origin, size, prim.corner_radius, shadow_blur);
        // Apply clip alpha to shadow as well
        shadow_color_premult = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha * shadow_opacity * clip_alpha);

        // If we're completely outside the glass panel, just render the shadow
        if mask < 0.001 {
            if shadow_alpha > 0.001 && clip_alpha > 0.001 {
                return shadow_color_premult;
            }
            discard;
        }
    } else {
        // No shadow - discard if outside glass
        if mask < 0.001 {
            discard;
        }
    }

    // Glass parameters
    let blur_radius = prim.params.x;
    let saturation = prim.params.y;
    let brightness = prim.params.z;
    let noise_amount = prim.params.w;

    // Distance from edge (0 at edge, positive going inward)
    let inner_dist = max(0.0, -d);

    // ========================================================================
    // TWO-LAYER LIQUID GLASS (Apple-style)
    // ========================================================================
    // Layer 1: EDGE BEVEL - wider rim with strong light bending for liquid effect
    // Layer 2: FLAT CENTER - undistorted frosted glass surface
    // The edge seamlessly connects to the flat center.

    // Edge bevel thickness - concentrated near edge for sharp liquid bevel
    let edge_thickness = min(25.0, min(size.x, size.y) * 0.2);

    // Progress through edge zone: 0 = at glass edge, 1 = into flat center
    let edge_progress = clamp(inner_dist / edge_thickness, 0.0, 1.0);

    // For depth shading (used later)
    let bevel = 1.0 - edge_progress;

    // ========================================================================
    // EDGE BEVEL REFRACTION - Liquid Glass Effect
    // ========================================================================
    // The refraction follows the edge NORMAL direction, not radial from center.
    // This creates proper glass rim bending where light bends perpendicular to the edge.

    // Get SDF gradient (points outward from shape - this IS the edge normal)
    let edge_normal = sdf_gradient(p, origin, size, prim.corner_radius);

    // Refraction strength: strongest at outer edge, fades smoothly to center
    // Using quadratic falloff concentrated at edge for visible bevel effect
    let refract_strength = bevel * bevel;

    // Refraction multiplier from type_info.w (0.0 = no refraction, 1.0 = full refraction)
    // We use a sentinel value: if type_info.w == 0 (unset), default to 1.0 (full refraction)
    // To disable refraction, set type_info.w to the bits of a small negative number like -1.0
    // This way 0 (unset) = full refraction, any other value = that value's refraction
    let refraction_mult = bitcast<f32>(prim.type_info.w);
    // Check if explicitly set (non-zero bits) - if unset (0), use 1.0 for backwards compat
    // If set to 0.0f (which has bits 0x00000000), we need a different sentinel
    // Solution: use -1.0 as "use explicit value" flag in the sign bit
    let is_explicitly_set = (prim.type_info.w & 0x80000000u) != 0u; // Check sign bit
    let explicit_value = abs(refraction_mult); // Remove sign to get actual value
    let effective_refract_mult = select(1.0, explicit_value, is_explicitly_set);

    // Offset UV along edge normal - sample backdrop from OUTSIDE the shape
    // This creates the "looking through curved glass rim" effect where
    // content appears pulled inward at the bevel
    // The offset is in PIXELS, then converted to UV space
    // Strong distortion for clearly visible bevel curve
    let refract_pixels = refract_strength * 60.0 * effective_refract_mult; // Up to 60 pixels of displacement at edge
    let refract_offset = edge_normal * refract_pixels;

    // Apply refraction - ADD offset to sample from outside (pulls content inward visually)
    let refracted_uv = in.screen_uv + refract_offset / uniforms.viewport_size;

    // ========================================================================
    // APPLE LIQUID GLASS EFFECT (WWDC25 Style)
    // ========================================================================
    // Key characteristics from reference:
    // 1. Nearly transparent interior - minimal blur/frost
    // 2. Crisp bright edge highlight line along perimeter
    // 3. Subtle edge shadow just inside the highlight
    // 4. Very subtle refraction - background barely distorted
    // 5. Optional chromatic aberration at edges

    // ========================================================================
    // BACKDROP - Blur based on blur_radius parameter
    // ========================================================================
    // Use blur_radius directly - user controls the blur amount
    // The blur is applied to the interior, edges remain clear due to refraction
    let effective_blur = blur_radius; // Direct control - user sets exact blur amount
    // Use clipped blur to prevent sampling outside scroll containers
    var backdrop = blur_backdrop_clipped(refracted_uv, effective_blur, prim.clip_bounds);
    backdrop = vec4<f32>(adjust_saturation(backdrop.rgb, saturation), 1.0);
    backdrop = vec4<f32>(backdrop.rgb * brightness, 1.0);

    var result = backdrop.rgb;

    // ========================================================================
    // EDGE HIGHLIGHT - Configurable thin line with angle-based light reflection
    // ========================================================================
    // This is the signature look - a thin bright line tracing the edge
    // The brightness varies based on the edge angle relative to light source
    let edge_line_width = prim.params2.x; // User-configurable border thickness
    let light_angle = prim.params2.y;     // Light source angle in radians

    let edge_line = smoothstep(0.0, edge_line_width * 0.3, inner_dist) *
                    (1.0 - smoothstep(edge_line_width, edge_line_width * 1.5, inner_dist));

    // Calculate light reflection based on edge normal vs light direction
    // Light direction vector from the light angle
    let light_dir = vec2<f32>(cos(light_angle), sin(light_angle));

    // Edge normal points outward from the shape (calculated earlier as sdf_gradient)
    // The reflection is strongest when edge normal faces the light
    // dot(edge_normal, -light_dir) = how much the edge faces the light source
    let facing_light = dot(edge_normal, -light_dir);

    // Map to 0-1 range with bias toward lit edges
    // -1 to 1 -> 0.2 to 1.0 (edges facing away still get some highlight)
    let light_factor = 0.2 + 0.8 * max(0.0, facing_light);

    // Combine edge line with light reflection
    // Multiply by mask to prevent highlight bleeding outside glass boundary
    let highlight_strength = edge_line * 0.6 * light_factor * mask; // Base strength 0.6, modulated by light
    result = result + vec3<f32>(highlight_strength);

    // ========================================================================
    // INNER EDGE SHADOW - Very subtle depth
    // ========================================================================
    let shadow_start = edge_line_width * 2.5;
    let shadow_end = edge_line_width * 8.0;
    let inner_shadow = smoothstep(shadow_start, shadow_end, inner_dist) *
                       (1.0 - smoothstep(shadow_end, shadow_end * 3.0, inner_dist));
    result = result - vec3<f32>(inner_shadow * 0.04 * mask); // More subtle, masked

    // ========================================================================
    // VERY SUBTLE TINT - Almost invisible
    // ========================================================================
    let tint = prim.tint_color;
    let tint_strength = tint.a * 0.08; // Even more subtle
    result = mix(result, tint.rgb, tint_strength);

    // Optional subtle noise
    if noise_amount > 0.0 {
        let n = noise(p * 0.3);
        result = result + vec3<f32>((n - 0.5) * noise_amount * 0.005);
    }

    // Glass type variants - adjust edge highlight intensity
    let glass_type = prim.type_info.x;
    switch glass_type {
        case GLASS_ULTRA_THIN: {
            // Even more transparent
            result = mix(backdrop.rgb, result, 0.7);
        }
        case GLASS_THIN: {
            // Slightly more visible
        }
        case GLASS_REGULAR: {
            // Default - as designed above
        }
        case GLASS_THICK: {
            // Stronger edge highlight
            result = result + vec3<f32>(highlight_strength * 0.3);
        }
        case GLASS_CHROME: {
            // Add slight metallic tint
            let chrome = vec3<f32>(0.96, 0.97, 0.99);
            result = mix(result, chrome, 0.1);
        }
        default: {}
    }

    result = clamp(result, vec3<f32>(0.0), vec3<f32>(1.0));

    // Blend shadow underneath the glass
    // Glass is rendered on top of shadow using standard alpha compositing
    // Final = glass_color * glass_alpha + shadow_color * shadow_alpha * (1 - glass_alpha)
    if has_shadow && shadow_color_premult.a > 0.001 {
        let glass_color = vec4<f32>(result, mask);
        let shadow_contrib = shadow_color_premult.a * (1.0 - mask);
        let final_alpha = mask + shadow_contrib;
        if final_alpha > 0.001 {
            let final_rgb = (result * mask + shadow_color_premult.rgb * shadow_contrib) / final_alpha;
            return vec4<f32>(final_rgb, final_alpha);
        }
    }

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
// Supports solid colors and gradients via per-vertex UV coordinates.
// Supports multi-stop gradients via 1D texture lookup.
// Supports clipping via rect/circle/ellipse shapes.

// Clip type constants
const CLIP_NONE: u32 = 0u;
const CLIP_RECT: u32 = 1u;
const CLIP_CIRCLE: u32 = 2u;
const CLIP_ELLIPSE: u32 = 3u;

struct Uniforms {
    // viewport_size (vec2) + padding (vec2) = 16 bytes, offset 0
    viewport_size: vec2<f32>,
    opacity: f32,
    _pad0: f32,
    // 3x3 transform stored as 3 vec4s (xyz used, w is padding) = 48 bytes, offset 16
    transform_row0: vec4<f32>,
    transform_row1: vec4<f32>,
    transform_row2: vec4<f32>,
    // Clip parameters = 32 bytes, offset 64
    clip_bounds: vec4<f32>,   // (x, y, width, height) or (cx, cy, rx, ry)
    clip_radius: vec4<f32>,   // corner radii or (rx, ry, 0, 0)
    // clip_type + flags = 16 bytes, offset 96
    clip_type: u32,
    use_gradient_texture: u32,  // 0=use vertex colors, 1=sample gradient texture
    use_image_texture: u32,     // 0=no image, 1=sample image texture
    use_glass_effect: u32,      // 0=no glass, 1=glass effect on path
    // Image UV bounds = 16 bytes, offset 112
    image_uv_bounds: vec4<f32>, // (u_min, v_min, u_max, v_max)
    // Glass parameters = 16 bytes, offset 128
    glass_params: vec4<f32>,    // (blur_radius, saturation, tint_strength, opacity)
    // Glass tint color = 16 bytes, offset 144
    glass_tint: vec4<f32>,      // RGBA tint color
}
// Total: 160 bytes

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var gradient_texture: texture_1d<f32>;
@group(0) @binding(2) var gradient_sampler: sampler;
@group(0) @binding(3) var image_texture: texture_2d<f32>;
@group(0) @binding(4) var image_sampler: sampler;
@group(0) @binding(5) var backdrop_texture: texture_2d<f32>;
@group(0) @binding(6) var backdrop_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,           // start color for gradients, solid color otherwise
    @location(2) end_color: vec4<f32>,       // end color for gradients
    @location(3) uv: vec2<f32>,
    @location(4) gradient_params: vec4<f32>, // linear: (x1,y1,x2,y2); radial: (cx,cy,r,0)
    @location(5) gradient_type: u32,
    @location(6) edge_distance: f32,         // distance to nearest edge (for AA)
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) end_color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) gradient_params: vec4<f32>,
    @location(4) @interpolate(flat) gradient_type: u32,
    @location(5) edge_distance: f32,
    @location(6) screen_pos: vec2<f32>,      // screen position for clip calculations
}

// ============================================================================
// SDF Functions for Clipping
// ============================================================================

// Rounded rectangle SDF
fn sd_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let rel = p - center;
    let q = abs(rel) - half_size;

    // Select corner radius based on quadrant
    // radius: (top-left, top-right, bottom-right, bottom-left)
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

// Calculate clip alpha (1.0 = inside clip, 0.0 = outside)
fn calculate_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>, clip_radius: vec4<f32>, clip_type: u32) -> f32 {
    if clip_type == CLIP_NONE {
        return 1.0;
    }

    var clip_d: f32;

    switch clip_type {
        case CLIP_RECT: {
            let clip_origin = clip_bounds.xy;
            let clip_size = clip_bounds.zw;
            clip_d = sd_rounded_rect(p, clip_origin, clip_size, clip_radius);
        }
        case CLIP_CIRCLE: {
            let center = clip_bounds.xy;
            let radius = clip_radius.x;
            clip_d = sd_circle(p, center, radius);
        }
        case CLIP_ELLIPSE: {
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
// Vertex Shader
// ============================================================================

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

    // Store screen position for clip calculations
    out.screen_pos = transformed.xy;

    // Convert to clip space (-1 to 1)
    let clip_pos = vec2<f32>(
        (transformed.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (transformed.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.color = in.color;
    out.end_color = in.end_color;
    out.uv = in.uv;
    out.gradient_params = in.gradient_params;
    out.gradient_type = in.gradient_type;
    out.edge_distance = in.edge_distance;

    return out;
}

// ============================================================================
// Fragment Shader
// ============================================================================

// Simple box blur for glass effect (samples backdrop in a small radius)
fn sample_blur(uv: vec2<f32>, blur_radius: f32, viewport_size: vec2<f32>) -> vec4<f32> {
    let pixel_size = 1.0 / viewport_size;
    var total = vec4<f32>(0.0);
    var samples = 0.0;

    // Simple 5x5 box blur
    let sample_radius = i32(clamp(blur_radius * 0.1, 1.0, 3.0));
    for (var x = -sample_radius; x <= sample_radius; x++) {
        for (var y = -sample_radius; y <= sample_radius; y++) {
            let offset = vec2<f32>(f32(x), f32(y)) * pixel_size * blur_radius * 0.5;
            let sample_uv = clamp(uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));
            total += textureSample(backdrop_texture, backdrop_sampler, sample_uv);
            samples += 1.0;
        }
    }

    return total / samples;
}

// Adjust saturation of a color
fn adjust_saturation(color: vec3<f32>, saturation: f32) -> vec3<f32> {
    let gray = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    return mix(vec3<f32>(gray), color, saturation);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Calculate clip alpha first
    let clip_alpha = calculate_clip_alpha(
        in.screen_pos,
        uniforms.clip_bounds,
        uniforms.clip_radius,
        uniforms.clip_type
    );

    // Early out if fully clipped
    if clip_alpha < 0.001 {
        discard;
    }

    var color: vec4<f32>;

    // Check for glass effect first
    if (uniforms.use_glass_effect == 1u) {
        // Glass effect: sample and blur backdrop, apply tint
        let screen_uv = in.screen_pos / uniforms.viewport_size;
        let blur_radius = uniforms.glass_params.x;
        let saturation = uniforms.glass_params.y;
        let tint_strength = uniforms.glass_params.z;
        let glass_opacity = uniforms.glass_params.w;

        // Sample blurred backdrop
        var backdrop = sample_blur(screen_uv, blur_radius, uniforms.viewport_size);

        // Adjust saturation
        backdrop = vec4<f32>(adjust_saturation(backdrop.rgb, saturation), backdrop.a);

        // Apply tint
        let tinted = mix(backdrop.rgb, uniforms.glass_tint.rgb, tint_strength * uniforms.glass_tint.a);

        // Final color with glass opacity
        color = vec4<f32>(tinted, glass_opacity);
    } else if (uniforms.use_image_texture == 1u) {
        // Image brush: sample from image texture using UV coordinates
        // Map the path UV (0-1 in bounding box) to image UV bounds
        let uv_min = uniforms.image_uv_bounds.xy;
        let uv_max = uniforms.image_uv_bounds.zw;
        let image_uv = uv_min + in.uv * (uv_max - uv_min);
        color = textureSample(image_texture, image_sampler, image_uv);
        // Apply tint from vertex color (multiply)
        color = vec4<f32>(color.rgb * in.color.rgb, color.a * in.color.a);
    } else if (in.gradient_type == 0u) {
        // Solid color
        color = in.color;
    } else if (in.gradient_type == 1u) {
        // Linear gradient - use gradient_params for direction
        // params: (x1, y1, x2, y2) in ObjectBoundingBox space (0-1)
        let g_start = in.gradient_params.xy;
        let g_end = in.gradient_params.zw;
        let g_dir = g_end - g_start;
        let g_len_sq = dot(g_dir, g_dir);

        // Project UV onto gradient line
        var t: f32;
        if (g_len_sq > 0.0001) {
            let p = in.uv - g_start;
            t = clamp(dot(p, g_dir) / g_len_sq, 0.0, 1.0);
        } else {
            t = 0.0;
        }

        // Sample from gradient texture or mix vertex colors
        if (uniforms.use_gradient_texture == 1u) {
            // Multi-stop gradient: sample from 1D texture
            color = textureSample(gradient_texture, gradient_sampler, t);
        } else {
            // 2-stop fast path: mix vertex colors
            color = mix(in.color, in.end_color, t);
        }
    } else {
        // Radial gradient - params: (cx, cy, r, 0) in ObjectBoundingBox space
        let center = in.gradient_params.xy;
        let radius = in.gradient_params.z;
        let dist = length(in.uv - center);
        let t = clamp(dist / max(radius, 0.001), 0.0, 1.0);

        // Sample from gradient texture or mix vertex colors
        if (uniforms.use_gradient_texture == 1u) {
            // Multi-stop gradient: sample from 1D texture
            color = textureSample(gradient_texture, gradient_sampler, t);
        } else {
            // 2-stop fast path: mix vertex colors
            color = mix(in.color, in.end_color, t);
        }
    }

    // Apply opacity and clip alpha
    color.a *= uniforms.opacity * clip_alpha;
    return color;
}
"#;

/// Shader for image rendering
///
/// Renders images with:
/// - UV cropping for box-fit modes
/// - Tinting and opacity
/// - Optional rounded corners
pub const IMAGE_SHADER: &str = include_str!("shaders/image.wgsl");
