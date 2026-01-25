//! Particle system for visual effects
//!
//! Provides GPU-accelerated particle rendering with various emitter shapes
//! and force affectors. All simulation and rendering happens on the GPU.
//!
//! # Example
//!
//! ```ignore
//! use blinc_3d::utils::particles::*;
//!
//! // Create a fire particle system
//! let fire = ParticleSystem::fire();
//!
//! // Or build custom effects
//! let particles = ParticleSystem::new()
//!     .with_emitter(EmitterShape::Cone { angle: 0.3, radius: 0.5 })
//!     .with_emission_rate(100.0)
//!     .with_lifetime(1.0, 2.0)
//!     .with_colors(Color::rgb(1.0, 0.8, 0.2), Color::rgba(1.0, 0.2, 0.0, 0.0))
//!     .with_size(0.1, 0.2, 0.0, 0.05)
//!     .with_force(Force::gravity(Vec3::new(0.0, 2.0, 0.0)));
//!
//! // Attach to entity - rendering is automatic
//! world.spawn().insert(particles);
//! ```

use crate::ecs::{Component, System, SystemContext, SystemStage};
use blinc_core::{Color, Vec3};

/// Particle system component
///
/// Add this to an entity to create particle effects at that location.
/// The system automatically handles GPU simulation and rendering.
#[derive(Clone, Debug)]
pub struct ParticleSystem {
    /// Maximum number of particles
    pub max_particles: u32,
    /// Emitter shape
    pub emitter: EmitterShape,
    /// Emission rate (particles per second)
    pub emission_rate: f32,
    /// Emission direction
    pub direction: Vec3,
    /// Direction randomness (0 = straight, 1 = fully random)
    pub direction_randomness: f32,
    /// Particle lifetime (min, max)
    pub lifetime: (f32, f32),
    /// Start speed (min, max)
    pub start_speed: (f32, f32),
    /// Start size (min, max)
    pub start_size: (f32, f32),
    /// End size (min, max)
    pub end_size: (f32, f32),
    /// Start color (base of fire - young particles)
    pub start_color: Color,
    /// Mid color (middle of fire - mid-life particles)
    pub mid_color: Color,
    /// End color (tip of fire - old/dying particles)
    pub end_color: Color,
    /// Start rotation (min, max)
    pub start_rotation: (f32, f32),
    /// Rotation speed (min, max)
    pub rotation_speed: (f32, f32),
    /// Force affectors
    pub forces: Vec<Force>,
    /// Gravity scale
    pub gravity_scale: f32,
    /// Render mode
    pub render_mode: RenderMode,
    /// Blend mode
    pub blend_mode: BlendMode,
    /// Soft particles (fade near geometry)
    pub soft_particles: bool,
    /// Soft particles distance
    pub soft_particle_distance: f32,
    /// Stretch length scale (for stretched billboards)
    pub length_scale: f32,
    /// Stretch speed scale
    pub speed_scale: f32,
    /// Sprite sheet (columns, rows)
    pub sprite_sheet: Option<(u32, u32)>,
    /// Animation speed for sprite sheets
    pub animation_speed: f32,
    /// Whether system is playing
    pub playing: bool,
    /// Whether to loop
    pub looping: bool,
    /// Duration (for non-looping)
    pub duration: f32,
    // Internal state
    time: f32,
    spawn_accumulated: f32,
}

impl Component for ParticleSystem {}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleSystem {
    /// Create a new particle system
    pub fn new() -> Self {
        Self {
            max_particles: 10000,
            emitter: EmitterShape::Point,
            emission_rate: 100.0,
            direction: Vec3::new(0.0, 1.0, 0.0),
            direction_randomness: 0.0,
            lifetime: (1.0, 2.0),
            start_speed: (1.0, 2.0),
            start_size: (0.1, 0.2),
            end_size: (0.0, 0.1),
            start_color: Color::WHITE,
            mid_color: Color::rgba(1.0, 1.0, 1.0, 0.5),
            end_color: Color::rgba(1.0, 1.0, 1.0, 0.0),
            start_rotation: (0.0, 0.0),
            rotation_speed: (0.0, 0.0),
            forces: Vec::new(),
            gravity_scale: 1.0,
            render_mode: RenderMode::Billboard,
            blend_mode: BlendMode::Alpha,
            soft_particles: false,
            soft_particle_distance: 0.5,
            length_scale: 1.0,
            speed_scale: 0.1,
            sprite_sheet: None,
            animation_speed: 1.0,
            playing: true,
            looping: true,
            duration: 5.0,
            time: 0.0,
            spawn_accumulated: 0.0,
        }
    }

    // ========== Builder Methods ==========

    /// Set max particles
    pub fn with_max_particles(mut self, max: u32) -> Self {
        self.max_particles = max;
        self
    }

    /// Set emitter shape
    pub fn with_emitter(mut self, shape: EmitterShape) -> Self {
        self.emitter = shape;
        self
    }

    /// Set emission rate (particles per second)
    pub fn with_emission_rate(mut self, rate: f32) -> Self {
        self.emission_rate = rate;
        self
    }

    /// Set emission direction and randomness
    pub fn with_direction(mut self, dir: Vec3, randomness: f32) -> Self {
        self.direction = dir;
        self.direction_randomness = randomness.clamp(0.0, 1.0);
        self
    }

    /// Set particle lifetime range
    pub fn with_lifetime(mut self, min: f32, max: f32) -> Self {
        self.lifetime = (min, max);
        self
    }

    /// Set particle speed range
    pub fn with_speed(mut self, min: f32, max: f32) -> Self {
        self.start_speed = (min, max);
        self
    }

    /// Set particle size ranges
    pub fn with_size(mut self, start_min: f32, start_max: f32, end_min: f32, end_max: f32) -> Self {
        self.start_size = (start_min, start_max);
        self.end_size = (end_min, end_max);
        self
    }

    /// Set start, mid, and end colors for 3-stage gradient
    /// - start: Base color (young particles, e.g., yellow for fire)
    /// - mid: Middle color (mid-life particles, e.g., red-orange for fire)
    /// - end: Tip color (dying particles, e.g., dark/burnt for fire)
    pub fn with_colors(mut self, start: Color, mid: Color, end: Color) -> Self {
        self.start_color = start;
        self.mid_color = mid;
        self.end_color = end;
        self
    }

    /// Set particle rotation
    pub fn with_rotation(mut self, start_min: f32, start_max: f32, speed_min: f32, speed_max: f32) -> Self {
        self.start_rotation = (start_min, start_max);
        self.rotation_speed = (speed_min, speed_max);
        self
    }

    /// Add a force affector
    pub fn with_force(mut self, force: Force) -> Self {
        self.forces.push(force);
        self
    }

    /// Set gravity scale
    pub fn with_gravity_scale(mut self, scale: f32) -> Self {
        self.gravity_scale = scale;
        self
    }

    /// Set render mode
    pub fn with_render_mode(mut self, mode: RenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Set blend mode
    pub fn with_blend_mode(mut self, mode: BlendMode) -> Self {
        self.blend_mode = mode;
        self
    }

    /// Enable soft particles
    pub fn with_soft_particles(mut self, distance: f32) -> Self {
        self.soft_particles = true;
        self.soft_particle_distance = distance;
        self
    }

    /// Set stretched billboard parameters
    pub fn with_stretch(mut self, length_scale: f32, speed_scale: f32) -> Self {
        self.render_mode = RenderMode::Stretched;
        self.length_scale = length_scale;
        self.speed_scale = speed_scale;
        self
    }

    /// Set sprite sheet animation
    pub fn with_sprite_sheet(mut self, columns: u32, rows: u32, speed: f32) -> Self {
        self.sprite_sheet = Some((columns, rows));
        self.animation_speed = speed;
        self
    }

    /// Set looping
    pub fn with_looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Set duration (for non-looping)
    pub fn with_duration(mut self, duration: f32) -> Self {
        self.duration = duration;
        self
    }

    // ========== Control Methods ==========

    /// Play the system
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pause the system
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Stop and reset
    pub fn stop(&mut self) {
        self.playing = false;
        self.time = 0.0;
        self.spawn_accumulated = 0.0;
    }

    /// Emit a burst of particles
    pub fn burst(&mut self, count: u32) {
        self.spawn_accumulated += count as f32;
    }

    /// Get the accumulated spawn count (for burst effects)
    pub fn spawn_accumulated(&self) -> f32 {
        self.spawn_accumulated
    }

    /// Clear the accumulated spawn count (called after spawning)
    pub fn clear_spawn_accumulated(&mut self) {
        self.spawn_accumulated = 0.0;
    }

    // ========== Presets ==========

    /// Fire effect - default campfire/bonfire style
    pub fn fire() -> Self {
        Self::fire_bonfire()
    }

    /// Bonfire style - cohesive flames rising upward with dancing motion
    pub fn fire_bonfire() -> Self {
        Self::new()
            .with_max_particles(12000)
            .with_emitter(EmitterShape::Box { half_extents: Vec3::new(0.35, 0.03, 0.35) })
            .with_emission_rate(1200.0)
            .with_direction(Vec3::new(0.0, 1.0, 0.0), 0.4)
            .with_lifetime(0.4, 0.9)
            .with_speed(0.6, 1.5)
            .with_size(0.12, 0.28, 0.04, 0.08)
            .with_colors(
                Color::rgba(1.0, 0.95, 0.4, 0.25),   // Base: bright yellow
                Color::rgba(1.0, 0.3, 0.05, 0.18),   // Middle: red-orange
                Color::rgba(0.2, 0.05, 0.0, 0.0),    // Tip: dark burnt
            )
            .with_force(Force::gravity(Vec3::new(0.0, 2.2, 0.0)))
            .with_force(Force::drag(3.0))
            .with_force(Force::turbulence(2.0, 8.0))
            .with_blend_mode(BlendMode::Additive)
    }

    /// Candle flame - small, gentle, single flame with subtle flicker
    pub fn fire_candle() -> Self {
        Self::new()
            .with_max_particles(1500)
            .with_emitter(EmitterShape::Box { half_extents: Vec3::new(0.025, 0.008, 0.025) })
            .with_emission_rate(250.0)
            .with_direction(Vec3::new(0.0, 1.0, 0.0), 0.25)
            .with_lifetime(0.2, 0.45)
            .with_speed(0.25, 0.6)
            .with_size(0.025, 0.055, 0.008, 0.02)
            .with_colors(
                Color::rgba(1.0, 0.9, 0.5, 0.3),    // Base: bright yellow
                Color::rgba(1.0, 0.4, 0.1, 0.2),    // Middle: orange-red
                Color::rgba(0.3, 0.08, 0.0, 0.0),   // Tip: dark burnt
            )
            .with_force(Force::gravity(Vec3::new(0.0, 1.0, 0.0)))
            .with_force(Force::drag(4.5))
            .with_force(Force::turbulence(0.7, 12.0))
            .with_blend_mode(BlendMode::Additive)
    }

    /// Inferno - intense, large, aggressive flames with chaotic dancing
    pub fn fire_inferno() -> Self {
        Self::new()
            .with_max_particles(10000)
            .with_emitter(EmitterShape::Box { half_extents: Vec3::new(0.3, 0.03, 0.3) })
            .with_emission_rate(1100.0)
            .with_direction(Vec3::new(0.0, 1.0, 0.0), 0.45)
            .with_lifetime(0.35, 0.85)
            .with_speed(0.7, 1.8)
            .with_size(0.08, 0.18, 0.025, 0.06)
            .with_colors(
                Color::rgba(1.0, 0.92, 0.35, 0.28),  // Base: intense yellow-white
                Color::rgba(1.0, 0.25, 0.0, 0.2),    // Middle: deep red
                Color::rgba(0.15, 0.02, 0.0, 0.0),   // Tip: very dark burnt
            )
            .with_force(Force::gravity(Vec3::new(0.0, 2.2, 0.0)))
            .with_force(Force::drag(3.0))
            .with_force(Force::turbulence(2.2, 8.0))
            .with_blend_mode(BlendMode::Additive)
    }

    /// Smoke effect - billowing smoke that rises and expands
    pub fn smoke() -> Self {
        Self::new()
            .with_max_particles(2000)
            .with_emitter(EmitterShape::Cone { angle: 0.15, radius: 0.15 })
            .with_emission_rate(40.0)
            .with_direction(Vec3::new(0.0, 1.0, 0.0), 0.1)
            .with_lifetime(3.0, 5.0) // Long lifetime for billowing effect
            .with_speed(0.3, 0.8) // Slow rising
            .with_size(0.15, 0.25, 0.6, 1.0) // Expands significantly as it rises
            .with_colors(
                Color::rgba(0.15, 0.15, 0.15, 0.5), // Dark gray start
                Color::rgba(0.3, 0.3, 0.3, 0.35),   // Mid gray
                Color::rgba(0.45, 0.45, 0.45, 0.0), // Light gray, fades out
            )
            .with_force(Force::gravity(Vec3::new(0.0, 0.3, 0.0))) // Very gentle upward drift
            .with_force(Force::drag(1.5)) // Moderate drag for slow terminal velocity
            .with_force(Force::turbulence(0.3, 1.0)) // Gentle turbulence for billowing
    }

    /// Sparks effect
    pub fn sparks() -> Self {
        Self::new()
            .with_max_particles(3000)
            .with_emitter(EmitterShape::Point)
            .with_emission_rate(100.0)
            .with_direction(Vec3::new(0.0, 1.0, 0.0), 0.8)
            .with_lifetime(0.3, 0.8)
            .with_speed(5.0, 10.0)
            .with_size(0.02, 0.05, 0.0, 0.01)
            .with_colors(
                Color::rgb(1.0, 0.95, 0.6),  // Bright yellow-white
                Color::rgb(1.0, 0.5, 0.1),   // Orange
                Color::rgb(0.8, 0.2, 0.0),   // Dark red
            )
            .with_force(Force::gravity(Vec3::new(0.0, -9.8, 0.0)))
            .with_force(Force::drag(0.5)) // Light air resistance
            .with_stretch(0.1, 0.2)
            .with_blend_mode(BlendMode::Additive)
    }

    /// Rain effect
    pub fn rain() -> Self {
        Self::new()
            .with_max_particles(20000)
            .with_emitter(EmitterShape::Box { half_extents: Vec3::new(10.0, 0.1, 10.0) })
            .with_emission_rate(1000.0)
            .with_direction(Vec3::new(0.0, -1.0, 0.0), 0.05)
            .with_lifetime(1.0, 2.0)
            .with_speed(10.0, 15.0)
            .with_size(0.01, 0.02, 0.01, 0.02)
            .with_colors(
                Color::rgba(0.7, 0.8, 1.0, 0.5),  // Light blue
                Color::rgba(0.7, 0.8, 1.0, 0.4),  // Light blue
                Color::rgba(0.7, 0.8, 1.0, 0.3),  // Light blue fading
            )
            .with_gravity_scale(0.0)
            .with_stretch(0.5, 0.3)
    }

    /// Snow effect
    pub fn snow() -> Self {
        Self::new()
            .with_max_particles(10000)
            .with_emitter(EmitterShape::Box { half_extents: Vec3::new(10.0, 0.1, 10.0) })
            .with_emission_rate(200.0)
            .with_direction(Vec3::new(0.0, -1.0, 0.0), 0.3)
            .with_lifetime(4.0, 8.0)
            .with_speed(0.5, 1.0)
            .with_size(0.02, 0.05, 0.02, 0.05)
            .with_colors(Color::WHITE, Color::WHITE, Color::WHITE)
            .with_force(Force::gravity(Vec3::new(0.0, -1.0, 0.0)))
            .with_force(Force::turbulence(0.3, 0.5))
    }

    /// Explosion effect
    pub fn explosion() -> Self {
        Self::new()
            .with_max_particles(500)
            .with_emitter(EmitterShape::Sphere { radius: 0.1 })
            .with_emission_rate(0.0)
            .with_direction(Vec3::ZERO, 1.0)
            .with_lifetime(0.5, 1.5)
            .with_speed(5.0, 15.0)
            .with_size(0.2, 0.5, 0.0, 0.1)
            .with_colors(
                Color::rgb(1.0, 0.9, 0.4),         // Bright yellow-white
                Color::rgb(1.0, 0.4, 0.1),         // Orange
                Color::rgba(0.2, 0.05, 0.0, 0.0),  // Dark burnt, fading
            )
            .with_force(Force::gravity(Vec3::new(0.0, -5.0, 0.0)))
            .with_force(Force::drag(2.0))
            .with_blend_mode(BlendMode::Additive)
            .with_looping(false)
    }

    /// Magic sparkle effect
    pub fn magic() -> Self {
        Self::new()
            .with_max_particles(2000)
            .with_emitter(EmitterShape::Sphere { radius: 0.5 })
            .with_emission_rate(80.0)
            .with_direction(Vec3::new(0.0, 1.0, 0.0), 0.5)
            .with_lifetime(1.0, 2.0)
            .with_speed(0.2, 0.5)
            .with_size(0.05, 0.1, 0.0, 0.02)
            .with_colors(
                Color::rgb(0.5, 0.8, 1.0),         // Cyan-blue
                Color::rgb(0.8, 0.5, 1.0),         // Purple
                Color::rgba(1.0, 0.4, 0.9, 0.0),   // Pink, fading
            )
            .with_force(Force::vortex(Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO, 3.0)) // Stronger swirl
            .with_force(Force::gravity(Vec3::new(0.0, 0.5, 0.0))) // Gentle upward drift
            .with_force(Force::drag(1.5)) // Prevent runaway velocity
            .with_blend_mode(BlendMode::Additive)
    }

    /// Confetti effect
    pub fn confetti() -> Self {
        Self::new()
            .with_max_particles(1000)
            .with_emitter(EmitterShape::Point)
            .with_emission_rate(0.0)
            .with_direction(Vec3::new(0.0, 1.0, 0.0), 0.5)
            .with_lifetime(3.0, 5.0)
            .with_speed(5.0, 10.0)
            .with_size(0.05, 0.1, 0.05, 0.1)
            .with_colors(Color::WHITE, Color::WHITE, Color::WHITE)
            .with_rotation(0.0, std::f32::consts::TAU, -2.0, 2.0)
            .with_force(Force::gravity(Vec3::new(0.0, -3.0, 0.0)))
            .with_force(Force::drag(0.5))
            .with_looping(false)
    }
}

/// Emitter shape
#[derive(Clone, Debug)]
pub enum EmitterShape {
    /// Single point
    Point,
    /// Sphere volume/surface
    Sphere { radius: f32 },
    /// Upper hemisphere
    Hemisphere { radius: f32 },
    /// Cone shape
    Cone { angle: f32, radius: f32 },
    /// Box volume
    Box { half_extents: Vec3 },
    /// Circle (XZ plane)
    Circle { radius: f32 },
}

impl Default for EmitterShape {
    fn default() -> Self {
        Self::Point
    }
}

/// Force affector
#[derive(Clone, Debug)]
pub enum Force {
    /// Constant directional force
    Gravity(Vec3),
    /// Wind with turbulence
    Wind { direction: Vec3, strength: f32, turbulence: f32 },
    /// Vortex/swirl
    Vortex { axis: Vec3, center: Vec3, strength: f32 },
    /// Velocity damping
    Drag(f32),
    /// Noise-based force
    Turbulence { strength: f32, frequency: f32 },
    /// Point attractor/repeller
    Attractor { position: Vec3, strength: f32 },
    /// Radial force
    Radial { center: Vec3, strength: f32 },
}

impl Force {
    /// Create gravity force
    pub fn gravity(acceleration: Vec3) -> Self {
        Self::Gravity(acceleration)
    }

    /// Create wind force
    pub fn wind(direction: Vec3, strength: f32, turbulence: f32) -> Self {
        Self::Wind { direction, strength, turbulence }
    }

    /// Create vortex force
    pub fn vortex(axis: Vec3, center: Vec3, strength: f32) -> Self {
        Self::Vortex { axis, center, strength }
    }

    /// Create drag force
    pub fn drag(coefficient: f32) -> Self {
        Self::Drag(coefficient)
    }

    /// Create turbulence force
    pub fn turbulence(strength: f32, frequency: f32) -> Self {
        Self::Turbulence { strength, frequency }
    }

    /// Create attractor force
    pub fn attractor(position: Vec3, strength: f32) -> Self {
        Self::Attractor { position, strength }
    }

    /// Create radial force
    pub fn radial(center: Vec3, strength: f32) -> Self {
        Self::Radial { center, strength }
    }
}

/// Particle render mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Camera-facing billboards
    #[default]
    Billboard,
    /// Stretched in velocity direction
    Stretched,
    /// Horizontal billboards
    Horizontal,
    /// Vertical billboards
    Vertical,
}

/// Particle blend mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlendMode {
    /// Standard alpha blending
    #[default]
    Alpha,
    /// Additive blending (for glowing effects)
    Additive,
    /// Multiplicative blending
    Multiply,
    /// Premultiplied alpha
    Premultiplied,
}

/// System for updating particle systems
pub struct ParticleUpdateSystem;

impl System for ParticleUpdateSystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        let dt = ctx.delta_time;

        let entities: Vec<_> = ctx.world
            .query::<(&ParticleSystem,)>()
            .iter()
            .map(|(e, _)| e)
            .collect();

        for entity in entities {
            if let Some(system) = ctx.world.get_mut::<ParticleSystem>(entity) {
                if system.playing {
                    system.time += dt;
                    system.spawn_accumulated += system.emission_rate * dt;

                    if !system.looping && system.time >= system.duration {
                        system.playing = false;
                    }
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "ParticleUpdateSystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }

    fn priority(&self) -> i32 {
        10
    }
}

// ============================================================================
// Internal implementation - not exposed to users
// ============================================================================

pub(crate) mod internal {
    use super::*;
    use bytemuck::{Pod, Zeroable};

    /// GPU particle data
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct Particle {
        pub position: [f32; 3],
        pub life: f32,
        pub velocity: [f32; 3],
        pub max_life: f32,
        pub color: [f32; 4],
        pub size: [f32; 2],
        pub rotation: f32,
        pub rotation_velocity: f32,
    }

    /// GPU emitter uniform
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct EmitterUniform {
        pub position: [f32; 4],
        pub shape_params: [f32; 4],
        pub direction: [f32; 4],
        pub emission_rate: f32,
        pub burst_count: u32,
        pub spawn_accumulated: f32,
        pub _pad0: f32,
        pub lifetime: [f32; 2],
        pub start_speed: [f32; 2],
        pub start_size: [f32; 2],
        pub end_size: [f32; 2],
        pub start_color: [f32; 4],
        pub mid_color: [f32; 4],
        pub end_color: [f32; 4],
        pub start_rotation: [f32; 2],
        pub rotation_speed: [f32; 2],
    }

    impl Default for EmitterUniform {
        fn default() -> Self {
            Self {
                position: [0.0; 4],
                shape_params: [0.0; 4],
                direction: [0.0, 1.0, 0.0, 0.0],
                emission_rate: 100.0,
                burst_count: 0,
                spawn_accumulated: 0.0,
                _pad0: 0.0,
                lifetime: [1.0, 2.0],
                start_speed: [1.0, 2.0],
                start_size: [0.1, 0.2],
                end_size: [0.0, 0.1],
                start_color: [1.0; 4],
                mid_color: [1.0, 1.0, 1.0, 0.5],
                end_color: [1.0, 1.0, 1.0, 0.0],
                start_rotation: [0.0; 2],
                rotation_speed: [0.0; 2],
            }
        }
    }

    /// GPU force affector
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct ForceUniform {
        pub force_type: u32,
        pub strength: f32,
        pub _pad: [f32; 2],
        pub direction: [f32; 4],
        pub params: [f32; 4],
    }

    impl Default for ForceUniform {
        fn default() -> Self {
            Self {
                force_type: 0,
                strength: 0.0,
                _pad: [0.0; 2],
                direction: [0.0; 4],
                params: [0.0; 4],
            }
        }
    }

    /// GPU system uniform
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct SystemUniform {
        pub max_particles: u32,
        pub active_particles: u32,
        pub delta_time: f32,
        pub time: f32,
        pub simulation_space: u32,
        pub num_forces: u32,
        pub gravity_scale: f32,
        pub _pad: f32,
        pub bounds_min: [f32; 4],
        pub bounds_max: [f32; 4],
    }

    impl Default for SystemUniform {
        fn default() -> Self {
            Self {
                max_particles: 10000,
                active_particles: 0,
                delta_time: 0.016,
                time: 0.0,
                simulation_space: 0,
                num_forces: 0,
                gravity_scale: 1.0,
                _pad: 0.0,
                bounds_min: [-1000.0; 4],
                bounds_max: [1000.0; 4],
            }
        }
    }

    /// GPU render uniform
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Pod, Zeroable)]
    pub struct RenderUniform {
        pub render_mode: u32,
        pub blend_mode: u32,
        pub sort_mode: u32,
        pub soft_particles_enabled: u32,
        pub soft_particles_distance: f32,
        pub length_scale: f32,
        pub speed_scale: f32,
        pub _pad: f32,
        pub sprite_sheet_size: [f32; 2],
        pub animation_speed: f32,
        pub _pad2: f32,
    }

    impl Default for RenderUniform {
        fn default() -> Self {
            Self {
                render_mode: 0,
                blend_mode: 0,
                sort_mode: 0,
                soft_particles_enabled: 0,
                soft_particles_distance: 0.5,
                length_scale: 1.0,
                speed_scale: 0.1,
                _pad: 0.0,
                sprite_sheet_size: [1.0, 1.0],
                animation_speed: 1.0,
                _pad2: 0.0,
            }
        }
    }

    fn shape_to_type(shape: &EmitterShape) -> u32 {
        match shape {
            EmitterShape::Point => 0,
            EmitterShape::Sphere { .. } => 1,
            EmitterShape::Hemisphere { .. } => 2,
            EmitterShape::Cone { .. } => 3,
            EmitterShape::Box { .. } => 4,
            EmitterShape::Circle { .. } => 5,
        }
    }

    fn shape_to_params(shape: &EmitterShape) -> [f32; 4] {
        match shape {
            EmitterShape::Point => [0.0; 4],
            EmitterShape::Sphere { radius } => [*radius, 0.0, 0.0, 0.0],
            EmitterShape::Hemisphere { radius } => [*radius, 0.0, 0.0, 0.0],
            EmitterShape::Cone { angle, radius } => [*angle, *radius, 0.0, 0.0],
            EmitterShape::Box { half_extents } => [half_extents.x, half_extents.y, half_extents.z, 0.0],
            EmitterShape::Circle { radius } => [*radius, 0.0, 0.0, 0.0],
        }
    }

    /// Build emitter uniform from ParticleSystem
    pub fn build_emitter_uniform(system: &ParticleSystem, position: Vec3) -> EmitterUniform {
        EmitterUniform {
            position: [position.x, position.y, position.z, shape_to_type(&system.emitter) as f32],
            shape_params: shape_to_params(&system.emitter),
            direction: [system.direction.x, system.direction.y, system.direction.z, system.direction_randomness],
            emission_rate: system.emission_rate,
            burst_count: 0,
            spawn_accumulated: system.spawn_accumulated,
            _pad0: 0.0,
            lifetime: [system.lifetime.0, system.lifetime.1],
            start_speed: [system.start_speed.0, system.start_speed.1],
            start_size: [system.start_size.0, system.start_size.1],
            end_size: [system.end_size.0, system.end_size.1],
            start_color: [system.start_color.r, system.start_color.g, system.start_color.b, system.start_color.a],
            mid_color: [system.mid_color.r, system.mid_color.g, system.mid_color.b, system.mid_color.a],
            end_color: [system.end_color.r, system.end_color.g, system.end_color.b, system.end_color.a],
            start_rotation: [system.start_rotation.0, system.start_rotation.1],
            rotation_speed: [system.rotation_speed.0, system.rotation_speed.1],
        }
    }

    /// Build force uniform from Force
    pub fn build_force_uniform(force: &Force) -> ForceUniform {
        match force {
            Force::Gravity(dir) => ForceUniform {
                force_type: 0,
                strength: 1.0,
                direction: [dir.x, dir.y, dir.z, 0.0],
                ..Default::default()
            },
            Force::Wind { direction, strength, turbulence } => ForceUniform {
                force_type: 1,
                strength: *strength,
                direction: [direction.x, direction.y, direction.z, 0.0],
                params: [*turbulence, 0.0, 0.0, 0.0],
                ..Default::default()
            },
            Force::Vortex { axis, center, strength } => ForceUniform {
                force_type: 2,
                strength: *strength,
                direction: [axis.x, axis.y, axis.z, 0.0],
                params: [center.x, center.y, center.z, 0.0],
                ..Default::default()
            },
            Force::Drag(coefficient) => ForceUniform {
                force_type: 3,
                strength: *coefficient,
                ..Default::default()
            },
            Force::Turbulence { strength, frequency } => ForceUniform {
                force_type: 4,
                strength: *strength,
                params: [*frequency, 0.0, 0.0, 0.0],
                ..Default::default()
            },
            Force::Attractor { position, strength } => ForceUniform {
                force_type: 5,
                strength: *strength,
                direction: [position.x, position.y, position.z, 0.0],
                ..Default::default()
            },
            Force::Radial { center, strength } => ForceUniform {
                force_type: 6,
                strength: *strength,
                direction: [center.x, center.y, center.z, 0.0],
                ..Default::default()
            },
        }
    }

    /// Build system uniform
    pub fn build_system_uniform(system: &ParticleSystem, dt: f32) -> SystemUniform {
        SystemUniform {
            max_particles: system.max_particles,
            active_particles: 0,
            delta_time: dt,
            time: system.time,
            simulation_space: 0,
            num_forces: system.forces.len() as u32,
            gravity_scale: system.gravity_scale,
            ..Default::default()
        }
    }

    /// Build render uniform
    pub fn build_render_uniform(system: &ParticleSystem) -> RenderUniform {
        RenderUniform {
            render_mode: system.render_mode as u32,
            blend_mode: system.blend_mode as u32,
            sort_mode: 0,
            soft_particles_enabled: if system.soft_particles { 1 } else { 0 },
            soft_particles_distance: system.soft_particle_distance,
            length_scale: system.length_scale,
            speed_scale: system.speed_scale,
            _pad: 0.0,
            sprite_sheet_size: system.sprite_sheet.map(|(c, r)| [c as f32, r as f32]).unwrap_or([1.0, 1.0]),
            animation_speed: system.animation_speed,
            _pad2: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_presets() {
        let fire = ParticleSystem::fire();
        assert!(fire.emission_rate > 100.0);
        assert!(!fire.forces.is_empty());
    }

    #[test]
    fn test_particle_builder() {
        let system = ParticleSystem::new()
            .with_emission_rate(200.0)
            .with_lifetime(1.0, 3.0)
            .with_force(Force::gravity(Vec3::new(0.0, -9.8, 0.0)));

        assert!((system.emission_rate - 200.0).abs() < 0.001);
        assert!((system.lifetime.0 - 1.0).abs() < 0.001);
        assert_eq!(system.forces.len(), 1);
    }

    #[test]
    fn test_burst() {
        let mut system = ParticleSystem::explosion();
        system.burst(100);
        assert!((system.spawn_accumulated - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_control_methods() {
        let mut system = ParticleSystem::new();
        assert!(system.playing);

        system.pause();
        assert!(!system.playing);

        system.play();
        assert!(system.playing);

        system.stop();
        assert!(!system.playing);
        assert!((system.time - 0.0).abs() < 0.001);
    }
}
