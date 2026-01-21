//! Force affectors for particle systems

use super::particle::Particle;
use blinc_core::Vec3;

/// Force affector that influences particle velocity
#[derive(Clone, Debug)]
pub enum ForceAffector {
    /// Constant directional force (gravity, wind base)
    Gravity(Vec3),

    /// Wind with turbulence
    Wind {
        /// Base wind direction (normalized internally)
        direction: Vec3,
        /// Wind strength
        strength: f32,
        /// Turbulence amount (0 = steady, 1 = chaotic)
        turbulence: f32,
    },

    /// Vortex/swirl around an axis
    Vortex {
        /// Axis of rotation
        axis: Vec3,
        /// Rotation strength (positive = counter-clockwise when looking down axis)
        strength: f32,
        /// Optional center point (defaults to origin)
        center: Option<Vec3>,
    },

    /// Velocity damping
    Drag(f32),

    /// Turbulent noise-based force
    Turbulence {
        /// Turbulence strength
        strength: f32,
        /// Noise frequency (higher = more chaotic)
        frequency: f32,
    },

    /// Point attractor/repeller
    Attractor {
        /// Attractor position
        position: Vec3,
        /// Attraction strength (negative = repel)
        strength: f32,
        /// Radius of effect (0 = infinite)
        radius: f32,
    },

    /// Radial force from a point
    Radial {
        /// Center of the radial force
        center: Vec3,
        /// Force strength (positive = outward, negative = inward)
        strength: f32,
    },

    /// Force along particle's velocity direction
    VelocityScale {
        /// Scale factor for velocity
        scale: f32,
    },

    /// Limit maximum velocity
    SpeedLimit {
        /// Maximum speed
        max_speed: f32,
    },

    /// Collision plane (simple bounce)
    CollisionPlane {
        /// Plane normal (pointing up/outward)
        normal: Vec3,
        /// Distance from origin along normal
        offset: f32,
        /// Bounce factor (0 = absorb, 1 = perfect bounce)
        bounce: f32,
        /// Friction (0 = none, 1 = full stop on contact)
        friction: f32,
    },
}

impl ForceAffector {
    /// Calculate force/acceleration to apply to a particle
    pub fn calculate(&self, particle: &Particle) -> Vec3 {
        match self {
            ForceAffector::Gravity(g) => *g,

            ForceAffector::Wind { direction, strength, turbulence } => {
                let base = normalize(*direction);
                let base_force = Vec3::new(
                    base.x * *strength,
                    base.y * *strength,
                    base.z * *strength,
                );

                if *turbulence > 0.0 {
                    // Simple pseudo-random turbulence based on position and time
                    let noise = simple_noise_3d(
                        particle.position.x + particle.age * 2.0,
                        particle.position.y + particle.age * 2.0,
                        particle.position.z + particle.age * 2.0,
                    );
                    Vec3::new(
                        base_force.x + noise.x * *turbulence * *strength,
                        base_force.y + noise.y * *turbulence * *strength,
                        base_force.z + noise.z * *turbulence * *strength,
                    )
                } else {
                    base_force
                }
            }

            ForceAffector::Vortex { axis, strength, center } => {
                let center = center.unwrap_or(Vec3::ZERO);
                let to_particle = Vec3::new(
                    particle.position.x - center.x,
                    particle.position.y - center.y,
                    particle.position.z - center.z,
                );

                // Cross product with axis to get tangent direction
                let axis_norm = normalize(*axis);
                let tangent = cross(axis_norm, to_particle);
                let tangent_len = length(tangent);

                if tangent_len > 0.0001 {
                    let inv_len = 1.0 / tangent_len;
                    Vec3::new(
                        tangent.x * inv_len * *strength,
                        tangent.y * inv_len * *strength,
                        tangent.z * inv_len * *strength,
                    )
                } else {
                    Vec3::ZERO
                }
            }

            ForceAffector::Drag(drag) => {
                // Drag opposes velocity
                Vec3::new(
                    -particle.velocity.x * *drag,
                    -particle.velocity.y * *drag,
                    -particle.velocity.z * *drag,
                )
            }

            ForceAffector::Turbulence { strength, frequency } => {
                let noise = simple_noise_3d(
                    particle.position.x * *frequency + particle.age,
                    particle.position.y * *frequency + particle.age * 1.3,
                    particle.position.z * *frequency + particle.age * 0.7,
                );
                Vec3::new(
                    noise.x * *strength,
                    noise.y * *strength,
                    noise.z * *strength,
                )
            }

            ForceAffector::Attractor { position, strength, radius } => {
                let to_attractor = Vec3::new(
                    position.x - particle.position.x,
                    position.y - particle.position.y,
                    position.z - particle.position.z,
                );
                let dist = length(to_attractor);

                if dist < 0.0001 {
                    return Vec3::ZERO;
                }

                // Check radius
                if *radius > 0.0 && dist > *radius {
                    return Vec3::ZERO;
                }

                // Falloff with distance
                let falloff = if *radius > 0.0 {
                    1.0 - (dist / *radius)
                } else {
                    1.0 / (1.0 + dist * dist)
                };

                let inv_dist = 1.0 / dist;
                Vec3::new(
                    to_attractor.x * inv_dist * *strength * falloff,
                    to_attractor.y * inv_dist * *strength * falloff,
                    to_attractor.z * inv_dist * *strength * falloff,
                )
            }

            ForceAffector::Radial { center, strength } => {
                let from_center = Vec3::new(
                    particle.position.x - center.x,
                    particle.position.y - center.y,
                    particle.position.z - center.z,
                );
                let dist = length(from_center);

                if dist < 0.0001 {
                    return Vec3::ZERO;
                }

                let inv_dist = 1.0 / dist;
                Vec3::new(
                    from_center.x * inv_dist * *strength,
                    from_center.y * inv_dist * *strength,
                    from_center.z * inv_dist * *strength,
                )
            }

            ForceAffector::VelocityScale { scale } => {
                // Return force that scales velocity
                Vec3::new(
                    particle.velocity.x * (*scale - 1.0),
                    particle.velocity.y * (*scale - 1.0),
                    particle.velocity.z * (*scale - 1.0),
                )
            }

            ForceAffector::SpeedLimit { .. } => {
                // Speed limit is handled separately in apply_to_particle
                Vec3::ZERO
            }

            ForceAffector::CollisionPlane { .. } => {
                // Collision is handled separately in apply_to_particle
                Vec3::ZERO
            }
        }
    }

    /// Apply force to particle (for forces that need direct modification)
    pub fn apply_to_particle(&self, particle: &mut Particle, dt: f32) {
        match self {
            ForceAffector::SpeedLimit { max_speed } => {
                let speed = length(particle.velocity);
                if speed > *max_speed {
                    let scale = *max_speed / speed;
                    particle.velocity.x *= scale;
                    particle.velocity.y *= scale;
                    particle.velocity.z *= scale;
                }
            }

            ForceAffector::CollisionPlane { normal, offset, bounce, friction } => {
                // Distance from plane
                let normal_norm = normalize(*normal);
                let dist = dot(particle.position, normal_norm) - *offset;

                if dist < 0.0 {
                    // Below plane - push back and bounce
                    particle.position.x -= normal_norm.x * dist;
                    particle.position.y -= normal_norm.y * dist;
                    particle.position.z -= normal_norm.z * dist;

                    // Reflect velocity
                    let vel_dot = dot(particle.velocity, normal_norm);
                    if vel_dot < 0.0 {
                        // Separate velocity into normal and tangent components
                        let vel_normal = Vec3::new(
                            normal_norm.x * vel_dot,
                            normal_norm.y * vel_dot,
                            normal_norm.z * vel_dot,
                        );
                        let vel_tangent = Vec3::new(
                            particle.velocity.x - vel_normal.x,
                            particle.velocity.y - vel_normal.y,
                            particle.velocity.z - vel_normal.z,
                        );

                        // Apply bounce and friction
                        particle.velocity.x = vel_tangent.x * (1.0 - *friction) - vel_normal.x * *bounce;
                        particle.velocity.y = vel_tangent.y * (1.0 - *friction) - vel_normal.y * *bounce;
                        particle.velocity.z = vel_tangent.z * (1.0 - *friction) - vel_normal.z * *bounce;
                    }
                }

                // Consume dt to avoid unused warning
                let _ = dt;
            }

            _ => {
                // Other forces use calculate() and are applied externally
                let _ = dt;
            }
        }
    }

    /// Create a gravity force
    pub fn gravity(acceleration: Vec3) -> Self {
        ForceAffector::Gravity(acceleration)
    }

    /// Create standard earth-like gravity
    pub fn earth_gravity() -> Self {
        ForceAffector::Gravity(Vec3::new(0.0, -9.81, 0.0))
    }

    /// Create a wind force
    pub fn wind(direction: Vec3, strength: f32) -> Self {
        ForceAffector::Wind {
            direction,
            strength,
            turbulence: 0.0,
        }
    }

    /// Create a turbulent wind force
    pub fn wind_turbulent(direction: Vec3, strength: f32, turbulence: f32) -> Self {
        ForceAffector::Wind {
            direction,
            strength,
            turbulence,
        }
    }

    /// Create a vortex force
    pub fn vortex(axis: Vec3, strength: f32) -> Self {
        ForceAffector::Vortex {
            axis,
            strength,
            center: None,
        }
    }

    /// Create a vortex force with custom center
    pub fn vortex_at(axis: Vec3, strength: f32, center: Vec3) -> Self {
        ForceAffector::Vortex {
            axis,
            strength,
            center: Some(center),
        }
    }

    /// Create a drag force
    pub fn drag(coefficient: f32) -> Self {
        ForceAffector::Drag(coefficient)
    }

    /// Create a turbulence force
    pub fn turbulence(strength: f32, frequency: f32) -> Self {
        ForceAffector::Turbulence { strength, frequency }
    }

    /// Create a point attractor
    pub fn attractor(position: Vec3, strength: f32) -> Self {
        ForceAffector::Attractor {
            position,
            strength,
            radius: 0.0,
        }
    }

    /// Create a point attractor with limited radius
    pub fn attractor_limited(position: Vec3, strength: f32, radius: f32) -> Self {
        ForceAffector::Attractor {
            position,
            strength,
            radius,
        }
    }

    /// Create a point repeller
    pub fn repeller(position: Vec3, strength: f32) -> Self {
        ForceAffector::Attractor {
            position,
            strength: -strength,
            radius: 0.0,
        }
    }

    /// Create a radial outward force
    pub fn radial_outward(center: Vec3, strength: f32) -> Self {
        ForceAffector::Radial { center, strength }
    }

    /// Create a radial inward force
    pub fn radial_inward(center: Vec3, strength: f32) -> Self {
        ForceAffector::Radial {
            center,
            strength: -strength,
        }
    }

    /// Create a speed limit
    pub fn speed_limit(max_speed: f32) -> Self {
        ForceAffector::SpeedLimit { max_speed }
    }

    /// Create a collision plane (e.g., ground)
    pub fn ground_plane(height: f32, bounce: f32, friction: f32) -> Self {
        ForceAffector::CollisionPlane {
            normal: Vec3::new(0.0, 1.0, 0.0),
            offset: height,
            bounce,
            friction,
        }
    }

    /// Create a collision plane with custom orientation
    pub fn collision_plane(normal: Vec3, offset: f32, bounce: f32, friction: f32) -> Self {
        ForceAffector::CollisionPlane {
            normal,
            offset,
            bounce,
            friction,
        }
    }
}

// Helper math functions
fn length(v: Vec3) -> f32 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

fn normalize(v: Vec3) -> Vec3 {
    let len = length(v);
    if len < 0.0001 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        let inv = 1.0 / len;
        Vec3::new(v.x * inv, v.y * inv, v.z * inv)
    }
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// Simple 3D noise function (pseudo-random but deterministic)
fn simple_noise_3d(x: f32, y: f32, z: f32) -> Vec3 {
    // Simple hash-based noise
    let hash = |n: f32| -> f32 {
        let s = (n * 12.9898 + 78.233).sin() * 43758.5453;
        s.fract() * 2.0 - 1.0
    };

    Vec3::new(
        hash(x * 127.1 + y * 311.7 + z * 74.7),
        hash(x * 269.5 + y * 183.3 + z * 246.1),
        hash(x * 113.5 + y * 271.9 + z * 124.6),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gravity() {
        let force = ForceAffector::earth_gravity();
        let particle = Particle::default();
        let accel = force.calculate(&particle);
        assert!((accel.y - (-9.81)).abs() < 0.001);
    }

    #[test]
    fn test_drag() {
        let force = ForceAffector::drag(0.5);
        let mut particle = Particle::default();
        particle.velocity = Vec3::new(10.0, 0.0, 0.0);
        let accel = force.calculate(&particle);
        assert!((accel.x - (-5.0)).abs() < 0.001);
    }

    #[test]
    fn test_attractor() {
        let force = ForceAffector::attractor(Vec3::new(10.0, 0.0, 0.0), 1.0);
        let particle = Particle::default(); // at origin
        let accel = force.calculate(&particle);
        assert!(accel.x > 0.0); // Should pull toward attractor
    }

    #[test]
    fn test_speed_limit() {
        let force = ForceAffector::speed_limit(5.0);
        let mut particle = Particle::default();
        particle.velocity = Vec3::new(10.0, 0.0, 0.0);
        force.apply_to_particle(&mut particle, 0.016);
        let speed = length(particle.velocity);
        assert!((speed - 5.0).abs() < 0.001);
    }
}
