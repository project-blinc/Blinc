//! Animations as plain data.
//!
//! An [`AnimationDescriptor`] says where a value starts and ends, when, and by which [`Curve`].
//! Nothing is stepped: the value and its velocity at any time are computed from the descriptor,
//! so a reader never depends on a tick having run, a stalled frame skips ahead as a CSS
//! animation does, and the same descriptor can be given to a shader.
//!
//! Times are milliseconds on the frame clock's scale (see [`now_ms`]). Velocities are in units
//! per second, as [`Spring::velocity`](crate::Spring::velocity) reports them.

use crate::easing::cubic_bezier_with_slope;
use crate::spring::SpringConfig;

/// Where the jumps of a `steps()` curve fall, as CSS names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepJump {
    /// The first jump is at the start.
    Start,
    /// The last jump is at the end.
    End,
    /// No jump at either end: `n` steps need `n >= 2`.
    None,
    /// A jump at both ends.
    Both,
}

/// How a power curve eases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerMode {
    /// `t^n`.
    In,
    /// `1 - (1 - t)^n`.
    Out,
    /// `In` over the first half and `Out` over the second.
    InOut,
}

/// The shape of an animation's progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Linear,
    /// A CSS `cubic-bezier(x1, y1, x2, y2)`.
    CubicBezier(f32, f32, f32, f32),
    /// A CSS `steps(n, jump)`.
    Steps(u32, StepJump),
    /// A power of progress: the polynomial easings (`n` is 2, 3 or 4 for quad, cubic and quart).
    Power(u8, PowerMode),
    /// A damped spring in closed form, from rest at `from` or with `initial_velocity`, towards
    /// `to`. The descriptor's duration is when it has settled.
    Spring {
        stiffness: f32,
        damping: f32,
        mass: f32,
    },
}

/// One value's animation as data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationDescriptor {
    /// When it starts, on the frame clock's scale, in ms.
    pub start_ms: f64,
    /// How long it runs, in ms. For a spring, how long until it has settled.
    pub duration_ms: f32,
    pub from: f32,
    pub to: f32,
    pub curve: Curve,
    /// Units per second at the start. Only a spring uses it.
    pub initial_velocity: f32,
}

/// The frame clock's time in ms, the scale descriptors are evaluated on.
pub fn now_ms() -> f64 {
    blinc_core::FrameClock::now().as_secs_f64() * 1000.0
}

impl Curve {
    /// The eased progress for linear progress `t` in `[0, 1]`. A spring has no progress of its
    /// own: it is evaluated from the descriptor's values, so this is `t`.
    pub fn progress(&self, t: f32) -> f32 {
        self.progress_and_slope(t).0
    }

    /// The eased progress and its slope with respect to `t`.
    pub fn progress_and_slope(&self, t: f32) -> (f32, f32) {
        let t = t.clamp(0.0, 1.0);
        match *self {
            Curve::Linear | Curve::Spring { .. } => (t, 1.0),
            Curve::CubicBezier(x1, y1, x2, y2) => {
                let (p, slope) = cubic_bezier_with_slope(t, x1, y1, x2, y2);
                (p, slope)
            }
            Curve::Steps(n, jump) => (steps(t, n, jump), 0.0),
            Curve::Power(n, mode) => power(t, n as i32, mode),
        }
    }
}

fn steps(t: f32, n: u32, jump: StepJump) -> f32 {
    let n = n.max(1) as f32;
    let step = (t * n).floor();
    let out = match jump {
        StepJump::End => step / n,
        StepJump::Start => (step + 1.0) / n,
        StepJump::None => step / (n - 1.0).max(1.0),
        StepJump::Both => (step + 1.0) / (n + 1.0),
    };
    out.min(1.0)
}

fn power(t: f32, n: i32, mode: PowerMode) -> (f32, f32) {
    let nf = n as f32;
    match mode {
        PowerMode::In => (t.powi(n), nf * t.powi(n - 1)),
        PowerMode::Out => (1.0 - (1.0 - t).powi(n), nf * (1.0 - t).powi(n - 1)),
        PowerMode::InOut => {
            if t < 0.5 {
                (
                    2f32.powi(n - 1) * t.powi(n),
                    nf * 2f32.powi(n - 1) * t.powi(n - 1),
                )
            } else {
                let u = -2.0 * t + 2.0;
                (1.0 - u.powi(n) / 2.0, nf * u.powi(n - 1))
            }
        }
    }
}

/// The damped oscillator `x'' + 2 ζ ω x' + ω² x = 0` solved in closed form, for the displacement
/// `x` from the target. Computed in f64: the exponentials are stiff, and the closed form is
/// the thing the shader's f32 version is compared against.
#[derive(Clone, Copy)]
struct SpringSolution {
    omega: f64,
    zeta: f64,
    x0: f64,
    v0: f64,
}

impl SpringSolution {
    fn new(config: SpringConfig, x0: f32, v0: f32) -> Self {
        let (k, c, m) = (
            config.stiffness as f64,
            config.damping as f64,
            (config.mass as f64).max(1e-9),
        );
        let omega = (k.max(0.0) / m).sqrt().max(1e-9);
        Self {
            omega,
            zeta: c / (2.0 * (k.max(1e-12) * m).sqrt()),
            x0: x0 as f64,
            v0: v0 as f64,
        }
    }

    /// Displacement and velocity (per second) at `t` seconds.
    fn at(&self, t: f64) -> (f64, f64) {
        let Self {
            omega: w,
            zeta: z,
            x0,
            v0,
        } = *self;
        if (z - 1.0).abs() < 1e-6 {
            let b = v0 + w * x0;
            let e = (-w * t).exp();
            ((x0 + b * t) * e, (b - w * (x0 + b * t)) * e)
        } else if z < 1.0 {
            let wd = w * (1.0 - z * z).sqrt();
            let b = (v0 + z * w * x0) / wd;
            let e = (-z * w * t).exp();
            let (s, c) = (wd * t).sin_cos();
            (
                e * (x0 * c + b * s),
                e * (-z * w * (x0 * c + b * s) + wd * (-x0 * s + b * c)),
            )
        } else {
            let root = (z * z - 1.0).sqrt();
            let (r1, r2) = (-w * (z - root), -w * (z + root));
            let c2 = (v0 - r1 * x0) / (r2 - r1);
            let c1 = x0 - c2;
            let (e1, e2) = ((r1 * t).exp(), (r2 * t).exp());
            (c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
        }
    }

    /// An upper bound on `|x|` that falls monotonically once it is past its peak.
    fn envelope(&self, t: f64) -> f64 {
        let Self {
            omega: w,
            zeta: z,
            x0,
            v0,
        } = *self;
        if (z - 1.0).abs() < 1e-6 {
            (x0.abs() + (v0 + w * x0).abs() * t) * (-w * t).exp()
        } else if z < 1.0 {
            let wd = w * (1.0 - z * z).sqrt();
            let b = (v0 + z * w * x0) / wd;
            (x0 * x0 + b * b).sqrt() * (-z * w * t).exp()
        } else {
            let root = (z * z - 1.0).sqrt();
            let (r1, r2) = (-w * (z - root), -w * (z + root));
            let c2 = (v0 - r1 * x0) / (r2 - r1);
            let c1 = x0 - c2;
            c1.abs() * (r1 * t).exp() + c2.abs() * (r2 * t).exp()
        }
    }

    /// Seconds until the motion stays within a thousandth of its own scale of the target.
    /// Relative, so it does not depend on the units the value is in.
    fn settle_seconds(&self) -> f64 {
        const CAP: f64 = 60.0;
        let scale = self.x0.abs().max(self.v0.abs() / self.omega);
        if scale <= 0.0 {
            return 0.0;
        }
        let tol = scale * 1e-3;
        // The envelope has one peak at most, so what is above `tol` is one stretch of time. Find
        // the last point of a geometric grid still above it, then bisect to the crossing.
        let mut above = None;
        let mut t = 0.001;
        while t < CAP {
            if self.envelope(t) >= tol {
                above = Some(t);
            }
            t *= 1.12;
        }
        let Some(lo) = above else {
            return 0.0;
        };
        let (mut lo, mut hi) = (lo, (lo * 1.12).min(CAP));
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if self.envelope(mid) >= tol {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        hi
    }
}

impl AnimationDescriptor {
    /// A timed animation from `from` to `to` over `duration_ms`, by `curve`.
    pub fn timed(start_ms: f64, duration_ms: f32, from: f32, to: f32, curve: Curve) -> Self {
        Self {
            start_ms,
            duration_ms,
            from,
            to,
            curve,
            initial_velocity: 0.0,
        }
    }

    /// A spring from `from` towards `to`, leaving at `initial_velocity` units per second. Its
    /// duration is when it has settled.
    pub fn spring(
        start_ms: f64,
        from: f32,
        to: f32,
        initial_velocity: f32,
        config: SpringConfig,
    ) -> Self {
        let solution = SpringSolution::new(config, from - to, initial_velocity);
        Self {
            start_ms,
            duration_ms: (solution.settle_seconds() * 1000.0) as f32,
            from,
            to,
            curve: Curve::Spring {
                stiffness: config.stiffness,
                damping: config.damping,
                mass: config.mass,
            },
            initial_velocity,
        }
    }

    /// When it ends, on the same scale as `start_ms`.
    pub fn end_ms(&self) -> f64 {
        self.start_ms + self.duration_ms as f64
    }

    /// Whether `now_ms` is at or past the end.
    pub fn is_finished(&self, now_ms: f64) -> bool {
        now_ms >= self.end_ms()
    }

    fn spring_solution(&self) -> Option<SpringSolution> {
        match self.curve {
            Curve::Spring {
                stiffness,
                damping,
                mass,
            } => Some(SpringSolution::new(
                SpringConfig {
                    stiffness,
                    damping,
                    mass,
                },
                self.from - self.to,
                self.initial_velocity,
            )),
            _ => None,
        }
    }

    /// The value at `now_ms`: `from` before it starts and `to` once it has ended.
    pub fn evaluate(&self, now_ms: f64) -> f32 {
        if now_ms <= self.start_ms {
            return self.from;
        }
        if self.is_finished(now_ms) {
            return self.to;
        }
        if let Some(solution) = self.spring_solution() {
            let (x, _) = solution.at((now_ms - self.start_ms) / 1000.0);
            return self.to + x as f32;
        }
        let t = ((now_ms - self.start_ms) / self.duration_ms as f64) as f32;
        self.from + (self.to - self.from) * self.curve.progress(t)
    }

    /// The velocity at `now_ms`, in units per second: zero before it starts and after it ends.
    pub fn velocity(&self, now_ms: f64) -> f32 {
        if now_ms <= self.start_ms || self.is_finished(now_ms) {
            return if now_ms <= self.start_ms {
                self.initial_velocity
            } else {
                0.0
            };
        }
        if let Some(solution) = self.spring_solution() {
            let (_, v) = solution.at((now_ms - self.start_ms) / 1000.0);
            return v as f32;
        }
        let t = ((now_ms - self.start_ms) / self.duration_ms as f64) as f32;
        let (_, slope) = self.curve.progress_and_slope(t);
        (self.to - self.from) * slope / (self.duration_ms / 1000.0)
    }

    /// The same animation aimed at `new_to` from `now_ms`.
    ///
    /// A spring keeps its value and its velocity, as `Spring::set_target` does. A timed animation
    /// starts a new run from the current value, over the full duration again, with no velocity
    /// carried: what a CSS transition does when its target changes.
    pub fn retarget(&self, now_ms: f64, new_to: f32) -> Self {
        let from = self.evaluate(now_ms);
        match self.curve {
            Curve::Spring {
                stiffness,
                damping,
                mass,
            } => Self::spring(
                now_ms,
                from,
                new_to,
                self.velocity(now_ms),
                SpringConfig {
                    stiffness,
                    damping,
                    mass,
                },
            ),
            _ => Self::timed(now_ms, self.duration_ms, from, new_to, self.curve),
        }
    }
}

/// Two colours mixed the way CSS mixes them: in sRGB, premultiplied by alpha, so a fade from
/// transparent does not pass through black. The one definition the CPU and the shader share.
pub fn lerp_color_premultiplied(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let alpha = a[3] + (b[3] - a[3]) * t;
    if alpha <= 0.0 {
        return [b[0], b[1], b[2], 0.0];
    }
    let mix = |i: usize| (a[i] * a[3] + (b[i] * b[3] - a[i] * a[3]) * t) / alpha;
    [mix(0), mix(1), mix(2), alpha]
}

#[cfg(test)]
mod tests;
