//! Filled path contours captured in physical screen space.
use blinc_core::{Affine2D, FillRule, Path, PathCommand, Point};
use lyon::geom::euclid::{Angle, default::Point2D};
use lyon::geom::{ArcFlags, CubicBezierSegment, QuadraticBezierSegment, SvgArc};
use std::cell::OnceCell;

type P = Point2D<f64>;
const SEPARATOR: [f32; 2] = [1e30; 2];
// Bound both curve subdivision and the fragment shader's contour traversal.
const MAX_POINTS: usize = 65_536;
// Keep squared distances and crossing products finite in the f32 clip shader.
const MAX_SCREEN_COORD: f64 = 1e18;

pub(super) struct PathClipData {
    affine: Affine2D,
    rule: FillRule,
    rows: OnceCell<Vec<[f32; 4]>>,
    pub(super) offsets: smallvec::SmallVec<[(super::AuxBatch, u32); 2]>,
}

impl PathClipData {
    pub fn new(affine: Affine2D, rule: FillRule) -> Self {
        Self {
            affine,
            rule,
            rows: OnceCell::new(),
            offsets: smallvec::SmallVec::new(),
        }
    }

    pub fn rows(&self, path: &Path) -> &[[f32; 4]] {
        self.rows
            .get_or_init(|| prepare(path, self.affine, self.rule))
    }
}

fn prepare(path: &Path, affine: Affine2D, rule: FillRule) -> Vec<[f32; 4]> {
    // Empty, invalid or excessively complex clips cover no pixels.
    let points = flatten(path, affine).unwrap_or_default();
    let mut rows = Vec::with_capacity(8 + points.len().div_ceil(2));
    rows.resize(8, [0.0; 4]);
    rows[4] = [1.0, 0.75, 0.0, 6.0];
    rows[5] = [1.0, 0.0, 0.0, 1.0];
    rows[6][2] = 3.0;
    rows[7] = [
        3.0,
        points.len() as f32,
        f32::from(rule == FillRule::EvenOdd),
        0.0,
    ];
    for pair in points.chunks(2) {
        let a = pair[0];
        let b = pair.get(1).copied().unwrap_or([0.0; 2]);
        rows.push([a[0], a[1], b[0], b[1]]);
    }
    rows
}

struct Contours {
    points: Vec<[f32; 2]>,
    first: Option<[f32; 2]>,
    start: usize,
}
impl Contours {
    fn push(&mut self, point: P) -> Option<()> {
        if !valid(point) || self.points.len() >= MAX_POINTS {
            return None;
        }
        let point = [point.x as f32, point.y as f32];
        if self.first.is_none() {
            self.first = Some(point);
            self.start = self.points.len();
        }
        self.points.push(point);
        Some(())
    }
    fn end(&mut self) -> Option<()> {
        if let Some(first) = self.first.take() {
            let contour = &self.points[self.start..];
            let noncollinear = contour.iter().find(|p| **p != first).is_some_and(|second| {
                let dx = f64::from(second[0]) - f64::from(first[0]);
                let dy = f64::from(second[1]) - f64::from(first[1]);
                contour.iter().any(|p| {
                    dx * (f64::from(p[1]) - f64::from(first[1]))
                        != dy * (f64::from(p[0]) - f64::from(first[0]))
                })
            });
            if !noncollinear {
                self.points.truncate(self.start);
                return Some(());
            }
            if self.points.len() + 2 > MAX_POINTS {
                return None;
            }
            self.points.push(first);
            self.points.push(SEPARATOR);
        }
        Some(())
    }
}

fn valid(p: P) -> bool {
    finite(p) && p.x.abs() < MAX_SCREEN_COORD && p.y.abs() < MAX_SCREEN_COORD
}
fn finite(p: P) -> bool {
    p.x.is_finite() && p.y.is_finite()
}
fn local(p: Point) -> P {
    P::new(p.x as f64, p.y as f64)
}

fn flatten(path: &Path, affine: Affine2D) -> Option<Vec<[f32; 2]>> {
    let [a, b, c, d, tx, ty] = affine.elements.map(f64::from);
    let det = a * d - b * c;
    if path.commands().len() > MAX_POINTS
        || affine.elements.iter().any(|v| !v.is_finite())
        || det.abs() <= 1e-12
    {
        return None;
    }
    let screen = |p: P| {
        let p = P::new(a * p.x + c * p.y + tx, b * p.x + d * p.y + ty);
        valid(p).then_some(p)
    };
    let mut out = Contours {
        points: Vec::new(),
        first: None,
        start: 0,
    };
    let mut current = P::new(0.0, 0.0);
    let mut first = current;
    for command in path.commands() {
        if !matches!(command, PathCommand::MoveTo(_) | PathCommand::Close) && out.first.is_none() {
            first = current;
            out.push(screen(current)?)?;
        }
        match command {
            PathCommand::MoveTo(p) => {
                out.end()?;
                current = local(*p);
                first = current;
                out.push(screen(current)?)?;
            }
            PathCommand::LineTo(p) => {
                current = local(*p);
                out.push(screen(current)?)?;
            }
            PathCommand::QuadTo { control, end } => {
                let curve = QuadraticBezierSegment {
                    from: screen(current)?,
                    ctrl: screen(local(*control))?,
                    to: screen(local(*end))?,
                };
                for point in curve.flattened(0.25) {
                    out.push(point)?;
                }
                current = local(*end);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                end,
            } => {
                let curve = CubicBezierSegment {
                    from: screen(current)?,
                    ctrl1: screen(local(*control1))?,
                    ctrl2: screen(local(*control2))?,
                    to: screen(local(*end))?,
                };
                for point in curve.flattened(0.25) {
                    out.push(point)?;
                }
                current = local(*end);
            }
            PathCommand::ArcTo {
                radii,
                rotation,
                large_arc,
                sweep,
                end,
            } => {
                let to = local(*end);
                if !finite(current)
                    || !finite(to)
                    || !radii.x.is_finite()
                    || !radii.y.is_finite()
                    || !rotation.is_finite()
                {
                    return None;
                }
                let arc = SvgArc {
                    from: current,
                    to,
                    radii: lyon::geom::vector(radii.x as f64, radii.y as f64),
                    x_rotation: Angle::radians(*rotation as f64),
                    flags: ArcFlags {
                        large_arc: *large_arc,
                        sweep: *sweep,
                    },
                };
                if current == to {
                    continue;
                } else if arc.is_straight_line() {
                    out.push(screen(to)?)?;
                } else {
                    let arc = arc.to_arc();
                    if !finite(arc.center)
                        || !arc.radii.x.is_finite()
                        || !arc.radii.y.is_finite()
                        || !arc.start_angle.radians.is_finite()
                        || !arc.sweep_angle.radians.is_finite()
                    {
                        return None;
                    }
                    let mut cubics = Vec::new();
                    arc.for_each_cubic_bezier(&mut |curve| cubics.push(*curve));
                    for curve in cubics {
                        let curve = CubicBezierSegment {
                            from: screen(curve.from)?,
                            ctrl1: screen(curve.ctrl1)?,
                            ctrl2: screen(curve.ctrl2)?,
                            to: screen(curve.to)?,
                        };
                        for point in curve.flattened(0.25) {
                            out.push(point)?;
                        }
                    }
                }
                current = to;
            }
            PathCommand::Close => {
                out.end()?;
                current = first;
            }
        }
    }
    out.end()?;
    Some(out.points)
}
