//! Animation integration
//!
//! Provides wrappers for animating 3D properties using spring physics.
//! Uses blinc_animation's Spring directly for standalone operation.

use crate::math::Quat;
use blinc_animation::{Spring, SpringConfig};
use blinc_core::Vec3;

/// Animated Vec3 using spring physics
#[derive(Clone, Debug)]
pub struct AnimatedVec3 {
    /// X component spring
    x: Spring,
    /// Y component spring
    y: Spring,
    /// Z component spring
    z: Spring,
}

impl AnimatedVec3 {
    /// Create a new animated Vec3 at the given position
    pub fn new(value: Vec3) -> Self {
        let config = SpringConfig::stiff();
        Self {
            x: Spring::new(config, value.x),
            y: Spring::new(config, value.y),
            z: Spring::new(config, value.z),
        }
    }

    /// Create with spring configuration
    pub fn with_spring(value: Vec3, stiffness: f32, damping: f32) -> Self {
        let config = SpringConfig::new(stiffness, damping, 1.0);
        Self {
            x: Spring::new(config, value.x),
            y: Spring::new(config, value.y),
            z: Spring::new(config, value.z),
        }
    }

    /// Create with a preset spring config
    pub fn with_config(value: Vec3, config: SpringConfig) -> Self {
        Self {
            x: Spring::new(config, value.x),
            y: Spring::new(config, value.y),
            z: Spring::new(config, value.z),
        }
    }

    /// Set target position
    pub fn set_target(&mut self, target: Vec3) {
        self.x.set_target(target.x);
        self.y.set_target(target.y);
        self.z.set_target(target.z);
    }

    /// Set value immediately (no animation)
    pub fn set_immediate(&mut self, value: Vec3) {
        let config = SpringConfig::stiff();
        self.x = Spring::new(config, value.x);
        self.y = Spring::new(config, value.y);
        self.z = Spring::new(config, value.z);
    }

    /// Get current value
    pub fn get(&self) -> Vec3 {
        Vec3::new(self.x.value(), self.y.value(), self.z.value())
    }

    /// Get target value
    pub fn target(&self) -> Vec3 {
        Vec3::new(self.x.target(), self.y.target(), self.z.target())
    }

    /// Check if animation is complete
    pub fn is_at_rest(&self) -> bool {
        self.x.is_settled() && self.y.is_settled() && self.z.is_settled()
    }

    /// Update animation (called each frame)
    pub fn update(&mut self, dt: f32) {
        self.x.step(dt);
        self.y.step(dt);
        self.z.step(dt);
    }
}

impl Default for AnimatedVec3 {
    fn default() -> Self {
        Self::new(Vec3::ZERO)
    }
}

/// Animated quaternion for smooth rotation interpolation
#[derive(Clone, Debug)]
pub struct AnimatedQuat {
    /// Current quaternion
    current: Quat,
    /// Target quaternion
    target: Quat,
    /// Animation speed (higher = faster)
    speed: f32,
    /// Whether animation is complete
    at_rest: bool,
}

impl AnimatedQuat {
    /// Create a new animated quaternion
    pub fn new(value: Quat) -> Self {
        Self {
            current: value,
            target: value,
            speed: 10.0,
            at_rest: true,
        }
    }

    /// Set animation speed
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Set target rotation
    pub fn set_target(&mut self, target: Quat) {
        self.target = target.normalize();
        self.at_rest = false;
    }

    /// Set rotation immediately
    pub fn set_immediate(&mut self, value: Quat) {
        self.current = value.normalize();
        self.target = self.current;
        self.at_rest = true;
    }

    /// Get current rotation
    pub fn get(&self) -> Quat {
        self.current
    }

    /// Get target rotation
    pub fn target(&self) -> Quat {
        self.target
    }

    /// Check if at rest
    pub fn is_at_rest(&self) -> bool {
        self.at_rest
    }

    /// Update animation
    pub fn update(&mut self, dt: f32) {
        if self.at_rest {
            return;
        }

        let t = (self.speed * dt).min(1.0);
        self.current = self.current.slerp(self.target, t);

        // Check if we've reached the target
        let dot = self.current.dot(self.target).abs();
        if dot > 0.9999 {
            self.current = self.target;
            self.at_rest = true;
        }
    }

    /// Set animation speed
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }
}

impl Default for AnimatedQuat {
    fn default() -> Self {
        Self::new(Quat::IDENTITY)
    }
}

/// Animated transform combining position, rotation, and scale
#[derive(Clone, Debug)]
pub struct AnimatedTransform {
    /// Animated position
    pub position: AnimatedVec3,
    /// Animated rotation
    pub rotation: AnimatedQuat,
    /// Animated scale
    pub scale: AnimatedVec3,
}

impl AnimatedTransform {
    /// Create a new animated transform
    pub fn new() -> Self {
        Self {
            position: AnimatedVec3::new(Vec3::ZERO),
            rotation: AnimatedQuat::new(Quat::IDENTITY),
            scale: AnimatedVec3::new(Vec3::ONE),
        }
    }

    /// Create from position
    pub fn from_position(position: Vec3) -> Self {
        Self {
            position: AnimatedVec3::new(position),
            rotation: AnimatedQuat::new(Quat::IDENTITY),
            scale: AnimatedVec3::new(Vec3::ONE),
        }
    }

    /// Set spring configuration for position and scale
    pub fn with_spring(mut self, stiffness: f32, damping: f32) -> Self {
        self.position = AnimatedVec3::with_spring(self.position.get(), stiffness, damping);
        self.scale = AnimatedVec3::with_spring(self.scale.get(), stiffness, damping);
        self
    }

    /// Set rotation speed
    pub fn with_rotation_speed(mut self, speed: f32) -> Self {
        self.rotation = self.rotation.with_speed(speed);
        self
    }

    /// Set target position
    pub fn set_target_position(&mut self, pos: Vec3) {
        self.position.set_target(pos);
    }

    /// Set target rotation
    pub fn set_target_rotation(&mut self, rot: Quat) {
        self.rotation.set_target(rot);
    }

    /// Set target scale
    pub fn set_target_scale(&mut self, scale: Vec3) {
        self.scale.set_target(scale);
    }

    /// Set uniform target scale
    pub fn set_target_scale_uniform(&mut self, scale: f32) {
        self.scale.set_target(Vec3::new(scale, scale, scale));
    }

    /// Check if all animations are complete
    pub fn is_at_rest(&self) -> bool {
        self.position.is_at_rest() && self.rotation.is_at_rest() && self.scale.is_at_rest()
    }

    /// Update all animations
    pub fn update(&mut self, dt: f32) {
        self.position.update(dt);
        self.rotation.update(dt);
        self.scale.update(dt);
    }

    /// Apply to an Object3D component
    pub fn apply_to(&self, object: &mut crate::scene::Object3D) {
        object.position = self.position.get();
        object.rotation = self.rotation.get();
        object.scale = self.scale.get();
    }

    /// Create from an Object3D component
    pub fn from_object3d(object: &crate::scene::Object3D) -> Self {
        Self {
            position: AnimatedVec3::new(object.position),
            rotation: AnimatedQuat::new(object.rotation),
            scale: AnimatedVec3::new(object.scale),
        }
    }
}

impl Default for AnimatedTransform {
    fn default() -> Self {
        Self::new()
    }
}

/// Animation state that can be stored in ECS components
#[derive(Clone, Debug)]
pub struct AnimationState {
    /// Current animation time
    pub time: f32,
    /// Animation duration
    pub duration: f32,
    /// Whether animation is playing
    pub playing: bool,
    /// Whether animation loops
    pub looping: bool,
    /// Playback speed
    pub speed: f32,
}

impl AnimationState {
    /// Create a new animation state
    pub fn new(duration: f32) -> Self {
        Self {
            time: 0.0,
            duration,
            playing: false,
            looping: false,
            speed: 1.0,
        }
    }

    /// Start playing
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pause
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Stop and reset
    pub fn stop(&mut self) {
        self.playing = false;
        self.time = 0.0;
    }

    /// Set looping
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// Get normalized progress (0-1)
    pub fn progress(&self) -> f32 {
        if self.duration > 0.0 {
            self.time / self.duration
        } else {
            0.0
        }
    }

    /// Update animation
    pub fn update(&mut self, dt: f32) {
        if !self.playing {
            return;
        }

        self.time += dt * self.speed;

        if self.time >= self.duration {
            if self.looping {
                self.time %= self.duration;
            } else {
                self.time = self.duration;
                self.playing = false;
            }
        }
    }

    /// Check if finished (for non-looping animations)
    pub fn is_finished(&self) -> bool {
        !self.looping && self.time >= self.duration
    }
}

impl Default for AnimationState {
    fn default() -> Self {
        Self::new(1.0)
    }
}
