//! Parry collision-only backend
//!
//! Lightweight collision detection using Parry (no dynamics simulation).
//! Useful for trigger detection, raycasting, and overlap tests.

use super::*;
use blinc_core::Vec3;
use std::collections::HashMap;

#[cfg(feature = "utils-parry")]
use parry3d::prelude::*;

/// Parry collision backend (collision detection only, no dynamics)
pub struct ParryBackend {
    /// Collider shapes and transforms
    #[cfg(feature = "utils-parry")]
    shapes: HashMap<u64, (SharedShape, Isometry<f32>)>,
    /// Entity mapping
    shape_to_entity: HashMap<u64, Entity>,
    /// Collision events buffer
    collision_events: Vec<CollisionEvent>,
    /// Next handle ID
    next_handle: u64,
}

impl ParryBackend {
    /// Create a new Parry backend
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "utils-parry")]
            shapes: HashMap::new(),
            shape_to_entity: HashMap::new(),
            collision_events: Vec::new(),
            next_handle: 1,
        }
    }
}

impl Default for ParryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "utils-parry")]
impl PhysicsBackend for ParryBackend {
    fn init(&mut self, _config: &PhysicsConfig) {
        // Parry doesn't need initialization for collision-only
    }

    fn step(&mut self, _dt: f32) {
        // No dynamics to step - just check for collisions
        self.collision_events.clear();

        // Perform broad-phase collision detection
        let handles: Vec<u64> = self.shapes.keys().copied().collect();

        for i in 0..handles.len() {
            for j in (i + 1)..handles.len() {
                let handle_a = handles[i];
                let handle_b = handles[j];

                if let (Some((shape_a, iso_a)), Some((shape_b, iso_b))) =
                    (self.shapes.get(&handle_a), self.shapes.get(&handle_b))
                {
                    // Check intersection
                    if parry3d::query::intersection_test(iso_a, shape_a.as_ref(), iso_b, shape_b.as_ref()).unwrap_or(false) {
                        if let (Some(&entity_a), Some(&entity_b)) =
                            (self.shape_to_entity.get(&handle_a), self.shape_to_entity.get(&handle_b))
                        {
                            // Get contact points
                            let contacts = if let Ok(Some(contact)) = parry3d::query::contact(
                                iso_a,
                                shape_a.as_ref(),
                                iso_b,
                                shape_b.as_ref(),
                                0.01,
                            ) {
                                vec![ContactPoint {
                                    position: Vec3::new(
                                        contact.point1.x,
                                        contact.point1.y,
                                        contact.point1.z,
                                    ),
                                    normal: Vec3::new(
                                        contact.normal1.x,
                                        contact.normal1.y,
                                        contact.normal1.z,
                                    ),
                                    depth: contact.dist.abs(),
                                }]
                            } else {
                                Vec::new()
                            };

                            self.collision_events.push(CollisionEvent {
                                entity_a,
                                entity_b,
                                event_type: CollisionEventType::Ongoing,
                                contacts,
                            });
                        }
                    }
                }
            }
        }
    }

    fn add_rigid_body(&mut self, entity: Entity, _body: &super::RigidBody, position: Vec3) -> super::RigidBodyHandle {
        // Create a placeholder - actual shape added via add_collider
        let handle = self.next_handle;
        self.next_handle += 1;

        // Store entity mapping
        self.shape_to_entity.insert(handle, entity);

        // Add default sphere shape at position
        let iso = Isometry::translation(position.x, position.y, position.z);
        self.shapes.insert(handle, (SharedShape::ball(0.5), iso));

        super::RigidBodyHandle(handle)
    }

    fn remove_rigid_body(&mut self, handle: super::RigidBodyHandle) {
        self.shapes.remove(&handle.0);
        self.shape_to_entity.remove(&handle.0);
    }

    fn add_collider(&mut self, body_handle: super::RigidBodyHandle, collider: &super::Collider) -> super::ColliderHandle {
        let shape: SharedShape = match &collider.shape {
            super::ColliderShape::Sphere { radius } => SharedShape::ball(*radius),
            super::ColliderShape::Box { half_extents } => {
                SharedShape::cuboid(half_extents.x, half_extents.y, half_extents.z)
            }
            super::ColliderShape::Capsule { half_height, radius } => {
                SharedShape::capsule_y(*half_height, *radius)
            }
            super::ColliderShape::Cylinder { half_height, radius } => {
                SharedShape::cylinder(*half_height, *radius)
            }
            super::ColliderShape::Cone { half_height, radius } => {
                SharedShape::cone(*half_height, *radius)
            }
            super::ColliderShape::ConvexHull { points } => {
                let parry_points: Vec<Point<f32>> = points
                    .iter()
                    .map(|p| Point::new(p.x, p.y, p.z))
                    .collect();
                SharedShape::convex_hull(&parry_points).unwrap_or_else(|| SharedShape::ball(0.1))
            }
            _ => SharedShape::ball(0.5),
        };

        // Update the shape for this body
        if let Some((_, iso)) = self.shapes.get(&body_handle.0) {
            let new_iso = Isometry::from_parts(
                Translation::new(
                    iso.translation.x + collider.offset.x,
                    iso.translation.y + collider.offset.y,
                    iso.translation.z + collider.offset.z,
                ),
                *iso.rotation.quaternion(),
            );
            self.shapes.insert(body_handle.0, (shape, new_iso));
        }

        super::ColliderHandle(body_handle.0)
    }

    fn remove_collider(&mut self, handle: super::ColliderHandle) {
        self.shapes.remove(&handle.0);
    }

    fn get_position(&self, handle: super::RigidBodyHandle) -> Option<Vec3> {
        self.shapes.get(&handle.0).map(|(_, iso)| {
            Vec3::new(iso.translation.x, iso.translation.y, iso.translation.z)
        })
    }

    fn get_rotation(&self, handle: super::RigidBodyHandle) -> Option<Vec3> {
        self.shapes.get(&handle.0).map(|(_, iso)| {
            let (roll, pitch, yaw) = iso.rotation.euler_angles();
            Vec3::new(roll, pitch, yaw)
        })
    }

    fn get_linear_velocity(&self, _handle: super::RigidBodyHandle) -> Option<Vec3> {
        // No velocity tracking in collision-only mode
        None
    }

    fn get_angular_velocity(&self, _handle: super::RigidBodyHandle) -> Option<Vec3> {
        None
    }

    fn set_position(&mut self, handle: super::RigidBodyHandle, position: Vec3) {
        if let Some((shape, iso)) = self.shapes.get(&handle.0) {
            let new_iso = Isometry::from_parts(
                Translation::new(position.x, position.y, position.z),
                *iso.rotation.quaternion(),
            );
            self.shapes.insert(handle.0, (shape.clone(), new_iso));
        }
    }

    fn set_rotation(&mut self, handle: super::RigidBodyHandle, rotation: Vec3) {
        if let Some((shape, iso)) = self.shapes.get(&handle.0) {
            let new_iso = Isometry::from_parts(
                iso.translation,
                UnitQuaternion::from_euler_angles(rotation.x, rotation.y, rotation.z),
            );
            self.shapes.insert(handle.0, (shape.clone(), new_iso));
        }
    }

    fn set_linear_velocity(&mut self, _handle: super::RigidBodyHandle, _velocity: Vec3) {
        // No velocity in collision-only mode
    }

    fn set_angular_velocity(&mut self, _handle: super::RigidBodyHandle, _velocity: Vec3) {
        // No velocity in collision-only mode
    }

    fn apply_force(&mut self, _handle: super::RigidBodyHandle, _force: Vec3) {
        // No forces in collision-only mode
    }

    fn apply_force_at_point(&mut self, _handle: super::RigidBodyHandle, _force: Vec3, _point: Vec3) {}

    fn apply_impulse(&mut self, _handle: super::RigidBodyHandle, _impulse: Vec3) {}

    fn apply_impulse_at_point(&mut self, _handle: super::RigidBodyHandle, _impulse: Vec3, _point: Vec3) {}

    fn apply_torque(&mut self, _handle: super::RigidBodyHandle, _torque: Vec3) {}

    fn apply_torque_impulse(&mut self, _handle: super::RigidBodyHandle, _impulse: Vec3) {}

    fn raycast(&self, ray: &super::Ray, max_dist: f32, _filter: super::QueryFilter) -> Option<super::RaycastHit> {
        let parry_ray = parry3d::query::Ray::new(
            Point::new(ray.origin.x, ray.origin.y, ray.origin.z),
            parry3d::na::Vector3::new(ray.direction.x, ray.direction.y, ray.direction.z),
        );

        let mut closest: Option<(f32, u64, Point<f32>)> = None;

        for (&handle, (shape, iso)) in &self.shapes {
            if let Some(toi) = shape.cast_ray(iso, &parry_ray, max_dist, true) {
                if closest.is_none() || toi < closest.as_ref().unwrap().0 {
                    let hit_point = parry_ray.point_at(toi);
                    closest = Some((toi, handle, hit_point));
                }
            }
        }

        closest.map(|(toi, handle, point)| {
            let entity = self.shape_to_entity.get(&handle).copied()
                .unwrap_or_else(|| Entity::from(slotmap::KeyData::from_ffi(0)));

            super::RaycastHit {
                entity,
                position: Vec3::new(point.x, point.y, point.z),
                normal: Vec3::new(0.0, 1.0, 0.0), // Would need to compute actual normal
                distance: toi,
                uv: None,
                triangle_index: None,
            }
        })
    }

    fn raycast_all(&self, ray: &super::Ray, max_dist: f32, _filter: super::QueryFilter) -> Vec<super::RaycastHit> {
        let parry_ray = parry3d::query::Ray::new(
            Point::new(ray.origin.x, ray.origin.y, ray.origin.z),
            parry3d::na::Vector3::new(ray.direction.x, ray.direction.y, ray.direction.z),
        );

        let mut hits = Vec::new();

        for (&handle, (shape, iso)) in &self.shapes {
            if let Some(toi) = shape.cast_ray(iso, &parry_ray, max_dist, true) {
                let hit_point = parry_ray.point_at(toi);
                let entity = self.shape_to_entity.get(&handle).copied()
                    .unwrap_or_else(|| Entity::from(slotmap::KeyData::from_ffi(0)));

                hits.push(super::RaycastHit {
                    entity,
                    position: Vec3::new(hit_point.x, hit_point.y, hit_point.z),
                    normal: Vec3::new(0.0, 1.0, 0.0),
                    distance: toi,
                    uv: None,
                    triangle_index: None,
                });
            }
        }

        hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
        hits
    }

    fn shapecast(&self, shape: &super::ColliderShape, origin: Vec3, direction: Vec3, max_dist: f32, _filter: super::QueryFilter) -> Option<super::ShapecastHit> {
        let parry_shape: SharedShape = match shape {
            super::ColliderShape::Sphere { radius } => SharedShape::ball(*radius),
            super::ColliderShape::Box { half_extents } => {
                SharedShape::cuboid(half_extents.x, half_extents.y, half_extents.z)
            }
            _ => SharedShape::ball(0.5),
        };

        let iso = Isometry::translation(origin.x, origin.y, origin.z);
        let vel = parry3d::na::Vector3::new(direction.x, direction.y, direction.z);

        let mut closest: Option<(f32, u64)> = None;

        for (&handle, (target_shape, target_iso)) in &self.shapes {
            if let Ok(Some(toi)) = parry3d::query::time_of_impact(
                &iso,
                &vel,
                parry_shape.as_ref(),
                target_iso,
                &parry3d::na::Vector3::zeros(),
                target_shape.as_ref(),
                max_dist,
                true,
            ) {
                if closest.is_none() || toi.time_of_impact < closest.as_ref().unwrap().0 {
                    closest = Some((toi.time_of_impact, handle));
                }
            }
        }

        closest.map(|(toi, handle)| {
            let entity = self.shape_to_entity.get(&handle).copied()
                .unwrap_or_else(|| Entity::from(slotmap::KeyData::from_ffi(0)));

            super::ShapecastHit {
                entity,
                position: Vec3::new(
                    origin.x + direction.x * toi,
                    origin.y + direction.y * toi,
                    origin.z + direction.z * toi,
                ),
                normal: Vec3::new(0.0, 1.0, 0.0),
                time_of_impact: toi,
                witness_point_a: origin,
                witness_point_b: origin,
            }
        })
    }

    fn overlap(&self, shape: &super::ColliderShape, position: Vec3, _filter: super::QueryFilter) -> Vec<Entity> {
        let parry_shape: SharedShape = match shape {
            super::ColliderShape::Sphere { radius } => SharedShape::ball(*radius),
            super::ColliderShape::Box { half_extents } => {
                SharedShape::cuboid(half_extents.x, half_extents.y, half_extents.z)
            }
            _ => SharedShape::ball(0.5),
        };

        let iso = Isometry::translation(position.x, position.y, position.z);
        let mut overlapping = Vec::new();

        for (&handle, (target_shape, target_iso)) in &self.shapes {
            if parry3d::query::intersection_test(&iso, parry_shape.as_ref(), target_iso, target_shape.as_ref()).unwrap_or(false) {
                if let Some(&entity) = self.shape_to_entity.get(&handle) {
                    overlapping.push(entity);
                }
            }
        }

        overlapping
    }

    fn collision_events(&self) -> &[super::CollisionEvent] {
        &self.collision_events
    }

    fn clear_events(&mut self) {
        self.collision_events.clear();
    }

    fn add_joint(&mut self, _body_a: super::RigidBodyHandle, _body_b: super::RigidBodyHandle, _joint: &super::Joint) -> super::JointHandle {
        // No joints in collision-only mode
        super::JointHandle(0)
    }

    fn remove_joint(&mut self, _handle: super::JointHandle) {}

    fn name(&self) -> &'static str {
        "Parry 3D (collision-only)"
    }
}

// Stub implementation when parry is not enabled
#[cfg(not(feature = "utils-parry"))]
impl PhysicsBackend for ParryBackend {
    fn init(&mut self, _config: &PhysicsConfig) {}
    fn step(&mut self, _dt: f32) {}
    fn add_rigid_body(&mut self, _entity: Entity, _body: &super::RigidBody, _position: Vec3) -> super::RigidBodyHandle {
        let handle = super::RigidBodyHandle(self.next_handle);
        self.next_handle += 1;
        handle
    }
    fn remove_rigid_body(&mut self, _handle: super::RigidBodyHandle) {}
    fn add_collider(&mut self, body_handle: super::RigidBodyHandle, _collider: &super::Collider) -> super::ColliderHandle {
        super::ColliderHandle(body_handle.0)
    }
    fn remove_collider(&mut self, _handle: super::ColliderHandle) {}
    fn get_position(&self, _handle: super::RigidBodyHandle) -> Option<Vec3> { None }
    fn get_rotation(&self, _handle: super::RigidBodyHandle) -> Option<Vec3> { None }
    fn get_linear_velocity(&self, _handle: super::RigidBodyHandle) -> Option<Vec3> { None }
    fn get_angular_velocity(&self, _handle: super::RigidBodyHandle) -> Option<Vec3> { None }
    fn set_position(&mut self, _handle: super::RigidBodyHandle, _position: Vec3) {}
    fn set_rotation(&mut self, _handle: super::RigidBodyHandle, _rotation: Vec3) {}
    fn set_linear_velocity(&mut self, _handle: super::RigidBodyHandle, _velocity: Vec3) {}
    fn set_angular_velocity(&mut self, _handle: super::RigidBodyHandle, _velocity: Vec3) {}
    fn apply_force(&mut self, _handle: super::RigidBodyHandle, _force: Vec3) {}
    fn apply_force_at_point(&mut self, _handle: super::RigidBodyHandle, _force: Vec3, _point: Vec3) {}
    fn apply_impulse(&mut self, _handle: super::RigidBodyHandle, _impulse: Vec3) {}
    fn apply_impulse_at_point(&mut self, _handle: super::RigidBodyHandle, _impulse: Vec3, _point: Vec3) {}
    fn apply_torque(&mut self, _handle: super::RigidBodyHandle, _torque: Vec3) {}
    fn apply_torque_impulse(&mut self, _handle: super::RigidBodyHandle, _impulse: Vec3) {}
    fn raycast(&self, _ray: &super::Ray, _max_dist: f32, _filter: super::QueryFilter) -> Option<super::RaycastHit> { None }
    fn raycast_all(&self, _ray: &super::Ray, _max_dist: f32, _filter: super::QueryFilter) -> Vec<super::RaycastHit> { Vec::new() }
    fn shapecast(&self, _shape: &super::ColliderShape, _origin: Vec3, _direction: Vec3, _max_dist: f32, _filter: super::QueryFilter) -> Option<super::ShapecastHit> { None }
    fn overlap(&self, _shape: &super::ColliderShape, _position: Vec3, _filter: super::QueryFilter) -> Vec<Entity> { Vec::new() }
    fn collision_events(&self) -> &[super::CollisionEvent] { &self.collision_events }
    fn clear_events(&mut self) { self.collision_events.clear(); }
    fn add_joint(&mut self, _body_a: super::RigidBodyHandle, _body_b: super::RigidBodyHandle, _joint: &super::Joint) -> super::JointHandle { super::JointHandle(0) }
    fn remove_joint(&mut self, _handle: super::JointHandle) {}
    fn name(&self) -> &'static str { "Parry 3D (stub)" }
}
