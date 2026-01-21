//! Rigid body component for physics simulation

use crate::ecs::Component;
use blinc_core::Vec3;

/// Type of rigid body
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RigidBodyType {
    /// Moves according to physics forces
    #[default]
    Dynamic,
    /// Moved by user, affects dynamic bodies
    Kinematic,
    /// Never moves, infinite mass
    Static,
}

/// Rigid body component
///
/// Defines physical properties of an entity.
#[derive(Clone, Debug)]
pub struct RigidBody {
    /// Body type
    pub body_type: RigidBodyType,
    /// Mass in kg (ignored for static/kinematic)
    pub mass: f32,
    /// Linear damping (air resistance)
    pub linear_damping: f32,
    /// Angular damping (rotational resistance)
    pub angular_damping: f32,
    /// Gravity scale (1.0 = normal, 0.0 = no gravity)
    pub gravity_scale: f32,
    /// Whether body can sleep when at rest
    pub can_sleep: bool,
    /// Whether body is currently awake
    pub is_awake: bool,
    /// Lock position axes
    pub lock_position: AxisLock,
    /// Lock rotation axes
    pub lock_rotation: AxisLock,
    /// Enable continuous collision detection
    pub ccd_enabled: bool,
    /// Initial linear velocity
    pub initial_velocity: Vec3,
    /// Initial angular velocity
    pub initial_angular_velocity: Vec3,
}

impl Component for RigidBody {}

impl Default for RigidBody {
    fn default() -> Self {
        Self::dynamic()
    }
}

impl RigidBody {
    /// Create a dynamic rigid body
    pub fn dynamic() -> Self {
        Self {
            body_type: RigidBodyType::Dynamic,
            mass: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.05,
            gravity_scale: 1.0,
            can_sleep: true,
            is_awake: true,
            lock_position: AxisLock::default(),
            lock_rotation: AxisLock::default(),
            ccd_enabled: false,
            initial_velocity: Vec3::ZERO,
            initial_angular_velocity: Vec3::ZERO,
        }
    }

    /// Create a kinematic rigid body
    pub fn kinematic() -> Self {
        Self {
            body_type: RigidBodyType::Kinematic,
            mass: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            gravity_scale: 0.0,
            can_sleep: false,
            is_awake: true,
            lock_position: AxisLock::default(),
            lock_rotation: AxisLock::default(),
            ccd_enabled: false,
            initial_velocity: Vec3::ZERO,
            initial_angular_velocity: Vec3::ZERO,
        }
    }

    /// Create a static rigid body
    pub fn static_body() -> Self {
        Self {
            body_type: RigidBodyType::Static,
            mass: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            gravity_scale: 0.0,
            can_sleep: false,
            is_awake: true,
            lock_position: AxisLock::all(),
            lock_rotation: AxisLock::all(),
            ccd_enabled: false,
            initial_velocity: Vec3::ZERO,
            initial_angular_velocity: Vec3::ZERO,
        }
    }

    /// Set mass
    pub fn with_mass(mut self, mass: f32) -> Self {
        self.mass = mass.max(0.001);
        self
    }

    /// Set linear damping
    pub fn with_linear_damping(mut self, damping: f32) -> Self {
        self.linear_damping = damping.max(0.0);
        self
    }

    /// Set angular damping
    pub fn with_angular_damping(mut self, damping: f32) -> Self {
        self.angular_damping = damping.max(0.0);
        self
    }

    /// Set gravity scale
    pub fn with_gravity_scale(mut self, scale: f32) -> Self {
        self.gravity_scale = scale;
        self
    }

    /// Disable gravity
    pub fn without_gravity(mut self) -> Self {
        self.gravity_scale = 0.0;
        self
    }

    /// Set whether body can sleep
    pub fn with_can_sleep(mut self, can_sleep: bool) -> Self {
        self.can_sleep = can_sleep;
        self
    }

    /// Lock position on specified axes
    pub fn with_locked_position(mut self, lock: AxisLock) -> Self {
        self.lock_position = lock;
        self
    }

    /// Lock rotation on specified axes
    pub fn with_locked_rotation(mut self, lock: AxisLock) -> Self {
        self.lock_rotation = lock;
        self
    }

    /// Lock all rotation (no tumbling)
    pub fn with_fixed_rotation(mut self) -> Self {
        self.lock_rotation = AxisLock::all();
        self
    }

    /// Enable continuous collision detection
    pub fn with_ccd(mut self, enabled: bool) -> Self {
        self.ccd_enabled = enabled;
        self
    }

    /// Set initial velocity
    pub fn with_velocity(mut self, velocity: Vec3) -> Self {
        self.initial_velocity = velocity;
        self
    }

    /// Set initial angular velocity
    pub fn with_angular_velocity(mut self, velocity: Vec3) -> Self {
        self.initial_angular_velocity = velocity;
        self
    }

    /// Check if body is dynamic
    pub fn is_dynamic(&self) -> bool {
        self.body_type == RigidBodyType::Dynamic
    }

    /// Check if body is kinematic
    pub fn is_kinematic(&self) -> bool {
        self.body_type == RigidBodyType::Kinematic
    }

    /// Check if body is static
    pub fn is_static(&self) -> bool {
        self.body_type == RigidBodyType::Static
    }

    // ========== Presets ==========

    /// Preset for a player character (no rotation, responsive)
    pub fn player_character() -> Self {
        Self::dynamic()
            .with_mass(70.0)
            .with_linear_damping(0.5)
            .with_fixed_rotation()
            .with_can_sleep(false)
    }

    /// Preset for a projectile (fast moving, CCD enabled)
    pub fn projectile() -> Self {
        Self::dynamic()
            .with_mass(0.1)
            .with_ccd(true)
            .with_can_sleep(false)
    }

    /// Preset for a vehicle
    pub fn vehicle() -> Self {
        Self::dynamic()
            .with_mass(1500.0)
            .with_linear_damping(0.3)
            .with_angular_damping(0.5)
    }

    /// Preset for debris/particles
    pub fn debris() -> Self {
        Self::dynamic()
            .with_mass(0.5)
            .with_linear_damping(0.1)
    }

    /// Preset for floating objects
    pub fn floating() -> Self {
        Self::dynamic()
            .with_gravity_scale(0.3)
            .with_linear_damping(0.5)
    }

    /// Preset for a platform (kinematic)
    pub fn platform() -> Self {
        Self::kinematic()
    }

    /// Preset for environment (static)
    pub fn environment() -> Self {
        Self::static_body()
    }
}

/// Axis lock configuration
#[derive(Clone, Copy, Debug, Default)]
pub struct AxisLock {
    /// Lock X axis
    pub x: bool,
    /// Lock Y axis
    pub y: bool,
    /// Lock Z axis
    pub z: bool,
}

impl AxisLock {
    /// No axes locked
    pub fn none() -> Self {
        Self { x: false, y: false, z: false }
    }

    /// All axes locked
    pub fn all() -> Self {
        Self { x: true, y: true, z: true }
    }

    /// Lock X axis only
    pub fn x_only() -> Self {
        Self { x: true, y: false, z: false }
    }

    /// Lock Y axis only
    pub fn y_only() -> Self {
        Self { x: false, y: true, z: false }
    }

    /// Lock Z axis only
    pub fn z_only() -> Self {
        Self { x: false, y: false, z: true }
    }

    /// Lock X and Y axes (2D mode)
    pub fn xy() -> Self {
        Self { x: true, y: true, z: false }
    }

    /// Lock X and Z axes
    pub fn xz() -> Self {
        Self { x: true, y: false, z: true }
    }

    /// Lock Y and Z axes
    pub fn yz() -> Self {
        Self { x: false, y: true, z: true }
    }

    /// Set X lock
    pub fn with_x(mut self, locked: bool) -> Self {
        self.x = locked;
        self
    }

    /// Set Y lock
    pub fn with_y(mut self, locked: bool) -> Self {
        self.y = locked;
        self
    }

    /// Set Z lock
    pub fn with_z(mut self, locked: bool) -> Self {
        self.z = locked;
        self
    }

    /// Check if any axis is locked
    pub fn any(&self) -> bool {
        self.x || self.y || self.z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rigid_body_creation() {
        let body = RigidBody::dynamic();
        assert_eq!(body.body_type, RigidBodyType::Dynamic);
        assert!((body.mass - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_body_presets() {
        let player = RigidBody::player_character();
        assert!(player.lock_rotation.x);
        assert!(player.lock_rotation.y);
        assert!(player.lock_rotation.z);
        assert!(!player.can_sleep);

        let projectile = RigidBody::projectile();
        assert!(projectile.ccd_enabled);
    }

    #[test]
    fn test_axis_lock() {
        let lock = AxisLock::xy();
        assert!(lock.x);
        assert!(lock.y);
        assert!(!lock.z);
    }
}
