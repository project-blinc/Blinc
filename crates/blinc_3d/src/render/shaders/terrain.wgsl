// GPU-based procedural terrain shader with vertex displacement
// Computes heightmap noise on GPU for real-time terrain deformation

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

struct TerrainUniform {
    // Transform
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    // Terrain parameters
    world_offset: vec4<f32>,      // xy = world offset, zw = chunk offset
    terrain_scale: vec4<f32>,     // x = size, y = max_height, z = uv_scale, w = unused
    // Noise parameters (up to 4 layers)
    noise_params0: vec4<f32>,     // x = frequency, y = amplitude, z = octaves, w = type
    noise_params1: vec4<f32>,
    noise_params2: vec4<f32>,
    noise_params3: vec4<f32>,
    // LOD and morphing
    lod_params: vec4<f32>,        // x = lod_level, y = morph_factor, z = resolution, w = unused
    // Water level for shore blending
    water_level: f32,
    time: f32,
    _padding: vec2<f32>,
}

struct TerrainMaterial {
    // Texture splatting weights thresholds
    grass_height: f32,
    rock_slope: f32,
    snow_height: f32,
    sand_height: f32,
    // Colors for non-textured mode
    grass_color: vec4<f32>,
    rock_color: vec4<f32>,
    snow_color: vec4<f32>,
    sand_color: vec4<f32>,
    // Tiling
    texture_scale: vec4<f32>,     // xy = grass, zw = rock
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
@group(1) @binding(0) var<uniform> terrain: TerrainUniform;
@group(1) @binding(1) var<uniform> material: TerrainMaterial;
@group(2) @binding(0) var<uniform> lights: LightsUniform;
@group(2) @binding(1) var<storage, read> light_data: array<LightUniform>;
@group(3) @binding(0) var grass_texture: texture_2d<f32>;
@group(3) @binding(1) var grass_sampler: sampler;
@group(3) @binding(2) var rock_texture: texture_2d<f32>;
@group(3) @binding(3) var rock_sampler: sampler;
@group(3) @binding(4) var snow_texture: texture_2d<f32>;
@group(3) @binding(5) var snow_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) height: f32,
    @location(4) slope: f32,
}

// ==================== Noise Functions ====================

// Hash function for pseudo-random values
fn hash2(p: vec2<f32>) -> f32 {
    let p2 = vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)));
    return fract(sin(dot(p2, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn hash3(p: vec3<f32>) -> f32 {
    let p2 = fract(p * 0.1031);
    let p3 = p2 + dot(p2, p2.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Smooth interpolation
fn fade(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

// Perlin noise 2D
fn perlin_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);

    let u = vec2<f32>(fade(f.x), fade(f.y));

    let a = hash2(i + vec2<f32>(0.0, 0.0));
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));

    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * 2.0 - 1.0;
}

// Simplex-like noise 2D
fn simplex_noise(p: vec2<f32>) -> f32 {
    let K1 = 0.366025404;  // (sqrt(3)-1)/2
    let K2 = 0.211324865;  // (3-sqrt(3))/6

    let i = floor(p + (p.x + p.y) * K1);
    let a = p - i + (i.x + i.y) * K2;
    let o = select(vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), a.x > a.y);
    let b = a - o + K2;
    let c = a - 1.0 + 2.0 * K2;

    let h = max(vec3<f32>(0.5) - vec3<f32>(dot(a, a), dot(b, b), dot(c, c)), vec3<f32>(0.0));
    let n = h * h * h * h * vec3<f32>(
        dot(a, vec2<f32>(hash2(i) - 0.5, hash2(i + vec2<f32>(1.0, 0.0)) - 0.5)),
        dot(b, vec2<f32>(hash2(i + o) - 0.5, hash2(i + o + vec2<f32>(1.0, 0.0)) - 0.5)),
        dot(c, vec2<f32>(hash2(i + vec2<f32>(1.0, 1.0)) - 0.5, hash2(i + vec2<f32>(2.0, 1.0)) - 0.5))
    );

    return dot(n, vec3<f32>(70.0));
}

// Worley/cellular noise
fn worley_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);

    var min_dist = 1.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let neighbor = vec2<f32>(f32(x), f32(y));
            let point = vec2<f32>(hash2(i + neighbor), hash2((i + neighbor) * 1.7));
            let diff = neighbor + point - f;
            let dist = length(diff);
            min_dist = min(min_dist, dist);
        }
    }

    return min_dist;
}

// Ridged noise (absolute value creates ridges)
fn ridged_noise(p: vec2<f32>) -> f32 {
    return 1.0 - abs(perlin_noise(p));
}

// Billow noise (absolute value creates billowy clouds)
fn billow_noise(p: vec2<f32>) -> f32 {
    return abs(perlin_noise(p)) * 2.0 - 1.0;
}

// FBM (Fractal Brownian Motion) with configurable noise type
fn fbm(p: vec2<f32>, octaves: u32, noise_type: u32) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var frequency = 1.0;
    var max_value = 0.0;

    for (var i = 0u; i < octaves; i++) {
        let sample_pos = p * frequency;
        var noise_value: f32;

        switch noise_type {
            case 0u: { noise_value = perlin_noise(sample_pos); }   // Perlin
            case 1u: { noise_value = simplex_noise(sample_pos); }  // Simplex
            case 2u: { noise_value = worley_noise(sample_pos); }   // Worley
            case 3u: { noise_value = ridged_noise(sample_pos); }   // Ridged
            case 4u: { noise_value = billow_noise(sample_pos); }   // Billow
            default: { noise_value = perlin_noise(sample_pos); }
        }

        value += noise_value * amplitude;
        max_value += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }

    return value / max_value;
}

// Sample a single noise layer
fn sample_noise_layer(p: vec2<f32>, params: vec4<f32>) -> f32 {
    let frequency = params.x;
    let amplitude = params.y;
    let octaves = u32(params.z);
    let noise_type = u32(params.w);

    if (amplitude < 0.001) {
        return 0.0;
    }

    return fbm(p * frequency, octaves, noise_type) * amplitude;
}

// Compute terrain height at world position
fn compute_height(world_xz: vec2<f32>) -> f32 {
    var height = 0.0;

    // Sample each noise layer
    height += sample_noise_layer(world_xz, terrain.noise_params0);
    height += sample_noise_layer(world_xz, terrain.noise_params1);
    height += sample_noise_layer(world_xz, terrain.noise_params2);
    height += sample_noise_layer(world_xz, terrain.noise_params3);

    // Normalize to 0-1 range then scale by max height
    height = (height + 1.0) * 0.5;
    height = clamp(height, 0.0, 1.0);

    return height * terrain.terrain_scale.y;
}

// Compute normal from heightmap gradient
fn compute_normal(world_xz: vec2<f32>, height: f32) -> vec3<f32> {
    let epsilon = terrain.terrain_scale.x / terrain.lod_params.z;

    let h_left = compute_height(world_xz - vec2<f32>(epsilon, 0.0));
    let h_right = compute_height(world_xz + vec2<f32>(epsilon, 0.0));
    let h_down = compute_height(world_xz - vec2<f32>(0.0, epsilon));
    let h_up = compute_height(world_xz + vec2<f32>(0.0, epsilon));

    let dx = (h_right - h_left) / (2.0 * epsilon);
    let dz = (h_up - h_down) / (2.0 * epsilon);

    return normalize(vec3<f32>(-dx, 1.0, -dz));
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Calculate world XZ position
    let world_xz = in.position.xz + terrain.world_offset.xy;

    // Compute height using GPU noise
    let height = compute_height(world_xz);

    // Apply LOD morphing for smooth transitions
    var final_height = height;
    if (terrain.lod_params.y > 0.0) {
        // Morph between LOD levels
        let coarse_xz = floor(world_xz / 2.0) * 2.0;
        let coarse_height = compute_height(coarse_xz);
        final_height = mix(height, coarse_height, terrain.lod_params.y);
    }

    // Create displaced position
    var world_pos = vec3<f32>(world_xz.x, final_height, world_xz.y);

    // Transform to clip space
    out.world_position = world_pos;
    out.clip_position = camera.view_projection * vec4<f32>(world_pos, 1.0);

    // Compute normal
    out.world_normal = compute_normal(world_xz, final_height);

    // Pass through UVs and height data
    out.uv = in.uv * terrain.terrain_scale.z;
    out.height = final_height / terrain.terrain_scale.y;  // Normalized 0-1
    out.slope = 1.0 - out.world_normal.y;  // 0 = flat, 1 = vertical

    return out;
}

// ==================== Fragment Shader ====================

// Triplanar texture mapping for steep surfaces
fn triplanar_sample(
    tex: texture_2d<f32>,
    tex_sampler: sampler,
    world_pos: vec3<f32>,
    normal: vec3<f32>,
    scale: f32
) -> vec4<f32> {
    let blend = abs(normal);
    let blend_normalized = blend / (blend.x + blend.y + blend.z);

    let x_proj = textureSample(tex, tex_sampler, world_pos.zy * scale);
    let y_proj = textureSample(tex, tex_sampler, world_pos.xz * scale);
    let z_proj = textureSample(tex, tex_sampler, world_pos.xy * scale);

    return x_proj * blend_normalized.x + y_proj * blend_normalized.y + z_proj * blend_normalized.z;
}

// Calculate terrain material blend weights
fn calculate_blend_weights(height: f32, slope: f32, world_y: f32) -> vec4<f32> {
    var weights = vec4<f32>(0.0);

    // Sand near water level
    let sand_factor = smoothstep(material.sand_height + 0.05, material.sand_height, height);
    weights.w = sand_factor;

    // Snow at high altitudes
    let snow_factor = smoothstep(material.snow_height - 0.1, material.snow_height, height);
    weights.z = snow_factor * (1.0 - sand_factor);

    // Rock on steep slopes
    let rock_factor = smoothstep(material.rock_slope - 0.1, material.rock_slope, slope);
    weights.y = rock_factor * (1.0 - snow_factor) * (1.0 - sand_factor);

    // Grass everywhere else
    weights.x = 1.0 - weights.y - weights.z - weights.w;
    weights.x = max(weights.x, 0.0);

    return weights;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let V = normalize(camera.position.xyz - in.world_position);

    // Calculate material blend weights
    let blend = calculate_blend_weights(in.height, in.slope, in.world_position.y);

    // Sample textures with triplanar mapping for rocky areas
    let grass = triplanar_sample(grass_texture, grass_sampler, in.world_position, N, material.texture_scale.x);
    let rock = triplanar_sample(rock_texture, rock_sampler, in.world_position, N, material.texture_scale.z);
    let snow = triplanar_sample(snow_texture, snow_sampler, in.world_position, N, material.texture_scale.x);

    // Blend textures
    var albedo = grass.rgb * blend.x + rock.rgb * blend.y + snow.rgb * blend.z;

    // Add color tinting
    albedo = albedo * (
        material.grass_color.rgb * blend.x +
        material.rock_color.rgb * blend.y +
        material.snow_color.rgb * blend.z +
        material.sand_color.rgb * blend.w
    );

    // Simple Lambertian lighting
    var Lo = vec3<f32>(0.0);

    for (var i = 0u; i < lights.num_lights; i++) {
        let light = light_data[i];
        let light_type = u32(light.params.x);

        var L: vec3<f32>;
        var radiance: vec3<f32>;

        if (light_type == 0u) {
            // Directional light
            L = normalize(-light.position_or_direction.xyz);
            radiance = light.color.rgb * light.color.a;
        } else {
            // Point light
            let light_vec = light.position_or_direction.xyz - in.world_position;
            let distance = length(light_vec);
            L = normalize(light_vec);
            let attenuation = 1.0 / (1.0 + light.params.z * distance * distance);
            radiance = light.color.rgb * light.color.a * attenuation;
        }

        let NdotL = max(dot(N, L), 0.0);
        Lo += albedo * radiance * NdotL;
    }

    // Ambient
    let ambient = lights.ambient.rgb * albedo * 0.3;

    // Shore blending (fade to water color near water level)
    let shore_blend = smoothstep(terrain.water_level - 0.5, terrain.water_level + 0.5, in.world_position.y);

    var color = ambient + Lo;

    // Simple fog based on distance
    let fog_distance = length(camera.position.xyz - in.world_position);
    let fog_factor = 1.0 - exp(-fog_distance * 0.0005);
    let fog_color = vec3<f32>(0.7, 0.8, 0.9);
    color = mix(color, fog_color, clamp(fog_factor, 0.0, 0.5));

    // Gamma correction
    color = pow(color, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(color, 1.0);
}
