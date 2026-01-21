// GPU-based water shader with Gerstner waves and reflections
// Performs wave calculations entirely on GPU for smooth animation

const PI: f32 = 3.14159265359;

struct CameraUniform {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    view_projection: mat4x4<f32>,
    inverse_view: mat4x4<f32>,
    position: vec4<f32>,
    direction: vec4<f32>,
    near_far: vec4<f32>,
}

struct WaterUniform {
    model: mat4x4<f32>,
    // Water parameters
    water_level: f32,
    time: f32,
    transparency: f32,
    fresnel_strength: f32,
    // Wave layers (4 layers max)
    wave0: vec4<f32>,  // x = frequency, y = amplitude, z = speed, w = steepness
    wave1: vec4<f32>,
    wave2: vec4<f32>,
    wave3: vec4<f32>,
    // Wave directions (packed as xy pairs)
    wave_dir01: vec4<f32>,  // xy = wave0 dir, zw = wave1 dir
    wave_dir23: vec4<f32>,  // xy = wave2 dir, zw = wave3 dir
    // Colors
    shallow_color: vec4<f32>,
    deep_color: vec4<f32>,
    // Foam
    foam_color: vec4<f32>,
    foam_threshold: f32,
    foam_intensity: f32,
    shore_fade_distance: f32,
    specular_intensity: f32,
}

struct LightUniform {
    position_or_direction: vec4<f32>,
    color: vec4<f32>,
    params: vec4<f32>,
}

struct LightsUniform {
    ambient: vec4<f32>,
    num_lights: u32,
    _padding: vec3<u32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> water: WaterUniform;
@group(2) @binding(0) var<uniform> lights: LightsUniform;
@group(2) @binding(1) var<storage, read> light_data: array<LightUniform>;
@group(3) @binding(0) var reflection_texture: texture_2d<f32>;
@group(3) @binding(1) var reflection_sampler: sampler;
@group(3) @binding(2) var refraction_texture: texture_2d<f32>;
@group(3) @binding(3) var refraction_sampler: sampler;
@group(3) @binding(4) var depth_texture: texture_2d<f32>;
@group(3) @binding(5) var depth_sampler: sampler;
@group(3) @binding(6) var foam_texture: texture_2d<f32>;
@group(3) @binding(7) var foam_sampler: sampler;
@group(3) @binding(8) var normal_texture: texture_2d<f32>;
@group(3) @binding(9) var normal_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) clip_space: vec4<f32>,
    @location(4) foam_factor: f32,
}

// ==================== Gerstner Wave Functions ====================

// Single Gerstner wave contribution
fn gerstner_wave(
    position: vec2<f32>,
    time: f32,
    direction: vec2<f32>,
    frequency: f32,
    amplitude: f32,
    speed: f32,
    steepness: f32
) -> vec3<f32> {
    let d = normalize(direction);
    let f = frequency;
    let a = amplitude;
    let phi = speed * 2.0 * PI;
    let q = steepness / (f * a * 4.0);  // Clamp steepness to prevent looping

    let dot_d_pos = dot(d, position);
    let phase = f * dot_d_pos + phi * time;
    let c = cos(phase);
    let s = sin(phase);

    // Gerstner wave displacement
    return vec3<f32>(
        q * a * d.x * c,     // X displacement
        a * s,               // Y displacement (height)
        q * a * d.y * c      // Z displacement
    );
}

// Compute total wave displacement and normal
fn compute_waves(world_xz: vec2<f32>, time: f32) -> vec4<f32> {
    var displacement = vec3<f32>(0.0, water.water_level, 0.0);
    var tangent = vec3<f32>(1.0, 0.0, 0.0);
    var binormal = vec3<f32>(0.0, 0.0, 1.0);

    // Wave layer 0
    if (water.wave0.y > 0.001) {
        let d0 = normalize(water.wave_dir01.xy);
        let w0 = gerstner_wave(world_xz, time, d0, water.wave0.x, water.wave0.y, water.wave0.z, water.wave0.w);
        displacement += w0;

        // Accumulate tangent/binormal contributions
        let phase0 = water.wave0.x * dot(d0, world_xz) + water.wave0.z * 2.0 * PI * time;
        let wa0 = water.wave0.x * water.wave0.y;
        let s0 = sin(phase0);
        let c0 = cos(phase0);
        tangent += vec3<f32>(-d0.x * d0.x * wa0 * s0, d0.x * wa0 * c0, -d0.x * d0.y * wa0 * s0);
        binormal += vec3<f32>(-d0.x * d0.y * wa0 * s0, d0.y * wa0 * c0, -d0.y * d0.y * wa0 * s0);
    }

    // Wave layer 1
    if (water.wave1.y > 0.001) {
        let d1 = normalize(water.wave_dir01.zw);
        let w1 = gerstner_wave(world_xz, time, d1, water.wave1.x, water.wave1.y, water.wave1.z, water.wave1.w);
        displacement += w1;

        let phase1 = water.wave1.x * dot(d1, world_xz) + water.wave1.z * 2.0 * PI * time;
        let wa1 = water.wave1.x * water.wave1.y;
        let s1 = sin(phase1);
        let c1 = cos(phase1);
        tangent += vec3<f32>(-d1.x * d1.x * wa1 * s1, d1.x * wa1 * c1, -d1.x * d1.y * wa1 * s1);
        binormal += vec3<f32>(-d1.x * d1.y * wa1 * s1, d1.y * wa1 * c1, -d1.y * d1.y * wa1 * s1);
    }

    // Wave layer 2
    if (water.wave2.y > 0.001) {
        let d2 = normalize(water.wave_dir23.xy);
        let w2 = gerstner_wave(world_xz, time, d2, water.wave2.x, water.wave2.y, water.wave2.z, water.wave2.w);
        displacement += w2;

        let phase2 = water.wave2.x * dot(d2, world_xz) + water.wave2.z * 2.0 * PI * time;
        let wa2 = water.wave2.x * water.wave2.y;
        let s2 = sin(phase2);
        let c2 = cos(phase2);
        tangent += vec3<f32>(-d2.x * d2.x * wa2 * s2, d2.x * wa2 * c2, -d2.x * d2.y * wa2 * s2);
        binormal += vec3<f32>(-d2.x * d2.y * wa2 * s2, d2.y * wa2 * c2, -d2.y * d2.y * wa2 * s2);
    }

    // Wave layer 3
    if (water.wave3.y > 0.001) {
        let d3 = normalize(water.wave_dir23.zw);
        let w3 = gerstner_wave(world_xz, time, d3, water.wave3.x, water.wave3.y, water.wave3.z, water.wave3.w);
        displacement += w3;

        let phase3 = water.wave3.x * dot(d3, world_xz) + water.wave3.z * 2.0 * PI * time;
        let wa3 = water.wave3.x * water.wave3.y;
        let s3 = sin(phase3);
        let c3 = cos(phase3);
        tangent += vec3<f32>(-d3.x * d3.x * wa3 * s3, d3.x * wa3 * c3, -d3.x * d3.y * wa3 * s3);
        binormal += vec3<f32>(-d3.x * d3.y * wa3 * s3, d3.y * wa3 * c3, -d3.y * d3.y * wa3 * s3);
    }

    // Compute normal from tangent and binormal
    let normal = normalize(cross(binormal, tangent));

    // Pack height in w component (normalized wave height for foam)
    let wave_height = (displacement.y - water.water_level) / max(water.wave0.y + water.wave1.y + water.wave2.y + water.wave3.y, 0.001);

    return vec4<f32>(displacement, wave_height);
}

fn compute_wave_normal(world_xz: vec2<f32>, time: f32) -> vec3<f32> {
    let epsilon = 0.1;
    let h_center = compute_waves(world_xz, time).y;
    let h_right = compute_waves(world_xz + vec2<f32>(epsilon, 0.0), time).y;
    let h_forward = compute_waves(world_xz + vec2<f32>(0.0, epsilon), time).y;

    let dx = h_right - h_center;
    let dz = h_forward - h_center;

    return normalize(vec3<f32>(-dx, epsilon, -dz));
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Get world XZ from input position
    let world_xz = (water.model * vec4<f32>(in.position, 1.0)).xz;

    // Compute wave displacement
    let wave_data = compute_waves(world_xz, water.time);
    let displaced_pos = vec3<f32>(world_xz.x + wave_data.x, wave_data.y, world_xz.y + wave_data.z);

    out.world_position = displaced_pos;
    out.clip_space = camera.view_projection * vec4<f32>(displaced_pos, 1.0);
    out.clip_position = out.clip_space;

    // Compute normal
    out.world_normal = compute_wave_normal(world_xz, water.time);

    out.uv = in.uv;
    out.foam_factor = clamp(wave_data.w, 0.0, 1.0);

    return out;
}

// ==================== Fragment Shader ====================

// Fresnel effect (Schlick approximation)
fn fresnel(cos_theta: f32, strength: f32) -> f32 {
    let f0 = 0.02;  // Water IOR approximation
    let fresnel = f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
    return fresnel * strength;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let V = normalize(camera.position.xyz - in.world_position);
    let NdotV = max(dot(N, V), 0.0);

    // Screen-space coordinates for reflection/refraction
    let ndc = in.clip_space.xy / in.clip_space.w;
    let screen_uv = ndc * 0.5 + 0.5;

    // Distort UVs based on normal for refraction effect
    let distortion = N.xz * 0.02;
    let refract_uv = screen_uv + distortion;
    let reflect_uv = vec2<f32>(screen_uv.x + distortion.x, 1.0 - screen_uv.y + distortion.y);

    // Sample reflection and refraction textures
    let reflection = textureSample(reflection_texture, reflection_sampler, reflect_uv).rgb;
    let refraction = textureSample(refraction_texture, refraction_sampler, refract_uv).rgb;

    // Sample depth for shore fade
    let depth_sample = textureSample(depth_texture, depth_sampler, screen_uv).r;
    let linear_depth = camera.near_far.x * camera.near_far.y / (camera.near_far.y - depth_sample * (camera.near_far.y - camera.near_far.x));
    let water_depth = in.clip_space.w - linear_depth;
    let shore_factor = clamp(water_depth / water.shore_fade_distance, 0.0, 1.0);

    // Fresnel blend between reflection and refraction
    let fresnel_factor = fresnel(NdotV, water.fresnel_strength);

    // Water color based on depth
    let water_color = mix(water.shallow_color.rgb, water.deep_color.rgb, shore_factor);

    // Combine reflection and refraction
    var color = mix(refraction * water_color, reflection, fresnel_factor);

    // Add transparency
    color = mix(refraction, color, water.transparency);

    // Sample detail normal from texture for small ripples
    let normal_sample = textureSample(normal_texture, normal_sampler, in.uv * 10.0 + water.time * 0.1).xyz * 2.0 - 1.0;
    let detail_normal = normalize(N + normal_sample * 0.1);

    // Specular highlights
    var specular = vec3<f32>(0.0);
    for (var i = 0u; i < lights.num_lights; i++) {
        let light = light_data[i];
        let light_type = u32(light.params.x);

        var L: vec3<f32>;
        var light_color: vec3<f32>;

        if (light_type == 0u) {
            L = normalize(-light.position_or_direction.xyz);
            light_color = light.color.rgb * light.color.a;
        } else {
            let light_vec = light.position_or_direction.xyz - in.world_position;
            L = normalize(light_vec);
            let distance = length(light_vec);
            let attenuation = 1.0 / (1.0 + light.params.z * distance * distance);
            light_color = light.color.rgb * light.color.a * attenuation;
        }

        let H = normalize(V + L);
        let NdotH = max(dot(detail_normal, H), 0.0);
        let spec = pow(NdotH, 256.0) * water.specular_intensity;
        specular += light_color * spec;
    }

    color += specular;

    // Foam
    if (water.foam_intensity > 0.0) {
        let foam_sample = textureSample(foam_texture, foam_sampler, in.uv * 20.0 + water.time * 0.2);
        let foam_wave = smoothstep(water.foam_threshold, 1.0, in.foam_factor);
        let foam_shore = 1.0 - smoothstep(0.0, 2.0, water_depth);
        let foam_amount = max(foam_wave, foam_shore) * water.foam_intensity;
        color = mix(color, water.foam_color.rgb, foam_amount * foam_sample.r);
    }

    // Fog
    let fog_distance = length(camera.position.xyz - in.world_position);
    let fog_factor = 1.0 - exp(-fog_distance * 0.0003);
    let fog_color = vec3<f32>(0.7, 0.8, 0.9);
    color = mix(color, fog_color, clamp(fog_factor, 0.0, 0.4));

    return vec4<f32>(color, mix(0.5, 1.0, shore_factor));
}
