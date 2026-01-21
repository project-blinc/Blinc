//! Particle system for visual effects
//!
//! Provides GPU-instanced particle rendering with various emitter shapes
//! and force affectors.
//!
//! # Example
//!
//! ```ignore
//! use blinc_3d::utils::particles::*;
//!
//! // Create a fire particle system
//! let fire = ParticleSystem::new()
//!     .with_emitter(EmitterShape::Cone { angle: 0.3, radius: 0.5 })
//!     .with_emission_rate(100.0)
//!     .with_lifetime(1.0..2.0)
//!     .with_start_color(Color::rgb(1.0, 0.8, 0.2))
//!     .with_end_color(Color::rgba(1.0, 0.2, 0.0, 0.0))
//!     .with_start_size(0.1..0.2)
//!     .with_end_size(0.0..0.05)
//!     .with_force(GravityForce(Vec3::new(0.0, 2.0, 0.0))); // Upward for fire
//! ```

mod emitter;
mod particle;
mod forces;
mod modules;

pub use emitter::*;
pub use particle::*;
pub use forces::*;
pub use modules::*;

use crate::ecs::{Component, System, SystemContext, SystemStage};
use crate::geometry::GeometryHandle;
use blinc_core::{Color, Vec3};
use std::ops::Range;

/// Particle system component
///
/// Attach to an entity to create particle effects at that location.
#[derive(Clone, Debug)]
pub struct ParticleSystem {
    /// Emitter shape
    pub emitter: EmitterShape,
    /// Maximum number of particles
    pub max_particles: u32,
    /// Particles emitted per second
    pub emission_rate: f32,
    /// Burst emission (particles, interval)
    pub burst: Option<(u32, f32)>,
    /// Particle lifetime range in seconds
    pub lifetime: Range<f32>,
    /// Initial velocity range
    pub start_velocity: Range<f32>,
    /// Velocity spread angle (radians)
    pub velocity_spread: f32,
    /// Starting color
    pub start_color: Color,
    /// Ending color (interpolates over lifetime)
    pub end_color: Color,
    /// Starting size range
    pub start_size: Range<f32>,
    /// Ending size range
    pub end_size: Range<f32>,
    /// Force affectors
    pub forces: Vec<ForceAffector>,
    /// Color over lifetime curve
    pub color_over_lifetime: Option<ColorOverLifetime>,
    /// Size over lifetime curve
    pub size_over_lifetime: Option<SizeOverLifetime>,
    /// Rendering mode
    pub render_mode: ParticleRenderMode,
    /// Whether the system is playing
    pub playing: bool,
    /// Whether to loop
    pub looping: bool,
    /// Duration (for non-looping systems)
    pub duration: f32,
    /// World space vs local space
    pub simulation_space: SimulationSpace,
    /// Internal: accumulated emission time
    emission_accumulator: f32,
    /// Internal: burst timer
    burst_timer: f32,
    /// Internal: total elapsed time
    elapsed: f32,
}

impl Component for ParticleSystem {}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleSystem {
    /// Create a new particle system with default settings
    pub fn new() -> Self {
        Self {
            emitter: EmitterShape::Point,
            max_particles: 1000,
            emission_rate: 10.0,
            burst: None,
            lifetime: 1.0..2.0,
            start_velocity: 1.0..2.0,
            velocity_spread: 0.5,
            start_color: Color::WHITE,
            end_color: Color::rgba(1.0, 1.0, 1.0, 0.0),
            start_size: 0.1..0.2,
            end_size: 0.0..0.1,
            forces: Vec::new(),
            color_over_lifetime: None,
            size_over_lifetime: None,
            render_mode: ParticleRenderMode::Billboard,
            playing: true,
            looping: true,
            duration: 5.0,
            simulation_space: SimulationSpace::World,
            emission_accumulator: 0.0,
            burst_timer: 0.0,
            elapsed: 0.0,
        }
    }

    /// Set emitter shape
    pub fn with_emitter(mut self, emitter: EmitterShape) -> Self {
        self.emitter = emitter;
        self
    }

    /// Set maximum particles
    pub fn with_max_particles(mut self, max: u32) -> Self {
        self.max_particles = max;
        self
    }

    /// Set emission rate (particles per second)
    pub fn with_emission_rate(mut self, rate: f32) -> Self {
        self.emission_rate = rate;
        self
    }

    /// Set burst emission
    pub fn with_burst(mut self, count: u32, interval: f32) -> Self {
        self.burst = Some((count, interval));
        self
    }

    /// Set particle lifetime range
    pub fn with_lifetime(mut self, lifetime: Range<f32>) -> Self {
        self.lifetime = lifetime;
        self
    }

    /// Set starting velocity range
    pub fn with_start_velocity(mut self, velocity: Range<f32>) -> Self {
        self.start_velocity = velocity;
        self
    }

    /// Set velocity spread angle
    pub fn with_velocity_spread(mut self, spread: f32) -> Self {
        self.velocity_spread = spread;
        self
    }

    /// Set starting color
    pub fn with_start_color(mut self, color: Color) -> Self {
        self.start_color = color;
        self
    }

    /// Set ending color
    pub fn with_end_color(mut self, color: Color) -> Self {
        self.end_color = color;
        self
    }

    /// Set starting size range
    pub fn with_start_size(mut self, size: Range<f32>) -> Self {
        self.start_size = size;
        self
    }

    /// Set ending size range
    pub fn with_end_size(mut self, size: Range<f32>) -> Self {
        self.end_size = size;
        self
    }

    /// Add a force affector
    pub fn with_force(mut self, force: ForceAffector) -> Self {
        self.forces.push(force);
        self
    }

    /// Set color over lifetime
    pub fn with_color_over_lifetime(mut self, curve: ColorOverLifetime) -> Self {
        self.color_over_lifetime = Some(curve);
        self
    }

    /// Set size over lifetime
    pub fn with_size_over_lifetime(mut self, curve: SizeOverLifetime) -> Self {
        self.size_over_lifetime = Some(curve);
        self
    }

    /// Set render mode
    pub fn with_render_mode(mut self, mode: ParticleRenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Set looping
    pub fn with_looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Set duration
    pub fn with_duration(mut self, duration: f32) -> Self {
        self.duration = duration;
        self
    }

    /// Set simulation space
    pub fn with_simulation_space(mut self, space: SimulationSpace) -> Self {
        self.simulation_space = space;
        self
    }

    /// Play the particle system
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pause the particle system
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Stop and reset the particle system
    pub fn stop(&mut self) {
        self.playing = false;
        self.elapsed = 0.0;
        self.emission_accumulator = 0.0;
        self.burst_timer = 0.0;
    }

    /// Check if the system is playing
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Get elapsed time
    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    // ========== Presets ==========

    /// Fire particle effect
    pub fn fire() -> Self {
        Self::new()
            .with_emitter(EmitterShape::Cone { angle: 0.2, radius: 0.3 })
            .with_emission_rate(80.0)
            .with_lifetime(0.5..1.5)
            .with_start_velocity(2.0..4.0)
            .with_start_color(Color::rgb(1.0, 0.8, 0.2))
            .with_end_color(Color::rgba(1.0, 0.2, 0.0, 0.0))
            .with_start_size(0.15..0.25)
            .with_end_size(0.0..0.05)
            .with_force(ForceAffector::Gravity(Vec3::new(0.0, 3.0, 0.0)))
            .with_force(ForceAffector::Turbulence { strength: 0.5, frequency: 2.0 })
    }

    /// Smoke particle effect
    pub fn smoke() -> Self {
        Self::new()
            .with_emitter(EmitterShape::Cone { angle: 0.1, radius: 0.2 })
            .with_emission_rate(30.0)
            .with_lifetime(2.0..4.0)
            .with_start_velocity(0.5..1.5)
            .with_start_color(Color::rgba(0.3, 0.3, 0.3, 0.6))
            .with_end_color(Color::rgba(0.5, 0.5, 0.5, 0.0))
            .with_start_size(0.2..0.4)
            .with_end_size(0.8..1.2)
            .with_force(ForceAffector::Gravity(Vec3::new(0.0, 0.5, 0.0)))
            .with_force(ForceAffector::Wind { direction: Vec3::new(1.0, 0.0, 0.0), strength: 0.3, turbulence: 0.0 })
    }

    /// Sparks particle effect
    pub fn sparks() -> Self {
        Self::new()
            .with_emitter(EmitterShape::Point)
            .with_emission_rate(50.0)
            .with_lifetime(0.3..0.8)
            .with_start_velocity(5.0..10.0)
            .with_velocity_spread(1.0)
            .with_start_color(Color::rgb(1.0, 0.9, 0.5))
            .with_end_color(Color::rgb(1.0, 0.3, 0.0))
            .with_start_size(0.02..0.05)
            .with_end_size(0.0..0.01)
            .with_force(ForceAffector::Gravity(Vec3::new(0.0, -9.8, 0.0)))
            .with_render_mode(ParticleRenderMode::StretchedBillboard { length_scale: 0.1 })
    }

    /// Rain particle effect
    pub fn rain() -> Self {
        Self::new()
            .with_emitter(EmitterShape::Box { half_extents: Vec3::new(10.0, 0.1, 10.0) })
            .with_emission_rate(500.0)
            .with_lifetime(1.0..2.0)
            .with_start_velocity(10.0..15.0)
            .with_velocity_spread(0.05)
            .with_start_color(Color::rgba(0.7, 0.8, 1.0, 0.5))
            .with_end_color(Color::rgba(0.7, 0.8, 1.0, 0.3))
            .with_start_size(0.01..0.02)
            .with_end_size(0.01..0.02)
            .with_force(ForceAffector::Gravity(Vec3::new(0.0, -9.8, 0.0)))
            .with_render_mode(ParticleRenderMode::StretchedBillboard { length_scale: 0.5 })
    }

    /// Snow particle effect
    pub fn snow() -> Self {
        Self::new()
            .with_emitter(EmitterShape::Box { half_extents: Vec3::new(10.0, 0.1, 10.0) })
            .with_emission_rate(100.0)
            .with_lifetime(4.0..8.0)
            .with_start_velocity(0.5..1.0)
            .with_velocity_spread(0.3)
            .with_start_color(Color::WHITE)
            .with_end_color(Color::WHITE)
            .with_start_size(0.02..0.05)
            .with_end_size(0.02..0.05)
            .with_force(ForceAffector::Gravity(Vec3::new(0.0, -1.0, 0.0)))
            .with_force(ForceAffector::Turbulence { strength: 0.3, frequency: 0.5 })
    }

    /// Explosion burst effect
    pub fn explosion() -> Self {
        Self::new()
            .with_emitter(EmitterShape::Sphere { radius: 0.1 })
            .with_emission_rate(0.0)
            .with_burst(200, 0.0)
            .with_looping(false)
            .with_lifetime(0.5..1.5)
            .with_start_velocity(5.0..15.0)
            .with_velocity_spread(3.14)
            .with_start_color(Color::rgb(1.0, 0.8, 0.3))
            .with_end_color(Color::rgba(0.3, 0.1, 0.0, 0.0))
            .with_start_size(0.2..0.5)
            .with_end_size(0.0..0.1)
            .with_force(ForceAffector::Gravity(Vec3::new(0.0, -5.0, 0.0)))
            .with_force(ForceAffector::Drag(2.0))
    }

    /// Magic sparkle effect
    pub fn magic() -> Self {
        Self::new()
            .with_emitter(EmitterShape::Sphere { radius: 0.5 })
            .with_emission_rate(40.0)
            .with_lifetime(1.0..2.0)
            .with_start_velocity(0.2..0.5)
            .with_start_color(Color::rgb(0.5, 0.8, 1.0))
            .with_end_color(Color::rgba(1.0, 0.5, 1.0, 0.0))
            .with_start_size(0.05..0.1)
            .with_end_size(0.0..0.02)
            .with_force(ForceAffector::Vortex {
                axis: Vec3::new(0.0, 1.0, 0.0),
                strength: 1.0,
                center: None,
            })
    }
}

/// Simulation space for particles
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimulationSpace {
    /// Particles move in world space
    World,
    /// Particles move relative to emitter
    Local,
}

/// Particle rendering mode
#[derive(Clone, Debug)]
pub enum ParticleRenderMode {
    /// Camera-facing billboards
    Billboard,
    /// Stretched in velocity direction
    StretchedBillboard {
        /// Length scale based on velocity
        length_scale: f32,
    },
    /// Horizontal billboards
    HorizontalBillboard,
    /// Vertical billboards
    VerticalBillboard,
    /// 3D mesh instances
    Mesh {
        /// Geometry to use
        geometry: GeometryHandle,
    },
}

/// Particle buffer for storing active particles
#[derive(Clone, Debug)]
pub struct ParticleBuffer {
    /// Active particles
    pub particles: Vec<Particle>,
    /// Pool of dead particles for reuse
    dead_indices: Vec<usize>,
}

impl ParticleBuffer {
    /// Create a new particle buffer with capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            particles: Vec::with_capacity(capacity),
            dead_indices: Vec::with_capacity(capacity),
        }
    }

    /// Spawn a new particle, reusing dead slots if available
    pub fn spawn(&mut self, particle: Particle) -> Option<usize> {
        if let Some(index) = self.dead_indices.pop() {
            self.particles[index] = particle;
            Some(index)
        } else if self.particles.len() < self.particles.capacity() {
            let index = self.particles.len();
            self.particles.push(particle);
            Some(index)
        } else {
            None // At capacity
        }
    }

    /// Mark a particle as dead
    pub fn kill(&mut self, index: usize) {
        if index < self.particles.len() {
            self.particles[index].alive = false;
            self.dead_indices.push(index);
        }
    }

    /// Get number of alive particles
    pub fn alive_count(&self) -> usize {
        self.particles.len() - self.dead_indices.len()
    }

    /// Clear all particles
    pub fn clear(&mut self) {
        self.particles.clear();
        self.dead_indices.clear();
    }

    /// Iterate over alive particles
    pub fn iter_alive(&self) -> impl Iterator<Item = (usize, &Particle)> {
        self.particles
            .iter()
            .enumerate()
            .filter(|(_, p)| p.alive)
    }

    /// Iterate over alive particles mutably
    pub fn iter_alive_mut(&mut self) -> impl Iterator<Item = (usize, &mut Particle)> {
        self.particles
            .iter_mut()
            .enumerate()
            .filter(|(_, p)| p.alive)
    }
}

/// System for updating particle systems
pub struct ParticleUpdateSystem;

impl System for ParticleUpdateSystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        let dt = ctx.delta_time;

        // Collect entities with particle systems
        let entities: Vec<_> = ctx.world
            .query::<(&ParticleSystem,)>()
            .iter()
            .map(|(e, _)| e)
            .collect();

        // Update each particle system
        for entity in entities {
            if let Some(system) = ctx.world.get_mut::<ParticleSystem>(entity) {
                if system.playing {
                    system.elapsed += dt;

                    // Check duration for non-looping systems
                    if !system.looping && system.elapsed >= system.duration {
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
        10 // Run after most game logic
    }
}
