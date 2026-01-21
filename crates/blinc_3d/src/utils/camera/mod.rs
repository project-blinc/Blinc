//! Camera controllers for 3D navigation
//!
//! Provides various camera control schemes:
//!
//! - [`OrbitController`] - Orbits around a target point (like Blender)
//! - [`FlyController`] - Free-flight WASD + mouse look
//! - [`FollowController`] - Follows an entity with offset and damping
//! - [`DroneController`] - Smooth cinematic camera paths
//! - [`CameraShake`] - Trauma-based screen shake effect

mod orbit;
mod fly;
mod follow;
mod drone;
mod shake;
mod input;

pub use orbit::OrbitController;
pub use fly::FlyController;
pub use follow::FollowController;
pub use drone::{DroneController, CameraWaypoint};
pub use shake::CameraShake;
pub use input::{CameraInput, CameraKeys};

use crate::ecs::Component;
use crate::math::Quat;
use blinc_core::Vec3;

/// Transform output from a camera controller
#[derive(Clone, Debug)]
pub struct CameraTransform {
    /// World position
    pub position: Vec3,
    /// Rotation as quaternion
    pub rotation: Quat,
}

impl Default for CameraTransform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        }
    }
}

/// Context passed to camera controllers during update
pub struct CameraUpdateContext<'a> {
    /// Delta time in seconds
    pub dt: f32,
    /// Total elapsed time
    pub elapsed: f32,
    /// Current camera transform (for relative updates)
    pub current: &'a CameraTransform,
}

/// Trait for camera controllers
pub trait CameraController: Send + Sync {
    /// Update the camera and return new transform
    fn update(&mut self, ctx: &CameraUpdateContext, input: &CameraInput) -> CameraTransform;

    /// Reset to initial state
    fn reset(&mut self);

    /// Enable or disable the controller
    fn set_enabled(&mut self, enabled: bool);

    /// Check if controller is enabled
    fn is_enabled(&self) -> bool;
}
