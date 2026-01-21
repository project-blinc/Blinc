//! Particle emitter shapes

use blinc_core::Vec3;
use crate::geometry::GeometryHandle;

/// Emitter shape for particle spawning
#[derive(Clone, Debug)]
pub enum EmitterShape {
    /// Single point emitter
    Point,
    /// Spherical emitter
    Sphere {
        /// Radius of the sphere
        radius: f32,
    },
    /// Hemisphere emitter
    Hemisphere {
        /// Radius of the hemisphere
        radius: f32,
    },
    /// Cone emitter (particles emit in cone direction)
    Cone {
        /// Half-angle of the cone in radians
        angle: f32,
        /// Base radius
        radius: f32,
    },
    /// Box emitter
    Box {
        /// Half extents of the box
        half_extents: Vec3,
    },
    /// Circle emitter (2D, in XZ plane)
    Circle {
        /// Radius of the circle
        radius: f32,
    },
    /// Edge emitter (line segment)
    Edge {
        /// Start point
        start: Vec3,
        /// End point
        end: Vec3,
    },
    /// Mesh surface emitter
    MeshSurface {
        /// Geometry to emit from
        geometry: GeometryHandle,
    },
}

impl Default for EmitterShape {
    fn default() -> Self {
        Self::Point
    }
}

impl EmitterShape {
    /// Generate a random position within the emitter shape
    pub fn sample_position(&self, rng: &mut impl FnMut() -> f32) -> Vec3 {
        match self {
            EmitterShape::Point => Vec3::ZERO,

            EmitterShape::Sphere { radius } => {
                // Uniform distribution in sphere
                let theta = rng() * std::f32::consts::TAU;
                let phi = (1.0 - 2.0 * rng()).acos();
                let r = rng().cbrt() * radius;
                Vec3::new(
                    r * phi.sin() * theta.cos(),
                    r * phi.cos(),
                    r * phi.sin() * theta.sin(),
                )
            }

            EmitterShape::Hemisphere { radius } => {
                let theta = rng() * std::f32::consts::TAU;
                let phi = rng().acos(); // Only upper hemisphere
                let r = rng().cbrt() * radius;
                Vec3::new(
                    r * phi.sin() * theta.cos(),
                    r * phi.cos().abs(), // Ensure positive Y
                    r * phi.sin() * theta.sin(),
                )
            }

            EmitterShape::Cone { angle, radius } => {
                let theta = rng() * std::f32::consts::TAU;
                let r = rng().sqrt() * radius;
                Vec3::new(
                    r * theta.cos(),
                    0.0,
                    r * theta.sin(),
                )
            }

            EmitterShape::Box { half_extents } => {
                Vec3::new(
                    (rng() * 2.0 - 1.0) * half_extents.x,
                    (rng() * 2.0 - 1.0) * half_extents.y,
                    (rng() * 2.0 - 1.0) * half_extents.z,
                )
            }

            EmitterShape::Circle { radius } => {
                let theta = rng() * std::f32::consts::TAU;
                let r = rng().sqrt() * radius;
                Vec3::new(r * theta.cos(), 0.0, r * theta.sin())
            }

            EmitterShape::Edge { start, end } => {
                let t = rng();
                Vec3::new(
                    start.x + t * (end.x - start.x),
                    start.y + t * (end.y - start.y),
                    start.z + t * (end.z - start.z),
                )
            }

            EmitterShape::MeshSurface { .. } => {
                // Would need actual mesh data to sample properly
                // For now, return origin
                Vec3::ZERO
            }
        }
    }

    /// Generate a velocity direction based on emitter shape
    pub fn sample_velocity(&self, position: Vec3, rng: &mut impl FnMut() -> f32) -> Vec3 {
        match self {
            EmitterShape::Point => {
                // Random direction
                let theta = rng() * std::f32::consts::TAU;
                let phi = (1.0 - 2.0 * rng()).acos();
                Vec3::new(
                    phi.sin() * theta.cos(),
                    phi.cos(),
                    phi.sin() * theta.sin(),
                )
            }

            EmitterShape::Sphere { .. } | EmitterShape::Hemisphere { .. } => {
                // Outward from center
                let len = (position.x * position.x + position.y * position.y + position.z * position.z).sqrt();
                if len > 0.001 {
                    Vec3::new(position.x / len, position.y / len, position.z / len)
                } else {
                    Vec3::new(0.0, 1.0, 0.0)
                }
            }

            EmitterShape::Cone { angle, .. } => {
                // Upward with spread
                let theta = rng() * std::f32::consts::TAU;
                let phi = rng() * angle;
                Vec3::new(
                    phi.sin() * theta.cos(),
                    phi.cos(),
                    phi.sin() * theta.sin(),
                )
            }

            EmitterShape::Box { .. } | EmitterShape::Circle { .. } => {
                // Upward
                Vec3::new(0.0, 1.0, 0.0)
            }

            EmitterShape::Edge { start, end } => {
                // Perpendicular to edge, upward bias
                let edge = Vec3::new(
                    end.x - start.x,
                    end.y - start.y,
                    end.z - start.z,
                );
                let up = Vec3::new(0.0, 1.0, 0.0);
                // Cross product for perpendicular
                let perp = Vec3::new(
                    edge.y * up.z - edge.z * up.y,
                    edge.z * up.x - edge.x * up.z,
                    edge.x * up.y - edge.y * up.x,
                );
                let len = (perp.x * perp.x + perp.y * perp.y + perp.z * perp.z).sqrt();
                if len > 0.001 {
                    Vec3::new(perp.x / len, perp.y / len, perp.z / len)
                } else {
                    up
                }
            }

            EmitterShape::MeshSurface { .. } => {
                // Would need mesh normals
                Vec3::new(0.0, 1.0, 0.0)
            }
        }
    }
}

/// Emitter configuration for detailed control
#[derive(Clone, Debug)]
pub struct EmitterConfig {
    /// Base shape
    pub shape: EmitterShape,
    /// Whether to emit from surface only (vs volume)
    pub surface_only: bool,
    /// Emit from random position within shape
    pub randomize_position: bool,
    /// Arc of emission (for circle/cone shapes, in radians)
    pub arc: f32,
    /// Arc mode
    pub arc_mode: ArcMode,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            shape: EmitterShape::Point,
            surface_only: false,
            randomize_position: true,
            arc: std::f32::consts::TAU,
            arc_mode: ArcMode::Random,
        }
    }
}

/// How to distribute particles along an arc
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArcMode {
    /// Random distribution
    Random,
    /// Sequential around the arc
    Loop,
    /// Ping-pong around the arc
    PingPong,
    /// Burst spread evenly
    BurstSpread,
}
