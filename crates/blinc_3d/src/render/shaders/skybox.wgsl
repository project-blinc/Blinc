// Skybox shader (cubemap or procedural)

struct CameraUniform {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    view_projection: mat4x4<f32>,
    inverse_view: mat4x4<f32>,
    position: vec4<f32>,
    direction: vec4<f32>,
    near_far: vec4<f32>,
}

struct SkyboxUniform {
    // For procedural sky
    sun_direction: vec4<f32>,
    sun_color: vec4<f32>,
    sky_color: vec4<f32>,
    horizon_color: vec4<f32>,
    ground_color: vec4<f32>,
    sun_size: f32,
    atmosphere_density: f32,
    use_cubemap: u32,
    _padding: u32,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> skybox: SkyboxUniform;
@group(1) @binding(1) var cubemap_texture: texture_cube<f32>;
@group(1) @binding(2) var cubemap_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) direction: vec3<f32>,
}

// Fullscreen triangle
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Generate fullscreen triangle
    let x = f32(i32(vertex_index) - 1);
    let y = f32(i32(vertex_index & 1u) * 2 - 1);

    out.position = vec4<f32>(x, y, 1.0, 1.0);

    // Calculate world direction from clip space
    let clip = vec4<f32>(x, y, 1.0, 1.0);

    // Inverse projection (without translation)
    var view_no_translate = camera.view;
    view_no_translate[3] = vec4<f32>(0.0, 0.0, 0.0, 1.0);

    let inv_vp = camera.inverse_view * inverse(camera.projection);
    let world_dir = inv_vp * clip;
    out.direction = normalize(world_dir.xyz);

    return out;
}

// Procedural sky based on direction
fn procedural_sky(dir: vec3<f32>) -> vec3<f32> {
    let sun_dir = normalize(skybox.sun_direction.xyz);

    // Sun disc
    let sun_dot = dot(dir, sun_dir);
    let sun_disc = smoothstep(1.0 - skybox.sun_size * 0.01, 1.0 - skybox.sun_size * 0.005, sun_dot);
    let sun = skybox.sun_color.rgb * sun_disc * skybox.sun_color.a;

    // Sun glow
    let sun_glow = pow(max(sun_dot, 0.0), 8.0) * 0.5;
    let glow = skybox.sun_color.rgb * sun_glow;

    // Sky gradient
    let up_factor = dir.y * 0.5 + 0.5;

    var sky: vec3<f32>;
    if (dir.y > 0.0) {
        // Above horizon: horizon to sky
        let t = pow(dir.y, skybox.atmosphere_density);
        sky = mix(skybox.horizon_color.rgb, skybox.sky_color.rgb, t);
    } else {
        // Below horizon: horizon to ground
        let t = pow(-dir.y, 0.5);
        sky = mix(skybox.horizon_color.rgb, skybox.ground_color.rgb, t);
    }

    // Combine
    return sky + sun + glow;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.direction);

    var color: vec3<f32>;

    if (skybox.use_cubemap == 1u) {
        // Sample cubemap
        color = textureSample(cubemap_texture, cubemap_sampler, dir).rgb;
    } else {
        // Procedural sky
        color = procedural_sky(dir);
    }

    return vec4<f32>(color, 1.0);
}
