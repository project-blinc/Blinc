//! Physics system for rigid body simulation
//!
//! Provides physics simulation with support for multiple backends:
//!
//! - **Rapier**: Full physics simulation (rigid bodies, joints, continuous collision)
//! - **Parry**: Collision detection only (no dynamics)
//!
//! # Example
//!
//! ```ignore
//! use blinc_3d::utils::physics::*;
//!
//! // Create a physics world with Rapier backend
//! let mut physics = PhysicsWorld::new(PhysicsConfig::default());
//!
//! // Add a dynamic rigid body
//! let body = RigidBody::dynamic()
//!     .with_mass(1.0)
//!     .with_restitution(0.5);
//!
//! // Add a collider
//! let collider = Collider::sphere(0.5)
//!     .with_friction(0.3);
//! ```

mod rigid_body;
mod collider;
mod joints;
mod queries;

#[cfg(feature = "utils-rapier")]
mod rapier_backend;

#[cfg(feature = "utils-parry")]
mod parry_backend;

pub use rigid_body::*;
pub use collider::*;
pub use joints::*;
pub use queries::*;

use crate::ecs::{Component, System, SystemContext, SystemStage, Entity};
use blinc_core::Vec3;
use std::collections::HashMap;

/// Physics simulation configuration
#[derive(Clone, Debug)]
pub struct PhysicsConfig {
    /// Gravity vector
    pub gravity: Vec3,
    /// Physics timestep (0 = use frame delta)
    pub timestep: f32,
    /// Maximum substeps per frame
    pub max_substeps: u32,
    /// Enable continuous collision detection
    pub ccd_enabled: bool,
    /// Solver iterations
    pub solver_iterations: u32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            timestep: 1.0 / 60.0,
            max_substeps: 4,
            ccd_enabled: true,
            solver_iterations: 4,
        }
    }
}

impl PhysicsConfig {
    /// Create config with custom gravity
    pub fn with_gravity(mut self, gravity: Vec3) -> Self {
        self.gravity = gravity;
        self
    }

    /// Create config with fixed timestep
    pub fn with_timestep(mut self, timestep: f32) -> Self {
        self.timestep = timestep;
        self
    }

    /// Enable or disable CCD
    pub fn with_ccd(mut self, enabled: bool) -> Self {
        self.ccd_enabled = enabled;
        self
    }

    /// Set solver iterations
    pub fn with_solver_iterations(mut self, iterations: u32) -> Self {
        self.solver_iterations = iterations;
        self
    }

    /// Zero gravity preset
    pub fn zero_gravity() -> Self {
        Self::default().with_gravity(Vec3::ZERO)
    }

    /// 2D side-scroller preset
    pub fn sidescroller() -> Self {
        Self::default().with_gravity(Vec3::new(0.0, -20.0, 0.0))
    }

    /// Space/low gravity preset
    pub fn low_gravity() -> Self {
        Self::default().with_gravity(Vec3::new(0.0, -1.62, 0.0)) // Moon gravity
    }
}

/// Physics backend trait
///
/// Implement this trait to add a new physics backend.
pub trait PhysicsBackend: Send + Sync {
    /// Initialize the backend with config
    fn init(&mut self, config: &PhysicsConfig);

    /// Step the simulation
    fn step(&mut self, dt: f32);

    /// Add a rigid body and return its handle
    fn add_rigid_body(&mut self, entity: Entity, body: &RigidBody, position: Vec3) -> RigidBodyHandle;

    /// Remove a rigid body
    fn remove_rigid_body(&mut self, handle: RigidBodyHandle);

    /// Add a collider to a rigid body
    fn add_collider(&mut self, body_handle: RigidBodyHandle, collider: &Collider) -> ColliderHandle;

    /// Remove a collider
    fn remove_collider(&mut self, handle: ColliderHandle);

    /// Get rigid body position
    fn get_position(&self, handle: RigidBodyHandle) -> Option<Vec3>;

    /// Get rigid body rotation (as euler angles)
    fn get_rotation(&self, handle: RigidBodyHandle) -> Option<Vec3>;

    /// Get rigid body linear velocity
    fn get_linear_velocity(&self, handle: RigidBodyHandle) -> Option<Vec3>;

    /// Get rigid body angular velocity
    fn get_angular_velocity(&self, handle: RigidBodyHandle) -> Option<Vec3>;

    /// Set rigid body position
    fn set_position(&mut self, handle: RigidBodyHandle, position: Vec3);

    /// Set rigid body rotation
    fn set_rotation(&mut self, handle: RigidBodyHandle, rotation: Vec3);

    /// Set linear velocity
    fn set_linear_velocity(&mut self, handle: RigidBodyHandle, velocity: Vec3);

    /// Set angular velocity
    fn set_angular_velocity(&mut self, handle: RigidBodyHandle, velocity: Vec3);

    /// Apply force at center of mass
    fn apply_force(&mut self, handle: RigidBodyHandle, force: Vec3);

    /// Apply force at world point
    fn apply_force_at_point(&mut self, handle: RigidBodyHandle, force: Vec3, point: Vec3);

    /// Apply impulse at center of mass
    fn apply_impulse(&mut self, handle: RigidBodyHandle, impulse: Vec3);

    /// Apply impulse at world point
    fn apply_impulse_at_point(&mut self, handle: RigidBodyHandle, impulse: Vec3, point: Vec3);

    /// Apply torque
    fn apply_torque(&mut self, handle: RigidBodyHandle, torque: Vec3);

    /// Apply torque impulse
    fn apply_torque_impulse(&mut self, handle: RigidBodyHandle, impulse: Vec3);

    /// Cast a ray and return the first hit
    fn raycast(&self, ray: &Ray, max_dist: f32, filter: QueryFilter) -> Option<RaycastHit>;

    /// Cast a ray and return all hits
    fn raycast_all(&self, ray: &Ray, max_dist: f32, filter: QueryFilter) -> Vec<RaycastHit>;

    /// Cast a shape and return the first hit
    fn shapecast(&self, shape: &ColliderShape, origin: Vec3, direction: Vec3, max_dist: f32, filter: QueryFilter) -> Option<ShapecastHit>;

    /// Check for overlap with a shape
    fn overlap(&self, shape: &ColliderShape, position: Vec3, filter: QueryFilter) -> Vec<Entity>;

    /// Get collision events since last step
    fn collision_events(&self) -> &[CollisionEvent];

    /// Clear collision events
    fn clear_events(&mut self);

    /// Add a joint constraint
    fn add_joint(&mut self, body_a: RigidBodyHandle, body_b: RigidBodyHandle, joint: &Joint) -> JointHandle;

    /// Remove a joint
    fn remove_joint(&mut self, handle: JointHandle);

    /// Backend name
    fn name(&self) -> &'static str;
}

/// Handle to a rigid body in the physics backend
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RigidBodyHandle(pub(crate) u64);

/// Handle to a collider in the physics backend
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColliderHandle(pub(crate) u64);

/// Handle to a joint in the physics backend
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JointHandle(pub(crate) u64);

/// Collision event
#[derive(Clone, Debug)]
pub struct CollisionEvent {
    /// First entity involved
    pub entity_a: Entity,
    /// Second entity involved
    pub entity_b: Entity,
    /// Collision type
    pub event_type: CollisionEventType,
    /// Contact points (if available)
    pub contacts: Vec<ContactPoint>,
}

/// Type of collision event
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionEventType {
    /// Collision started
    Started,
    /// Collision ongoing
    Ongoing,
    /// Collision ended
    Stopped,
}

/// Contact point information
#[derive(Clone, Debug)]
pub struct ContactPoint {
    /// World position of contact
    pub position: Vec3,
    /// Contact normal (from A to B)
    pub normal: Vec3,
    /// Penetration depth
    pub depth: f32,
}

/// Physics world resource
///
/// Manages the physics simulation and synchronizes with ECS.
pub struct PhysicsWorld {
    /// Configuration
    pub config: PhysicsConfig,
    /// The physics backend
    #[cfg(feature = "utils-rapier")]
    backend: rapier_backend::RapierBackend,
    #[cfg(all(feature = "utils-parry", not(feature = "utils-rapier")))]
    backend: parry_backend::ParryBackend,
    /// Entity to body handle mapping
    entity_to_body: HashMap<Entity, RigidBodyHandle>,
    /// Body handle to entity mapping
    body_to_entity: HashMap<RigidBodyHandle, Entity>,
    /// Accumulated time for fixed timestep
    accumulator: f32,
}

impl PhysicsWorld {
    /// Create a new physics world
    #[cfg(feature = "utils-rapier")]
    pub fn new(config: PhysicsConfig) -> Self {
        let mut backend = rapier_backend::RapierBackend::new();
        backend.init(&config);
        Self {
            config,
            backend,
            entity_to_body: HashMap::new(),
            body_to_entity: HashMap::new(),
            accumulator: 0.0,
        }
    }

    #[cfg(all(feature = "utils-parry", not(feature = "utils-rapier")))]
    pub fn new(config: PhysicsConfig) -> Self {
        let mut backend = parry_backend::ParryBackend::new();
        backend.init(&config);
        Self {
            config,
            backend,
            entity_to_body: HashMap::new(),
            body_to_entity: HashMap::new(),
            accumulator: 0.0,
        }
    }

    /// Add a rigid body for an entity
    pub fn add_body(&mut self, entity: Entity, body: &RigidBody, position: Vec3) -> RigidBodyHandle {
        let handle = self.backend.add_rigid_body(entity, body, position);
        self.entity_to_body.insert(entity, handle);
        self.body_to_entity.insert(handle, entity);
        handle
    }

    /// Remove a rigid body
    pub fn remove_body(&mut self, entity: Entity) {
        if let Some(handle) = self.entity_to_body.remove(&entity) {
            self.body_to_entity.remove(&handle);
            self.backend.remove_rigid_body(handle);
        }
    }

    /// Add a collider to a body
    pub fn add_collider(&mut self, entity: Entity, collider: &Collider) -> Option<ColliderHandle> {
        self.entity_to_body.get(&entity)
            .map(|&handle| self.backend.add_collider(handle, collider))
    }

    /// Get body handle for entity
    pub fn get_handle(&self, entity: Entity) -> Option<RigidBodyHandle> {
        self.entity_to_body.get(&entity).copied()
    }

    /// Get entity for body handle
    pub fn get_entity(&self, handle: RigidBodyHandle) -> Option<Entity> {
        self.body_to_entity.get(&handle).copied()
    }

    /// Step the physics simulation
    pub fn step(&mut self, dt: f32) {
        if self.config.timestep > 0.0 {
            // Fixed timestep with interpolation
            self.accumulator += dt;
            let mut steps = 0;
            while self.accumulator >= self.config.timestep && steps < self.config.max_substeps {
                self.backend.step(self.config.timestep);
                self.accumulator -= self.config.timestep;
                steps += 1;
            }
        } else {
            // Variable timestep
            self.backend.step(dt);
        }
    }

    /// Cast a ray
    pub fn raycast(&self, ray: &Ray, max_dist: f32) -> Option<RaycastHit> {
        self.backend.raycast(ray, max_dist, QueryFilter::default())
    }

    /// Cast a ray with filter
    pub fn raycast_filtered(&self, ray: &Ray, max_dist: f32, filter: QueryFilter) -> Option<RaycastHit> {
        self.backend.raycast(ray, max_dist, filter)
    }

    /// Get collision events
    pub fn collision_events(&self) -> &[CollisionEvent] {
        self.backend.collision_events()
    }

    /// Clear collision events
    pub fn clear_events(&mut self) {
        self.backend.clear_events();
    }

    /// Get position
    pub fn get_position(&self, entity: Entity) -> Option<Vec3> {
        self.entity_to_body.get(&entity)
            .and_then(|&h| self.backend.get_position(h))
    }

    /// Set position
    pub fn set_position(&mut self, entity: Entity, position: Vec3) {
        if let Some(&handle) = self.entity_to_body.get(&entity) {
            self.backend.set_position(handle, position);
        }
    }

    /// Apply force
    pub fn apply_force(&mut self, entity: Entity, force: Vec3) {
        if let Some(&handle) = self.entity_to_body.get(&entity) {
            self.backend.apply_force(handle, force);
        }
    }

    /// Apply impulse
    pub fn apply_impulse(&mut self, entity: Entity, impulse: Vec3) {
        if let Some(&handle) = self.entity_to_body.get(&entity) {
            self.backend.apply_impulse(handle, impulse);
        }
    }

    /// Backend name
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }
}

/// Physics marker component
///
/// Attach to entities that participate in physics simulation.
#[derive(Clone, Debug, Default)]
pub struct PhysicsBody {
    /// The physics handle (set by PhysicsSystem)
    pub(crate) handle: Option<RigidBodyHandle>,
    /// Whether the body has been registered
    pub(crate) registered: bool,
}

impl Component for PhysicsBody {}

/// System for synchronizing physics with ECS
pub struct PhysicsSystem {
    /// Accumulated time
    _accumulator: f32,
}

impl PhysicsSystem {
    /// Create a new physics system
    pub fn new() -> Self {
        Self { _accumulator: 0.0 }
    }
}

impl Default for PhysicsSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for PhysicsSystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        let dt = ctx.delta_time;

        // Get physics world resource
        if let Some(physics) = ctx.world.resource_mut::<PhysicsWorld>() {
            // Step simulation
            physics.step(dt as f32);

            // Clear events at end of frame
            physics.clear_events();
        }
    }

    fn name(&self) -> &'static str {
        "PhysicsSystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }

    fn priority(&self) -> i32 {
        -10 // Run before most game logic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_physics_config() {
        let config = PhysicsConfig::default();
        assert!((config.gravity.y - (-9.81)).abs() < 0.001);
    }

    #[test]
    fn test_config_presets() {
        let zero_g = PhysicsConfig::zero_gravity();
        assert!((zero_g.gravity.y - 0.0).abs() < 0.001);

        let moon = PhysicsConfig::low_gravity();
        assert!((moon.gravity.y - (-1.62)).abs() < 0.001);
    }
}
