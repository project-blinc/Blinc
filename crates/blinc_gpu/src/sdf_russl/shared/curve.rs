//! Animation curves on the GPU: the progress and slope of a curve, the value and velocity of a
//! descriptor at a time, and the colour mix. The maths is `blinc_animation`'s descriptor, kept in
//! step by checking both against one file of vectors computed outside either.
//!
//! Curve kinds: 0 linear, 1 cubic bezier (`p` is x1 y1 x2 y2), 2 steps (`n` steps, `jump` 0
//! start 1 end 2 none 3 both), 3 power (`p.x` is the exponent, `jump` 0 in 1 out 2 in-out),
//! 4 spring (`p` is stiffness damping mass). Times are ms, velocities units per second.

russl::russl! {
    // x^n for a small whole n, by multiplying: exact where `pow` is not.
    fn ipow(x: f32, n: u32) -> f32 {
        let mut r = 1.0;
        for i in 0u32..n {
            r = r * x;
        }
        return r;
    }

    fn bezier_sample(s: f32, p1: f32, p2: f32) -> f32 {
        let u = 1.0 - s;
        return 3.0 * u * u * s * p1 + 3.0 * u * s * s * p2 + s * s * s;
    }

    fn bezier_slope(s: f32, p1: f32, p2: f32) -> f32 {
        let u = 1.0 - s;
        return 3.0 * u * u * p1 + 6.0 * u * s * (p2 - p1) + 3.0 * s * s * (1.0 - p2);
    }

    // Progress and its slope for a CSS cubic-bezier: the parameter where x(s) = t by bisection,
    // then y and dy/dx there.
    fn bezier_progress(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> Vec2 {
        let mut lo = 0.0;
        let mut hi = 1.0;
        for i in 0u32..26u32 {
            let mid = 0.5 * (lo + hi);
            if bezier_sample(mid, x1, x2) < t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let mut s = 0.5 * (lo + hi);
        if t <= 0.0 {
            s = 0.0;
        }
        if t >= 1.0 {
            s = 1.0;
        }
        let dx = bezier_slope(s, x1, x2);
        let mut slope = 0.0;
        if abs(dx) > 0.000000001 {
            slope = bezier_slope(s, y1, y2) / dx;
        }
        let mut value = bezier_sample(s, y1, y2);
        if t <= 0.0 {
            value = 0.0;
        }
        if t >= 1.0 {
            value = 1.0;
        }
        return Vec2::new(value, slope);
    }

    fn steps_progress(t: f32, n: u32, jump: u32) -> f32 {
        let nf = n as f32;
        let step = floor(t * nf);
        let mut out = (step + 1.0) / (nf + 1.0);
        if jump == 0u32 {
            out = (step + 1.0) / nf;
        }
        if jump == 1u32 {
            out = step / nf;
        }
        if jump == 2u32 {
            out = step / max(nf - 1.0, 1.0);
        }
        return min(out, 1.0);
    }

    fn power_progress(t: f32, n: u32, mode: u32) -> Vec2 {
        let nf = n as f32;
        if mode == 0u32 {
            return Vec2::new(ipow(t, n), nf * ipow(t, n - 1u32));
        }
        if mode == 1u32 {
            return Vec2::new(1.0 - ipow(1.0 - t, n), nf * ipow(1.0 - t, n - 1u32));
        }
        let scale = ipow(2.0, n - 1u32);
        if t < 0.5 {
            return Vec2::new(scale * ipow(t, n), nf * scale * ipow(t, n - 1u32));
        }
        let u = -2.0 * t + 2.0;
        return Vec2::new(1.0 - ipow(u, n) / 2.0, nf * ipow(u, n - 1u32));
    }

    // Progress and slope of a curve at linear progress `t`.
    pub fn curve_progress(kind: u32, p: Vec4, n: u32, jump: u32, t: f32) -> Vec2 {
        let tc = clamp(t, 0.0, 1.0);
        if kind == 1u32 {
            return bezier_progress(tc, p.x, p.y, p.z, p.w);
        }
        if kind == 2u32 {
            return Vec2::new(steps_progress(tc, n, jump), 0.0);
        }
        if kind == 3u32 {
            return power_progress(tc, p.x as u32, jump);
        }
        return Vec2::new(tc, 1.0);
    }

    // The damped oscillator solved in closed form: displacement from the target and its velocity
    // at `t` seconds, from displacement `x0` and velocity `v0`.
    pub fn spring_motion(k: f32, c: f32, m: f32, x0: f32, v0: f32, t: f32) -> Vec2 {
        let w = sqrt(k / m);
        let z = c / (2.0 * sqrt(k * m));
        if abs(z - 1.0) < 0.000001 {
            let b = v0 + w * x0;
            let e = exp(-w * t);
            return Vec2::new((x0 + b * t) * e, (b - w * (x0 + b * t)) * e);
        }
        if z < 1.0 {
            let wd = w * sqrt(1.0 - z * z);
            let b = (v0 + z * w * x0) / wd;
            let e = exp(-z * w * t);
            let s = sin(wd * t);
            let co = cos(wd * t);
            return Vec2::new(
                e * (x0 * co + b * s),
                e * (-z * w * (x0 * co + b * s) + wd * (-x0 * s + b * co))
            );
        }
        let root = sqrt(z * z - 1.0);
        let r1 = -w * (z - root);
        let r2 = -w * (z + root);
        let c2 = (v0 - r1 * x0) / (r2 - r1);
        let c1 = x0 - c2;
        let e1 = exp(r1 * t);
        let e2 = exp(r2 * t);
        return Vec2::new(c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2);
    }

    // A descriptor's value and velocity `elapsed_ms` after it starts: `from` before, `to` once
    // it has run `duration_ms`.
    pub fn eval_descriptor(
        from: f32, to: f32, duration_ms: f32, v0: f32,
        kind: u32, p: Vec4, n: u32, jump: u32, elapsed_ms: f32
    ) -> Vec2 {
        if elapsed_ms <= 0.0 {
            return Vec2::new(from, v0);
        }
        if elapsed_ms >= duration_ms {
            return Vec2::new(to, 0.0);
        }
        if kind == 4u32 {
            let motion = spring_motion(p.x, p.y, p.z, from - to, v0, elapsed_ms / 1000.0);
            return Vec2::new(to + motion.x, motion.y);
        }
        let progress = curve_progress(kind, p, n, jump, elapsed_ms / duration_ms);
        return Vec2::new(
            from + (to - from) * progress.x,
            (to - from) * progress.y / (duration_ms / 1000.0)
        );
    }

    // Two colours mixed in sRGB premultiplied by alpha, as CSS mixes them.
    pub fn lerp_color_premultiplied(a: Vec4, b: Vec4, t: f32) -> Vec4 {
        let alpha = a.w + (b.w - a.w) * t;
        if alpha <= 0.0 {
            return Vec4::new(b.x, b.y, b.z, 0.0);
        }
        let rgb = (a.xyz * a.w + (b.xyz * b.w - a.xyz * a.w) * t) / alpha;
        return Vec4::new(rgb.x, rgb.y, rgb.z, alpha);
    }
}
