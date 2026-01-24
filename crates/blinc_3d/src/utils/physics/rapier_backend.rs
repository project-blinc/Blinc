//! Rapier physics backend implementation
//!
//! Full physics simulation using the Rapier physics engine.

use super::{
    Collider, ColliderShape, Entity, Joint, PhysicsBackend, PhysicsConfig, QueryFilter, Ray,
    RaycastHit, RigidBody, RigidBodyType,
};
use blinc_core::Vec3;
use std::collections::HashMap;

// Use type aliases to avoid ambiguity with rapier types
pub use super::ColliderHandle as BlincColliderHandle;
pub use super::CollisionEvent as BlincCollisionEvent;
pub use super::JointHandle as BlincJointHandle;
pub use super::RigidBodyHandle as BlincRigidBodyHandle;

#[cfg(feature = "utils-rapier")]
use rapier3d::prelude::*;

/// Rapier physics backend
pub struct RapierBackend {
    #[cfg(feature = "utils-rapier")]
    rigid_body_set: RigidBodySet,
    #[cfg(feature = "utils-rapier")]
    collider_set: ColliderSet,
    #[cfg(feature = "utils-rapier")]
    integration_parameters: IntegrationParameters,
    #[cfg(feature = "utils-rapier")]
    physics_pipeline: PhysicsPipeline,
    #[cfg(feature = "utils-rapier")]
    island_manager: IslandManager,
    #[cfg(feature = "utils-rapier")]
    broad_phase: DefaultBroadPhase,
    #[cfg(feature = "utils-rapier")]
    narrow_phase: NarrowPhase,
    #[cfg(feature = "utils-rapier")]
    impulse_joint_set: ImpulseJointSet,
    #[cfg(feature = "utils-rapier")]
    multibody_joint_set: MultibodyJointSet,
    #[cfg(feature = "utils-rapier")]
    ccd_solver: CCDSolver,
    #[cfg(feature = "utils-rapier")]
    query_pipeline: QueryPipeline,
    #[cfg(feature = "utils-rapier")]
    gravity: Vector<Real>,

    /// Handle mapping
    #[cfg(feature = "utils-rapier")]
    handle_to_body: HashMap<BlincRigidBodyHandle, rapier3d::prelude::RigidBodyHandle>,
    #[cfg(feature = "utils-rapier")]
    body_to_handle: HashMap<rapier3d::prelude::RigidBodyHandle, BlincRigidBodyHandle>,
    #[cfg(feature = "utils-rapier")]
    handle_to_collider: HashMap<BlincColliderHandle, rapier3d::prelude::ColliderHandle>,
    #[cfg(feature = "utils-rapier")]
    body_to_entity: HashMap<rapier3d::prelude::RigidBodyHandle, Entity>,

    /// Next handle IDs
    next_body_handle: u64,
    next_collider_handle: u64,
    next_joint_handle: u64,

    /// Collision events buffer
    collision_events: Vec<BlincCollisionEvent>,
}

impl RapierBackend {
    /// Create a new Rapier backend
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "utils-rapier")]
            rigid_body_set: RigidBodySet::new(),
            #[cfg(feature = "utils-rapier")]
            collider_set: ColliderSet::new(),
            #[cfg(feature = "utils-rapier")]
            integration_parameters: IntegrationParameters::default(),
            #[cfg(feature = "utils-rapier")]
            physics_pipeline: PhysicsPipeline::new(),
            #[cfg(feature = "utils-rapier")]
            island_manager: IslandManager::new(),
            #[cfg(feature = "utils-rapier")]
            broad_phase: DefaultBroadPhase::new(),
            #[cfg(feature = "utils-rapier")]
            narrow_phase: NarrowPhase::new(),
            #[cfg(feature = "utils-rapier")]
            impulse_joint_set: ImpulseJointSet::new(),
            #[cfg(feature = "utils-rapier")]
            multibody_joint_set: MultibodyJointSet::new(),
            #[cfg(feature = "utils-rapier")]
            ccd_solver: CCDSolver::new(),
            #[cfg(feature = "utils-rapier")]
            query_pipeline: QueryPipeline::new(),
            #[cfg(feature = "utils-rapier")]
            gravity: vector![0.0, -9.81, 0.0],
            #[cfg(feature = "utils-rapier")]
            handle_to_body: HashMap::new(),
            #[cfg(feature = "utils-rapier")]
            body_to_handle: HashMap::new(),
            #[cfg(feature = "utils-rapier")]
            handle_to_collider: HashMap::new(),
            #[cfg(feature = "utils-rapier")]
            body_to_entity: HashMap::new(),
            next_body_handle: 1,
            next_collider_handle: 1,
            next_joint_handle: 1,
            collision_events: Vec::new(),
        }
    }
}

impl Default for RapierBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "utils-rapier")]
impl PhysicsBackend for RapierBackend {
    fn init(&mut self, config: &PhysicsConfig) {
        self.gravity = vector![config.gravity.x, config.gravity.y, config.gravity.z];
        self.integration_parameters.dt = config.timestep;
        if let Some(iterations) = std::num::NonZeroUsize::new(config.solver_iterations as usize) {
            self.integration_parameters.num_solver_iterations = iterations;
        }
    }

    fn step(&mut self, dt: f32) {
        self.integration_parameters.dt = dt;

        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            Some(&mut self.query_pipeline),
            &(),
            &(),
        );

        // Collect collision events
        self.collision_events.clear();
        // Note: Would need event handler integration for collision events
    }

    fn add_rigid_body(&mut self, entity: Entity, body: &super::RigidBody, position: Vec3) -> super::RigidBodyHandle {
        let body_type = match body.body_type {
            super::RigidBodyType::Dynamic => rapier3d::prelude::RigidBodyType::Dynamic,
            super::RigidBodyType::Kinematic => rapier3d::prelude::RigidBodyType::KinematicPositionBased,
            super::RigidBodyType::Static => rapier3d::prelude::RigidBodyType::Fixed,
        };

        let mut builder = RigidBodyBuilder::new(body_type)
            .translation(vector![position.x, position.y, position.z])
            .linear_damping(body.linear_damping)
            .angular_damping(body.angular_damping)
            .gravity_scale(body.gravity_scale)
            .can_sleep(body.can_sleep)
            .ccd_enabled(body.ccd_enabled)
            .linvel(vector![
                body.initial_velocity.x,
                body.initial_velocity.y,
                body.initial_velocity.z
            ])
            .angvel(vector![
                body.initial_angular_velocity.x,
                body.initial_angular_velocity.y,
                body.initial_angular_velocity.z
            ]);

        // Apply axis locks
        if body.lock_position.any() || body.lock_rotation.any() {
            builder = builder.locked_axes(
                if body.lock_position.x { LockedAxes::TRANSLATION_LOCKED_X } else { LockedAxes::empty() }
                | if body.lock_position.y { LockedAxes::TRANSLATION_LOCKED_Y } else { LockedAxes::empty() }
                | if body.lock_position.z { LockedAxes::TRANSLATION_LOCKED_Z } else { LockedAxes::empty() }
                | if body.lock_rotation.x { LockedAxes::ROTATION_LOCKED_X } else { LockedAxes::empty() }
                | if body.lock_rotation.y { LockedAxes::ROTATION_LOCKED_Y } else { LockedAxes::empty() }
                | if body.lock_rotation.z { LockedAxes::ROTATION_LOCKED_Z } else { LockedAxes::empty() }
            );
        }

        let rapier_handle = self.rigid_body_set.insert(builder.build());

        let handle = super::RigidBodyHandle(self.next_body_handle);
        self.next_body_handle += 1;

        self.handle_to_body.insert(handle, rapier_handle);
        self.body_to_handle.insert(rapier_handle, handle);
        self.body_to_entity.insert(rapier_handle, entity);

        handle
    }

    fn remove_rigid_body(&mut self, handle: super::RigidBodyHandle) {
        if let Some(rapier_handle) = self.handle_to_body.remove(&handle) {
            self.body_to_handle.remove(&rapier_handle);
            self.body_to_entity.remove(&rapier_handle);
            self.rigid_body_set.remove(
                rapier_handle,
                &mut self.island_manager,
                &mut self.collider_set,
                &mut self.impulse_joint_set,
                &mut self.multibody_joint_set,
                true,
            );
        }
    }

    fn add_collider(&mut self, body_handle: super::RigidBodyHandle, collider: &super::Collider) -> super::ColliderHandle {
        let rapier_body_handle = self.handle_to_body.get(&body_handle).copied();

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
                let rapier_points: Vec<Point<Real>> = points
                    .iter()
                    .map(|p| point![p.x, p.y, p.z])
                    .collect();
                SharedShape::convex_hull(&rapier_points).unwrap_or_else(|| SharedShape::ball(0.1))
            }
            _ => SharedShape::ball(0.5), // Fallback for unsupported shapes
        };

        let mut builder = ColliderBuilder::new(shape)
            .friction(collider.friction)
            .restitution(collider.restitution)
            .density(collider.density)
            .sensor(collider.is_sensor)
            .translation(vector![collider.offset.x, collider.offset.y, collider.offset.z]);

        if collider.collision_group != 0xFFFFFFFF || collider.collision_mask != 0xFFFFFFFF {
            builder = builder.collision_groups(InteractionGroups::new(
                Group::from_bits_truncate(collider.collision_group),
                Group::from_bits_truncate(collider.collision_mask),
            ));
        }

        let rapier_collider = builder.build();

        let rapier_handle = if let Some(body) = rapier_body_handle {
            self.collider_set.insert_with_parent(rapier_collider, body, &mut self.rigid_body_set)
        } else {
            self.collider_set.insert(rapier_collider)
        };

        let handle = super::ColliderHandle(self.next_collider_handle);
        self.next_collider_handle += 1;
        self.handle_to_collider.insert(handle, rapier_handle);

        handle
    }

    fn remove_collider(&mut self, handle: super::ColliderHandle) {
        if let Some(rapier_handle) = self.handle_to_collider.remove(&handle) {
            self.collider_set.remove(
                rapier_handle,
                &mut self.island_manager,
                &mut self.rigid_body_set,
                true,
            );
        }
    }

    fn get_position(&self, handle: super::RigidBodyHandle) -> Option<Vec3> {
        self.handle_to_body.get(&handle)
            .and_then(|h| self.rigid_body_set.get(*h))
            .map(|body| {
                let pos = body.translation();
                Vec3::new(pos.x, pos.y, pos.z)
            })
    }

    fn get_rotation(&self, handle: super::RigidBodyHandle) -> Option<Vec3> {
        self.handle_to_body.get(&handle)
            .and_then(|h| self.rigid_body_set.get(*h))
            .map(|body| {
                let rot = body.rotation();
                let (roll, pitch, yaw) = rot.euler_angles();
                Vec3::new(roll, pitch, yaw)
            })
    }

    fn get_linear_velocity(&self, handle: super::RigidBodyHandle) -> Option<Vec3> {
        self.handle_to_body.get(&handle)
            .and_then(|h| self.rigid_body_set.get(*h))
            .map(|body| {
                let vel = body.linvel();
                Vec3::new(vel.x, vel.y, vel.z)
            })
    }

    fn get_angular_velocity(&self, handle: super::RigidBodyHandle) -> Option<Vec3> {
        self.handle_to_body.get(&handle)
            .and_then(|h| self.rigid_body_set.get(*h))
            .map(|body| {
                let vel = body.angvel();
                Vec3::new(vel.x, vel.y, vel.z)
            })
    }

    fn set_position(&mut self, handle: super::RigidBodyHandle, position: Vec3) {
        if let Some(rapier_handle) = self.handle_to_body.get(&handle) {
            if let Some(body) = self.rigid_body_set.get_mut(*rapier_handle) {
                body.set_translation(vector![position.x, position.y, position.z], true);
            }
        }
    }

    fn set_rotation(&mut self, handle: super::RigidBodyHandle, rotation: Vec3) {
        if let Some(rapier_handle) = self.handle_to_body.get(&handle) {
            if let Some(body) = self.rigid_body_set.get_mut(*rapier_handle) {
                let quat = rapier3d::na::UnitQuaternion::from_euler_angles(rotation.x, rotation.y, rotation.z);
                body.set_rotation(quat, true);
            }
        }
    }

    fn set_linear_velocity(&mut self, handle: super::RigidBodyHandle, velocity: Vec3) {
        if let Some(rapier_handle) = self.handle_to_body.get(&handle) {
            if let Some(body) = self.rigid_body_set.get_mut(*rapier_handle) {
                body.set_linvel(vector![velocity.x, velocity.y, velocity.z], true);
            }
        }
    }

    fn set_angular_velocity(&mut self, handle: super::RigidBodyHandle, velocity: Vec3) {
        if let Some(rapier_handle) = self.handle_to_body.get(&handle) {
            if let Some(body) = self.rigid_body_set.get_mut(*rapier_handle) {
                body.set_angvel(vector![velocity.x, velocity.y, velocity.z], true);
            }
        }
    }

    fn apply_force(&mut self, handle: super::RigidBodyHandle, force: Vec3) {
        if let Some(rapier_handle) = self.handle_to_body.get(&handle) {
            if let Some(body) = self.rigid_body_set.get_mut(*rapier_handle) {
                body.add_force(vector![force.x, force.y, force.z], true);
            }
        }
    }

    fn apply_force_at_point(&mut self, handle: super::RigidBodyHandle, force: Vec3, point: Vec3) {
        if let Some(rapier_handle) = self.handle_to_body.get(&handle) {
            if let Some(body) = self.rigid_body_set.get_mut(*rapier_handle) {
                body.add_force_at_point(
                    vector![force.x, force.y, force.z],
                    point![point.x, point.y, point.z],
                    true,
                );
            }
        }
    }

    fn apply_impulse(&mut self, handle: super::RigidBodyHandle, impulse: Vec3) {
        if let Some(rapier_handle) = self.handle_to_body.get(&handle) {
            if let Some(body) = self.rigid_body_set.get_mut(*rapier_handle) {
                body.apply_impulse(vector![impulse.x, impulse.y, impulse.z], true);
            }
        }
    }

    fn apply_impulse_at_point(&mut self, handle: super::RigidBodyHandle, impulse: Vec3, point: Vec3) {
        if let Some(rapier_handle) = self.handle_to_body.get(&handle) {
            if let Some(body) = self.rigid_body_set.get_mut(*rapier_handle) {
                body.apply_impulse_at_point(
                    vector![impulse.x, impulse.y, impulse.z],
                    point![point.x, point.y, point.z],
                    true,
                );
            }
        }
    }

    fn apply_torque(&mut self, handle: super::RigidBodyHandle, torque: Vec3) {
        if let Some(rapier_handle) = self.handle_to_body.get(&handle) {
            if let Some(body) = self.rigid_body_set.get_mut(*rapier_handle) {
                body.add_torque(vector![torque.x, torque.y, torque.z], true);
            }
        }
    }

    fn apply_torque_impulse(&mut self, handle: super::RigidBodyHandle, impulse: Vec3) {
        if let Some(rapier_handle) = self.handle_to_body.get(&handle) {
            if let Some(body) = self.rigid_body_set.get_mut(*rapier_handle) {
                body.apply_torque_impulse(vector![impulse.x, impulse.y, impulse.z], true);
            }
        }
    }

    fn raycast(&self, ray: &super::Ray, max_dist: f32, filter: super::QueryFilter) -> Option<super::RaycastHit> {
        let rapier_ray = rapier3d::prelude::Ray::new(
            point![ray.origin.x, ray.origin.y, ray.origin.z],
            vector![ray.direction.x, ray.direction.y, ray.direction.z],
        );

        let query_filter = rapier3d::prelude::QueryFilter::default();

        self.query_pipeline.cast_ray(
            &self.rigid_body_set,
            &self.collider_set,
            &rapier_ray,
            max_dist,
            true,
            query_filter,
        ).map(|(collider_handle, toi)| {
            let position = ray.point_at(toi);

            // Get entity from collider's parent body
            let entity = self.collider_set.get(collider_handle)
                .and_then(|c| c.parent())
                .and_then(|body_handle| self.body_to_entity.get(&body_handle).copied())
                .unwrap_or_else(|| Entity::from(slotmap::KeyData::from_ffi(0)));

            super::RaycastHit {
                entity,
                position,
                normal: Vec3::new(0.0, 1.0, 0.0), // Would need intersection details for actual normal
                distance: toi,
                uv: None,
                triangle_index: None,
            }
        })
    }

    fn raycast_all(&self, ray: &super::Ray, max_dist: f32, _filter: super::QueryFilter) -> Vec<super::RaycastHit> {
        let rapier_ray = rapier3d::prelude::Ray::new(
            point![ray.origin.x, ray.origin.y, ray.origin.z],
            vector![ray.direction.x, ray.direction.y, ray.direction.z],
        );

        let query_filter = rapier3d::prelude::QueryFilter::default();

        let mut hits = Vec::new();
        self.query_pipeline.intersections_with_ray(
            &self.rigid_body_set,
            &self.collider_set,
            &rapier_ray,
            max_dist,
            true,
            query_filter,
            |collider_handle, intersection| {
                let position = ray.point_at(intersection.time_of_impact);
                let entity = self.collider_set.get(collider_handle)
                    .and_then(|c| c.parent())
                    .and_then(|body_handle| self.body_to_entity.get(&body_handle).copied())
                    .unwrap_or_else(|| Entity::from(slotmap::KeyData::from_ffi(0)));

                hits.push(super::RaycastHit {
                    entity,
                    position,
                    normal: Vec3::new(intersection.normal.x, intersection.normal.y, intersection.normal.z),
                    distance: intersection.time_of_impact,
                    uv: None,
                    triangle_index: None,
                });
                true // Continue searching
            },
        );

        hits
    }

    fn shapecast(&self, _shape: &super::ColliderShape, _origin: Vec3, _direction: Vec3, _max_dist: f32, _filter: super::QueryFilter) -> Option<super::ShapecastHit> {
        // Shape casting requires more complex setup
        None
    }

    fn overlap(&self, _shape: &super::ColliderShape, _position: Vec3, _filter: super::QueryFilter) -> Vec<Entity> {
        Vec::new()
    }

    fn collision_events(&self) -> &[super::CollisionEvent] {
        &self.collision_events
    }

    fn clear_events(&mut self) {
        self.collision_events.clear();
    }

    fn add_joint(&mut self, body_a: super::RigidBodyHandle, body_b: super::RigidBodyHandle, joint: &super::Joint) -> super::JointHandle {
        let rapier_a = self.handle_to_body.get(&body_a).copied();
        let rapier_b = self.handle_to_body.get(&body_b).copied();

        if let (Some(handle_a), Some(handle_b)) = (rapier_a, rapier_b) {
            let _rapier_joint: GenericJoint = match joint {
                super::Joint::Fixed { anchor_a, anchor_b } => {
                    FixedJointBuilder::new()
                        .local_anchor1(point![anchor_a.x, anchor_a.y, anchor_a.z])
                        .local_anchor2(point![anchor_b.x, anchor_b.y, anchor_b.z])
                        .build()
                        .into()
                }
                super::Joint::Ball { anchor_a, anchor_b } => {
                    SphericalJointBuilder::new()
                        .local_anchor1(point![anchor_a.x, anchor_a.y, anchor_a.z])
                        .local_anchor2(point![anchor_b.x, anchor_b.y, anchor_b.z])
                        .build()
                        .into()
                }
                super::Joint::Revolute { anchor_a, anchor_b, axis, .. } => {
                    RevoluteJointBuilder::new(UnitVector::new_normalize(vector![axis.x, axis.y, axis.z]))
                        .local_anchor1(point![anchor_a.x, anchor_a.y, anchor_a.z])
                        .local_anchor2(point![anchor_b.x, anchor_b.y, anchor_b.z])
                        .build()
                        .into()
                }
                super::Joint::Prismatic { anchor_a, anchor_b, axis, .. } => {
                    PrismaticJointBuilder::new(UnitVector::new_normalize(vector![axis.x, axis.y, axis.z]))
                        .local_anchor1(point![anchor_a.x, anchor_a.y, anchor_a.z])
                        .local_anchor2(point![anchor_b.x, anchor_b.y, anchor_b.z])
                        .build()
                        .into()
                }
                _ => {
                    // Fallback to fixed joint
                    FixedJointBuilder::new().build().into()
                }
            };

            self.impulse_joint_set.insert(handle_a, handle_b, _rapier_joint, true);
        }

        let handle = super::JointHandle(self.next_joint_handle);
        self.next_joint_handle += 1;
        handle
    }

    fn remove_joint(&mut self, _handle: super::JointHandle) {
        // Would need joint handle mapping
    }

    fn name(&self) -> &'static str {
        "Rapier 3D"
    }
}

// Stub implementation when rapier is not enabled
#[cfg(not(feature = "utils-rapier"))]
impl PhysicsBackend for RapierBackend {
    fn init(&mut self, _config: &PhysicsConfig) {}
    fn step(&mut self, _dt: f32) {}
    fn add_rigid_body(&mut self, _entity: Entity, _body: &super::RigidBody, _position: Vec3) -> super::RigidBodyHandle {
        let handle = super::RigidBodyHandle(self.next_body_handle);
        self.next_body_handle += 1;
        handle
    }
    fn remove_rigid_body(&mut self, _handle: super::RigidBodyHandle) {}
    fn add_collider(&mut self, _body_handle: super::RigidBodyHandle, _collider: &super::Collider) -> super::ColliderHandle {
        let handle = super::ColliderHandle(self.next_collider_handle);
        self.next_collider_handle += 1;
        handle
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
    fn add_joint(&mut self, _body_a: super::RigidBodyHandle, _body_b: super::RigidBodyHandle, _joint: &super::Joint) -> super::JointHandle {
        let handle = super::JointHandle(self.next_joint_handle);
        self.next_joint_handle += 1;
        handle
    }
    fn remove_joint(&mut self, _handle: super::JointHandle) {}
    fn name(&self) -> &'static str { "Rapier 3D (stub)" }
}
