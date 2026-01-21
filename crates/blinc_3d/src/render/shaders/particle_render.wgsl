// GPU Particle System - Render Shader
// Renders particles as camera-facing billboards with various modes

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

struct RenderUniform {
    // Render mode: 0=billboard, 1=stretched, 2=horizontal, 3=vertical, 4=mesh
    render_mode: u32,
    // Blend mode: 0=alpha, 1=additive, 2=multiply, 3=premultiplied
    blend_mode: u32,
    // Sorting: 0=none, 1=back_to_front, 2=front_to_back
    sort_mode: u32,
    // Soft particles
    soft_particles_enabled: u32,
    soft_particles_distance: f32,
    // Stretch parameters (for stretched billboard mode)
    length_scale: f32,
    speed_scale: f32,
    _pad: f32,
    // Animation
    sprite_sheet_size: vec2<f32>,  // columns, rows
    animation_speed: f32,
    _pad2: f32,
}

// Particle data (matches compute shader)
struct Particle {
    position: vec3<f32>,
    life: f32,
    velocity: vec3<f32>,
    max_life: f32,
    color: vec4<f32>,
    size: vec2<f32>,
    rotation: f32,
    rotation_velocity: f32,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> render: RenderUniform;
@group(1) @binding(0) var<storage, read> particles: array<Particle>;
@group(1) @binding(1) var<storage, read> alive_indices: array<u32>;
@group(2) @binding(0) var particle_texture: texture_2d<f32>;
@group(2) @binding(1) var particle_sampler: sampler;
@group(2) @binding(2) var depth_texture: texture_2d<f32>;
@group(2) @binding(3) var depth_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_position: vec3<f32>,
    @location(3) @interpolate(flat) particle_idx: u32,
}

// Billboard quad vertices (two triangles)
const QUAD_POSITIONS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-0.5, -0.5),
    vec2<f32>(0.5, -0.5),
    vec2<f32>(0.5, 0.5),
    vec2<f32>(-0.5, -0.5),
    vec2<f32>(0.5, 0.5),
    vec2<f32>(-0.5, 0.5),
);

const QUAD_UVS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.0, 0.0),
);

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_idx: u32,
    @builtin(instance_index) instance_idx: u32
) -> VertexOutput {
    var out: VertexOutput;

    // Get particle from alive list
    let particle_idx = alive_indices[instance_idx];
    let p = particles[particle_idx];

    // Skip dead particles
    if (p.life <= 0.0) {
        out.clip_position = vec4<f32>(0.0, 0.0, -1000.0, 1.0);
        return out;
    }

    // Get quad vertex position
    let local_idx = vertex_idx % 6u;
    var quad_pos = QUAD_POSITIONS[local_idx];
    var uv = QUAD_UVS[local_idx];

    // Apply rotation
    let cos_r = cos(p.rotation);
    let sin_r = sin(p.rotation);
    let rotated_pos = vec2<f32>(
        quad_pos.x * cos_r - quad_pos.y * sin_r,
        quad_pos.x * sin_r + quad_pos.y * cos_r
    );

    // Scale by particle size
    let scaled_pos = rotated_pos * p.size;

    // Get camera basis vectors from inverse view matrix
    let cam_right = normalize(camera.inverse_view[0].xyz);
    let cam_up = normalize(camera.inverse_view[1].xyz);
    let cam_forward = normalize(camera.inverse_view[2].xyz);

    // Calculate world position based on render mode
    var world_pos: vec3<f32>;

    switch render.render_mode {
        case 0u: {
            // Standard billboard - always face camera
            world_pos = p.position +
                        cam_right * scaled_pos.x +
                        cam_up * scaled_pos.y;
        }
        case 1u: {
            // Stretched billboard - stretch along velocity
            let vel_len = length(p.velocity);
            if (vel_len > 0.001) {
                let vel_dir = p.velocity / vel_len;
                let stretch = render.length_scale + vel_len * render.speed_scale;

                // Right vector perpendicular to velocity and view direction
                let view_dir = normalize(camera.position.xyz - p.position);
                var right = normalize(cross(vel_dir, view_dir));
                if (length(right) < 0.001) {
                    right = cam_right;
                }

                world_pos = p.position +
                            right * scaled_pos.x +
                            vel_dir * scaled_pos.y * stretch;
            } else {
                // Fallback to billboard
                world_pos = p.position +
                            cam_right * scaled_pos.x +
                            cam_up * scaled_pos.y;
            }
        }
        case 2u: {
            // Horizontal billboard - always flat on XZ plane
            world_pos = p.position +
                        vec3<f32>(1.0, 0.0, 0.0) * scaled_pos.x +
                        vec3<f32>(0.0, 0.0, 1.0) * scaled_pos.y;
        }
        case 3u: {
            // Vertical billboard - vertical but rotates on Y to face camera
            let to_cam = normalize(vec3<f32>(camera.position.x - p.position.x, 0.0, camera.position.z - p.position.z));
            let right = vec3<f32>(-to_cam.z, 0.0, to_cam.x);
            world_pos = p.position +
                        right * scaled_pos.x +
                        vec3<f32>(0.0, 1.0, 0.0) * scaled_pos.y;
        }
        default: {
            world_pos = p.position + cam_right * scaled_pos.x + cam_up * scaled_pos.y;
        }
    }

    // Apply sprite sheet animation if enabled
    if (render.sprite_sheet_size.x > 1.0 || render.sprite_sheet_size.y > 1.0) {
        let total_frames = u32(render.sprite_sheet_size.x * render.sprite_sheet_size.y);
        let life_factor = 1.0 - (p.life / p.max_life);
        var frame = u32(life_factor * f32(total_frames) * render.animation_speed) % total_frames;

        let col = frame % u32(render.sprite_sheet_size.x);
        let row = frame / u32(render.sprite_sheet_size.x);

        let frame_size = vec2<f32>(1.0 / render.sprite_sheet_size.x, 1.0 / render.sprite_sheet_size.y);
        uv = uv * frame_size + vec2<f32>(f32(col), f32(row)) * frame_size;
    }

    out.clip_position = camera.view_projection * vec4<f32>(world_pos, 1.0);
    out.world_position = world_pos;
    out.uv = uv;
    out.color = p.color;
    out.particle_idx = particle_idx;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample particle texture
    var tex_color = textureSample(particle_texture, particle_sampler, in.uv);

    // Apply particle color
    var color = tex_color * in.color;

    // Soft particles (fade near geometry)
    if (render.soft_particles_enabled != 0u) {
        let screen_uv = (in.clip_position.xy / vec2<f32>(textureDimensions(depth_texture)));
        let scene_depth = textureSample(depth_texture, depth_sampler, screen_uv).r;

        // Convert depths to linear
        let near = camera.near_far.x;
        let far = camera.near_far.y;
        let particle_depth = in.clip_position.z;

        let scene_linear = near * far / (far - scene_depth * (far - near));
        let particle_linear = near * far / (far - particle_depth * (far - near));

        let depth_diff = scene_linear - particle_linear;
        let fade = clamp(depth_diff / render.soft_particles_distance, 0.0, 1.0);

        color.a *= fade;
    }

    // Apply blend mode adjustments
    switch render.blend_mode {
        case 0u: {
            // Alpha blend - standard
        }
        case 1u: {
            // Additive - multiply by alpha for proper additive
            color = vec4<f32>(color.rgb * color.a, color.a);
        }
        case 2u: {
            // Multiply - invert for multiply effect
            color.rgb = 1.0 - (1.0 - color.rgb) * color.a;
        }
        case 3u: {
            // Premultiplied alpha
            color.rgb *= color.a;
        }
        default: {}
    }

    // Discard fully transparent pixels
    if (color.a < 0.001) {
        discard;
    }

    return color;
}

// Alternative fragment shader for additive blending (separate entry point)
@fragment
fn fs_additive(in: VertexOutput) -> @location(0) vec4<f32> {
    var tex_color = textureSample(particle_texture, particle_sampler, in.uv);
    var color = tex_color * in.color;

    // Soft particles
    if (render.soft_particles_enabled != 0u) {
        let screen_uv = (in.clip_position.xy / vec2<f32>(textureDimensions(depth_texture)));
        let scene_depth = textureSample(depth_texture, depth_sampler, screen_uv).r;
        let near = camera.near_far.x;
        let far = camera.near_far.y;
        let scene_linear = near * far / (far - scene_depth * (far - near));
        let particle_linear = near * far / (far - in.clip_position.z * (far - near));
        let depth_diff = scene_linear - particle_linear;
        let fade = clamp(depth_diff / render.soft_particles_distance, 0.0, 1.0);
        color *= fade;
    }

    // For additive, output color multiplied by alpha, no alpha write
    return vec4<f32>(color.rgb * color.a, 0.0);
}
