//! Camera shake effect
//!
//! Trauma-based screen shake using Perlin-like noise.

use super::CameraTransform;
use crate::ecs::Component;
use crate::math::Quat;
use blinc_core::Vec3;

/// Camera shake effect
///
/// Implements the "trauma" model where shake intensity is trauma squared,
/// providing natural-feeling screen shake that ramps up and decays smoothly.
///
/// # Example
///
/// ```ignore
/// let mut shake = CameraShake::new();
/// shake.max_offset = Vec3::new(0.5, 0.5, 0.2);
/// shake.max_rotation = Vec3::new(0.03, 0.03, 0.05);
///
/// // Add trauma when something happens
/// shake.add_trauma(0.5); // Moderate shake
/// shake.add_trauma(1.0); // Maximum shake
/// ```
#[derive(Clone, Debug)]
pub struct CameraShake {
    /// Current trauma level (0.0 to 1.0)
    trauma: f32,
    /// Trauma decay rate per second
    pub decay_rate: f32,

    /// Maximum position offset
    pub max_offset: Vec3,
    /// Maximum rotation (in radians, as euler angles)
    pub max_rotation: Vec3,

    /// Noise frequency (higher = more rapid shake)
    pub frequency: f32,
    /// Current noise time (advances with real time)
    noise_time: f32,

    /// Whether shake is enabled
    enabled: bool,
}

impl Component for CameraShake {}

impl CameraShake {
    /// Create a new camera shake effect
    pub fn new() -> Self {
        Self {
            trauma: 0.0,
            decay_rate: 1.5,

            max_offset: Vec3::new(0.3, 0.3, 0.1),
            max_rotation: Vec3::new(0.02, 0.02, 0.04),

            frequency: 15.0,
            noise_time: 0.0,

            enabled: true,
        }
    }

    /// Add trauma (clamped to 0-1)
    pub fn add_trauma(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }

    /// Set trauma directly
    pub fn set_trauma(&mut self, trauma: f32) {
        self.trauma = trauma.clamp(0.0, 1.0);
    }

    /// Get current trauma level
    pub fn trauma(&self) -> f32 {
        self.trauma
    }

    /// Get current shake intensity (trauma squared)
    pub fn intensity(&self) -> f32 {
        self.trauma * self.trauma
    }

    /// Check if currently shaking
    pub fn is_shaking(&self) -> bool {
        self.trauma > 0.001
    }

    /// Update shake state and return offset transform
    pub fn update(&mut self, dt: f32) -> CameraTransform {
        if !self.enabled {
            return CameraTransform::default();
        }

        // Decay trauma
        self.trauma = (self.trauma - self.decay_rate * dt).max(0.0);

        if self.trauma < 0.001 {
            return CameraTransform::default();
        }

        // Advance noise time
        self.noise_time += dt * self.frequency;

        let intensity = self.intensity();

        // Generate pseudo-random noise values using simple hash
        let offset = Vec3::new(
            self.noise(self.noise_time, 0) * self.max_offset.x * intensity,
            self.noise(self.noise_time, 1) * self.max_offset.y * intensity,
            self.noise(self.noise_time, 2) * self.max_offset.z * intensity,
        );

        let rotation_angles = Vec3::new(
            self.noise(self.noise_time, 3) * self.max_rotation.x * intensity,
            self.noise(self.noise_time, 4) * self.max_rotation.y * intensity,
            self.noise(self.noise_time, 5) * self.max_rotation.z * intensity,
        );

        let rotation = Quat::from_euler_yxz(rotation_angles.y, rotation_angles.x, rotation_angles.z);

        CameraTransform {
            position: offset,
            rotation,
        }
    }

    /// Apply shake to an existing transform
    pub fn apply(&mut self, dt: f32, transform: &CameraTransform) -> CameraTransform {
        let shake = self.update(dt);

        // Apply shake offset in camera-local space
        let offset = transform.rotation.rotate_vec3(shake.position);

        CameraTransform {
            position: Vec3::new(
                transform.position.x + offset.x,
                transform.position.y + offset.y,
                transform.position.z + offset.z,
            ),
            rotation: transform.rotation.multiply(shake.rotation),
        }
    }

    /// Simple noise function using sine waves
    fn noise(&self, t: f32, seed: u32) -> f32 {
        let s = seed as f32;
        let a = (t * 1.0 + s * 12.9898).sin() * 43758.5453;
        let b = (t * 2.3 + s * 78.233).sin() * 24634.6345;
        let c = (t * 0.7 + s * 45.164).sin() * 83456.2345;
        ((a + b + c).fract() * 2.0 - 1.0)
    }

    /// Enable or disable shake
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.trauma = 0.0;
        }
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for CameraShake {
    fn default() -> Self {
        Self::new()
    }
}

/// Preset shake configurations
impl CameraShake {
    /// Light shake for minor impacts
    pub fn light() -> Self {
        Self {
            max_offset: Vec3::new(0.1, 0.1, 0.05),
            max_rotation: Vec3::new(0.01, 0.01, 0.02),
            decay_rate: 2.0,
            frequency: 20.0,
            ..Self::new()
        }
    }

    /// Medium shake for explosions
    pub fn medium() -> Self {
        Self {
            max_offset: Vec3::new(0.3, 0.3, 0.15),
            max_rotation: Vec3::new(0.03, 0.03, 0.05),
            decay_rate: 1.5,
            frequency: 15.0,
            ..Self::new()
        }
    }

    /// Heavy shake for massive impacts
    pub fn heavy() -> Self {
        Self {
            max_offset: Vec3::new(0.6, 0.6, 0.3),
            max_rotation: Vec3::new(0.05, 0.05, 0.08),
            decay_rate: 1.0,
            frequency: 12.0,
            ..Self::new()
        }
    }

    /// Earthquake-like sustained shake
    pub fn earthquake() -> Self {
        Self {
            max_offset: Vec3::new(0.4, 0.2, 0.4),
            max_rotation: Vec3::new(0.02, 0.04, 0.03),
            decay_rate: 0.3,
            frequency: 8.0,
            ..Self::new()
        }
    }
}
