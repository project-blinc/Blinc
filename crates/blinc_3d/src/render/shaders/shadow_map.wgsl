// Shadow map depth pass shader

struct LightSpaceUniform {
    light_view: mat4x4<f32>,
    light_projection: mat4x4<f32>,
    light_view_projection: mat4x4<f32>,
}

struct ModelUniform {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> light: LightSpaceUniform;
@group(1) @binding(0) var<uniform> model: ModelUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let world_position = model.model * vec4<f32>(in.position, 1.0);
    out.position = light.light_view_projection * world_position;

    return out;
}

// Fragment shader just writes depth (happens automatically)
// But we can add alpha testing for transparent objects
@fragment
fn fs_main(in: VertexOutput) {
    // Depth is written automatically
    // No color output needed for shadow map
}

// ==================== Shadow Sampling Utilities ====================
// These would be used by other shaders that read shadow maps

// PCF (Percentage Closer Filtering) for soft shadows
fn sample_shadow_pcf(
    shadow_map: texture_depth_2d,
    shadow_sampler: sampler_comparison,
    coords: vec3<f32>,  // xy = uv, z = depth
    texel_size: vec2<f32>
) -> f32 {
    var shadow = 0.0;

    // 3x3 PCF kernel
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel_size;
            shadow += textureSampleCompare(
                shadow_map,
                shadow_sampler,
                coords.xy + offset,
                coords.z
            );
        }
    }

    return shadow / 9.0;
}

// VSM (Variance Shadow Mapping) moment calculation
fn calculate_vsm_moments(depth: f32) -> vec2<f32> {
    return vec2<f32>(depth, depth * depth);
}

// VSM shadow sampling
fn sample_shadow_vsm(moments: vec2<f32>, depth: f32) -> f32 {
    let p = step(depth, moments.x);
    let variance = max(moments.y - moments.x * moments.x, 0.00002);
    let d = depth - moments.x;
    let p_max = variance / (variance + d * d);
    return max(p, p_max);
}

// Cascaded shadow map selection
fn select_cascade(
    view_depth: f32,
    cascade_splits: vec4<f32>
) -> u32 {
    for (var i = 0u; i < 4u; i++) {
        if (view_depth < cascade_splits[i]) {
            return i;
        }
    }
    return 3u;
}

// Apply shadow bias
fn apply_shadow_bias(
    light_space_pos: vec4<f32>,
    normal: vec3<f32>,
    light_dir: vec3<f32>,
    bias: f32,
    normal_bias: f32
) -> vec3<f32> {
    let n_dot_l = dot(normal, light_dir);
    let slope_bias = bias * tan(acos(n_dot_l));
    let total_bias = min(slope_bias, bias * 2.0);

    var coords = light_space_pos.xyz / light_space_pos.w;
    coords.xy = coords.xy * 0.5 + 0.5;
    coords.y = 1.0 - coords.y;  // Flip Y for texture coordinates
    coords.z -= total_bias;
    coords.z += normal_bias * (1.0 - n_dot_l);

    return coords;
}
