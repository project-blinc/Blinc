//! Animation system integration
//!
//! Provides ECS integration for animations and extends blinc_animation
//! with quaternion support for 3D rotations.

use crate::ecs::{System, SystemContext};
use crate::math::Quat;

// Re-export animation types from blinc_animation for convenience
pub use blinc_animation::{
    ColorAnimation, Easing, FloatAnimation, Interpolate, SphericalInterpolate,
    TypedKeyframe, TypedKeyframeAnimation, Vec3Animation,
};

// ============================================================================
// Quaternion Interpolation (3D-specific)
// ============================================================================

impl SphericalInterpolate for Quat {
    fn slerp(&self, other: &Self, t: f32) -> Self {
        self.slerp(*other, t)
    }

    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool {
        let dot = self.dot(*other).abs();
        dot > 1.0 - epsilon
    }
}

impl Interpolate for Quat {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        // For quaternions, use slerp for proper rotation interpolation
        self.slerp(*other, t)
    }

    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool {
        let dot = self.dot(*other).abs();
        dot > 1.0 - epsilon
    }
}

/// Keyframe animation for quaternion rotations
pub type QuatAnimation = TypedKeyframeAnimation<Quat>;

/// Quaternion keyframe
pub type QuatKeyframe = TypedKeyframe<Quat>;

// ============================================================================
// ECS Time Resources
// ============================================================================

/// Resource for storing delta time (seconds since last frame)
#[derive(Clone, Copy, Debug, Default)]
pub struct DeltaTime(pub f32);

/// Resource for storing total elapsed time (seconds)
#[derive(Clone, Copy, Debug, Default)]
pub struct TotalTime(pub f32);

/// Resource for storing frame count
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameCount(pub u64);

// ============================================================================
// Animation Sync System
// ============================================================================

/// System for synchronizing time resources
///
/// This system updates time-related resources that animations can use.
/// It should run early in the frame, before game logic.
pub struct AnimationSyncSystem {
    dt: f32,
}

impl AnimationSyncSystem {
    /// Create a new animation sync system
    pub fn new() -> Self {
        Self { dt: 0.0 }
    }

    /// Set delta time for this frame
    pub fn set_delta_time(&mut self, dt: f32) {
        self.dt = dt;
    }

    /// Get the current delta time
    pub fn delta_time(&self) -> f32 {
        self.dt
    }
}

impl Default for AnimationSyncSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for AnimationSyncSystem {
    fn name(&self) -> &'static str {
        "AnimationSyncSystem"
    }

    fn priority(&self) -> i32 {
        -90 // Run early, after input but before game logic
    }

    fn run(&mut self, ctx: &mut SystemContext) {
        // Delta time comes from resources
        let dt = ctx
            .world
            .resource::<DeltaTime>()
            .map(|d| d.0)
            .unwrap_or(0.016);

        self.dt = dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::Vec3;
    use std::f32::consts::PI;

    #[test]
    fn test_quat_animation() {
        let start = Quat::IDENTITY;
        let end = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), PI / 2.0);

        let mut anim = QuatAnimation::new(1000)
            .at(0.0, start)
            .at(1.0, end);

        anim.start();

        // At start
        let q = anim.value().unwrap();
        assert!(SphericalInterpolate::approx_eq(&q, &start, 1e-4));

        // Advance to end
        anim.tick(1000.0);
        let q = anim.value().unwrap();
        assert!(SphericalInterpolate::approx_eq(&q, &end, 1e-4));
    }

    #[test]
    fn test_vec3_animation() {
        let mut anim = Vec3Animation::new(1000)
            .at(0.0, Vec3::ZERO)
            .at(1.0, Vec3::new(100.0, 0.0, 0.0));

        anim.start();
        anim.tick(500.0);

        let v = anim.value().unwrap();
        assert!((v.x - 50.0).abs() < 1e-4);
    }
}
