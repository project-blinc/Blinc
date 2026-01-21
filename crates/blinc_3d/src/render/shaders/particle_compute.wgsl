// GPU Particle System - Compute Shader
// Handles particle spawning, simulation, and lifecycle entirely on GPU

const PI: f32 = 3.14159265359;
const MAX_FORCES: u32 = 8u;

// Particle data structure (must match Rust struct)
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

// Indirect draw arguments for GPU-driven rendering
struct DrawIndirect {
    vertex_count: u32,
    instance_count: atomic<u32>,
    first_vertex: u32,
    first_instance: u32,
}

// Emitter configuration
struct EmitterUniform {
    // Position and shape
    position: vec4<f32>,        // xyz = position, w = shape type (0=point, 1=sphere, 2=hemisphere, 3=cone, 4=box, 5=circle)
    shape_params: vec4<f32>,    // Depends on shape: sphere/hemi radius, cone angle/radius, box half_extents
    direction: vec4<f32>,       // xyz = emit direction, w = direction randomness
    // Emission
    emission_rate: f32,
    burst_count: u32,
    spawn_accumulated: f32,
    _pad0: f32,
    // Particle properties
    lifetime: vec2<f32>,        // min, max
    start_speed: vec2<f32>,     // min, max
    start_size: vec2<f32>,      // min, max
    end_size: vec2<f32>,        // min, max
    start_color: vec4<f32>,
    end_color: vec4<f32>,
    start_rotation: vec2<f32>,  // min, max
    rotation_speed: vec2<f32>,  // min, max
}

// Force affector
struct ForceAffector {
    force_type: u32,     // 0=gravity, 1=wind, 2=vortex, 3=drag, 4=turbulence, 5=attractor, 6=radial
    strength: f32,
    _pad0: vec2<f32>,
    direction: vec4<f32>,  // xyz = direction or center, w = extra param
    params: vec4<f32>,     // Type-specific parameters
}

// System uniforms
struct SystemUniform {
    max_particles: u32,
    active_particles: u32,
    delta_time: f32,
    time: f32,
    // Simulation space (0 = world, 1 = local)
    simulation_space: u32,
    num_forces: u32,
    gravity_scale: f32,
    _pad: f32,
    // Bounds for culling/collision
    bounds_min: vec4<f32>,
    bounds_max: vec4<f32>,
}

// Bindings
@group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(1) var<storage, read_write> alive_indices: array<u32>;
@group(0) @binding(2) var<storage, read_write> dead_indices: array<u32>;
@group(0) @binding(3) var<storage, read_write> counters: array<atomic<u32>>;  // 0=alive, 1=dead, 2=emit
@group(0) @binding(4) var<storage, read_write> draw_indirect: DrawIndirect;

@group(1) @binding(0) var<uniform> system: SystemUniform;
@group(1) @binding(1) var<uniform> emitter: EmitterUniform;
@group(1) @binding(2) var<storage, read> forces: array<ForceAffector>;

// ==================== Random Number Generation ====================

fn pcg_hash(input: u32) -> u32 {
    let state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn rand_float(seed: ptr<function, u32>) -> f32 {
    *seed = pcg_hash(*seed);
    return f32(*seed) / f32(0xFFFFFFFFu);
}

fn rand_range(seed: ptr<function, u32>, min_val: f32, max_val: f32) -> f32 {
    return min_val + rand_float(seed) * (max_val - min_val);
}

fn rand_vec3(seed: ptr<function, u32>) -> vec3<f32> {
    return vec3<f32>(
        rand_float(seed) * 2.0 - 1.0,
        rand_float(seed) * 2.0 - 1.0,
        rand_float(seed) * 2.0 - 1.0
    );
}

fn rand_unit_sphere(seed: ptr<function, u32>) -> vec3<f32> {
    let theta = rand_float(seed) * 2.0 * PI;
    let phi = acos(2.0 * rand_float(seed) - 1.0);
    return vec3<f32>(
        sin(phi) * cos(theta),
        sin(phi) * sin(theta),
        cos(phi)
    );
}

fn rand_unit_hemisphere(seed: ptr<function, u32>, normal: vec3<f32>) -> vec3<f32> {
    var dir = rand_unit_sphere(seed);
    if (dot(dir, normal) < 0.0) {
        dir = -dir;
    }
    return dir;
}

fn rand_in_cone(seed: ptr<function, u32>, direction: vec3<f32>, angle: f32) -> vec3<f32> {
    let cos_angle = cos(angle);
    let z = rand_range(seed, cos_angle, 1.0);
    let phi = rand_float(seed) * 2.0 * PI;
    let sin_theta = sqrt(1.0 - z * z);

    let local_dir = vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), z);

    // Build rotation from (0,0,1) to direction
    let up = vec3<f32>(0.0, 0.0, 1.0);
    let axis = cross(up, direction);
    let axis_len = length(axis);

    if (axis_len < 0.001) {
        if (direction.z > 0.0) {
            return local_dir;
        } else {
            return vec3<f32>(local_dir.x, local_dir.y, -local_dir.z);
        }
    }

    let axis_norm = axis / axis_len;
    let angle_rot = acos(dot(up, direction));
    let s = sin(angle_rot);
    let c = cos(angle_rot);
    let oc = 1.0 - c;

    // Rodrigues rotation
    let rotated = local_dir * c +
                  cross(axis_norm, local_dir) * s +
                  axis_norm * dot(axis_norm, local_dir) * oc;

    return normalize(rotated);
}

// ==================== Emitter Sampling ====================

fn sample_emitter_position(seed: ptr<function, u32>) -> vec3<f32> {
    let shape = u32(emitter.position.w);
    var pos = emitter.position.xyz;

    switch shape {
        case 0u: {
            // Point - no offset
        }
        case 1u: {
            // Sphere
            let radius = emitter.shape_params.x;
            pos += rand_unit_sphere(seed) * radius * rand_float(seed);
        }
        case 2u: {
            // Hemisphere
            let radius = emitter.shape_params.x;
            let dir = normalize(emitter.direction.xyz);
            pos += rand_unit_hemisphere(seed, dir) * radius * rand_float(seed);
        }
        case 3u: {
            // Cone
            let angle = emitter.shape_params.x;
            let radius = emitter.shape_params.y;
            let height = emitter.shape_params.z;
            let t = rand_float(seed);
            let r = radius * t;
            let theta = rand_float(seed) * 2.0 * PI;
            pos += vec3<f32>(r * cos(theta), t * height, r * sin(theta));
        }
        case 4u: {
            // Box
            let half_extents = emitter.shape_params.xyz;
            pos += (rand_vec3(seed) * 0.5 + 0.5) * half_extents * 2.0 - half_extents;
        }
        case 5u: {
            // Circle
            let radius = emitter.shape_params.x;
            let theta = rand_float(seed) * 2.0 * PI;
            let r = sqrt(rand_float(seed)) * radius;
            pos += vec3<f32>(r * cos(theta), 0.0, r * sin(theta));
        }
        default: {}
    }

    return pos;
}

fn sample_emitter_velocity(seed: ptr<function, u32>, position: vec3<f32>) -> vec3<f32> {
    let speed = rand_range(seed, emitter.start_speed.x, emitter.start_speed.y);
    let randomness = emitter.direction.w;

    var direction = normalize(emitter.direction.xyz);

    // Add randomness to direction
    if (randomness > 0.0) {
        let random_dir = rand_unit_sphere(seed);
        direction = normalize(mix(direction, random_dir, randomness));
    }

    return direction * speed;
}

// ==================== Force Application ====================

fn apply_force(particle: ptr<function, Particle>, force_idx: u32, dt: f32) {
    let force = forces[force_idx];
    var force_vec = vec3<f32>(0.0);

    switch force.force_type {
        case 0u: {
            // Gravity
            force_vec = force.direction.xyz * force.strength;
        }
        case 1u: {
            // Wind
            let turbulence = force.params.x;
            let base_wind = force.direction.xyz * force.strength;
            // Simple turbulence using position-based noise approximation
            let turb = sin((*particle).position.x * 0.1 + system.time) *
                       cos((*particle).position.z * 0.1 + system.time * 0.7) * turbulence;
            force_vec = base_wind + vec3<f32>(turb, 0.0, turb * 0.5);
        }
        case 2u: {
            // Vortex
            let axis = normalize(force.direction.xyz);
            let center = force.params.xyz;
            let to_center = (*particle).position - center;
            let dist = length(to_center - axis * dot(to_center, axis));
            let tangent = normalize(cross(axis, to_center));
            force_vec = tangent * force.strength / max(dist, 0.1);
        }
        case 3u: {
            // Drag
            let vel_mag = length((*particle).velocity);
            if (vel_mag > 0.001) {
                force_vec = -normalize((*particle).velocity) * vel_mag * vel_mag * force.strength;
            }
        }
        case 4u: {
            // Turbulence (3D noise-based)
            let frequency = force.params.x;
            let p = (*particle).position * frequency + system.time;
            // Simplified noise
            let noise_x = sin(p.x) * cos(p.y + p.z);
            let noise_y = sin(p.y) * cos(p.z + p.x);
            let noise_z = sin(p.z) * cos(p.x + p.y);
            force_vec = vec3<f32>(noise_x, noise_y, noise_z) * force.strength;
        }
        case 5u: {
            // Attractor
            let center = force.direction.xyz;
            let to_center = center - (*particle).position;
            let dist = length(to_center);
            if (dist > 0.01) {
                force_vec = normalize(to_center) * force.strength / (dist * dist + 1.0);
            }
        }
        case 6u: {
            // Radial (outward from center)
            let center = force.direction.xyz;
            let from_center = (*particle).position - center;
            let dist = length(from_center);
            if (dist > 0.01) {
                force_vec = normalize(from_center) * force.strength;
            }
        }
        default: {}
    }

    (*particle).velocity += force_vec * dt;
}

// ==================== Compute Kernels ====================

// Emit new particles
@compute @workgroup_size(64)
fn cs_emit(@builtin(global_invocation_id) gid: vec3<u32>) {
    let emit_idx = gid.x;

    // Check how many particles to emit this frame
    let emit_count = atomicLoad(&counters[2]);
    if (emit_idx >= emit_count) {
        return;
    }

    // Try to get a dead particle slot
    let dead_count = atomicSub(&counters[1], 1u);
    if (dead_count == 0u) {
        atomicAdd(&counters[1], 1u);  // Restore counter
        return;
    }

    let particle_idx = dead_indices[dead_count - 1u];

    // Initialize particle with random seed based on index and time
    var seed = pcg_hash(particle_idx + u32(system.time * 1000.0));

    var p: Particle;
    p.position = sample_emitter_position(&seed);
    p.velocity = sample_emitter_velocity(&seed, p.position);
    p.max_life = rand_range(&seed, emitter.lifetime.x, emitter.lifetime.y);
    p.life = p.max_life;
    p.color = emitter.start_color;
    p.size = vec2<f32>(rand_range(&seed, emitter.start_size.x, emitter.start_size.y));
    p.rotation = rand_range(&seed, emitter.start_rotation.x, emitter.start_rotation.y);
    p.rotation_velocity = rand_range(&seed, emitter.rotation_speed.x, emitter.rotation_speed.y);

    particles[particle_idx] = p;

    // Add to alive list
    let alive_idx = atomicAdd(&counters[0], 1u);
    alive_indices[alive_idx] = particle_idx;
}

// Update existing particles
@compute @workgroup_size(64)
fn cs_update(@builtin(global_invocation_id) gid: vec3<u32>) {
    let alive_count = atomicLoad(&counters[0]);
    if (gid.x >= alive_count) {
        return;
    }

    let particle_idx = alive_indices[gid.x];
    var p = particles[particle_idx];

    // Update life
    p.life -= system.delta_time;

    if (p.life <= 0.0) {
        // Particle died - move to dead list
        let dead_idx = atomicAdd(&counters[1], 1u);
        dead_indices[dead_idx] = particle_idx;
        particles[particle_idx].life = -1.0;  // Mark as dead
        return;
    }

    // Calculate life factor (0 = born, 1 = dying)
    let life_factor = 1.0 - (p.life / p.max_life);

    // Apply forces
    for (var i = 0u; i < system.num_forces; i++) {
        apply_force(&p, i, system.delta_time);
    }

    // Apply gravity
    if (system.gravity_scale != 0.0) {
        p.velocity.y -= 9.81 * system.gravity_scale * system.delta_time;
    }

    // Update position
    p.position += p.velocity * system.delta_time;

    // Update rotation
    p.rotation += p.rotation_velocity * system.delta_time;

    // Interpolate color over lifetime
    p.color = mix(emitter.start_color, emitter.end_color, life_factor);

    // Interpolate size over lifetime
    let start_size = p.size.x;
    let end_size = mix(emitter.end_size.x, emitter.end_size.y, (start_size - emitter.start_size.x) / max(emitter.start_size.y - emitter.start_size.x, 0.001));
    p.size = vec2<f32>(mix(start_size, end_size, life_factor));

    particles[particle_idx] = p;
}

// Compact alive particles and prepare draw indirect
@compute @workgroup_size(64)
fn cs_compact(@builtin(global_invocation_id) gid: vec3<u32>) {
    // Reset draw indirect count at thread 0
    if (gid.x == 0u) {
        atomicStore(&draw_indirect.instance_count, 0u);
    }

    workgroupBarrier();

    let alive_count = atomicLoad(&counters[0]);
    if (gid.x >= alive_count) {
        return;
    }

    let particle_idx = alive_indices[gid.x];
    let p = particles[particle_idx];

    // Only include alive particles in draw
    if (p.life > 0.0) {
        let draw_idx = atomicAdd(&draw_indirect.instance_count, 1u);
        // Could write to a sorted/compacted buffer here for depth sorting
    }
}

// Reset counters between frames
@compute @workgroup_size(1)
fn cs_reset() {
    // Calculate emit count for this frame
    let emit_this_frame = u32(emitter.spawn_accumulated + emitter.emission_rate * system.delta_time);
    atomicStore(&counters[2], emit_this_frame);

    // Reset alive counter (will be rebuilt during compaction)
    // Note: Dead counter persists
}
