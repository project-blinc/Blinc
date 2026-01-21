//! Animatable value types
//!
//! Provides traits and implementations for values that can be animated,
//! including linear interpolation for vectors and colors.

use blinc_core::{Color, Vec3};

/// Trait for values that can be linearly interpolated
pub trait Interpolate: Clone {
    /// Linearly interpolate between self and other by factor t (0.0 to 1.0)
    fn lerp(&self, other: &Self, t: f32) -> Self;

    /// Check if two values are approximately equal (for settling detection)
    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool;
}

/// Trait for values that use spherical interpolation (quaternions)
pub trait SphericalInterpolate: Clone {
    /// Spherically interpolate between self and other by factor t (0.0 to 1.0)
    fn slerp(&self, other: &Self, t: f32) -> Self;

    /// Check if two values are approximately equal
    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool;
}

// ============================================================================
// f32 Implementation
// ============================================================================

impl Interpolate for f32 {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        self + (other - self) * t
    }

    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool {
        (self - other).abs() < epsilon
    }
}

// ============================================================================
// Vec3 Implementation
// ============================================================================

impl Interpolate for Vec3 {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        Vec3::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
            self.z + (other.z - self.z) * t,
        )
    }

    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool {
        (self.x - other.x).abs() < epsilon
            && (self.y - other.y).abs() < epsilon
            && (self.z - other.z).abs() < epsilon
    }
}

// ============================================================================
// Color Implementation
// ============================================================================

impl Interpolate for Color {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        Color::lerp(self, other, t)
    }

    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool {
        (self.r - other.r).abs() < epsilon
            && (self.g - other.g).abs() < epsilon
            && (self.b - other.b).abs() < epsilon
            && (self.a - other.a).abs() < epsilon
    }
}

// ============================================================================
// Generic Keyframe Animation for any Interpolate type
// ============================================================================

use crate::easing::Easing;

/// A keyframe holding a value of type T
#[derive(Clone, Debug)]
pub struct TypedKeyframe<T: Interpolate> {
    /// Time position (0.0 to 1.0)
    pub time: f32,
    /// Value at this keyframe
    pub value: T,
    /// Easing function when transitioning TO this keyframe
    pub easing: Easing,
}

impl<T: Interpolate> TypedKeyframe<T> {
    /// Create a new keyframe
    pub fn new(time: f32, value: T, easing: Easing) -> Self {
        Self { time, value, easing }
    }

    /// Create a keyframe with linear easing
    pub fn linear(time: f32, value: T) -> Self {
        Self::new(time, value, Easing::Linear)
    }
}

/// A keyframe animation for any interpolatable type
#[derive(Clone, Debug)]
pub struct TypedKeyframeAnimation<T: Interpolate> {
    /// Duration in milliseconds
    duration_ms: u32,
    /// Keyframes sorted by time
    keyframes: Vec<TypedKeyframe<T>>,
    /// Current time in milliseconds
    current_time: f32,
    /// Whether animation is playing
    playing: bool,
    /// Whether to loop
    looping: bool,
}

impl<T: Interpolate> TypedKeyframeAnimation<T> {
    /// Create a new animation with given duration
    pub fn new(duration_ms: u32) -> Self {
        Self {
            duration_ms,
            keyframes: Vec::new(),
            current_time: 0.0,
            playing: false,
            looping: false,
        }
    }

    /// Add a keyframe (builder pattern)
    pub fn keyframe(mut self, time: f32, value: T, easing: Easing) -> Self {
        self.keyframes.push(TypedKeyframe::new(time, value, easing));
        self.keyframes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
        self
    }

    /// Add a keyframe with linear easing
    pub fn at(mut self, time: f32, value: T) -> Self {
        self.keyframes.push(TypedKeyframe::linear(time, value));
        self.keyframes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
        self
    }

    /// Set looping
    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Start the animation
    pub fn start(&mut self) {
        self.current_time = 0.0;
        self.playing = true;
    }

    /// Stop the animation
    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Check if playing
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Get progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        if self.duration_ms == 0 {
            return 1.0;
        }
        (self.current_time / self.duration_ms as f32).clamp(0.0, 1.0)
    }

    /// Get current interpolated value
    pub fn value(&self) -> Option<T> {
        if self.keyframes.is_empty() {
            return None;
        }

        let progress = self.progress();

        // Find surrounding keyframes
        let mut prev_kf = &self.keyframes[0];
        let mut next_kf = &self.keyframes[0];

        for kf in &self.keyframes {
            if kf.time <= progress {
                prev_kf = kf;
            }
            if kf.time >= progress {
                next_kf = kf;
                break;
            }
        }

        if (prev_kf.time - next_kf.time).abs() < f32::EPSILON {
            return Some(prev_kf.value.clone());
        }

        // Interpolate
        let local_progress = (progress - prev_kf.time) / (next_kf.time - prev_kf.time);
        let eased = next_kf.easing.apply(local_progress);

        Some(prev_kf.value.lerp(&next_kf.value, eased))
    }

    /// Advance animation by delta time (in milliseconds)
    pub fn tick(&mut self, dt_ms: f32) {
        if !self.playing {
            return;
        }

        self.current_time += dt_ms;

        if self.current_time >= self.duration_ms as f32 {
            if self.looping {
                self.current_time %= self.duration_ms as f32;
            } else {
                self.current_time = self.duration_ms as f32;
                self.playing = false;
            }
        }
    }

    /// Sample at a specific progress (0.0 to 1.0)
    pub fn sample_at(&self, progress: f32) -> Option<T> {
        if self.keyframes.is_empty() {
            return None;
        }

        let progress = progress.clamp(0.0, 1.0);

        let mut prev_kf = &self.keyframes[0];
        let mut next_kf = &self.keyframes[0];

        for kf in &self.keyframes {
            if kf.time <= progress {
                prev_kf = kf;
            }
            if kf.time >= progress {
                next_kf = kf;
                break;
            }
        }

        if (prev_kf.time - next_kf.time).abs() < f32::EPSILON {
            return Some(prev_kf.value.clone());
        }

        let local_progress = (progress - prev_kf.time) / (next_kf.time - prev_kf.time);
        let eased = next_kf.easing.apply(local_progress);

        Some(prev_kf.value.lerp(&next_kf.value, eased))
    }
}

// ============================================================================
// Type Aliases for Common Types
// ============================================================================

/// Keyframe animation for f32 values
pub type FloatAnimation = TypedKeyframeAnimation<f32>;

/// Keyframe animation for Vec3 values (positions, scales)
pub type Vec3Animation = TypedKeyframeAnimation<Vec3>;

/// Keyframe animation for Color values
pub type ColorAnimation = TypedKeyframeAnimation<Color>;

/// Float keyframe
pub type FloatKeyframe = TypedKeyframe<f32>;

/// Vec3 keyframe
pub type Vec3Keyframe = TypedKeyframe<Vec3>;

/// Color keyframe
pub type ColorKeyframe = TypedKeyframe<Color>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_float_interpolation() {
        assert!((0.0_f32.lerp(&1.0, 0.5) - 0.5).abs() < 1e-6);
        assert!((10.0_f32.lerp(&20.0, 0.25) - 12.5).abs() < 1e-6);
    }

    #[test]
    fn test_vec3_interpolation() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 20.0, 30.0);
        let mid = a.lerp(&b, 0.5);

        assert!((mid.x - 5.0).abs() < 1e-6);
        assert!((mid.y - 10.0).abs() < 1e-6);
        assert!((mid.z - 15.0).abs() < 1e-6);
    }

    #[test]
    fn test_typed_keyframe_animation() {
        let mut anim = Vec3Animation::new(1000)
            .at(0.0, Vec3::new(0.0, 0.0, 0.0))
            .at(1.0, Vec3::new(100.0, 0.0, 0.0));

        anim.start();

        // At start
        let v = anim.value().unwrap();
        assert!((v.x - 0.0).abs() < 1e-4);

        // Advance to middle
        anim.tick(500.0);
        let v = anim.value().unwrap();
        assert!((v.x - 50.0).abs() < 1e-4);

        // Advance to end
        anim.tick(500.0);
        let v = anim.value().unwrap();
        assert!((v.x - 100.0).abs() < 1e-4);
    }
}
