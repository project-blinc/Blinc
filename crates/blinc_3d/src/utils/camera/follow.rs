//! Follow camera controller
//!
//! Follows a target with configurable offset and damping.

use super::{CameraController, CameraInput, CameraTransform, CameraUpdateContext};
use crate::ecs::{Component, Entity};
use crate::math::Quat;
use blinc_core::Vec3;

/// Follow camera controller
///
/// Follows a target entity or position with smooth interpolation.
/// Great for third-person cameras.
///
/// # Example
///
/// ```ignore
/// let mut follow = FollowController::new();
/// follow.offset = Vec3::new(0.0, 3.0, 8.0);
/// follow.look_offset = Vec3::new(0.0, 1.5, 0.0);
/// follow.damping = 0.05;
/// ```
#[derive(Clone, Debug)]
pub struct FollowController {
    /// Target position to follow (set externally each frame)
    pub target_position: Vec3,
    /// Target rotation (optional, for rotating offset with target)
    pub target_rotation: Option<Quat>,

    /// Offset from target in local space (rotates with target if rotation set)
    pub offset: Vec3,
    /// Point to look at relative to target
    pub look_offset: Vec3,

    /// Position smoothing (0 = instant, higher = slower)
    pub position_damping: f32,
    /// Rotation smoothing
    pub rotation_damping: f32,

    /// Minimum distance from target
    pub min_distance: f32,
    /// Maximum distance from target
    pub max_distance: f32,

    /// Whether controller is active
    enabled: bool,

    // Internal interpolated state
    current_position: Vec3,
    current_rotation: Quat,
}

impl Component for FollowController {}

impl FollowController {
    /// Create a new follow controller
    pub fn new() -> Self {
        Self {
            target_position: Vec3::ZERO,
            target_rotation: None,

            offset: Vec3::new(0.0, 3.0, 8.0),
            look_offset: Vec3::new(0.0, 1.0, 0.0),

            position_damping: 0.05,
            rotation_damping: 0.08,

            min_distance: 1.0,
            max_distance: 50.0,

            enabled: true,

            current_position: Vec3::new(0.0, 3.0, 8.0),
            current_rotation: Quat::IDENTITY,
        }
    }

    /// Create with specific offset
    pub fn with_offset(offset: Vec3) -> Self {
        let mut controller = Self::new();
        controller.offset = offset;
        controller.current_position = offset;
        controller
    }

    /// Set target to follow
    pub fn set_target(&mut self, position: Vec3, rotation: Option<Quat>) {
        self.target_position = position;
        self.target_rotation = rotation;
    }

    /// Calculate desired camera position
    fn calculate_target_position(&self) -> Vec3 {
        let offset = if let Some(rot) = self.target_rotation {
            rot.rotate_vec3(self.offset)
        } else {
            self.offset
        };

        Vec3::new(
            self.target_position.x + offset.x,
            self.target_position.y + offset.y,
            self.target_position.z + offset.z,
        )
    }

    /// Calculate look-at point
    fn calculate_look_at(&self) -> Vec3 {
        let look_offset = if let Some(rot) = self.target_rotation {
            rot.rotate_vec3(self.look_offset)
        } else {
            self.look_offset
        };

        Vec3::new(
            self.target_position.x + look_offset.x,
            self.target_position.y + look_offset.y,
            self.target_position.z + look_offset.z,
        )
    }

    fn lerp_vec3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
        Vec3::new(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        )
    }
}

impl CameraController for FollowController {
    fn update(&mut self, ctx: &CameraUpdateContext, _input: &CameraInput) -> CameraTransform {
        if !self.enabled {
            return ctx.current.clone();
        }

        // Calculate desired position
        let target_pos = self.calculate_target_position();
        let look_at = self.calculate_look_at();

        // Smooth position interpolation
        let pos_t = 1.0 - self.position_damping.powf(ctx.dt * 60.0);
        self.current_position = Self::lerp_vec3(self.current_position, target_pos, pos_t);

        // Calculate rotation to look at target
        let direction = Vec3::new(
            look_at.x - self.current_position.x,
            look_at.y - self.current_position.y,
            look_at.z - self.current_position.z,
        );

        let len = (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
        let target_rotation = if len > 1e-6 {
            let dir = Vec3::new(direction.x / len, direction.y / len, direction.z / len);
            Quat::look_rotation(dir, Vec3::new(0.0, 1.0, 0.0))
        } else {
            Quat::IDENTITY
        };

        // Smooth rotation interpolation
        let rot_t = 1.0 - self.rotation_damping.powf(ctx.dt * 60.0);
        self.current_rotation = self.current_rotation.slerp(target_rotation, rot_t);

        CameraTransform {
            position: self.current_position,
            rotation: self.current_rotation,
        }
    }

    fn reset(&mut self) {
        self.current_position = self.calculate_target_position();
        let look_at = self.calculate_look_at();
        let direction = Vec3::new(
            look_at.x - self.current_position.x,
            look_at.y - self.current_position.y,
            look_at.z - self.current_position.z,
        );
        let len = (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
        if len > 1e-6 {
            let dir = Vec3::new(direction.x / len, direction.y / len, direction.z / len);
            self.current_rotation = Quat::look_rotation(dir, Vec3::new(0.0, 1.0, 0.0));
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for FollowController {
    fn default() -> Self {
        Self::new()
    }
}
