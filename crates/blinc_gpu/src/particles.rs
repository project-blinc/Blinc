//! GPU Particle System
//!
//! Provides fully GPU-accelerated particle simulation and rendering.
//! Particles are simulated using compute shaders and rendered using
//! instanced billboards.
//!
//! # Architecture
//!
//! ```text
//! ParticleSystemGpu
//!        │
//!        ├── Compute Pass (Simulation)
//!        │   └── Updates particle positions, velocities, lifetimes
//!        │
//!        └── Render Pass (Drawing)
//!            └── Draws particle billboards as instanced quads
//! ```

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

/// Maximum particles per system for buffer allocation
pub const MAX_PARTICLES_PER_SYSTEM: u32 = 100_000;

/// GPU particle data structure
/// Must match the WGSL struct layout exactly
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuParticle {
    /// Position (xyz) and life remaining (w)
    pub position_life: [f32; 4],
    /// Velocity (xyz) and max lifetime (w)
    pub velocity_max_life: [f32; 4],
    /// Color (rgba)
    pub color: [f32; 4],
    /// Size (current, start, end, rotation)
    pub size_rotation: [f32; 4],
}

impl Default for GpuParticle {
    fn default() -> Self {
        Self {
            position_life: [0.0, 0.0, 0.0, 0.0], // life=0 means inactive
            velocity_max_life: [0.0, 0.0, 0.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            size_rotation: [0.1, 0.1, 0.0, 0.0],
        }
    }
}

/// Emitter configuration for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuEmitter {
    /// Emitter position (xyz) and shape type (w as u32 bits)
    pub position_shape: [f32; 4],
    /// Shape parameters (radius, angle, half_extents, etc.)
    pub shape_params: [f32; 4],
    /// Direction (xyz) and randomness (w)
    pub direction_randomness: [f32; 4],
    /// Emission rate, burst count, spawn accumulated, gravity scale
    pub emission_config: [f32; 4],
    /// Lifetime (min, max), speed (min, max)
    pub lifetime_speed: [f32; 4],
    /// Start size (min, max), end size (min, max)
    pub size_config: [f32; 4],
    /// Start color (rgba) - base of fire (yellow)
    pub start_color: [f32; 4],
    /// Mid color (rgba) - middle of fire (red)
    pub mid_color: [f32; 4],
    /// End color (rgba) - tip of fire (dark/burnt)
    pub end_color: [f32; 4],
}

impl Default for GpuEmitter {
    fn default() -> Self {
        Self {
            position_shape: [0.0, 0.0, 0.0, 0.0], // Point emitter
            shape_params: [0.0; 4],
            direction_randomness: [0.0, 1.0, 0.0, 0.0], // Up, no randomness
            emission_config: [100.0, 0.0, 0.0, 1.0], // 100/s, no burst, gravity=1
            lifetime_speed: [1.0, 2.0, 1.0, 2.0],
            size_config: [0.1, 0.2, 0.0, 0.1],
            start_color: [1.0, 1.0, 1.0, 1.0],
            mid_color: [1.0, 1.0, 1.0, 0.5],
            end_color: [1.0, 1.0, 1.0, 0.0],
        }
    }
}

/// Force affector for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuForce {
    /// Force type (0=gravity, 1=wind, 2=vortex, 3=drag, 4=turbulence, 5=attractor)
    /// and strength packed as (type, strength, 0, 0)
    pub type_strength: [f32; 4],
    /// Direction/position (xyz) and extra param (w)
    pub direction_params: [f32; 4],
}

impl Default for GpuForce {
    fn default() -> Self {
        Self {
            type_strength: [0.0, 0.0, 0.0, 0.0],
            direction_params: [0.0, -9.8, 0.0, 0.0], // Default gravity
        }
    }
}

/// Simulation uniforms for compute shader
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuSimulationUniforms {
    /// Delta time, total time, max particles, active particles
    pub time_config: [f32; 4],
    /// Random seed (4 values for better distribution)
    pub random_seed: [f32; 4],
    /// Number of forces, padding
    pub force_config: [f32; 4],
}

impl Default for GpuSimulationUniforms {
    fn default() -> Self {
        Self {
            time_config: [0.016, 0.0, MAX_PARTICLES_PER_SYSTEM as f32, 0.0],
            random_seed: [0.0; 4],
            force_config: [0.0; 4],
        }
    }
}

/// Render uniforms for vertex/fragment shaders
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuRenderUniforms {
    /// View-projection matrix (column-major)
    pub view_proj: [[f32; 4]; 4],
    /// Camera position (xyz) and FOV (w)
    pub camera_pos_fov: [f32; 4],
    /// Camera right vector (xyz) and aspect ratio (w)
    pub camera_right_aspect: [f32; 4],
    /// Camera up vector (xyz) and padding (w)
    pub camera_up: [f32; 4],
    /// Viewport size (width, height) and render mode, blend mode
    pub viewport_config: [f32; 4],
}

impl Default for GpuRenderUniforms {
    fn default() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            camera_pos_fov: [0.0, 0.0, 5.0, 0.8],
            camera_right_aspect: [1.0, 0.0, 0.0, 1.0],
            camera_up: [0.0, 1.0, 0.0, 0.0],
            viewport_config: [800.0, 600.0, 0.0, 0.0],
        }
    }
}

/// Particle viewport data for rendering
#[derive(Clone, Debug)]
pub struct ParticleViewport {
    /// Emitter configuration
    pub emitter: GpuEmitter,
    /// Force affectors (up to 8)
    pub forces: Vec<GpuForce>,
    /// Maximum particles
    pub max_particles: u32,
    /// Camera position
    pub camera_pos: [f32; 3],
    /// Camera target
    pub camera_target: [f32; 3],
    /// Camera up vector
    pub camera_up: [f32; 3],
    /// Field of view
    pub fov: f32,
    /// Current time
    pub time: f32,
    /// Delta time
    pub delta_time: f32,
    /// Viewport bounds (x, y, width, height)
    pub bounds: [f32; 4],
    /// Blend mode (0=alpha, 1=additive)
    pub blend_mode: u32,
    /// Whether system is playing
    pub playing: bool,
}

impl Default for ParticleViewport {
    fn default() -> Self {
        Self {
            emitter: GpuEmitter::default(),
            forces: Vec::new(),
            max_particles: 10000,
            camera_pos: [0.0, 0.0, 5.0],
            camera_target: [0.0, 0.0, 0.0],
            camera_up: [0.0, 1.0, 0.0],
            fov: 0.8,
            time: 0.0,
            delta_time: 0.016,
            bounds: [0.0, 0.0, 800.0, 600.0],
            blend_mode: 0,
            playing: true,
        }
    }
}

/// GPU particle compute shader
pub const PARTICLE_COMPUTE_SHADER: &str = r#"
// ============================================================================
// Blinc GPU Particle Compute Shader
// ============================================================================

struct Particle {
    position_life: vec4<f32>,      // xyz=position, w=life remaining
    velocity_max_life: vec4<f32>,  // xyz=velocity, w=max lifetime
    color: vec4<f32>,              // rgba
    size_rotation: vec4<f32>,      // current, start, end, rotation
}

struct Emitter {
    position_shape: vec4<f32>,       // xyz=position, w=shape type
    shape_params: vec4<f32>,         // shape-specific params
    direction_randomness: vec4<f32>, // xyz=direction, w=randomness
    emission_config: vec4<f32>,      // rate, burst, spawn_acc, gravity_scale
    lifetime_speed: vec4<f32>,       // min_life, max_life, min_speed, max_speed
    size_config: vec4<f32>,          // start_min, start_max, end_min, end_max
    start_color: vec4<f32>,          // rgba - base (young particles)
    mid_color: vec4<f32>,            // rgba - middle age
    end_color: vec4<f32>,            // rgba - tip (old particles)
}

struct Force {
    type_strength: vec4<f32>,    // type, strength, 0, 0
    direction_params: vec4<f32>, // xyz=dir/pos, w=extra
}

struct SimUniforms {
    time_config: vec4<f32>,   // dt, time, max_particles, active
    random_seed: vec4<f32>,   // 4 random seeds
    force_config: vec4<f32>,  // num_forces, 0, 0, 0
}

@group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(1) var<uniform> emitter: Emitter;
@group(0) @binding(2) var<uniform> uniforms: SimUniforms;
@group(0) @binding(3) var<storage, read> forces: array<Force>;

// Constants for emitter shapes
const SHAPE_POINT: u32 = 0u;
const SHAPE_SPHERE: u32 = 1u;
const SHAPE_HEMISPHERE: u32 = 2u;
const SHAPE_CONE: u32 = 3u;
const SHAPE_BOX: u32 = 4u;
const SHAPE_CIRCLE: u32 = 5u;

// Constants for force types
const FORCE_GRAVITY: u32 = 0u;
const FORCE_WIND: u32 = 1u;
const FORCE_VORTEX: u32 = 2u;
const FORCE_DRAG: u32 = 3u;
const FORCE_TURBULENCE: u32 = 4u;
const FORCE_ATTRACTOR: u32 = 5u;
const FORCE_RADIAL: u32 = 6u;

// PCG random number generator
fn pcg_hash(input: u32) -> u32 {
    let state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn random_float(seed: ptr<function, u32>) -> f32 {
    *seed = pcg_hash(*seed);
    return f32(*seed) / 4294967295.0;
}

fn random_range(seed: ptr<function, u32>, min_val: f32, max_val: f32) -> f32 {
    return min_val + random_float(seed) * (max_val - min_val);
}

fn random_unit_vector(seed: ptr<function, u32>) -> vec3<f32> {
    let theta = random_float(seed) * 6.283185;
    let z = random_range(seed, -1.0, 1.0);
    let r = sqrt(1.0 - z * z);
    return vec3<f32>(r * cos(theta), r * sin(theta), z);
}

// Get spawn position based on emitter shape
fn get_spawn_position(seed: ptr<function, u32>) -> vec3<f32> {
    let shape = u32(emitter.position_shape.w);
    let base_pos = emitter.position_shape.xyz;

    switch (shape) {
        case SHAPE_POINT: {
            return base_pos;
        }
        case SHAPE_SPHERE: {
            let radius = emitter.shape_params.x;
            let dir = random_unit_vector(seed);
            let r = radius * pow(random_float(seed), 1.0/3.0);
            return base_pos + dir * r;
        }
        case SHAPE_HEMISPHERE: {
            let radius = emitter.shape_params.x;
            var dir = random_unit_vector(seed);
            dir.y = abs(dir.y); // Upper hemisphere
            let r = radius * pow(random_float(seed), 1.0/3.0);
            return base_pos + dir * r;
        }
        case SHAPE_CONE: {
            let angle = emitter.shape_params.x;
            let radius = emitter.shape_params.y;
            let theta = random_float(seed) * 6.283185;
            let r = radius * sqrt(random_float(seed));
            return base_pos + vec3<f32>(r * cos(theta), 0.0, r * sin(theta));
        }
        case SHAPE_BOX: {
            let half = emitter.shape_params.xyz;
            return base_pos + vec3<f32>(
                random_range(seed, -half.x, half.x),
                random_range(seed, -half.y, half.y),
                random_range(seed, -half.z, half.z)
            );
        }
        case SHAPE_CIRCLE: {
            let radius = emitter.shape_params.x;
            let theta = random_float(seed) * 6.283185;
            let r = radius * sqrt(random_float(seed));
            return base_pos + vec3<f32>(r * cos(theta), 0.0, r * sin(theta));
        }
        default: {
            return base_pos;
        }
    }
}

// Get spawn velocity based on direction and randomness
fn get_spawn_velocity(seed: ptr<function, u32>) -> vec3<f32> {
    let base_dir = normalize(emitter.direction_randomness.xyz);
    let randomness = emitter.direction_randomness.w;

    // Mix base direction with random direction
    let random_dir = random_unit_vector(seed);
    let dir = normalize(mix(base_dir, random_dir, randomness));

    let speed = random_range(seed, emitter.lifetime_speed.z, emitter.lifetime_speed.w);
    return dir * speed;
}

// Apply force to velocity
fn apply_force(force: Force, pos: vec3<f32>, vel: vec3<f32>, dt: f32) -> vec3<f32> {
    let force_type = u32(force.type_strength.x);
    let strength = force.type_strength.y;
    var new_vel = vel;

    switch (force_type) {
        case FORCE_GRAVITY: {
            new_vel = vel + force.direction_params.xyz * dt;
        }
        case FORCE_WIND: {
            let turbulence = force.direction_params.w;
            new_vel = vel + force.direction_params.xyz * strength * dt;
        }
        case FORCE_VORTEX: {
            let axis = normalize(force.direction_params.xyz);
            let to_particle = pos;
            let tangent = cross(axis, to_particle);
            new_vel = vel + normalize(tangent) * strength * dt;
        }
        case FORCE_DRAG: {
            new_vel = vel * (1.0 - strength * dt);
        }
        case FORCE_TURBULENCE: {
            // 3D turbulence with per-particle variation
            let freq = force.direction_params.w;
            let t = uniforms.time_config.y;

            // Use velocity magnitude as per-particle seed for variation
            // This makes each particle get unique turbulence even at same position
            let vel_seed = dot(vel, vec3<f32>(12.9898, 78.233, 37.719));
            let particle_offset = fract(sin(vel_seed) * 43758.5453) * 6.28318; // 0 to 2*PI

            // Position-based noise with per-particle phase offset
            let p1 = pos * freq + vec3<f32>(t * 1.3 + particle_offset, t * 0.7, t * 1.1);
            let p2 = pos * freq * 1.7 + vec3<f32>(t * 0.9, t * 1.4 + particle_offset, t * 0.6);

            // Multi-octave noise
            let n1 = sin(p1.x + particle_offset) * cos(p1.y) * sin(p1.z + t);
            let n2 = cos(p1.y * 1.3 + particle_offset) * sin(p1.z * 0.8) * cos(p1.x + t * 0.7);
            let n3 = sin(p1.z * 1.1) * cos(p1.x * 0.9 + particle_offset) * sin(p1.y + t * 1.2);

            // Second octave
            let m1 = sin(p2.x + particle_offset * 0.7) * cos(p2.z) * 0.5;
            let m2 = cos(p2.y) * sin(p2.x + particle_offset * 1.3) * 0.5;
            let m3 = sin(p2.z + particle_offset * 0.5) * cos(p2.y) * 0.5;

            // Combine - allow more vertical variation for dancing flames
            let turb = vec3<f32>(
                n1 + m1,
                (n2 + m2) * 0.5,
                n3 + m3
            );

            new_vel = vel + turb * strength * dt;
        }
        case FORCE_ATTRACTOR: {
            let attractor_pos = force.direction_params.xyz;
            let to_attractor = attractor_pos - pos;
            let dist = max(length(to_attractor), 0.1);
            new_vel = vel + normalize(to_attractor) * strength / (dist * dist) * dt;
        }
        case FORCE_RADIAL: {
            // Centering force - pulls particles toward the Y-axis (center line)
            // Negative strength = pull toward center, positive = push away
            let center = force.direction_params.xyz;
            let to_center = vec3<f32>(center.x - pos.x, 0.0, center.z - pos.z);
            let dist = max(length(to_center), 0.01);
            // Linear falloff - stronger when further from center
            new_vel = vel + to_center * strength * dt;
        }
        default: {}
    }

    return new_vel;
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let idx = global_id.x;
    let max_particles = u32(uniforms.time_config.z);

    if (idx >= max_particles) {
        return;
    }

    var p = particles[idx];
    let dt = uniforms.time_config.x;
    let time = uniforms.time_config.y;

    // Initialize random seed based on particle index and time
    var seed = idx + u32(time * 1000.0);
    seed = pcg_hash(seed + u32(uniforms.random_seed.x * 1000000.0));

    // Check if particle is alive
    if (p.position_life.w > 0.0) {
        // Update particle
        p.position_life.w -= dt;

        if (p.position_life.w <= 0.0) {
            // Particle died
            p.position_life.w = 0.0;
        } else {
            // Apply forces
            var vel = p.velocity_max_life.xyz;
            let num_forces = u32(uniforms.force_config.x);
            let gravity_scale = emitter.emission_config.w;

            // Apply forces from the forces array
            // Note: Gravity is handled via Force::Gravity in the forces array, not hardcoded
            for (var i = 0u; i < num_forces; i++) {
                let force = forces[i];
                // For gravity forces (type 0), apply gravity_scale
                if (u32(force.type_strength.x) == FORCE_GRAVITY) {
                    let gravity_dir = force.direction_params.xyz * gravity_scale;
                    vel = vel + gravity_dir * dt;
                } else {
                    vel = apply_force(force, p.position_life.xyz, vel, dt);
                }
            }

            p.velocity_max_life = vec4<f32>(vel, p.velocity_max_life.w);

            // Update position
            p.position_life = vec4<f32>(
                p.position_life.xyz + vel * dt,
                p.position_life.w
            );

            // Update color based on lifetime (3-color gradient)
            // life_ratio: 1.0 = just born (start), 0.5 = mid-life (mid), 0.0 = dying (end)
            let life_ratio = p.position_life.w / p.velocity_max_life.w;
            if (life_ratio > 0.5) {
                // Young particle: interpolate from start to mid
                let t = (life_ratio - 0.5) * 2.0; // 0.5->1.0 maps to 0->1
                p.color = mix(emitter.mid_color, emitter.start_color, t);
            } else {
                // Older particle: interpolate from mid to end
                let t = life_ratio * 2.0; // 0->0.5 maps to 0->1
                p.color = mix(emitter.end_color, emitter.mid_color, t);
            }

            // Update size based on lifetime
            let start_size = p.size_rotation.y;
            let end_size = p.size_rotation.z;
            p.size_rotation.x = mix(end_size, start_size, life_ratio);
        }
    } else {
        // Try to spawn new particle
        let emission_rate = emitter.emission_config.x;
        let burst_count = emitter.emission_config.y;

        // Calculate spawn chance: combines continuous emission with burst
        var spawn_chance = 0.0;
        if (emission_rate > 0.0) {
            spawn_chance = emission_rate * dt / f32(max_particles);
        }
        // For burst effects, use burst_count to determine spawn probability
        if (burst_count > 0.0) {
            spawn_chance = spawn_chance + burst_count / f32(max_particles);
        }

        if (random_float(&seed) < spawn_chance) {
            // Spawn new particle
            let pos = get_spawn_position(&seed);
            let vel = get_spawn_velocity(&seed);
            let lifetime = random_range(&seed, emitter.lifetime_speed.x, emitter.lifetime_speed.y);
            let size = random_range(&seed, emitter.size_config.x, emitter.size_config.y);
            let end_size = random_range(&seed, emitter.size_config.z, emitter.size_config.w);

            p.position_life = vec4<f32>(pos, lifetime);
            p.velocity_max_life = vec4<f32>(vel, lifetime);
            p.color = emitter.start_color;
            p.size_rotation = vec4<f32>(size, size, end_size, 0.0);
        }
    }

    particles[idx] = p;
}
"#;

/// GPU particle render shader (billboard quads)
pub const PARTICLE_RENDER_SHADER: &str = r#"
// ============================================================================
// Blinc GPU Particle Render Shader
// ============================================================================

struct Particle {
    position_life: vec4<f32>,
    velocity_max_life: vec4<f32>,
    color: vec4<f32>,
    size_rotation: vec4<f32>,
}

struct RenderUniforms {
    view_proj: mat4x4<f32>,
    camera_pos_fov: vec4<f32>,
    camera_right_aspect: vec4<f32>,
    camera_up: vec4<f32>,
    viewport_config: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> particles: array<Particle>;
@group(0) @binding(1) var<uniform> uniforms: RenderUniforms;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let p = particles[instance_index];

    // Skip dead particles (move to clip space far away)
    if (p.position_life.w <= 0.0) {
        out.position = vec4<f32>(0.0, 0.0, 1000.0, 1.0);
        out.uv = vec2<f32>(0.0);
        out.color = vec4<f32>(0.0);
        return out;
    }

    // Billboard quad vertices
    let quad_verts = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    let quad_uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    let local_pos = quad_verts[vertex_index];
    let size = p.size_rotation.x;

    // Calculate billboard orientation
    let camera_right = uniforms.camera_right_aspect.xyz;
    let camera_up = uniforms.camera_up.xyz;

    // World position with billboard offset
    let world_pos = p.position_life.xyz +
                    camera_right * local_pos.x * size +
                    camera_up * local_pos.y * size;

    // Project to clip space
    out.position = uniforms.view_proj * vec4<f32>(world_pos, 1.0);
    out.uv = quad_uvs[vertex_index];
    out.color = p.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Circular particle with soft edges
    let center = vec2<f32>(0.5);
    let dist = length(in.uv - center) * 2.0;

    // Soft circle falloff
    let alpha = 1.0 - smoothstep(0.8, 1.0, dist);

    // Discard if too far from center
    if (alpha < 0.01) {
        discard;
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

/// Handle to a GPU particle system
#[derive(Debug)]
pub struct ParticleSystemGpu {
    /// Particle buffer (read/write for compute)
    particle_buffer: wgpu::Buffer,
    /// Emitter uniform buffer
    emitter_buffer: wgpu::Buffer,
    /// Simulation uniforms buffer
    sim_uniform_buffer: wgpu::Buffer,
    /// Render uniforms buffer
    render_uniform_buffer: wgpu::Buffer,
    /// Forces buffer
    forces_buffer: wgpu::Buffer,
    /// Compute pipeline
    compute_pipeline: wgpu::ComputePipeline,
    /// Render pipeline
    render_pipeline: wgpu::RenderPipeline,
    /// Compute bind group
    compute_bind_group: wgpu::BindGroup,
    /// Render bind group
    render_bind_group: wgpu::BindGroup,
    /// Max particles
    max_particles: u32,
    /// Current time
    time: f32,
}

impl ParticleSystemGpu {
    /// Create a new GPU particle system
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        max_particles: u32,
    ) -> Self {
        let max_particles = max_particles.min(MAX_PARTICLES_PER_SYSTEM);

        // Create particle buffer
        let particle_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Buffer"),
            size: (std::mem::size_of::<GpuParticle>() * max_particles as usize) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create emitter buffer
        let emitter_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Emitter Buffer"),
            size: std::mem::size_of::<GpuEmitter>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create simulation uniforms buffer
        let sim_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Simulation Uniforms"),
            size: std::mem::size_of::<GpuSimulationUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create render uniforms buffer
        let render_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Render Uniforms"),
            size: std::mem::size_of::<GpuRenderUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create forces buffer (support up to 8 forces)
        let forces_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Forces Buffer"),
            size: (std::mem::size_of::<GpuForce>() * 8) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create compute shader module
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(PARTICLE_COMPUTE_SHADER.into()),
        });

        // Create render shader module
        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Render Shader"),
            source: wgpu::ShaderSource::Wgsl(PARTICLE_RENDER_SHADER.into()),
        });

        // Create compute bind group layout
        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Particle Compute Bind Group Layout"),
                entries: &[
                    // Particles (storage, read_write)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Emitter (uniform)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Simulation uniforms
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Forces (storage, read)
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // Create render bind group layout
        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Particle Render Bind Group Layout"),
                entries: &[
                    // Particles (storage, read)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Render uniforms
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // Create compute pipeline
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Particle Compute Pipeline Layout"),
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Particle Compute Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // Create render pipeline
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Particle Render Pipeline Layout"),
                bind_group_layouts: &[&render_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Particle Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Billboards face camera
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Create bind groups
        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particle Compute Bind Group"),
            layout: &compute_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: particle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: emitter_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: sim_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: forces_buffer.as_entire_binding(),
                },
            ],
        });

        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particle Render Bind Group"),
            layout: &render_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: particle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: render_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            particle_buffer,
            emitter_buffer,
            sim_uniform_buffer,
            render_uniform_buffer,
            forces_buffer,
            compute_pipeline,
            render_pipeline,
            compute_bind_group,
            render_bind_group,
            max_particles,
            time: 0.0,
        }
    }

    /// Update the particle system for one frame
    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        viewport: &ParticleViewport,
    ) {
        if !viewport.playing {
            return;
        }

        self.time = viewport.time;

        // Update emitter buffer
        queue.write_buffer(&self.emitter_buffer, 0, bytemuck::bytes_of(&viewport.emitter));

        // Update simulation uniforms
        let sim_uniforms = GpuSimulationUniforms {
            time_config: [
                viewport.delta_time,
                viewport.time,
                self.max_particles as f32,
                0.0,
            ],
            random_seed: [
                viewport.time * 12345.6789,
                viewport.time * 98765.4321,
                (viewport.time * 11111.1111).fract(),
                (viewport.time * 22222.2222).fract(),
            ],
            force_config: [viewport.forces.len() as f32, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.sim_uniform_buffer, 0, bytemuck::bytes_of(&sim_uniforms));

        // Update forces buffer
        let mut forces = [GpuForce::default(); 8];
        for (i, force) in viewport.forces.iter().take(8).enumerate() {
            forces[i] = *force;
        }
        queue.write_buffer(&self.forces_buffer, 0, bytemuck::cast_slice(&forces));

        // Dispatch compute shader
        let workgroups = (self.max_particles + 63) / 64;
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Particle Compute Pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.compute_pipeline);
        compute_pass.set_bind_group(0, &self.compute_bind_group, &[]);
        compute_pass.dispatch_workgroups(workgroups, 1, 1);
    }

    /// Render the particles
    pub fn render<'a>(
        &'a self,
        queue: &wgpu::Queue,
        render_pass: &mut wgpu::RenderPass<'a>,
        viewport: &ParticleViewport,
    ) {
        // Calculate view-projection matrix
        let view = Self::look_at(
            viewport.camera_pos,
            viewport.camera_target,
            viewport.camera_up,
        );
        let aspect = viewport.bounds[2] / viewport.bounds[3];
        let proj = Self::perspective(viewport.fov, aspect, 0.1, 100.0);
        let view_proj = Self::mat4_mul(&proj, &view);

        // Calculate camera vectors
        let forward = [
            viewport.camera_target[0] - viewport.camera_pos[0],
            viewport.camera_target[1] - viewport.camera_pos[1],
            viewport.camera_target[2] - viewport.camera_pos[2],
        ];
        let forward = Self::normalize(forward);
        let right = Self::cross(forward, viewport.camera_up);
        let right = Self::normalize(right);
        let up = Self::cross(right, forward);

        // Update render uniforms
        let render_uniforms = GpuRenderUniforms {
            view_proj,
            camera_pos_fov: [
                viewport.camera_pos[0],
                viewport.camera_pos[1],
                viewport.camera_pos[2],
                viewport.fov,
            ],
            camera_right_aspect: [right[0], right[1], right[2], aspect],
            camera_up: [up[0], up[1], up[2], 0.0],
            viewport_config: [
                viewport.bounds[2],
                viewport.bounds[3],
                0.0, // render mode
                viewport.blend_mode as f32,
            ],
        };
        queue.write_buffer(
            &self.render_uniform_buffer,
            0,
            bytemuck::bytes_of(&render_uniforms),
        );

        // Draw particles
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.render_bind_group, &[]);
        render_pass.draw(0..6, 0..self.max_particles); // 6 vertices per quad (2 triangles)
    }

    // Helper math functions
    fn normalize(v: [f32; 3]) -> [f32; 3] {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len > 0.0001 {
            [v[0] / len, v[1] / len, v[2] / len]
        } else {
            [0.0, 1.0, 0.0]
        }
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
        let f = Self::normalize([
            target[0] - eye[0],
            target[1] - eye[1],
            target[2] - eye[2],
        ]);
        let r = Self::normalize(Self::cross(f, up));
        let u = Self::cross(r, f);

        [
            [r[0], u[0], -f[0], 0.0],
            [r[1], u[1], -f[1], 0.0],
            [r[2], u[2], -f[2], 0.0],
            [
                -(r[0] * eye[0] + r[1] * eye[1] + r[2] * eye[2]),
                -(u[0] * eye[0] + u[1] * eye[1] + u[2] * eye[2]),
                f[0] * eye[0] + f[1] * eye[1] + f[2] * eye[2],
                1.0,
            ],
        ]
    }

    fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
        let f = 1.0 / (fov * 0.5).tan();
        let nf = 1.0 / (near - far);

        [
            [f / aspect, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, (far + near) * nf, -1.0],
            [0.0, 0.0, 2.0 * far * near * nf, 0.0],
        ]
    }

    fn mat4_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
        let mut result = [[0.0f32; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                result[i][j] = a[0][j] * b[i][0]
                    + a[1][j] * b[i][1]
                    + a[2][j] * b[i][2]
                    + a[3][j] * b[i][3];
            }
        }
        result
    }
}

/// Manager for multiple particle systems
pub struct ParticleManager {
    systems: HashMap<u64, ParticleSystemGpu>,
    next_id: u64,
}

impl ParticleManager {
    pub fn new() -> Self {
        Self {
            systems: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn create_system(
        &mut self,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        max_particles: u32,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let system = ParticleSystemGpu::new(device, surface_format, max_particles);
        self.systems.insert(id, system);
        id
    }

    pub fn get_system(&self, id: u64) -> Option<&ParticleSystemGpu> {
        self.systems.get(&id)
    }

    pub fn get_system_mut(&mut self, id: u64) -> Option<&mut ParticleSystemGpu> {
        self.systems.get_mut(&id)
    }

    pub fn remove_system(&mut self, id: u64) {
        self.systems.remove(&id);
    }
}

impl Default for ParticleManager {
    fn default() -> Self {
        Self::new()
    }
}
