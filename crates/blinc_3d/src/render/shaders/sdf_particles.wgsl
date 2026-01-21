// SDF particle shader with compute update

struct CameraUniform {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    view_projection: mat4x4<f32>,
    inverse_view: mat4x4<f32>,
    position: vec4<f32>,
    direction: vec4<f32>,
    near_far: vec4<f32>,
}

struct TimeUniform {
    time: f32,
    delta_time: f32,
    frame: u32,
    _padding: u32,
}

struct Particle {
    position: vec3<f32>,
    life: f32,
    velocity: vec3<f32>,
    size: f32,
    color: vec4<f32>,
    // shape: 0=sphere, 1=box, 2=star
    shape: u32,
    rotation: f32,
    _padding: vec2<u32>,
}

struct ParticleSystemUniform {
    emitter_position: vec4<f32>,
    emitter_direction: vec4<f32>,
    gravity: vec4<f32>,
    color_start: vec4<f32>,
    color_end: vec4<f32>,
    size_start: f32,
    size_end: f32,
    life_min: f32,
    life_max: f32,
    speed_min: f32,
    speed_max: f32,
    spread: f32,
    num_particles: u32,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> time: TimeUniform;
@group(1) @binding(0) var<uniform> system: ParticleSystemUniform;
@group(1) @binding(1) var<storage, read_write> particles: array<Particle>;

// ==================== Compute Shader ====================

// Simple hash for random
fn hash(p: u32) -> f32 {
    var x = p;
    x = ((x >> 16u) ^ x) * 0x45d9f3bu;
    x = ((x >> 16u) ^ x) * 0x45d9f3bu;
    x = (x >> 16u) ^ x;
    return f32(x) / f32(0xffffffffu);
}

fn hash3(p: u32) -> vec3<f32> {
    return vec3<f32>(
        hash(p),
        hash(p + 1u),
        hash(p + 2u)
    );
}

@compute @workgroup_size(64)
fn cs_update(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= system.num_particles) {
        return;
    }

    var p = particles[idx];

    // Update life
    p.life -= time.delta_time;

    // Respawn dead particles
    if (p.life <= 0.0) {
        let seed = idx + time.frame * 1000u;
        let rand = hash3(seed);

        // Reset position at emitter
        p.position = system.emitter_position.xyz;

        // Random direction with spread
        let theta = rand.x * 6.28318;
        let phi = rand.y * system.spread;
        let dir = vec3<f32>(
            sin(phi) * cos(theta),
            cos(phi),
            sin(phi) * sin(theta)
        );

        // Apply emitter direction
        let speed = mix(system.speed_min, system.speed_max, rand.z);
        p.velocity = dir * speed + system.emitter_direction.xyz;

        // Reset life
        p.life = mix(system.life_min, system.life_max, hash(seed + 3u));

        // Set initial size and color
        p.size = system.size_start;
        p.color = system.color_start;
        p.shape = u32(hash(seed + 4u) * 3.0);
        p.rotation = 0.0;
    } else {
        // Update physics
        p.velocity += system.gravity.xyz * time.delta_time;
        p.position += p.velocity * time.delta_time;

        // Calculate life ratio (0 = just born, 1 = about to die)
        let max_life = mix(system.life_min, system.life_max, 0.5);
        let life_ratio = 1.0 - (p.life / max_life);

        // Interpolate size and color
        p.size = mix(system.size_start, system.size_end, life_ratio);
        p.color = mix(system.color_start, system.color_end, life_ratio);

        // Rotate
        p.rotation += time.delta_time * 2.0;
    }

    particles[idx] = p;
}

// ==================== Render Shader ====================

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) shape: u32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32
) -> VertexOutput {
    var out: VertexOutput;

    let p = particles[instance_index];

    // Skip dead particles
    if (p.life <= 0.0) {
        out.clip_position = vec4<f32>(0.0, 0.0, -2.0, 1.0);
        return out;
    }

    // Quad vertices
    let quad_verts = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 1.0)
    );

    let local_pos = quad_verts[vertex_index];

    // Billboard facing camera
    let camera_right = camera.inverse_view[0].xyz;
    let camera_up = camera.inverse_view[1].xyz;

    // Apply rotation
    let c = cos(p.rotation);
    let s = sin(p.rotation);
    let rotated = vec2<f32>(
        local_pos.x * c - local_pos.y * s,
        local_pos.x * s + local_pos.y * c
    );

    let world_offset = (rotated.x * camera_right + rotated.y * camera_up) * p.size;
    let world_pos = p.position + world_offset;

    out.clip_position = camera.view_projection * vec4<f32>(world_pos, 1.0);
    out.uv = local_pos * 0.5 + 0.5;
    out.color = p.color;
    out.shape = p.shape;

    return out;
}

// SDF shapes for particles
fn sdf_circle(uv: vec2<f32>) -> f32 {
    return length(uv - 0.5) - 0.4;
}

fn sdf_square(uv: vec2<f32>) -> f32 {
    let p = abs(uv - 0.5);
    return max(p.x, p.y) - 0.35;
}

fn sdf_star(uv: vec2<f32>) -> f32 {
    let p = uv - 0.5;
    let angle = atan2(p.y, p.x);
    let r = length(p);
    let star = 0.3 + 0.1 * cos(angle * 5.0);
    return r - star;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var dist: f32;

    switch (in.shape) {
        case 0u: {
            dist = sdf_circle(in.uv);
        }
        case 1u: {
            dist = sdf_square(in.uv);
        }
        case 2u: {
            dist = sdf_star(in.uv);
        }
        default: {
            dist = sdf_circle(in.uv);
        }
    }

    // Anti-aliased edge
    let alpha = 1.0 - smoothstep(-0.02, 0.02, dist);

    if (alpha < 0.01) {
        discard;
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
