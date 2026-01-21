//! Individual particle data

use blinc_core::{Color, Vec3};

/// A single particle instance
#[derive(Clone, Debug)]
pub struct Particle {
    /// Current position
    pub position: Vec3,
    /// Current velocity
    pub velocity: Vec3,
    /// Current color
    pub color: Color,
    /// Current size
    pub size: f32,
    /// Current rotation (radians)
    pub rotation: f32,
    /// Angular velocity (radians per second)
    pub angular_velocity: f32,
    /// Time since spawn
    pub age: f32,
    /// Total lifetime
    pub lifetime: f32,
    /// Whether particle is alive
    pub alive: bool,
    /// Initial values for interpolation
    pub start_color: Color,
    pub start_size: f32,
    pub end_color: Color,
    pub end_size: f32,
    /// Custom data slot
    pub custom: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            color: Color::WHITE,
            size: 0.1,
            rotation: 0.0,
            angular_velocity: 0.0,
            age: 0.0,
            lifetime: 1.0,
            alive: true,
            start_color: Color::WHITE,
            start_size: 0.1,
            end_color: Color::WHITE,
            end_size: 0.1,
            custom: 0.0,
        }
    }
}

impl Particle {
    /// Create a new particle
    pub fn new(position: Vec3, velocity: Vec3, lifetime: f32) -> Self {
        Self {
            position,
            velocity,
            lifetime,
            ..Default::default()
        }
    }

    /// Get normalized age (0 = just spawned, 1 = about to die)
    pub fn normalized_age(&self) -> f32 {
        (self.age / self.lifetime).clamp(0.0, 1.0)
    }

    /// Check if particle should be killed
    pub fn is_dead(&self) -> bool {
        self.age >= self.lifetime
    }

    /// Update particle for one frame
    pub fn update(&mut self, dt: f32, forces: &[super::ForceAffector]) {
        if !self.alive {
            return;
        }

        self.age += dt;

        if self.is_dead() {
            self.alive = false;
            return;
        }

        // Apply forces
        let mut acceleration = Vec3::ZERO;
        for force in forces {
            let force_vec = force.calculate(self);
            acceleration.x += force_vec.x;
            acceleration.y += force_vec.y;
            acceleration.z += force_vec.z;
        }

        // Integrate velocity
        self.velocity.x += acceleration.x * dt;
        self.velocity.y += acceleration.y * dt;
        self.velocity.z += acceleration.z * dt;

        // Integrate position
        self.position.x += self.velocity.x * dt;
        self.position.y += self.velocity.y * dt;
        self.position.z += self.velocity.z * dt;

        // Update rotation
        self.rotation += self.angular_velocity * dt;

        // Interpolate color and size
        let t = self.normalized_age();
        self.color = Color::lerp(&self.start_color, &self.end_color, t);
        self.size = self.start_size + (self.end_size - self.start_size) * t;
    }

    /// Set initial and final colors
    pub fn with_colors(mut self, start: Color, end: Color) -> Self {
        self.start_color = start;
        self.end_color = end;
        self.color = start;
        self
    }

    /// Set initial and final sizes
    pub fn with_sizes(mut self, start: f32, end: f32) -> Self {
        self.start_size = start;
        self.end_size = end;
        self.size = start;
        self
    }

    /// Set angular velocity
    pub fn with_angular_velocity(mut self, vel: f32) -> Self {
        self.angular_velocity = vel;
        self
    }
}

/// GPU-friendly particle data for instancing
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ParticleInstance {
    /// Position (xyz) and size (w)
    pub position_size: [f32; 4],
    /// Color (rgba)
    pub color: [f32; 4],
    /// Rotation and custom data
    pub rotation_custom: [f32; 4],
}

impl ParticleInstance {
    /// Create from a particle
    pub fn from_particle(particle: &Particle) -> Self {
        Self {
            position_size: [particle.position.x, particle.position.y, particle.position.z, particle.size],
            color: [particle.color.r, particle.color.g, particle.color.b, particle.color.a],
            rotation_custom: [particle.rotation, particle.normalized_age(), particle.custom, 0.0],
        }
    }
}

impl From<&Particle> for ParticleInstance {
    fn from(particle: &Particle) -> Self {
        Self::from_particle(particle)
    }
}
