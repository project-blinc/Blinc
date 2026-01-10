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

// Quarter ellipse SDF for inner corners with asymmetric borders (GPUI approach)
// This handles the case where adjacent border widths differ, creating an elliptical
// inner corner instead of circular. Returns negative inside, positive outside.
fn quarter_ellipse_sdf(point: vec2<f32>, radii: vec2<f32>) -> f32 {
    // Avoid division by zero
    let safe_radii = max(radii, vec2<f32>(0.001));
    // Map to unit circle space
    let circle_vec = point / safe_radii;
    let unit_circle_sdf = length(circle_vec) - 1.0;
    // Scale back using average radius for distance approximation
    return unit_circle_sdf * (safe_radii.x + safe_radii.y) * -0.5;
}

// Error function approximation for Gaussian blur
fn erf(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
    return s * y;
}

// Gaussian shadow for rectangle (without corner radii - legacy)
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

// Gaussian shadow for rounded rectangle - uses SDF for proper corner handling
fn shadow_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, corner_radius: vec4<f32>, sigma: f32) -> f32 {
    // Get signed distance to the rounded rectangle
    let sdf_dist = sd_rounded_rect(p, origin, size, corner_radius);

    if sigma < 0.001 {
        // No blur - use hard edge
        return select(0.0, 1.0, sdf_dist < 0.0);
    }

    // Gaussian falloff based on SDF distance
    // Same approach as shadow_circle: 1 inside, Gaussian falloff outside
    let d = 0.5 * sqrt(2.0) * sigma;
    return 0.5 * (1.0 + erf(-sdf_dist / d));
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

        // Adjust corner radii for spread (expand corners proportionally)
        let shadow_radii = prim.corner_radius + vec4<f32>(spread);

        let shadow_alpha = shadow_rounded_rect(p, shadow_origin, shadow_size, shadow_radii, blur);
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
            // Shadow-only primitive - mask out the shape interior
            // Shadow should be visible starting from the shape boundary (d >= 0)
            // Use fwidth-based AA to match fill rendering and prevent gaps at corners
            let shape_d = sd_rounded_rect(p, origin, size, prim.corner_radius);
            let aa_width = fwidth(shape_d) * 0.5;
            let shape_mask = smoothstep(-aa_width, aa_width, shape_d); // 0 inside, 1 outside, AA at edge
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
            // Use fwidth-based AA to match fill rendering and prevent gaps at edges
            let circle_d = sd_circle(p, center, radius);
            let aa_width = fwidth(circle_d) * 0.5;
            let shape_mask = smoothstep(-aa_width, aa_width, circle_d); // 0 inside, 1 outside, AA at edge

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

    // Handle border with proper inner corner radii (GPUI-style approach)
    // The border is the ring between the outer shape edge and an inner shape
    // For asymmetric borders, inner corners become elliptical, not circular
    // prim.border = [top, right, bottom, left] for per-side borders, or [uniform, 0, 0, 0] for uniform
    let border_top = prim.border.x;
    let border_right = prim.border.y;
    let border_bottom = prim.border.z;
    let border_left = prim.border.w;

    // Check if any border is present (using max of all sides)
    let max_border = max(max(border_top, border_right), max(border_bottom, border_left));
    if max_border > 0.0 {
        // For uniform border (legacy: only .x set), use it for all sides
        let bt = select(border_top, border_top, border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0);
        let br = select(border_top, border_right, border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0);
        let bb = select(border_top, border_bottom, border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0);
        let bl = select(border_top, border_left, border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0);

        let half_size = size * 0.5;
        let rel = p - center;  // Position relative to center (signed)
        let antialias_threshold = 0.5;

        // Select corner radius based on quadrant
        var corner_radius: f32;
        if rel.y < 0.0 {
            if rel.x > 0.0 { corner_radius = prim.corner_radius.y; }  // top-right
            else { corner_radius = prim.corner_radius.x; }           // top-left
        } else {
            if rel.x > 0.0 { corner_radius = prim.corner_radius.z; }  // bottom-right
            else { corner_radius = prim.corner_radius.w; }           // bottom-left
        }

        // Select border widths for nearest edges based on quadrant (GPUI approach)
        let border = vec2<f32>(
            select(br, bl, rel.x < 0.0),  // horizontal: left or right
            select(bb, bt, rel.y < 0.0)   // vertical: top or bottom
        );

        // Handle zero-width borders (treat as negative for AA purposes)
        let reduced_border = vec2<f32>(
            select(border.x, -antialias_threshold, border.x == 0.0),
            select(border.y, -antialias_threshold, border.y == 0.0)
        );

        // Calculate position relative to corner
        let corner_to_point = abs(rel) - half_size;
        let corner_center_to_point = corner_to_point + corner_radius;

        // Determine if we're near a rounded corner
        let is_near_rounded_corner = corner_center_to_point.x >= 0.0 && corner_center_to_point.y >= 0.0;

        // Inner straight border edge
        let straight_border_inner = corner_to_point + reduced_border;

        // Check if we're clearly inside the inner area (not near border)
        let is_within_inner_straight = straight_border_inner.x < -antialias_threshold &&
                                       straight_border_inner.y < -antialias_threshold;

        // Fast path: clearly inside inner area, not near rounded corner
        if is_within_inner_straight && !is_near_rounded_corner {
            // No border here, keep fill_color as-is
        } else {
            // Calculate inner SDF based on context
            var inner_sdf: f32;

            let is_beyond_inner_straight = straight_border_inner.x > 0.0 || straight_border_inner.y > 0.0;

            if corner_center_to_point.x <= 0.0 || corner_center_to_point.y <= 0.0 {
                // Not in corner region - use straight edge distance
                inner_sdf = -max(straight_border_inner.x, straight_border_inner.y);
            } else if is_beyond_inner_straight {
                // Beyond inner straight edge - definitely in border
                inner_sdf = -1.0;
            } else if abs(reduced_border.x - reduced_border.y) < 0.001 {
                // Equal border widths - inner corner is circular (simple offset from outer)
                let outer_sdf = length(max(vec2<f32>(0.0), corner_center_to_point)) +
                               min(0.0, max(corner_center_to_point.x, corner_center_to_point.y)) - corner_radius;
                inner_sdf = -(outer_sdf + reduced_border.x);
            } else {
                // Asymmetric borders - inner corner is ELLIPTICAL (key insight from GPUI)
                let ellipse_radii = max(vec2<f32>(0.0), vec2<f32>(corner_radius) - reduced_border);
                inner_sdf = quarter_ellipse_sdf(corner_center_to_point, ellipse_radii);
            }

            // Calculate border blend from inner SDF
            // inner_sdf > 0 means inside inner (no border), < 0 means in border region
            let border_blend = saturate(antialias_threshold - inner_sdf);

            // Only apply border color where we're inside the shape
            fill_color = mix(fill_color, prim.border_color, border_blend * step(0.001, fill_alpha));
        }
    }

    // Apply clip alpha to shadow
    result.a *= clip_alpha;

    // Mask shadow strictly outside the shape boundary
    // Use the same aa_width as fill_alpha to prevent gaps at corners
    // The shadow should render only where d > 0 (outside the shape)
    if result.a > 0.0 {
        // Use matching AA width to ensure shadow and fill meet seamlessly
        let shadow_mask = smoothstep(-aa_width, aa_width, d);
        result.a *= shadow_mask;
    }

    // Blend fill over shadow at FULL opacity first (fill fully covers shadow)
    // This ensures no shadow bleeds through the shape regardless of edge AA
    let full_fill = vec4<f32>(fill_color.rgb, fill_color.a * clip_alpha);
    result = full_fill + result * (1.0 - full_fill.a);

    // NOW apply outer edge anti-aliasing to the combined result
    // This gives smooth edges against the background without shadow bleed
    result.a *= fill_alpha;

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
const GLASS_SIMPLE: u32 = 5u;  // Simple frosted glass - no liquid effects

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
    let glass_type = prim.type_info.x;

    // ========================================================================
    // SIMPLE FROSTED GLASS (no liquid effects)
    // ========================================================================
    // Pure frosted glass: blur + tint + saturation/brightness
    // No refraction, no edge bevels, no light reflections
    if glass_type == GLASS_SIMPLE {
        // Sample and blur the backdrop directly at screen UV (no refraction)
        var simple_backdrop = blur_backdrop_clipped(in.screen_uv, blur_radius, prim.clip_bounds);

        // Apply saturation and brightness adjustments
        var result_rgb = adjust_saturation(simple_backdrop.rgb, saturation);
        result_rgb = result_rgb * brightness;

        // Apply tint as a subtle additive overlay (not heavy mixing)
        // This keeps the backdrop colors visible while adding a light tint
        let tint = prim.tint_color;
        if tint.a > 0.001 {
            // Soft light blend: backdrop + tint * tint_alpha (additive overlay)
            result_rgb = result_rgb + tint.rgb * tint.a * 0.5;
        }

        // Optional noise for frosted texture
        if noise_amount > 0.0 {
            let n = noise(p * 0.3);
            result_rgb = result_rgb + vec3<f32>((n - 0.5) * noise_amount * 0.02);
        }

        result_rgb = clamp(result_rgb, vec3<f32>(0.0), vec3<f32>(1.0));

        // Blend shadow underneath the glass
        if has_shadow && shadow_color_premult.a > 0.001 {
            let shadow_contrib = shadow_color_premult.a * (1.0 - mask);
            let final_alpha = mask + shadow_contrib;
            if final_alpha > 0.001 {
                let final_rgb = (result_rgb * mask + shadow_color_premult.rgb * shadow_contrib) / final_alpha;
                return vec4<f32>(final_rgb, final_alpha);
            }
        }

        return vec4<f32>(result_rgb, mask);
    }

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
pub const SIMPLE_GLASS_SHADER: &str = r#"
// ============================================================================
// Simple Frosted Glass Shader
// ============================================================================
//
// Pure backdrop blur without liquid glass effects.
// More performant and suitable for subtle UI backgrounds.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) screen_uv: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
}

struct SimpleGlassUniforms {
    viewport_size: vec2<f32>,
    time: f32,
    _padding: f32,
}

struct SimpleGlassPrimitive {
    bounds: vec4<f32>,
    corner_radius: vec4<f32>,
    tint_color: vec4<f32>,
    params: vec4<f32>,      // blur, saturation, brightness, noise
    params2: vec4<f32>,     // border_thickness, light_angle, shadow_blur, shadow_opacity
    type_info: vec4<u32>,   // glass_type, shadow_offset_x_bits, shadow_offset_y_bits, clip_type
    clip_bounds: vec4<f32>,
    clip_radius: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: SimpleGlassUniforms;
@group(0) @binding(1) var<storage, read> primitives: array<SimpleGlassPrimitive>;
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

fn sd_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let rel = p - center;
    let q = abs(rel) - half_size;

    var r: f32;
    if (rel.x < 0.0 && rel.y < 0.0) { r = radius.x; }
    else if (rel.x >= 0.0 && rel.y < 0.0) { r = radius.y; }
    else if (rel.x >= 0.0 && rel.y >= 0.0) { r = radius.z; }
    else { r = radius.w; }

    let outer_dist = length(max(q + r, vec2<f32>(0.0)));
    let inner_dist = min(max(q.x + r, q.y + r), 0.0);
    return outer_dist + inner_dist - r;
}

fn shadow_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>, blur: f32) -> f32 {
    let d = sd_rounded_rect(p, origin, size, radius);
    let sigma = blur * 0.5;
    return 1.0 - smoothstep(-sigma * 2.0, sigma * 2.0, d);
}

fn calculate_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>) -> f32 {
    let clip_min = clip_bounds.xy;
    let clip_max = clip_bounds.xy + clip_bounds.zw;
    let edge_dist = min(
        min(p.x - clip_min.x, clip_max.x - p.x),
        min(p.y - clip_min.y, clip_max.y - p.y)
    );
    return smoothstep(-0.5, 0.5, edge_dist);
}

fn adjust_saturation(color: vec3<f32>, saturation: f32) -> vec3<f32> {
    let luminance = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    return mix(vec3<f32>(luminance), color, saturation);
}

// Simple box blur - sample backdrop at offset positions
fn blur_backdrop(uv: vec2<f32>, radius: f32, clip_bounds: vec4<f32>) -> vec4<f32> {
    let tex_size = vec2<f32>(textureDimensions(backdrop_texture));
    let pixel_size = 1.0 / tex_size;

    // Use 5x5 box blur for simplicity
    let r = max(1.0, radius * 0.5);
    var sum = vec4<f32>(0.0);
    var weight = 0.0;

    let clip_min = clip_bounds.xy / uniforms.viewport_size;
    let clip_max = (clip_bounds.xy + clip_bounds.zw) / uniforms.viewport_size;

    for (var y = -2.0; y <= 2.0; y += 1.0) {
        for (var x = -2.0; x <= 2.0; x += 1.0) {
            let offset = vec2<f32>(x, y) * pixel_size * r;
            var sample_uv = uv + offset;

            // Clamp to clip bounds
            sample_uv = clamp(sample_uv, clip_min, clip_max);

            let w = 1.0;
            sum += textureSample(backdrop_texture, backdrop_sampler, sample_uv) * w;
            weight += w;
        }
    }

    return sum / weight;
}

// Noise function for frosted texture
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let a = fract(sin(dot(i, vec2<f32>(127.1, 311.7))) * 43758.5453);
    let b = fract(sin(dot(i + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);
    let c = fract(sin(dot(i + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);
    let d = fract(sin(dot(i + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);

    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// ============================================================================
// Fragment Shader
// ============================================================================

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let prim = primitives[in.instance_index];
    let p = in.uv;

    // Calculate clip alpha
    let clip_alpha = calculate_clip_alpha(p, prim.clip_bounds);
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

    // Calculate SDF
    let d = sd_rounded_rect(p, origin, size, prim.corner_radius);
    let aa = fwidth(d) * 2.0;
    let mask = (1.0 - smoothstep(-aa, aa, d)) * clip_alpha;

    // Drop shadow
    let has_shadow = shadow_opacity > 0.001 && shadow_blur > 0.001;
    var shadow_color_premult = vec4<f32>(0.0);

    if has_shadow {
        let shadow_origin = origin + vec2<f32>(shadow_offset_x, shadow_offset_y);
        let shadow_alpha = shadow_rounded_rect(p, shadow_origin, size, prim.corner_radius, shadow_blur);
        shadow_color_premult = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha * shadow_opacity * clip_alpha);

        if mask < 0.001 {
            if shadow_alpha > 0.001 && clip_alpha > 0.001 {
                return shadow_color_premult;
            }
            discard;
        }
    } else {
        if mask < 0.001 {
            discard;
        }
    }

    // Glass parameters
    let blur_radius = prim.params.x;
    let saturation = prim.params.y;
    let brightness = prim.params.z;
    let noise_amount = prim.params.w;

    // Sample and blur backdrop directly (NO refraction, NO distortion)
    var backdrop = blur_backdrop(in.screen_uv, blur_radius, prim.clip_bounds);

    // Apply saturation and brightness
    var result_rgb = adjust_saturation(backdrop.rgb, saturation);
    result_rgb = result_rgb * brightness;

    // Apply tint as subtle additive overlay
    let tint = prim.tint_color;
    if tint.a > 0.001 {
        result_rgb = result_rgb + tint.rgb * tint.a * 0.5;
    }

    // Optional noise for frosted texture
    if noise_amount > 0.0 {
        let n = noise(p * 0.3);
        result_rgb = result_rgb + vec3<f32>((n - 0.5) * noise_amount * 0.02);
    }

    result_rgb = clamp(result_rgb, vec3<f32>(0.0), vec3<f32>(1.0));

    // Blend shadow underneath
    if has_shadow && shadow_color_premult.a > 0.001 {
        let shadow_contrib = shadow_color_premult.a * (1.0 - mask);
        let final_alpha = mask + shadow_contrib;
        if final_alpha > 0.001 {
            let final_rgb = (result_rgb * mask + shadow_color_premult.rgb * shadow_contrib) / final_alpha;
            return vec4<f32>(final_rgb, final_alpha);
        }
    }

    return vec4<f32>(result_rgb, mask);
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
    // Padding
    _pad: vec3<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: LayerUniforms;
@group(0) @binding(1) var layer_texture: texture_2d<f32>;
@group(0) @binding(2) var layer_sampler: sampler;

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

    // Map to destination rectangle in viewport space
    let dest_pos = uniforms.dest_rect.xy + local_pos * uniforms.dest_rect.zw;

    // Convert to normalized device coordinates (-1 to 1)
    let ndc = (dest_pos / uniforms.viewport_size) * 2.0 - 1.0;

    // Map to source rectangle UV
    let uv = uniforms.source_rect.xy + local_pos * uniforms.source_rect.zw;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);  // Flip Y for wgpu
    out.uv = uv;
    out.frag_pos = dest_pos;  // Pass fragment position in viewport pixels
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

    // Sample layer texture
    let src = textureSample(layer_texture, layer_sampler, in.uv);

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

    // For blend modes other than normal, we'd need to read the destination.
    // Since wgpu doesn't support programmable blending, we use hardware blending
    // for Normal mode and would need a two-pass approach for other modes.
    //
    // For now, output premultiplied alpha for hardware blending:
    // result = src * src_alpha + dst * (1 - src_alpha)

    // Premultiply alpha
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
    // Use small fixed offsets to preserve shape during multi-pass blur
    // Larger radii use more passes rather than larger offsets
    // This prevents corner fill-in by spreading gradually
    let base_offset = f32(uniforms.iteration) + 0.5;
    // Fixed small multiplier - more passes will accumulate the blur
    let offset = base_offset * 1.2;
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
        // Element blur mode: preserve alpha, blur RGB only with alpha weighting
        // This preserves corner radius while preventing darkening
        let total_alpha = s0.a + s1.a + s2.a + s3.a + s4.a;

        if (total_alpha < 0.001) {
            return vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }

        // Weight RGB by alpha to prevent black transparent pixels from darkening
        let weighted_rgb = s0.rgb * s0.a + s1.rgb * s1.a + s2.rgb * s2.a + s3.rgb * s3.a + s4.rgb * s4.a;
        let avg_rgb = weighted_rgb / total_alpha;

        // Preserve center sample's alpha to maintain corner radius
        return vec4<f32>(avg_rgb, s0.a);
    } else {
        // Shadow blur mode: only blur alpha for shadow shape
        // Output white RGB since drop shadow shader uses uniform color, not texture RGB
        let total_alpha = s0.a + s1.a + s2.a + s3.a + s4.a;
        let avg_alpha = total_alpha / 5.0;

        return vec4<f32>(1.0, 1.0, 1.0, avg_alpha);
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
    let center = textureSample(original_texture, input_sampler, uv);
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
            let s = textureSample(original_texture, input_sampler, sample_uv);

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
    let original = textureSample(original_texture, input_sampler, in.uv);

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
    let center = textureSample(source_texture, source_sampler, uv);
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
            let s = textureSample(source_texture, source_sampler, sample_uv);

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
    let original = textureSample(source_texture, source_sampler, in.uv);

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
