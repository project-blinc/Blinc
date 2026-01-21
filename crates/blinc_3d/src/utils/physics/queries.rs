//! Physics query types (raycast, shapecast, overlap)

use crate::ecs::Entity;
use blinc_core::Vec3;

/// Ray for raycasting
#[derive(Clone, Debug)]
pub struct Ray {
    /// Ray origin
    pub origin: Vec3,
    /// Ray direction (should be normalized)
    pub direction: Vec3,
}

impl Ray {
    /// Create a new ray
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self { origin, direction }
    }

    /// Create a ray from two points
    pub fn from_points(from: Vec3, to: Vec3) -> Self {
        let dir = Vec3::new(
            to.x - from.x,
            to.y - from.y,
            to.z - from.z,
        );
        let len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt();
        let direction = if len > 0.0001 {
            Vec3::new(dir.x / len, dir.y / len, dir.z / len)
        } else {
            Vec3::new(0.0, 0.0, 1.0)
        };
        Self {
            origin: from,
            direction,
        }
    }

    /// Get point along ray at distance t
    pub fn point_at(&self, t: f32) -> Vec3 {
        Vec3::new(
            self.origin.x + self.direction.x * t,
            self.origin.y + self.direction.y * t,
            self.origin.z + self.direction.z * t,
        )
    }
}

/// Raycast hit result
#[derive(Clone, Debug)]
pub struct RaycastHit {
    /// Entity that was hit
    pub entity: Entity,
    /// Hit position in world space
    pub position: Vec3,
    /// Surface normal at hit point
    pub normal: Vec3,
    /// Distance from ray origin
    pub distance: f32,
    /// UV coordinates on hit surface (if available)
    pub uv: Option<(f32, f32)>,
    /// Triangle index (for mesh colliders)
    pub triangle_index: Option<u32>,
}

impl RaycastHit {
    /// Create a new raycast hit
    pub fn new(entity: Entity, position: Vec3, normal: Vec3, distance: f32) -> Self {
        Self {
            entity,
            position,
            normal,
            distance,
            uv: None,
            triangle_index: None,
        }
    }
}

/// Shapecast hit result
#[derive(Clone, Debug)]
pub struct ShapecastHit {
    /// Entity that was hit
    pub entity: Entity,
    /// Hit position in world space
    pub position: Vec3,
    /// Surface normal at hit point
    pub normal: Vec3,
    /// Time of impact (0-1, 0 = at origin, 1 = at max_dist)
    pub time_of_impact: f32,
    /// Witness point on the cast shape
    pub witness_point_a: Vec3,
    /// Witness point on the hit shape
    pub witness_point_b: Vec3,
}

/// Query filter for physics queries
#[derive(Clone, Debug, Default)]
pub struct QueryFilter {
    /// Collision groups to include
    pub groups: Option<u32>,
    /// Collision groups to exclude
    pub exclude_groups: Option<u32>,
    /// Specific entities to exclude
    pub exclude_entities: Vec<Entity>,
    /// Only return first hit (more efficient)
    pub first_only: bool,
    /// Include sensors/triggers
    pub include_sensors: bool,
}

impl QueryFilter {
    /// Create a default filter (all groups, first hit only)
    pub fn new() -> Self {
        Self {
            groups: None,
            exclude_groups: None,
            exclude_entities: Vec::new(),
            first_only: true,
            include_sensors: false,
        }
    }

    /// Filter by collision group
    pub fn with_groups(mut self, groups: u32) -> Self {
        self.groups = Some(groups);
        self
    }

    /// Exclude collision groups
    pub fn exclude_groups(mut self, groups: u32) -> Self {
        self.exclude_groups = Some(groups);
        self
    }

    /// Exclude specific entity
    pub fn exclude_entity(mut self, entity: Entity) -> Self {
        self.exclude_entities.push(entity);
        self
    }

    /// Exclude multiple entities
    pub fn exclude_entities(mut self, entities: &[Entity]) -> Self {
        self.exclude_entities.extend_from_slice(entities);
        self
    }

    /// Get all hits (not just first)
    pub fn all_hits(mut self) -> Self {
        self.first_only = false;
        self
    }

    /// Include sensor/trigger colliders
    pub fn with_sensors(mut self) -> Self {
        self.include_sensors = true;
        self
    }

    /// Check if entity passes filter
    pub fn passes(&self, entity: Entity, collision_group: u32, is_sensor: bool) -> bool {
        // Check excluded entities
        if self.exclude_entities.contains(&entity) {
            return false;
        }

        // Check sensors
        if is_sensor && !self.include_sensors {
            return false;
        }

        // Check groups
        if let Some(groups) = self.groups {
            if collision_group & groups == 0 {
                return false;
            }
        }

        // Check excluded groups
        if let Some(exclude) = self.exclude_groups {
            if collision_group & exclude != 0 {
                return false;
            }
        }

        true
    }
}

/// Overlap test result
#[derive(Clone, Debug)]
pub struct OverlapResult {
    /// Overlapping entity
    pub entity: Entity,
    /// Penetration depth (if available)
    pub penetration: Option<f32>,
    /// Penetration direction (if available)
    pub direction: Option<Vec3>,
}

/// Query presets for common use cases
pub mod query_presets {
    use super::*;
    use super::super::collider::collision_groups;

    /// Query for ground/floor detection
    pub fn ground_check() -> QueryFilter {
        QueryFilter::new()
            .with_groups(collision_groups::ENVIRONMENT)
    }

    /// Query for player interactions
    pub fn player_interaction() -> QueryFilter {
        QueryFilter::new()
            .with_groups(collision_groups::PLAYER | collision_groups::ENEMY)
    }

    /// Query for projectile hits
    pub fn projectile_hit() -> QueryFilter {
        QueryFilter::new()
            .with_groups(collision_groups::PLAYER | collision_groups::ENEMY | collision_groups::ENVIRONMENT)
    }

    /// Query for all physics objects
    pub fn all_physics() -> QueryFilter {
        QueryFilter::new()
            .with_groups(collision_groups::ALL)
    }

    /// Query for triggers/sensors
    pub fn triggers_only() -> QueryFilter {
        QueryFilter::new()
            .with_groups(collision_groups::TRIGGER)
            .with_sensors()
    }
}

/// Closest point query result
#[derive(Clone, Debug)]
pub struct ClosestPointResult {
    /// Entity with closest point
    pub entity: Entity,
    /// Closest point on the collider
    pub point: Vec3,
    /// Distance from query point
    pub distance: f32,
}

/// Contact manifold (detailed collision info)
#[derive(Clone, Debug)]
pub struct ContactManifold {
    /// Entity A
    pub entity_a: Entity,
    /// Entity B
    pub entity_b: Entity,
    /// Contact normal (A to B)
    pub normal: Vec3,
    /// Contact points
    pub points: Vec<ManifoldPoint>,
}

/// Single contact point in a manifold
#[derive(Clone, Debug)]
pub struct ManifoldPoint {
    /// Local point on A
    pub local_point_a: Vec3,
    /// Local point on B
    pub local_point_b: Vec3,
    /// World position
    pub world_point: Vec3,
    /// Penetration depth (positive = penetrating)
    pub penetration: f32,
    /// Normal impulse (after solving)
    pub normal_impulse: f32,
    /// Tangent impulse (after solving)
    pub tangent_impulse: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_creation() {
        let ray = Ray::new(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 1.0),
        );
        let point = ray.point_at(5.0);
        assert!((point.z - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_ray_from_points() {
        let ray = Ray::from_points(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 10.0),
        );
        assert!((ray.direction.z - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_query_filter() {
        use crate::ecs::Entity;
        use slotmap::KeyData;

        let entity = Entity::from(KeyData::from_ffi(1));
        let filter = QueryFilter::new()
            .exclude_entity(entity);

        assert!(!filter.passes(entity, 0xFFFFFFFF, false));
    }
}
