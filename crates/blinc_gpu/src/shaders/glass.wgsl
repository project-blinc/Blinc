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
    // Border color (RGBA) - when alpha > 0, renders a solid border instead of light-based highlights
    border_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: GlassUniforms;
@group(0) @binding(1) var<storage, read> primitives: array<GlassPrimitive>;
@group(0) @binding(2) var backdrop_texture: texture_2d<f32>;
@group(0) @binding(3) var backdrop_sampler: sampler;
@group(0) @binding(4) var<storage, read> aux_data: array<vec4<f32>>;


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

    // Generate quad vertices split along / diagonal (1-3 shared edge)
    // PowerVR Vulkan codegen bug workaround — see SDF_SHADER vs_main
    // for the rationale. `let array<...>(literal)[runtime]` is broken
    // for indices 3..5 on the Pixel 10 Pro PowerVR driver; an explicit
    // `switch` keeps the same vertex layout and renders correctly.
    var local_uv: vec2<f32>;
    switch vertex_index {
        case 0u: { local_uv = vec2<f32>(0.0, 0.0); } // 0 - top-left
        case 1u: { local_uv = vec2<f32>(1.0, 0.0); } // 1 - top-right
        case 2u: { local_uv = vec2<f32>(0.0, 1.0); } // 3 - bottom-left
        case 3u: { local_uv = vec2<f32>(1.0, 0.0); } // 1 - top-right
        case 4u: { local_uv = vec2<f32>(1.0, 1.0); } // 2 - bottom-right
        default: { local_uv = vec2<f32>(0.0, 1.0); } // 3 - bottom-left
    }
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

// High quality blur using golden-angle spiral sampling
// CSS spec: blur(Npx) means standard deviation = N pixels
//
// Uses `textureSampleLevel(.., .., uv, 0.0)` instead of `textureSample` so
// the call is legal from non-uniform control flow. The fragment shader
// `discard`s for shadow-only fragments before reaching the blur path,
// which makes flow non-uniform across the quad — and WGSL/WebGPU bans
// implicit-derivative samples (`textureSample`) in that case. The
// backdrop has `mip_level_count: 1`, so explicitly sampling LOD 0 is
// functionally identical to the implicit form.
fn blur_backdrop(uv: vec2<f32>, blur_radius: f32) -> vec4<f32> {
    if blur_radius < 0.5 {
        return textureSampleLevel(backdrop_texture, backdrop_sampler, uv, 0.0);
    }

    let texel_size = 1.0 / uniforms.viewport_size;
    let sigma = blur_radius; // CSS spec: blur radius IS the standard deviation

    // Start with center sample (highest weight)
    var color = textureSampleLevel(backdrop_texture, backdrop_sampler, uv, 0.0);
    var total_weight = 1.0;

    let golden_angle = 2.39996323; // 137.5 degrees in radians

    // Sample out to 2.5 sigma for proper Gaussian coverage
    let sample_extent = blur_radius * 2.5;

    // 6 rings with 12 samples each = 72 samples, linear spacing
    let num_rings = 6;
    let samples_per_ring = 12;

    for (var ring = 1; ring <= num_rings; ring++) {
        let ring_t = f32(ring) / f32(num_rings);
        let ring_radius = sample_extent * ring_t; // Linear spacing
        let ring_offset = ring_radius * texel_size;

        for (var i = 0; i < samples_per_ring; i++) {
            let angle = f32(i) * (6.283185 / f32(samples_per_ring)) + f32(ring) * golden_angle;
            let offset = vec2<f32>(cos(angle), sin(angle)) * ring_offset;

            let sample_pos = uv + offset;
            let weight = gaussian_weight(ring_radius, sigma);

            color += textureSampleLevel(backdrop_texture, backdrop_sampler, sample_pos, 0.0) * weight;
            total_weight += weight;
        }
    }

    return color / total_weight;
}

// High quality blur with clip bounds for scroll containers
// CSS spec: blur(Npx) means standard deviation = N pixels
//
// Uses `textureSampleLevel` for the same uniform-control-flow reason
// documented on `blur_backdrop` above.
fn blur_backdrop_clipped(uv: vec2<f32>, blur_radius: f32, clip_bounds: vec4<f32>) -> vec4<f32> {
    let clip_min = clip_bounds.xy / uniforms.viewport_size;
    let clip_max = (clip_bounds.xy + clip_bounds.zw) / uniforms.viewport_size;
    let has_clip = clip_bounds.x > -5000.0;

    if blur_radius < 0.5 {
        let clamped_uv = select(uv, clamp(uv, clip_min, clip_max), has_clip);
        return textureSampleLevel(backdrop_texture, backdrop_sampler, clamped_uv, 0.0);
    }

    let texel_size = 1.0 / uniforms.viewport_size;
    let sigma = blur_radius; // CSS spec: blur radius IS the standard deviation

    // Start with center sample (highest weight)
    let center_uv = select(uv, clamp(uv, clip_min, clip_max), has_clip);
    var color = textureSampleLevel(backdrop_texture, backdrop_sampler, center_uv, 0.0);
    var total_weight = 1.0;

    let golden_angle = 2.39996323; // 137.5 degrees in radians

    // Sample out to 2.5 sigma for proper Gaussian coverage
    let sample_extent = blur_radius * 2.5;

    // 6 rings with 12 samples each = 72 samples, linear spacing
    let num_rings = 6;
    let samples_per_ring = 12;

    for (var ring = 1; ring <= num_rings; ring++) {
        let ring_t = f32(ring) / f32(num_rings);
        let ring_radius = sample_extent * ring_t; // Linear spacing
        let ring_offset = ring_radius * texel_size;

        for (var i = 0; i < samples_per_ring; i++) {
            let angle = f32(i) * (6.283185 / f32(samples_per_ring)) + f32(ring) * golden_angle;
            let offset = vec2<f32>(cos(angle), sin(angle)) * ring_offset;

            var sample_pos = uv + offset;
            sample_pos = select(sample_pos, clamp(sample_pos, clip_min, clip_max), has_clip);

            let weight = gaussian_weight(ring_radius, sigma);
            color += textureSampleLevel(backdrop_texture, backdrop_sampler, sample_pos, 0.0) * weight;
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
    let clip_alpha = calculate_glass_clip_alpha(p, prim.clip_bounds) * abi_shape_clip_alpha(p, select(0.0, prim.clip_radius.x, prim.clip_radius.w < 0.0));
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
    let aa = 1.5; // Wide AA for smooth edges (constant to avoid Vulkan triangle seam artifacts)

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
    // EDGE HIGHLIGHT / BORDER - Configurable thin line with angle-based light reflection
    // ========================================================================
    // When border_color.a > 0, use the explicit border color.
    // Otherwise, use the signature light-based edge highlight.
    let edge_line_width = prim.params2.x; // User-configurable border thickness
    let light_angle = prim.params2.y;     // Light source angle in radians
    let bc = prim.border_color;

    let edge_line = smoothstep(0.0, edge_line_width * 0.3, inner_dist) *
                    (1.0 - smoothstep(edge_line_width, edge_line_width * 1.5, inner_dist));

    // Compute light-based highlight strength (used by glass type variants too)
    let light_dir = vec2<f32>(cos(light_angle), sin(light_angle));
    let facing_light = dot(edge_normal, -light_dir);
    let light_factor = 0.2 + 0.8 * max(0.0, facing_light);
    let highlight_strength = edge_line * 0.6 * light_factor * mask;

    if bc.a > 0.001 {
        // Solid border color mode: blend the user-specified color at the edge
        result = mix(result, bc.rgb, edge_line * bc.a * mask);
    } else {
        // Light-based highlight mode (default liquid glass look)
        result = result + vec3<f32>(highlight_strength);
    }

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
