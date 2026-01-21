//! Orbit camera controller
//!
//! Orbits around a target point, similar to 3D modeling software.

use super::{CameraController, CameraInput, CameraTransform, CameraUpdateContext};
use crate::ecs::Component;
use crate::math::Quat;
use blinc_core::Vec3;
use std::f32::consts::PI;

/// Orbit camera controller
///
/// Rotates around a target point with adjustable distance.
/// Supports panning, rotation, and zoom.
///
/// # Example
///
/// ```ignore
/// let mut orbit = OrbitController::new(Vec3::ZERO, 5.0);
/// orbit.rotation_speed = 0.005;
/// orbit.zoom_speed = 0.1;
/// ```
#[derive(Clone, Debug)]
pub struct OrbitController {
    /// Target point to orbit around
    pub target: Vec3,
    /// Distance from target
    pub distance: f32,
    /// Horizontal angle (radians, 0 = looking down -Z)
    pub azimuth: f32,
    /// Vertical angle (radians, 0 = horizontal, positive = looking down)
    pub elevation: f32,

    /// Minimum distance from target
    pub min_distance: f32,
    /// Maximum distance from target
    pub max_distance: f32,
    /// Minimum elevation angle (radians)
    pub min_elevation: f32,
    /// Maximum elevation angle (radians)
    pub max_elevation: f32,

    /// Rotation sensitivity (radians per pixel)
    pub rotation_speed: f32,
    /// Zoom sensitivity (distance units per scroll)
    pub zoom_speed: f32,
    /// Pan sensitivity (world units per pixel)
    pub pan_speed: f32,

    /// Smooth damping factor (0 = instant, 1 = no movement)
    pub damping: f32,

    /// Enable rotation
    pub rotate_enabled: bool,
    /// Enable zooming
    pub zoom_enabled: bool,
    /// Enable panning
    pub pan_enabled: bool,

    /// Whether controller is active
    enabled: bool,

    // Smooth interpolation state
    target_azimuth: f32,
    target_elevation: f32,
    target_distance: f32,
    target_position: Vec3,
}

impl Component for OrbitController {}

impl OrbitController {
    /// Create a new orbit controller
    pub fn new(target: Vec3, distance: f32) -> Self {
        Self {
            target,
            distance,
            azimuth: 0.0,
            elevation: 0.3, // Slight angle down

            min_distance: 0.1,
            max_distance: 1000.0,
            min_elevation: -PI * 0.45, // Almost straight down
            max_elevation: PI * 0.45,   // Almost straight up

            rotation_speed: 0.005,
            zoom_speed: 0.1,
            pan_speed: 0.01,

            damping: 0.1,

            rotate_enabled: true,
            zoom_enabled: true,
            pan_enabled: true,

            enabled: true,

            target_azimuth: 0.0,
            target_elevation: 0.3,
            target_distance: distance,
            target_position: target,
        }
    }

    /// Set the orbit target
    pub fn set_target(&mut self, target: Vec3) {
        self.target = target;
        self.target_position = target;
    }

    /// Set distance instantly
    pub fn set_distance(&mut self, distance: f32) {
        self.distance = distance.clamp(self.min_distance, self.max_distance);
        self.target_distance = self.distance;
    }

    /// Set angles instantly (in radians)
    pub fn set_angles(&mut self, azimuth: f32, elevation: f32) {
        self.azimuth = azimuth;
        self.elevation = elevation.clamp(self.min_elevation, self.max_elevation);
        self.target_azimuth = self.azimuth;
        self.target_elevation = self.elevation;
    }

    /// Calculate camera position from current state
    fn calculate_position(&self) -> Vec3 {
        let cos_elev = self.elevation.cos();
        let sin_elev = self.elevation.sin();
        let cos_azim = self.azimuth.cos();
        let sin_azim = self.azimuth.sin();

        Vec3::new(
            self.target.x + self.distance * cos_elev * sin_azim,
            self.target.y + self.distance * sin_elev,
            self.target.z + self.distance * cos_elev * cos_azim,
        )
    }

    /// Calculate camera rotation to look at target
    fn calculate_rotation(&self) -> Quat {
        let position = self.calculate_position();
        let direction = Vec3::new(
            self.target.x - position.x,
            self.target.y - position.y,
            self.target.z - position.z,
        );

        // Normalize direction
        let len = (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
        if len < 1e-6 {
            return Quat::IDENTITY;
        }

        let dir = Vec3::new(direction.x / len, direction.y / len, direction.z / len);

        // Calculate rotation from forward (-Z) to direction
        Quat::look_rotation(dir, Vec3::new(0.0, 1.0, 0.0))
    }

    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }
}

impl CameraController for OrbitController {
    fn update(&mut self, ctx: &CameraUpdateContext, input: &CameraInput) -> CameraTransform {
        if !self.enabled {
            return ctx.current.clone();
        }

        // Handle rotation (right mouse button or left+alt)
        if self.rotate_enabled && input.secondary_pressed {
            self.target_azimuth -= input.mouse_delta.x * self.rotation_speed;
            self.target_elevation += input.mouse_delta.y * self.rotation_speed;
            self.target_elevation = self.target_elevation.clamp(self.min_elevation, self.max_elevation);
        }

        // Handle panning (middle mouse button)
        if self.pan_enabled && input.middle_pressed {
            let right = Vec3::new(self.azimuth.cos(), 0.0, -self.azimuth.sin());
            let up = Vec3::new(0.0, 1.0, 0.0);

            let pan_x = input.mouse_delta.x * self.pan_speed * self.distance;
            let pan_y = input.mouse_delta.y * self.pan_speed * self.distance;

            self.target_position.x -= right.x * pan_x + up.x * pan_y;
            self.target_position.y -= right.y * pan_x + up.y * pan_y;
            self.target_position.z -= right.z * pan_x + up.z * pan_y;
        }

        // Handle zoom (scroll wheel)
        if self.zoom_enabled && input.scroll_delta.abs() > 0.0 {
            self.target_distance -= input.scroll_delta * self.zoom_speed * self.distance;
            self.target_distance = self.target_distance.clamp(self.min_distance, self.max_distance);
        }

        // Smooth interpolation
        let t = 1.0 - self.damping.powf(ctx.dt * 60.0);
        self.azimuth = Self::lerp(self.azimuth, self.target_azimuth, t);
        self.elevation = Self::lerp(self.elevation, self.target_elevation, t);
        self.distance = Self::lerp(self.distance, self.target_distance, t);
        self.target.x = Self::lerp(self.target.x, self.target_position.x, t);
        self.target.y = Self::lerp(self.target.y, self.target_position.y, t);
        self.target.z = Self::lerp(self.target.z, self.target_position.z, t);

        CameraTransform {
            position: self.calculate_position(),
            rotation: self.calculate_rotation(),
        }
    }

    fn reset(&mut self) {
        self.azimuth = 0.0;
        self.elevation = 0.3;
        self.target_azimuth = 0.0;
        self.target_elevation = 0.3;
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for OrbitController {
    fn default() -> Self {
        Self::new(Vec3::ZERO, 5.0)
    }
}
