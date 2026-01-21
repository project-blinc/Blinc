//! Particle modules for color, size, and other properties over lifetime

use blinc_core::Color;

/// Color gradient for particle color over lifetime
#[derive(Clone, Debug)]
pub struct ColorOverLifetime {
    /// Color keys (normalized time 0-1, color)
    pub keys: Vec<(f32, Color)>,
}

impl ColorOverLifetime {
    /// Create a new color gradient with start and end colors
    pub fn new(start: Color, end: Color) -> Self {
        Self {
            keys: vec![(0.0, start), (1.0, end)],
        }
    }

    /// Create a constant color (no change over lifetime)
    pub fn constant(color: Color) -> Self {
        Self {
            keys: vec![(0.0, color), (1.0, color)],
        }
    }

    /// Create a gradient with multiple color stops
    pub fn gradient(keys: Vec<(f32, Color)>) -> Self {
        let mut sorted_keys = keys;
        sorted_keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        Self { keys: sorted_keys }
    }

    /// Add a color key
    pub fn with_key(mut self, time: f32, color: Color) -> Self {
        self.keys.push((time.clamp(0.0, 1.0), color));
        self.keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        self
    }

    /// Sample color at normalized time (0 = birth, 1 = death)
    pub fn sample(&self, t: f32) -> Color {
        if self.keys.is_empty() {
            return Color::WHITE;
        }

        let t = t.clamp(0.0, 1.0);

        // Find surrounding keys
        let mut prev_idx = 0;
        for (i, (key_t, _)) in self.keys.iter().enumerate() {
            if *key_t <= t {
                prev_idx = i;
            } else {
                break;
            }
        }

        let next_idx = (prev_idx + 1).min(self.keys.len() - 1);

        if prev_idx == next_idx {
            return self.keys[prev_idx].1;
        }

        let (t0, c0) = self.keys[prev_idx];
        let (t1, c1) = self.keys[next_idx];

        let local_t = if (t1 - t0).abs() < 0.0001 {
            0.0
        } else {
            (t - t0) / (t1 - t0)
        };

        Color::lerp(&c0, &c1, local_t)
    }

    // ========== Presets ==========

    /// Fire color gradient (yellow -> orange -> red -> dark)
    pub fn fire() -> Self {
        Self::gradient(vec![
            (0.0, Color::rgb(1.0, 1.0, 0.8)),
            (0.2, Color::rgb(1.0, 0.8, 0.2)),
            (0.5, Color::rgb(1.0, 0.4, 0.1)),
            (0.8, Color::rgb(0.8, 0.1, 0.0)),
            (1.0, Color::rgba(0.2, 0.0, 0.0, 0.0)),
        ])
    }

    /// Smoke gradient (white/gray with fade out)
    pub fn smoke() -> Self {
        Self::gradient(vec![
            (0.0, Color::rgba(0.8, 0.8, 0.8, 0.6)),
            (0.5, Color::rgba(0.5, 0.5, 0.5, 0.4)),
            (1.0, Color::rgba(0.3, 0.3, 0.3, 0.0)),
        ])
    }

    /// Spark gradient (white -> yellow -> fade)
    pub fn sparks() -> Self {
        Self::gradient(vec![
            (0.0, Color::rgb(1.0, 1.0, 1.0)),
            (0.3, Color::rgb(1.0, 0.9, 0.5)),
            (0.7, Color::rgb(1.0, 0.5, 0.2)),
            (1.0, Color::rgba(0.8, 0.2, 0.0, 0.0)),
        ])
    }

    /// Magic/mystical gradient
    pub fn magic() -> Self {
        Self::gradient(vec![
            (0.0, Color::rgb(0.8, 0.4, 1.0)),
            (0.3, Color::rgb(0.4, 0.6, 1.0)),
            (0.6, Color::rgb(0.2, 0.8, 0.8)),
            (1.0, Color::rgba(0.1, 0.5, 1.0, 0.0)),
        ])
    }

    /// Simple fade out (keeps color, reduces alpha)
    pub fn fade_out(color: Color) -> Self {
        Self::gradient(vec![
            (0.0, color),
            (1.0, Color::rgba(color.r, color.g, color.b, 0.0)),
        ])
    }
}

impl Default for ColorOverLifetime {
    fn default() -> Self {
        Self::constant(Color::WHITE)
    }
}

/// Size curve for particle size over lifetime
#[derive(Clone, Debug)]
pub struct SizeOverLifetime {
    /// Size keys (normalized time 0-1, size multiplier)
    pub keys: Vec<(f32, f32)>,
}

impl SizeOverLifetime {
    /// Create a new size curve with start and end sizes
    pub fn new(start: f32, end: f32) -> Self {
        Self {
            keys: vec![(0.0, start), (1.0, end)],
        }
    }

    /// Create a constant size (no change over lifetime)
    pub fn constant(size: f32) -> Self {
        Self {
            keys: vec![(0.0, size), (1.0, size)],
        }
    }

    /// Create a curve with multiple size stops
    pub fn curve(keys: Vec<(f32, f32)>) -> Self {
        let mut sorted_keys = keys;
        sorted_keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        Self { keys: sorted_keys }
    }

    /// Add a size key
    pub fn with_key(mut self, time: f32, size: f32) -> Self {
        self.keys.push((time.clamp(0.0, 1.0), size));
        self.keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        self
    }

    /// Sample size at normalized time (0 = birth, 1 = death)
    pub fn sample(&self, t: f32) -> f32 {
        if self.keys.is_empty() {
            return 1.0;
        }

        let t = t.clamp(0.0, 1.0);

        // Find surrounding keys
        let mut prev_idx = 0;
        for (i, (key_t, _)) in self.keys.iter().enumerate() {
            if *key_t <= t {
                prev_idx = i;
            } else {
                break;
            }
        }

        let next_idx = (prev_idx + 1).min(self.keys.len() - 1);

        if prev_idx == next_idx {
            return self.keys[prev_idx].1;
        }

        let (t0, s0) = self.keys[prev_idx];
        let (t1, s1) = self.keys[next_idx];

        let local_t = if (t1 - t0).abs() < 0.0001 {
            0.0
        } else {
            (t - t0) / (t1 - t0)
        };

        // Linear interpolation
        s0 + (s1 - s0) * local_t
    }

    /// Sample with easing function
    pub fn sample_eased(&self, t: f32, easing: Easing) -> f32 {
        let eased_t = easing.apply(t);
        self.sample(eased_t)
    }

    // ========== Presets ==========

    /// Grow then shrink
    pub fn grow_shrink() -> Self {
        Self::curve(vec![
            (0.0, 0.0),
            (0.3, 1.0),
            (0.7, 1.0),
            (1.0, 0.0),
        ])
    }

    /// Start small, grow large
    pub fn expand() -> Self {
        Self::new(0.2, 1.5)
    }

    /// Start large, shrink to nothing
    pub fn shrink() -> Self {
        Self::new(1.0, 0.0)
    }

    /// Pulse/throb effect
    pub fn pulse() -> Self {
        Self::curve(vec![
            (0.0, 1.0),
            (0.25, 1.3),
            (0.5, 1.0),
            (0.75, 1.3),
            (1.0, 0.0),
        ])
    }

    /// Quick pop then fade
    pub fn pop() -> Self {
        Self::curve(vec![
            (0.0, 0.5),
            (0.1, 1.2),
            (0.2, 1.0),
            (1.0, 0.0),
        ])
    }
}

impl Default for SizeOverLifetime {
    fn default() -> Self {
        Self::constant(1.0)
    }
}

/// Velocity over lifetime modifier
#[derive(Clone, Debug)]
pub struct VelocityOverLifetime {
    /// Velocity multiplier keys (normalized time 0-1, multiplier)
    pub keys: Vec<(f32, f32)>,
}

impl VelocityOverLifetime {
    /// Create a new velocity curve
    pub fn new(start_multiplier: f32, end_multiplier: f32) -> Self {
        Self {
            keys: vec![(0.0, start_multiplier), (1.0, end_multiplier)],
        }
    }

    /// Constant velocity (no change)
    pub fn constant() -> Self {
        Self::new(1.0, 1.0)
    }

    /// Slow down over time
    pub fn decelerate() -> Self {
        Self::new(1.0, 0.0)
    }

    /// Speed up over time
    pub fn accelerate() -> Self {
        Self::new(0.2, 1.0)
    }

    /// Sample velocity multiplier
    pub fn sample(&self, t: f32) -> f32 {
        if self.keys.is_empty() {
            return 1.0;
        }

        let t = t.clamp(0.0, 1.0);

        let mut prev_idx = 0;
        for (i, (key_t, _)) in self.keys.iter().enumerate() {
            if *key_t <= t {
                prev_idx = i;
            } else {
                break;
            }
        }

        let next_idx = (prev_idx + 1).min(self.keys.len() - 1);

        if prev_idx == next_idx {
            return self.keys[prev_idx].1;
        }

        let (t0, v0) = self.keys[prev_idx];
        let (t1, v1) = self.keys[next_idx];

        let local_t = if (t1 - t0).abs() < 0.0001 {
            0.0
        } else {
            (t - t0) / (t1 - t0)
        };

        v0 + (v1 - v0) * local_t
    }
}

impl Default for VelocityOverLifetime {
    fn default() -> Self {
        Self::constant()
    }
}

/// Rotation over lifetime
#[derive(Clone, Debug)]
pub struct RotationOverLifetime {
    /// Angular velocity in radians per second
    pub angular_velocity: f32,
    /// Whether velocity changes over lifetime
    pub velocity_curve: Option<Vec<(f32, f32)>>,
}

impl RotationOverLifetime {
    /// Create constant rotation
    pub fn constant(angular_velocity: f32) -> Self {
        Self {
            angular_velocity,
            velocity_curve: None,
        }
    }

    /// Create rotation that slows down
    pub fn decelerate(start_velocity: f32) -> Self {
        Self {
            angular_velocity: start_velocity,
            velocity_curve: Some(vec![(0.0, 1.0), (1.0, 0.0)]),
        }
    }

    /// Sample angular velocity at time
    pub fn sample(&self, t: f32) -> f32 {
        if let Some(ref curve) = self.velocity_curve {
            let mut multiplier = 1.0;
            let t = t.clamp(0.0, 1.0);

            let mut prev_idx = 0;
            for (i, (key_t, _)) in curve.iter().enumerate() {
                if *key_t <= t {
                    prev_idx = i;
                } else {
                    break;
                }
            }

            let next_idx = (prev_idx + 1).min(curve.len() - 1);

            if prev_idx != next_idx {
                let (t0, v0) = curve[prev_idx];
                let (t1, v1) = curve[next_idx];
                let local_t = if (t1 - t0).abs() < 0.0001 {
                    0.0
                } else {
                    (t - t0) / (t1 - t0)
                };
                multiplier = v0 + (v1 - v0) * local_t;
            } else {
                multiplier = curve[prev_idx].1;
            }

            self.angular_velocity * multiplier
        } else {
            self.angular_velocity
        }
    }
}

impl Default for RotationOverLifetime {
    fn default() -> Self {
        Self::constant(0.0)
    }
}

/// Easing functions for curves
#[derive(Clone, Copy, Debug, Default)]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    EaseInQuad,
    EaseOutQuad,
    EaseInCubic,
    EaseOutCubic,
}

impl Easing {
    /// Apply easing to a value
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Easing::EaseInQuad => t * t,
            Easing::EaseOutQuad => t * (2.0 - t),
            Easing::EaseInCubic => t * t * t,
            Easing::EaseOutCubic => {
                let t = t - 1.0;
                t * t * t + 1.0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_gradient() {
        let gradient = ColorOverLifetime::new(Color::WHITE, Color::BLACK);

        let start = gradient.sample(0.0);
        assert!((start.r - 1.0).abs() < 0.001);

        let mid = gradient.sample(0.5);
        assert!((mid.r - 0.5).abs() < 0.001);

        let end = gradient.sample(1.0);
        assert!((end.r - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_size_curve() {
        let curve = SizeOverLifetime::grow_shrink();

        let start = curve.sample(0.0);
        assert!((start - 0.0).abs() < 0.001);

        let mid = curve.sample(0.5);
        assert!((mid - 1.0).abs() < 0.001);

        let end = curve.sample(1.0);
        assert!((end - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_easing() {
        assert!((Easing::Linear.apply(0.5) - 0.5).abs() < 0.001);
        assert!(Easing::EaseIn.apply(0.5) < 0.5);
        assert!(Easing::EaseOut.apply(0.5) > 0.5);
    }
}
