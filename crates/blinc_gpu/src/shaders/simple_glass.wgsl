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
    border_color: vec4<f32>, // Border color (RGBA) - when alpha > 0, renders solid border
}

@group(0) @binding(0) var<uniform> uniforms: SimpleGlassUniforms;
@group(0) @binding(1) var<storage, read> primitives: array<SimpleGlassPrimitive>;
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

// Gaussian weight function
fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

// High quality blur using golden-angle spiral sampling
// CSS spec: blur(Npx) means standard deviation = N pixels
//
// Uses `textureSampleLevel(.., .., uv, 0.0)` for the same uniform-control-flow
// reason documented on the equivalent function in GLASS_SHADER above:
// the fragment shader `discard`s shadow-only fragments, so plain
// `textureSample` (which needs implicit derivatives) is illegal here.
// The backdrop has `mip_level_count: 1`, so explicit LOD 0 is identical.
fn blur_backdrop(uv: vec2<f32>, radius: f32, clip_bounds: vec4<f32>) -> vec4<f32> {
    if radius < 0.5 {
        return textureSampleLevel(backdrop_texture, backdrop_sampler, uv, 0.0);
    }

    let texel_size = 1.0 / uniforms.viewport_size;
    let sigma = radius; // CSS spec: blur radius IS the standard deviation

    // Clip bounds in UV space
    let clip_min = clip_bounds.xy / uniforms.viewport_size;
    let clip_max = (clip_bounds.xy + clip_bounds.zw) / uniforms.viewport_size;
    let has_clip = clip_bounds.x > -5000.0;

    // Start with center sample (highest weight)
    let center_uv = select(uv, clamp(uv, clip_min, clip_max), has_clip);
    var color = textureSampleLevel(backdrop_texture, backdrop_sampler, center_uv, 0.0);
    var total_weight = 1.0;

    // Golden angle spiral for smooth sample distribution
    let golden_angle = 2.39996323; // 137.5 degrees in radians

    // Sample out to 2.5 sigma for proper Gaussian coverage (captures ~99% of kernel)
    let sample_extent = radius * 2.5;

    // 6 rings with 12 samples each = 72 samples, linear spacing
    let num_rings = 6;
    let samples_per_ring = 12;

    for (var ring = 1; ring <= num_rings; ring++) {
        let ring_t = f32(ring) / f32(num_rings);
        let ring_radius = sample_extent * ring_t; // Linear spacing for uniform coverage
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
    let clip_alpha = calculate_clip_alpha(p, prim.clip_bounds) * abi_shape_clip_alpha(p, select(0.0, prim.clip_radius.x, prim.clip_radius.w < 0.0));
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
    let aa = 1.5; // Constant AA to avoid Vulkan triangle seam artifacts
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

    // Border color support
    let bc = prim.border_color;
    if bc.a > 0.001 {
        let border_width = prim.params2.x;
        let inner_dist = -d; // Distance from edge into the shape
        let edge_band = smoothstep(0.0, border_width * 0.3, inner_dist) *
                        (1.0 - smoothstep(border_width, border_width * 1.5, inner_dist));
        result_rgb = mix(result_rgb, bc.rgb, edge_band * bc.a * mask);
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
