use ttf_parser::{
    NormalizedCoordinate, RgbaColor,
    colr::{ColorStop, GradientExtend, Paint},
};

pub(super) enum Brush {
    Solid([f32; 4]),
    Gradient {
        geometry: Geometry,
        stops: Vec<(f32, [f32; 4])>,
        extend: GradientExtend,
    },
}

pub(super) enum Geometry {
    Linear([f32; 6]),
    Radial([f32; 6]),
    Sweep([f32; 4]),
}

fn color(c: RgbaColor) -> [f32; 4] {
    let alpha = c.alpha as f32 / 255.0;
    [
        c.red as f32 * alpha,
        c.green as f32 * alpha,
        c.blue as f32 * alpha,
        c.alpha as f32,
    ]
}

impl Brush {
    pub fn new(paint: Paint<'_>, coordinates: &[NormalizedCoordinate]) -> Self {
        let (geometry, stops, extend) = match paint {
            Paint::Solid(c) => return Self::Solid(color(c)),
            Paint::LinearGradient(g) => (
                Geometry::Linear([g.x0, g.y0, g.x1, g.y1, g.x2, g.y2]),
                g.stops(0, coordinates).collect::<Vec<_>>(),
                g.extend,
            ),
            Paint::RadialGradient(g) => (
                Geometry::Radial([g.x0, g.y0, g.r0, g.x1, g.y1, g.r1]),
                g.stops(0, coordinates).collect::<Vec<_>>(),
                g.extend,
            ),
            Paint::SweepGradient(g) => (
                Geometry::Sweep([
                    g.center_x,
                    g.center_y,
                    (g.start_angle + 1.0) * std::f32::consts::PI,
                    (g.end_angle + 1.0) * std::f32::consts::PI,
                ]),
                g.stops(0, coordinates).collect::<Vec<_>>(),
                g.extend,
            ),
        };
        let mut stops: Vec<_> = stops
            .into_iter()
            .map(
                |ColorStop {
                     stop_offset,
                     color: c,
                 }| (stop_offset, color(c)),
            )
            .collect();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self::Gradient {
            geometry,
            stops,
            extend,
        }
    }

    pub fn sample(&self, x: f32, y: f32) -> [u8; 4] {
        let value = match self {
            Self::Solid(c) => *c,
            Self::Gradient {
                geometry,
                stops,
                extend,
            } => {
                let Some(t) = geometry.parameter(x, y, *extend) else {
                    return [0; 4];
                };
                let Some(&(first, first_color)) = stops.first() else {
                    return [0; 4];
                };
                let &(last, last_color) = stops.last().unwrap();
                let period = last - first;
                if period <= f32::EPSILON {
                    return match extend {
                        GradientExtend::Pad => last_color.map(|c| c.round() as u8),
                        _ => [0; 4],
                    };
                }
                let t = match extend {
                    GradientExtend::Pad => t,
                    GradientExtend::Repeat => (t - first).rem_euclid(period) + first,
                    GradientExtend::Reflect => {
                        let t = (t - first).rem_euclid(period * 2.0);
                        first + if t > period { period * 2.0 - t } else { t }
                    }
                };
                if t < first {
                    first_color
                } else if t >= last {
                    last_color
                } else {
                    let index = stops.partition_point(|s| s.0 <= t);
                    let (a, ca) = stops[index - 1];
                    let (b, cb) = stops[index];
                    let fraction = (t - a) / (b - a);
                    std::array::from_fn(|i| ca[i] + (cb[i] - ca[i]) * fraction)
                }
            }
        };
        value.map(|c| c.round().clamp(0.0, 255.0) as u8)
    }
}

impl Geometry {
    fn parameter(&self, x: f32, y: f32, extend: GradientExtend) -> Option<f32> {
        match *self {
            Self::Linear([x0, y0, x1, y1, x2, y2]) => {
                let (nx, ny) = (y2 - y0, x0 - x2);
                let denominator = (x1 - x0) * nx + (y1 - y0) * ny;
                (denominator.abs() > f32::EPSILON)
                    .then(|| ((x - x0) * nx + (y - y0) * ny) / denominator)
            }
            Self::Radial([x0, y0, r0, x1, y1, r1]) => {
                let (dx, dy, dr, vx, vy) = (x1 - x0, y1 - y0, r1 - r0, x - x0, y - y0);
                let a = dx * dx + dy * dy - dr * dr;
                let b = -2.0 * (vx * dx + vy * dy + r0 * dr);
                let c = vx * vx + vy * vy - r0 * r0;
                if a.abs() < 1e-6 {
                    let t = -c / b;
                    return (b.abs() > 1e-6 && t.is_finite() && r0 + t * dr >= 0.0).then_some(t);
                }
                let discriminant = b * b - 4.0 * a * c;
                if discriminant < 0.0 {
                    return None;
                }
                [
                    (-b - discriminant.sqrt()) / (2.0 * a),
                    (-b + discriminant.sqrt()) / (2.0 * a),
                ]
                .into_iter()
                .filter(|t| t.is_finite() && r0 + t * dr >= 0.0)
                .max_by(f32::total_cmp)
            }
            Self::Sweep([cx, cy, start, end]) => {
                let angle = (y - cy).atan2(x - cx).rem_euclid(std::f32::consts::TAU);
                if start == end {
                    return matches!(extend, GradientExtend::Pad).then_some(if angle < start {
                        f32::NEG_INFINITY
                    } else {
                        f32::INFINITY
                    });
                }
                Some((angle - start) / (end - start))
            }
        }
    }
}
