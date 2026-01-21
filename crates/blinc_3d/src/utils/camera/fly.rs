//! Fly camera controller
//!
//! Free-flight camera with WASD movement and mouse look.

use super::{CameraController, CameraInput, CameraTransform, CameraUpdateContext};
use crate::ecs::Component;
use crate::math::Quat;
use blinc_core::Vec3;
use std::f32::consts::PI;

/// Free-flight camera controller
///
/// WASD movement with mouse look, similar to FPS games or flight simulators.
///
/// # Example
///
/// ```ignore
/// let mut fly = FlyController::new(Vec3::new(0.0, 5.0, 10.0));
/// fly.move_speed = 10.0;
/// fly.look_speed = 0.003;
/// ```
#[derive(Clone, Debug)]
pub struct FlyController {
    /// Current position
    pub position: Vec3,
    /// Yaw angle (radians, 0 = looking down -Z)
    pub yaw: f32,
    /// Pitch angle (radians, 0 = horizontal)
    pub pitch: f32,

    /// Movement speed (units per second)
    pub move_speed: f32,
    /// Sprint speed multiplier
    pub sprint_multiplier: f32,
    /// Slow speed multiplier
    pub slow_multiplier: f32,
    /// Mouse look sensitivity (radians per pixel)
    pub look_speed: f32,

    /// Minimum pitch angle (radians)
    pub min_pitch: f32,
    /// Maximum pitch angle (radians)
    pub max_pitch: f32,

    /// Smooth movement damping (0 = instant, higher = smoother)
    pub move_damping: f32,

    /// Whether controller is active
    enabled: bool,

    // Velocity for smooth movement
    velocity: Vec3,
}

impl Component for FlyController {}

impl FlyController {
    /// Create a new fly controller at position
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,

            move_speed: 5.0,
            sprint_multiplier: 2.5,
            slow_multiplier: 0.3,
            look_speed: 0.003,

            min_pitch: -PI * 0.49,
            max_pitch: PI * 0.49,

            move_damping: 0.1,

            enabled: true,

            velocity: Vec3::ZERO,
        }
    }

    /// Set position instantly
    pub fn set_position(&mut self, position: Vec3) {
        self.position = position;
        self.velocity = Vec3::ZERO;
    }

    /// Set look direction from yaw and pitch (radians)
    pub fn set_rotation(&mut self, yaw: f32, pitch: f32) {
        self.yaw = yaw;
        self.pitch = pitch.clamp(self.min_pitch, self.max_pitch);
    }

    /// Look at a target point
    pub fn look_at(&mut self, target: Vec3) {
        let dir = Vec3::new(
            target.x - self.position.x,
            target.y - self.position.y,
            target.z - self.position.z,
        );

        let len_xz = (dir.x * dir.x + dir.z * dir.z).sqrt();
        if len_xz > 1e-6 {
            self.yaw = (-dir.x).atan2(-dir.z);
            self.pitch = (dir.y / len_xz).atan();
            self.pitch = self.pitch.clamp(self.min_pitch, self.max_pitch);
        }
    }

    /// Get forward direction vector
    pub fn forward(&self) -> Vec3 {
        let cos_pitch = self.pitch.cos();
        Vec3::new(
            -self.yaw.sin() * cos_pitch,
            -self.pitch.sin(),
            -self.yaw.cos() * cos_pitch,
        )
    }

    /// Get right direction vector
    pub fn right(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos(),
            0.0,
            -self.yaw.sin(),
        )
    }

    /// Get up direction vector (world up projected)
    pub fn up(&self) -> Vec3 {
        let forward = self.forward();
        let right = self.right();
        // Cross product: right x forward = up
        Vec3::new(
            right.y * forward.z - right.z * forward.y,
            right.z * forward.x - right.x * forward.z,
            right.x * forward.y - right.y * forward.x,
        )
    }

    fn calculate_rotation(&self) -> Quat {
        Quat::from_euler_yxz(self.yaw, self.pitch, 0.0)
    }

    fn lerp_vec3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
        Vec3::new(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        )
    }
}

impl CameraController for FlyController {
    fn update(&mut self, ctx: &CameraUpdateContext, input: &CameraInput) -> CameraTransform {
        if !self.enabled {
            return ctx.current.clone();
        }

        // Mouse look (when right mouse button is pressed)
        if input.secondary_pressed {
            self.yaw -= input.mouse_delta.x * self.look_speed;
            self.pitch -= input.mouse_delta.y * self.look_speed;
            self.pitch = self.pitch.clamp(self.min_pitch, self.max_pitch);
        }

        // Calculate movement direction in world space
        let move_dir = input.movement_direction();
        let speed_mult = input.keys.speed_multiplier(self.sprint_multiplier, self.slow_multiplier);
        let speed = self.move_speed * speed_mult;

        // Transform movement to world space
        let forward = self.forward();
        let right = self.right();
        let up = Vec3::new(0.0, 1.0, 0.0); // World up for vertical movement

        let target_velocity = Vec3::new(
            (right.x * move_dir.x + forward.x * move_dir.z) * speed,
            move_dir.y * speed, // Direct Y movement
            (right.z * move_dir.x + forward.z * move_dir.z) * speed,
        );

        // Smooth velocity
        let t = 1.0 - self.move_damping.powf(ctx.dt * 60.0);
        self.velocity = Self::lerp_vec3(self.velocity, target_velocity, t);

        // Apply velocity
        self.position.x += self.velocity.x * ctx.dt;
        self.position.y += self.velocity.y * ctx.dt;
        self.position.z += self.velocity.z * ctx.dt;

        CameraTransform {
            position: self.position,
            rotation: self.calculate_rotation(),
        }
    }

    fn reset(&mut self) {
        self.yaw = 0.0;
        self.pitch = 0.0;
        self.velocity = Vec3::ZERO;
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for FlyController {
    fn default() -> Self {
        Self::new(Vec3::new(0.0, 2.0, 5.0))
    }
}
