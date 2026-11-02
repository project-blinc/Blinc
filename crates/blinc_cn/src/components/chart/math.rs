//! The arithmetic charts share: value-axis ticks, compact numbers, and the
//! curves a line takes through its points.

use blinc_core::Path;

use super::ChartCurve;

/// A number as an axis or tooltip shows it by default: `12.5k`, `3.2M`, or
/// to two decimals below a thousand, without trailing zeros.
pub fn compact(v: f64) -> String {
    let a = v.abs();
    let (scaled, unit) = if a >= 1e9 {
        (v / 1e9, "B")
    } else if a >= 1e6 {
        (v / 1e6, "M")
    } else if a >= 1e3 {
        (v / 1e3, "k")
    } else {
        (v, "")
    };
    let rounded = (scaled * 100.0).round() / 100.0;
    let mut s = format!("{rounded}");
    if s == "-0" {
        s = "0".into();
    }
    s + unit
}

/// Evenly spaced round values covering `lo..=hi`, about `count` of them:
/// steps of 1, 2 or 5 times a power of ten. The first is at or below `lo`
/// and the last at or above `hi`.
pub fn nice_ticks(lo: f64, hi: f64, count: usize) -> Vec<f64> {
    let (lo, hi) = if hi > lo { (lo, hi) } else { (lo, lo + 1.0) };
    let rough = (hi - lo) / count.max(1) as f64;
    let power = 10f64.powf(rough.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * power)
        .find(|s| *s >= rough)
        .unwrap_or(10.0 * power);
    let first = (lo / step).floor() * step;
    let mut ticks = Vec::new();
    let mut t = first;
    // Rounded to the step's precision so 0.1 + 0.2 reads 0.3.
    let places = (-step.log10().floor()).max(0.0) as i32 + 1;
    let scale = 10f64.powi(places);
    // Up to the first tick at or past `hi`; bounded, whatever the floats do.
    for _ in 0..1000 {
        let tick = (t * scale).round() / scale;
        ticks.push(tick);
        if tick >= hi - step * 1e-9 {
            break;
        }
        t += step;
    }
    ticks
}

/// Adds the segments through `(xs, ys)` to `path`: a move to the first point
/// when `start`, otherwise a line to it.
pub fn follow(mut path: Path, xs: &[f32], ys: &[f32], curve: ChartCurve, start: bool) -> Path {
    let n = xs.len();
    if n == 0 {
        return path;
    }
    path = if start {
        path.move_to(xs[0], ys[0])
    } else {
        path.line_to(xs[0], ys[0])
    };
    if n < 2 {
        return path;
    }
    match curve {
        ChartCurve::Linear => {
            for i in 1..n {
                path = path.line_to(xs[i], ys[i]);
            }
        }
        ChartCurve::Step => {
            for i in 1..n {
                let mid = (xs[i - 1] + xs[i]) / 2.0;
                path = path
                    .line_to(mid, ys[i - 1])
                    .line_to(mid, ys[i])
                    .line_to(xs[i], ys[i]);
            }
        }
        ChartCurve::Smooth => {
            // Monotone cubic (Fritsch and Carlson): no bump past a value that
            // is not there.
            let d: Vec<f32> = (0..n - 1)
                .map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]))
                .collect();
            let mut m: Vec<f32> = (0..n)
                .map(|i| {
                    if i == 0 {
                        d[0]
                    } else if i == n - 1 {
                        d[n - 2]
                    } else if d[i - 1] * d[i] <= 0.0 {
                        0.0
                    } else {
                        (d[i - 1] + d[i]) / 2.0
                    }
                })
                .collect();
            for i in 0..n - 1 {
                if d[i] == 0.0 {
                    m[i] = 0.0;
                    m[i + 1] = 0.0;
                    continue;
                }
                let (a, b) = (m[i] / d[i], m[i + 1] / d[i]);
                let s = a * a + b * b;
                if s > 9.0 {
                    let t = 3.0 / s.sqrt();
                    m[i] = t * a * d[i];
                    m[i + 1] = t * b * d[i];
                }
            }
            for i in 0..n - 1 {
                let h = (xs[i + 1] - xs[i]) / 3.0;
                path = path.cubic_to(
                    xs[i] + h,
                    ys[i] + m[i] * h,
                    xs[i + 1] - h,
                    ys[i + 1] - m[i + 1] * h,
                    xs[i + 1],
                    ys[i + 1],
                );
            }
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_compactly() {
        assert_eq!(compact(0.0), "0");
        assert_eq!(compact(12.345), "12.35");
        assert_eq!(compact(1500.0), "1.5k");
        assert_eq!(compact(-2_400_000.0), "-2.4M");
        assert_eq!(compact(40.0), "40");
    }

    #[test]
    fn ticks_are_round_and_cover_the_range() {
        assert_eq!(nice_ticks(0.0, 600.0, 4), vec![0.0, 200.0, 400.0, 600.0]);
        assert_eq!(nice_ticks(0.2, 0.9, 4), vec![0.2, 0.4, 0.6, 0.8, 1.0]);
        let t = nice_ticks(13.0, 87.0, 5);
        assert!(t[0] <= 13.0 && *t.last().unwrap() >= 87.0, "{t:?}");
    }
}
