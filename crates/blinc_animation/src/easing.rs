//! Easing functions for animations

use crate::descriptor::{Curve, PowerMode};

/// Easing function type
#[derive(Clone, Copy, Debug, Default)]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,
    CubicBezier(f32, f32, f32, f32),
}

impl Easing {
    /// The same easing as a [`Curve`], exactly: the polynomial easings are power curves, and a
    /// cubic bezier stays one.
    pub fn to_curve(&self) -> Curve {
        use PowerMode::{In, InOut, Out};
        match *self {
            Easing::Linear => Curve::Linear,
            Easing::EaseIn | Easing::EaseInCubic => Curve::Power(3, In),
            Easing::EaseOut | Easing::EaseOutCubic => Curve::Power(3, Out),
            Easing::EaseInOut | Easing::EaseInOutCubic => Curve::Power(3, InOut),
            Easing::EaseInQuad => Curve::Power(2, In),
            Easing::EaseOutQuad => Curve::Power(2, Out),
            Easing::EaseInOutQuad => Curve::Power(2, InOut),
            Easing::EaseInQuart => Curve::Power(4, In),
            Easing::EaseOutQuart => Curve::Power(4, Out),
            Easing::EaseInOutQuart => Curve::Power(4, InOut),
            Easing::CubicBezier(x1, y1, x2, y2) => Curve::CubicBezier(x1, y1, x2, y2),
        }
    }

    /// Apply the easing function to a progress value (0.0 to 1.0)
    pub fn apply(&self, t: f32) -> f32 {
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t * t,
            Easing::EaseOut => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Easing::EaseInQuad => t * t,
            Easing::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOutQuad => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Easing::EaseInCubic => t * t * t,
            Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Easing::EaseInQuart => t * t * t * t,
            Easing::EaseOutQuart => 1.0 - (1.0 - t).powi(4),
            Easing::EaseInOutQuart => {
                if t < 0.5 {
                    8.0 * t * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
                }
            }
            Easing::CubicBezier(x1, y1, x2, y2) => cubic_bezier_ease(t, *x1, *y1, *x2, *y2),
        }
    }
}

/// Cubic bezier easing calculation (matches CSS spec / browser implementations).
fn cubic_bezier_ease(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    cubic_bezier_with_slope(t, x1, y1, x2, y2).0
}

/// The curve's progress and its slope with respect to `t`: the one solver, so an easing and a
/// descriptor agree to the last bit. Computes in f64 internally to avoid f32 precision jitter at
/// 120fps.
pub(crate) fn cubic_bezier_with_slope(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> (f32, f32) {
    let (x1, y1, x2, y2) = (x1 as f64, y1 as f64, x2 as f64, y2 as f64);
    // Endpoints are always exact
    let (p, value) = if t <= 0.0 {
        (0.0, 0.0)
    } else if t >= 1.0 {
        (1.0, 1.0)
    } else {
        let p = bezier_param(t as f64, x1, x2);
        (p, bezier_sample(p, y1, y2) as f32)
    };
    let dx = bezier_slope(p, x1, x2);
    let slope = if dx.abs() < 1e-9 {
        0.0
    } else {
        (bezier_slope(p, y1, y2) / dx) as f32
    };
    (value, slope)
}

/// The parameter `p` where `bezier_x(p) == x`, by Newton-Raphson with a binary-search fallback
/// for robustness.
fn bezier_param(x: f64, x1: f64, x2: f64) -> f64 {
    let mut p = x; // initial guess
    for _ in 0..8 {
        let err = bezier_sample(p, x1, x2) - x;
        if err.abs() < 1e-7 {
            return p;
        }
        let slope = bezier_slope(p, x1, x2);
        if slope.abs() < 1e-7 {
            break; // slope too flat, switch to binary search
        }
        p -= err / slope;
    }

    // Binary search fallback (always converges)
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    p = x;
    for _ in 0..20 {
        let val = bezier_sample(p, x1, x2);
        if (val - x).abs() < 1e-7 {
            break;
        }
        if val < x {
            lo = p;
        } else {
            hi = p;
        }
        p = (lo + hi) * 0.5;
    }
    p
}

/// Evaluate cubic bezier at parameter t: B(t) = 3(1-t)²t·p1 + 3(1-t)t²·p2 + t³
#[inline]
fn bezier_sample(t: f64, p1: f64, p2: f64) -> f64 {
    // Horner form: ((1-3p2+3p1)t + 3p2-6p1)t + 3p1) * t
    let a = 1.0 - 3.0 * p2 + 3.0 * p1;
    let b = 3.0 * p2 - 6.0 * p1;
    let c = 3.0 * p1;
    ((a * t + b) * t + c) * t
}

/// Derivative of cubic bezier: B'(t) = 3(1-t)²·p1 + 6(1-t)t·(p2-p1) + 3t²·(1-p2)
#[inline]
fn bezier_slope(t: f64, p1: f64, p2: f64) -> f64 {
    let a = 1.0 - 3.0 * p2 + 3.0 * p1;
    let b = 3.0 * p2 - 6.0 * p1;
    let c = 3.0 * p1;
    (3.0 * a * t + 2.0 * b) * t + c
}
