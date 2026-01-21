//! Collider shapes for physics collision detection

use crate::ecs::Component;
use crate::geometry::GeometryHandle;
use blinc_core::Vec3;

/// Collider shape types
#[derive(Clone, Debug)]
pub enum ColliderShape {
    /// Sphere collider
    Sphere {
        /// Radius
        radius: f32,
    },
    /// Box/cuboid collider
    Box {
        /// Half-extents (half width, height, depth)
        half_extents: Vec3,
    },
    /// Capsule collider (cylinder with hemispheres)
    Capsule {
        /// Half-height of cylindrical part
        half_height: f32,
        /// Radius of capsule
        radius: f32,
    },
    /// Cylinder collider
    Cylinder {
        /// Half-height
        half_height: f32,
        /// Radius
        radius: f32,
    },
    /// Cone collider
    Cone {
        /// Half-height
        half_height: f32,
        /// Base radius
        radius: f32,
    },
    /// Convex hull from points
    ConvexHull {
        /// Hull points
        points: Vec<Vec3>,
    },
    /// Triangle mesh (static only)
    TriMesh {
        /// Geometry handle
        geometry: GeometryHandle,
    },
    /// Heightfield terrain
    Heightfield {
        /// Width samples
        width: u32,
        /// Height samples
        height: u32,
        /// Height data
        heights: Vec<f32>,
        /// Scale
        scale: Vec3,
    },
    /// Compound shape (multiple shapes combined)
    Compound {
        /// Child shapes with local transforms
        children: Vec<(ColliderShape, Vec3, Vec3)>, // (shape, position, rotation)
    },
}

impl ColliderShape {
    /// Create a sphere collider
    pub fn sphere(radius: f32) -> Self {
        Self::Sphere { radius: radius.max(0.001) }
    }

    /// Create a box collider
    pub fn cube(half_size: f32) -> Self {
        Self::Box {
            half_extents: Vec3::new(half_size, half_size, half_size),
        }
    }

    /// Create a box collider with different dimensions
    pub fn cuboid(half_width: f32, half_height: f32, half_depth: f32) -> Self {
        Self::Box {
            half_extents: Vec3::new(
                half_width.max(0.001),
                half_height.max(0.001),
                half_depth.max(0.001),
            ),
        }
    }

    /// Create a box from half-extents vector
    pub fn box_from_extents(half_extents: Vec3) -> Self {
        Self::Box { half_extents }
    }

    /// Create a capsule collider (vertical by default)
    pub fn capsule(half_height: f32, radius: f32) -> Self {
        Self::Capsule {
            half_height: half_height.max(0.0),
            radius: radius.max(0.001),
        }
    }

    /// Create a cylinder collider
    pub fn cylinder(half_height: f32, radius: f32) -> Self {
        Self::Cylinder {
            half_height: half_height.max(0.001),
            radius: radius.max(0.001),
        }
    }

    /// Create a cone collider
    pub fn cone(half_height: f32, radius: f32) -> Self {
        Self::Cone {
            half_height: half_height.max(0.001),
            radius: radius.max(0.001),
        }
    }

    /// Create a convex hull from points
    pub fn convex_hull(points: Vec<Vec3>) -> Self {
        Self::ConvexHull { points }
    }

    /// Create a triangle mesh collider
    pub fn trimesh(geometry: GeometryHandle) -> Self {
        Self::TriMesh { geometry }
    }

    /// Create a heightfield collider
    pub fn heightfield(width: u32, height: u32, heights: Vec<f32>, scale: Vec3) -> Self {
        Self::Heightfield {
            width,
            height,
            heights,
            scale,
        }
    }

    /// Create a compound shape
    pub fn compound(children: Vec<(ColliderShape, Vec3, Vec3)>) -> Self {
        Self::Compound { children }
    }

    /// Compute approximate bounding sphere radius
    pub fn bounding_radius(&self) -> f32 {
        match self {
            Self::Sphere { radius } => *radius,
            Self::Box { half_extents } => {
                let hx = half_extents.x;
                let hy = half_extents.y;
                let hz = half_extents.z;
                (hx * hx + hy * hy + hz * hz).sqrt()
            }
            Self::Capsule { half_height, radius } => half_height + radius,
            Self::Cylinder { half_height, radius } => {
                (half_height * half_height + radius * radius).sqrt()
            }
            Self::Cone { half_height, radius } => {
                (half_height * half_height + radius * radius).sqrt()
            }
            Self::ConvexHull { points } => {
                points.iter()
                    .map(|p| (p.x * p.x + p.y * p.y + p.z * p.z).sqrt())
                    .fold(0.0f32, |a, b| a.max(b))
            }
            Self::TriMesh { .. } => 1.0, // Unknown without geometry data
            Self::Heightfield { scale, .. } => {
                let sx = scale.x;
                let sy = scale.y;
                let sz = scale.z;
                (sx * sx + sy * sy + sz * sz).sqrt()
            }
            Self::Compound { children } => {
                children.iter()
                    .map(|(shape, pos, _)| {
                        let plen = (pos.x * pos.x + pos.y * pos.y + pos.z * pos.z).sqrt();
                        shape.bounding_radius() + plen
                    })
                    .fold(0.0f32, |a, b| a.max(b))
            }
        }
    }
}

/// Collider component
///
/// Defines collision shape and physics material.
#[derive(Clone, Debug)]
pub struct Collider {
    /// Collision shape
    pub shape: ColliderShape,
    /// Friction coefficient (0-1)
    pub friction: f32,
    /// Restitution/bounciness (0-1)
    pub restitution: f32,
    /// Density for mass calculation
    pub density: f32,
    /// Whether this is a sensor (trigger)
    pub is_sensor: bool,
    /// Collision group (for filtering)
    pub collision_group: u32,
    /// Collision mask (what groups to collide with)
    pub collision_mask: u32,
    /// Local offset from entity position
    pub offset: Vec3,
    /// Local rotation offset (euler angles)
    pub rotation: Vec3,
}

impl Component for Collider {}

impl Default for Collider {
    fn default() -> Self {
        Self {
            shape: ColliderShape::sphere(0.5),
            friction: 0.5,
            restitution: 0.0,
            density: 1.0,
            is_sensor: false,
            collision_group: 0xFFFFFFFF,
            collision_mask: 0xFFFFFFFF,
            offset: Vec3::ZERO,
            rotation: Vec3::ZERO,
        }
    }
}

impl Collider {
    /// Create a sphere collider
    pub fn sphere(radius: f32) -> Self {
        Self {
            shape: ColliderShape::sphere(radius),
            ..Default::default()
        }
    }

    /// Create a box collider
    pub fn cube(half_size: f32) -> Self {
        Self {
            shape: ColliderShape::cube(half_size),
            ..Default::default()
        }
    }

    /// Create a box collider with different dimensions
    pub fn cuboid(half_width: f32, half_height: f32, half_depth: f32) -> Self {
        Self {
            shape: ColliderShape::cuboid(half_width, half_height, half_depth),
            ..Default::default()
        }
    }

    /// Create a capsule collider
    pub fn capsule(half_height: f32, radius: f32) -> Self {
        Self {
            shape: ColliderShape::capsule(half_height, radius),
            ..Default::default()
        }
    }

    /// Create a cylinder collider
    pub fn cylinder(half_height: f32, radius: f32) -> Self {
        Self {
            shape: ColliderShape::cylinder(half_height, radius),
            ..Default::default()
        }
    }

    /// Create a triangle mesh collider
    pub fn trimesh(geometry: GeometryHandle) -> Self {
        Self {
            shape: ColliderShape::trimesh(geometry),
            ..Default::default()
        }
    }

    /// Create from any shape
    pub fn from_shape(shape: ColliderShape) -> Self {
        Self {
            shape,
            ..Default::default()
        }
    }

    /// Set friction
    pub fn with_friction(mut self, friction: f32) -> Self {
        self.friction = friction.clamp(0.0, 1.0);
        self
    }

    /// Set restitution (bounciness)
    pub fn with_restitution(mut self, restitution: f32) -> Self {
        self.restitution = restitution.clamp(0.0, 1.0);
        self
    }

    /// Set density
    pub fn with_density(mut self, density: f32) -> Self {
        self.density = density.max(0.001);
        self
    }

    /// Make this a sensor (trigger)
    pub fn as_sensor(mut self) -> Self {
        self.is_sensor = true;
        self
    }

    /// Set collision group
    pub fn with_collision_group(mut self, group: u32) -> Self {
        self.collision_group = group;
        self
    }

    /// Set collision mask
    pub fn with_collision_mask(mut self, mask: u32) -> Self {
        self.collision_mask = mask;
        self
    }

    /// Set local offset
    pub fn with_offset(mut self, offset: Vec3) -> Self {
        self.offset = offset;
        self
    }

    /// Set local rotation
    pub fn with_rotation(mut self, rotation: Vec3) -> Self {
        self.rotation = rotation;
        self
    }

    // ========== Material Presets ==========

    /// Bouncy material (rubber ball)
    pub fn bouncy(mut self) -> Self {
        self.friction = 0.8;
        self.restitution = 0.9;
        self
    }

    /// Ice/slippery material
    pub fn slippery(mut self) -> Self {
        self.friction = 0.05;
        self.restitution = 0.1;
        self
    }

    /// Metal material
    pub fn metal(mut self) -> Self {
        self.friction = 0.3;
        self.restitution = 0.3;
        self.density = 7.8;
        self
    }

    /// Wood material
    pub fn wood(mut self) -> Self {
        self.friction = 0.5;
        self.restitution = 0.2;
        self.density = 0.6;
        self
    }

    /// Concrete/stone material
    pub fn stone(mut self) -> Self {
        self.friction = 0.7;
        self.restitution = 0.1;
        self.density = 2.5;
        self
    }
}

/// Collision groups for common use cases
pub mod collision_groups {
    /// Default group (collides with everything)
    pub const DEFAULT: u32 = 1;
    /// Static environment
    pub const ENVIRONMENT: u32 = 2;
    /// Player character
    pub const PLAYER: u32 = 4;
    /// Enemy characters
    pub const ENEMY: u32 = 8;
    /// Projectiles
    pub const PROJECTILE: u32 = 16;
    /// Triggers/sensors
    pub const TRIGGER: u32 = 32;
    /// Debris/particles
    pub const DEBRIS: u32 = 64;
    /// Vehicles
    pub const VEHICLE: u32 = 128;
    /// Everything
    pub const ALL: u32 = 0xFFFFFFFF;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collider_creation() {
        let collider = Collider::sphere(1.0);
        assert!((collider.friction - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_shape_bounding_radius() {
        let sphere = ColliderShape::sphere(2.0);
        assert!((sphere.bounding_radius() - 2.0).abs() < 0.001);

        let cube = ColliderShape::cube(1.0);
        let expected = (3.0f32).sqrt(); // sqrt(1^2 + 1^2 + 1^2)
        assert!((cube.bounding_radius() - expected).abs() < 0.001);
    }

    #[test]
    fn test_material_presets() {
        let bouncy = Collider::sphere(1.0).bouncy();
        assert!(bouncy.restitution > 0.8);

        let slippery = Collider::cube(1.0).slippery();
        assert!(slippery.friction < 0.1);
    }
}
