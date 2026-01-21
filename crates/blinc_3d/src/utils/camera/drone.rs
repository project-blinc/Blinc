//! Drone camera controller
//!
//! Smooth cinematic camera paths through waypoints.

use super::{CameraController, CameraInput, CameraTransform, CameraUpdateContext};
use crate::ecs::Component;
use crate::math::Quat;
use blinc_core::Vec3;

/// A waypoint on the camera path
#[derive(Clone, Debug)]
pub struct CameraWaypoint {
    /// Position at this waypoint
    pub position: Vec3,
    /// Optional rotation (if None, looks at next waypoint)
    pub rotation: Option<Quat>,
    /// Optional look-at target (if set, overrides rotation)
    pub look_at: Option<Vec3>,
    /// Time to reach this waypoint from the previous one (seconds)
    pub duration: f32,
    /// Ease in/out factor (0 = linear, 1 = smooth)
    pub ease: f32,
}

impl CameraWaypoint {
    /// Create a waypoint at position
    pub fn at(position: Vec3, duration: f32) -> Self {
        Self {
            position,
            rotation: None,
            look_at: None,
            duration,
            ease: 0.5,
        }
    }

    /// Set look-at target
    pub fn looking_at(mut self, target: Vec3) -> Self {
        self.look_at = Some(target);
        self
    }

    /// Set explicit rotation
    pub fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = Some(rotation);
        self
    }

    /// Set easing (0 = linear, 1 = smooth)
    pub fn with_ease(mut self, ease: f32) -> Self {
        self.ease = ease.clamp(0.0, 1.0);
        self
    }
}

/// Drone camera controller
///
/// Follows a predefined path through waypoints with smooth interpolation.
/// Great for cutscenes and cinematic sequences.
///
/// # Example
///
/// ```ignore
/// let mut drone = DroneController::new();
/// drone.add_waypoint(CameraWaypoint::at(Vec3::new(0.0, 5.0, 10.0), 0.0));
/// drone.add_waypoint(CameraWaypoint::at(Vec3::new(10.0, 3.0, 5.0), 3.0).looking_at(Vec3::ZERO));
/// drone.add_waypoint(CameraWaypoint::at(Vec3::new(5.0, 8.0, -5.0), 2.0).with_ease(0.8));
/// drone.play();
/// ```
#[derive(Clone, Debug)]
pub struct DroneController {
    /// Waypoints defining the path
    waypoints: Vec<CameraWaypoint>,
    /// Current time along the path
    current_time: f32,
    /// Total duration of the path
    total_duration: f32,
    /// Whether currently playing
    playing: bool,
    /// Whether to loop the path
    pub looping: bool,
    /// Playback speed multiplier
    pub speed: f32,

    /// Whether controller is active
    enabled: bool,
}

impl Component for DroneController {}

impl DroneController {
    /// Create a new drone controller
    pub fn new() -> Self {
        Self {
            waypoints: Vec::new(),
            current_time: 0.0,
            total_duration: 0.0,
            playing: false,
            looping: false,
            speed: 1.0,
            enabled: true,
        }
    }

    /// Add a waypoint to the path
    pub fn add_waypoint(&mut self, waypoint: CameraWaypoint) {
        self.total_duration += waypoint.duration;
        self.waypoints.push(waypoint);
    }

    /// Clear all waypoints
    pub fn clear(&mut self) {
        self.waypoints.clear();
        self.current_time = 0.0;
        self.total_duration = 0.0;
    }

    /// Start playback from the beginning
    pub fn play(&mut self) {
        self.current_time = 0.0;
        self.playing = true;
    }

    /// Pause playback
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Resume playback
    pub fn resume(&mut self) {
        self.playing = true;
    }

    /// Stop and reset to beginning
    pub fn stop(&mut self) {
        self.playing = false;
        self.current_time = 0.0;
    }

    /// Seek to a specific time
    pub fn seek(&mut self, time: f32) {
        self.current_time = time.clamp(0.0, self.total_duration);
    }

    /// Check if playback is complete
    pub fn is_complete(&self) -> bool {
        !self.looping && self.current_time >= self.total_duration
    }

    /// Get current progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        if self.total_duration > 0.0 {
            self.current_time / self.total_duration
        } else {
            0.0
        }
    }

    /// Find waypoint indices and local t for current time
    fn find_segment(&self) -> (usize, usize, f32) {
        if self.waypoints.is_empty() {
            return (0, 0, 0.0);
        }

        let mut accumulated = 0.0;
        for (i, wp) in self.waypoints.iter().enumerate() {
            if i == 0 {
                continue; // First waypoint has no duration (starting point)
            }

            let segment_end = accumulated + wp.duration;
            if self.current_time <= segment_end || i == self.waypoints.len() - 1 {
                let local_t = if wp.duration > 0.0 {
                    ((self.current_time - accumulated) / wp.duration).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                return (i - 1, i, local_t);
            }
            accumulated = segment_end;
        }

        let last = self.waypoints.len() - 1;
        (last.saturating_sub(1), last, 1.0)
    }

    /// Apply easing function
    fn ease(t: f32, ease_factor: f32) -> f32 {
        if ease_factor <= 0.0 {
            return t;
        }
        // Smoothstep-like easing
        let t2 = t * t;
        let t3 = t2 * t;
        let smooth = 3.0 * t2 - 2.0 * t3;
        t + (smooth - t) * ease_factor
    }

    fn lerp_vec3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
        Vec3::new(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        )
    }

    fn calculate_rotation(from: Vec3, to: Vec3) -> Quat {
        let direction = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
        let len = (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
        if len > 1e-6 {
            let dir = Vec3::new(direction.x / len, direction.y / len, direction.z / len);
            Quat::look_rotation(dir, Vec3::new(0.0, 1.0, 0.0))
        } else {
            Quat::IDENTITY
        }
    }
}

impl CameraController for DroneController {
    fn update(&mut self, ctx: &CameraUpdateContext, _input: &CameraInput) -> CameraTransform {
        if !self.enabled || self.waypoints.is_empty() {
            return ctx.current.clone();
        }

        // Advance time if playing
        if self.playing {
            self.current_time += ctx.dt * self.speed;

            if self.current_time >= self.total_duration {
                if self.looping {
                    self.current_time = self.current_time % self.total_duration;
                } else {
                    self.current_time = self.total_duration;
                    self.playing = false;
                }
            }
        }

        // Get current segment
        let (from_idx, to_idx, t) = self.find_segment();

        if from_idx >= self.waypoints.len() || to_idx >= self.waypoints.len() {
            return ctx.current.clone();
        }

        let from = &self.waypoints[from_idx];
        let to = &self.waypoints[to_idx];

        // Apply easing
        let eased_t = Self::ease(t, to.ease);

        // Interpolate position
        let position = Self::lerp_vec3(from.position, to.position, eased_t);

        // Calculate rotation
        let rotation = if let Some(look_at) = to.look_at {
            // Look at specified target
            let look_from = Self::lerp_vec3(
                from.look_at.unwrap_or(from.position),
                look_at,
                eased_t,
            );
            Self::calculate_rotation(position, look_from)
        } else if let (Some(from_rot), Some(to_rot)) = (from.rotation, to.rotation) {
            // Interpolate explicit rotations
            from_rot.slerp(to_rot, eased_t)
        } else {
            // Look towards next waypoint
            Self::calculate_rotation(from.position, to.position)
        };

        CameraTransform { position, rotation }
    }

    fn reset(&mut self) {
        self.current_time = 0.0;
        self.playing = false;
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for DroneController {
    fn default() -> Self {
        Self::new()
    }
}
