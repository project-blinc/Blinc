//! Path tessellation for GPU rendering
//!
//! Converts vector paths into GPU-renderable triangle meshes using lyon.

use blinc_core::{Brush, Color, Path, PathCommand, Point, Stroke, Vec2};
use lyon::lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};
use lyon::math::point;
use lyon::path::PathEvent;

/// A vertex for path rendering
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PathVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

/// Tessellated path geometry ready for GPU rendering
#[derive(Default)]
pub struct TessellatedPath {
    pub vertices: Vec<PathVertex>,
    pub indices: Vec<u32>,
}

impl TessellatedPath {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() || self.indices.is_empty()
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }
}

/// Convert an SVG arc to cubic bezier curves
/// Based on the SVG arc implementation algorithm from the W3C spec
fn arc_to_cubics(
    from: Point,
    radii: Vec2,
    x_rotation: f32,
    large_arc: bool,
    sweep: bool,
    to: Point,
) -> Vec<(Point, Point, Point)> {
    let mut curves = Vec::new();

    // Handle degenerate cases
    if from.x == to.x && from.y == to.y {
        return curves;
    }

    let mut rx = radii.x.abs();
    let mut ry = radii.y.abs();

    if rx == 0.0 || ry == 0.0 {
        // Treat as a line
        return curves;
    }

    let cos_phi = x_rotation.cos();
    let sin_phi = x_rotation.sin();

    // Step 1: Compute (x1', y1') - transformed start point
    let dx = (from.x - to.x) / 2.0;
    let dy = (from.y - to.y) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    // Step 2: Compute center point (cx', cy')
    let x1p_sq = x1p * x1p;
    let y1p_sq = y1p * y1p;
    let rx_sq = rx * rx;
    let ry_sq = ry * ry;

    // Ensure radii are large enough
    let lambda = x1p_sq / rx_sq + y1p_sq / ry_sq;
    if lambda > 1.0 {
        let lambda_sqrt = lambda.sqrt();
        rx *= lambda_sqrt;
        ry *= lambda_sqrt;
    }

    let rx_sq = rx * rx;
    let ry_sq = ry * ry;

    // Compute center point
    let sq_numer = (rx_sq * ry_sq - rx_sq * y1p_sq - ry_sq * x1p_sq).max(0.0);
    let sq_denom = rx_sq * y1p_sq + ry_sq * x1p_sq;
    let sq = if sq_denom > 0.0 {
        (sq_numer / sq_denom).sqrt()
    } else {
        0.0
    };

    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let cxp = sign * sq * rx * y1p / ry;
    let cyp = sign * sq * -ry * x1p / rx;

    // Step 3: Compute (cx, cy) from (cx', cy')
    let cx = cos_phi * cxp - sin_phi * cyp + (from.x + to.x) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (from.y + to.y) / 2.0;

    // Step 4: Compute theta1 and dtheta
    fn angle(ux: f32, uy: f32, vx: f32, vy: f32) -> f32 {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let cos_val = (dot / len).clamp(-1.0, 1.0);
        let angle = cos_val.acos();
        if ux * vy - uy * vx < 0.0 {
            -angle
        } else {
            angle
        }
    }

    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dtheta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );

    // Adjust dtheta based on sweep flag
    if sweep && dtheta < 0.0 {
        dtheta += std::f32::consts::TAU;
    } else if !sweep && dtheta > 0.0 {
        dtheta -= std::f32::consts::TAU;
    }

    // Split arc into segments (max 90 degrees each)
    let num_segments = ((dtheta.abs() / (std::f32::consts::PI / 2.0)).ceil() as usize).max(1);
    let segment_angle = dtheta / num_segments as f32;

    for i in 0..num_segments {
        let t1 = theta1 + i as f32 * segment_angle;
        let t2 = t1 + segment_angle;

        // Approximate arc segment with cubic bezier
        let alpha = (segment_angle / 2.0).tan() * 4.0 / 3.0;

        let cos_t1 = t1.cos();
        let sin_t1 = t1.sin();
        let cos_t2 = t2.cos();
        let sin_t2 = t2.sin();

        // Start point of this segment
        let p0x = cx + rx * (cos_phi * cos_t1 - sin_phi * sin_t1);
        let p0y = cy + rx * (sin_phi * cos_t1 + cos_phi * sin_t1);

        // End point of this segment
        let p3x = cx + rx * (cos_phi * cos_t2 - sin_phi * sin_t2);
        let p3y = cy + ry * (sin_phi * cos_t2 + cos_phi * sin_t2);

        // Control points
        let p1x = p0x - alpha * rx * (-cos_phi * sin_t1 - sin_phi * cos_t1);
        let p1y = p0y - alpha * ry * (-sin_phi * sin_t1 + cos_phi * cos_t1);

        let p2x = p3x + alpha * rx * (-cos_phi * sin_t2 - sin_phi * cos_t2);
        let p2y = p3y + alpha * ry * (-sin_phi * sin_t2 + cos_phi * cos_t2);

        curves.push((
            Point::new(p1x, p1y),
            Point::new(p2x, p2y),
            Point::new(p3x, p3y),
        ));
    }

    curves
}

/// Convert blinc_core Path to lyon path events
fn path_to_lyon_events(path: &Path) -> Vec<PathEvent> {
    let mut events = Vec::new();
    let mut first_point: Option<Point> = None;
    let mut current_point = Point::new(0.0, 0.0);

    for cmd in path.commands() {
        match cmd {
            PathCommand::MoveTo(p) => {
                if first_point.is_some() {
                    // End previous subpath
                    events.push(PathEvent::End {
                        last: point(current_point.x, current_point.y),
                        first: point(first_point.unwrap().x, first_point.unwrap().y),
                        close: false,
                    });
                }
                events.push(PathEvent::Begin {
                    at: point(p.x, p.y),
                });
                first_point = Some(*p);
                current_point = *p;
            }
            PathCommand::LineTo(p) => {
                if first_point.is_none() {
                    // Implicit moveto at origin
                    events.push(PathEvent::Begin {
                        at: point(0.0, 0.0),
                    });
                    first_point = Some(Point::new(0.0, 0.0));
                }
                events.push(PathEvent::Line {
                    from: point(current_point.x, current_point.y),
                    to: point(p.x, p.y),
                });
                current_point = *p;
            }
            PathCommand::QuadTo { control, end } => {
                if first_point.is_none() {
                    events.push(PathEvent::Begin {
                        at: point(0.0, 0.0),
                    });
                    first_point = Some(Point::new(0.0, 0.0));
                }
                events.push(PathEvent::Quadratic {
                    from: point(current_point.x, current_point.y),
                    ctrl: point(control.x, control.y),
                    to: point(end.x, end.y),
                });
                current_point = *end;
            }
            PathCommand::CubicTo {
                control1,
                control2,
                end,
            } => {
                if first_point.is_none() {
                    events.push(PathEvent::Begin {
                        at: point(0.0, 0.0),
                    });
                    first_point = Some(Point::new(0.0, 0.0));
                }
                events.push(PathEvent::Cubic {
                    from: point(current_point.x, current_point.y),
                    ctrl1: point(control1.x, control1.y),
                    ctrl2: point(control2.x, control2.y),
                    to: point(end.x, end.y),
                });
                current_point = *end;
            }
            PathCommand::ArcTo {
                radii,
                rotation,
                large_arc,
                sweep,
                end,
            } => {
                if first_point.is_none() {
                    events.push(PathEvent::Begin {
                        at: point(0.0, 0.0),
                    });
                    first_point = Some(Point::new(0.0, 0.0));
                }
                // Convert SVG arc to cubic bezier curves
                let cubics = arc_to_cubics(current_point, *radii, *rotation, *large_arc, *sweep, *end);

                if cubics.is_empty() {
                    // Degenerate arc - treat as line
                    events.push(PathEvent::Line {
                        from: point(current_point.x, current_point.y),
                        to: point(end.x, end.y),
                    });
                } else {
                    // Add cubic bezier curves approximating the arc
                    let mut prev = current_point;
                    for (ctrl1, ctrl2, end_pt) in cubics {
                        events.push(PathEvent::Cubic {
                            from: point(prev.x, prev.y),
                            ctrl1: point(ctrl1.x, ctrl1.y),
                            ctrl2: point(ctrl2.x, ctrl2.y),
                            to: point(end_pt.x, end_pt.y),
                        });
                        prev = end_pt;
                    }
                }
                current_point = *end;
            }
            PathCommand::Close => {
                if let Some(first) = first_point {
                    events.push(PathEvent::End {
                        last: point(current_point.x, current_point.y),
                        first: point(first.x, first.y),
                        close: true,
                    });
                    first_point = None;
                }
            }
        }
    }

    // Close any remaining open subpath
    if let Some(first) = first_point {
        events.push(PathEvent::End {
            last: point(current_point.x, current_point.y),
            first: point(first.x, first.y),
            close: false,
        });
    }

    events
}

/// Tessellate a path for filling
pub fn tessellate_fill(path: &Path, brush: &Brush) -> TessellatedPath {
    let color = brush_to_color(brush);
    let events = path_to_lyon_events(path);

    if events.is_empty() {
        return TessellatedPath::new();
    }

    let mut geometry: VertexBuffers<PathVertex, u32> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    let options = FillOptions::default().with_tolerance(0.1);

    let result = tessellator.tessellate(
        events.iter().cloned(),
        &options,
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| PathVertex {
            position: vertex.position().to_array(),
            color: [color.r, color.g, color.b, color.a],
        }),
    );

    if result.is_err() {
        tracing::warn!("Path fill tessellation failed: {:?}", result.err());
        return TessellatedPath::new();
    }

    TessellatedPath {
        vertices: geometry.vertices,
        indices: geometry.indices,
    }
}

/// Tessellate a path for stroking
pub fn tessellate_stroke(path: &Path, stroke: &Stroke, brush: &Brush) -> TessellatedPath {
    let color = brush_to_color(brush);
    let events = path_to_lyon_events(path);

    if events.is_empty() {
        return TessellatedPath::new();
    }

    let mut geometry: VertexBuffers<PathVertex, u32> = VertexBuffers::new();
    let mut tessellator = StrokeTessellator::new();

    let mut options = StrokeOptions::default()
        .with_line_width(stroke.width)
        .with_tolerance(0.1);

    // Convert line cap
    options = options.with_line_cap(match stroke.cap {
        blinc_core::LineCap::Butt => lyon::lyon_tessellation::LineCap::Butt,
        blinc_core::LineCap::Round => lyon::lyon_tessellation::LineCap::Round,
        blinc_core::LineCap::Square => lyon::lyon_tessellation::LineCap::Square,
    });

    // Convert line join
    options = options.with_line_join(match stroke.join {
        blinc_core::LineJoin::Miter => lyon::lyon_tessellation::LineJoin::Miter,
        blinc_core::LineJoin::Round => lyon::lyon_tessellation::LineJoin::Round,
        blinc_core::LineJoin::Bevel => lyon::lyon_tessellation::LineJoin::Bevel,
    });

    options = options.with_miter_limit(stroke.miter_limit);

    let result = tessellator.tessellate(
        events.iter().cloned(),
        &options,
        &mut BuffersBuilder::new(&mut geometry, |vertex: StrokeVertex| PathVertex {
            position: vertex.position().to_array(),
            color: [color.r, color.g, color.b, color.a],
        }),
    );

    if result.is_err() {
        tracing::warn!("Path stroke tessellation failed: {:?}", result.err());
        return TessellatedPath::new();
    }

    TessellatedPath {
        vertices: geometry.vertices,
        indices: geometry.indices,
    }
}

/// Extract solid color from brush (gradients not yet supported for paths)
fn brush_to_color(brush: &Brush) -> Color {
    match brush {
        Brush::Solid(color) => *color,
        Brush::Gradient(gradient) => {
            // Use first stop color as fallback
            gradient
                .stops()
                .first()
                .map(|s| s.color)
                .unwrap_or(Color::BLACK)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::Rect;

    #[test]
    fn test_tessellate_rect() {
        let path = Path::rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        let result = tessellate_fill(&path, &Color::RED.into());

        assert!(!result.is_empty());
        assert!(!result.vertices.is_empty());
        assert!(!result.indices.is_empty());
    }

    #[test]
    fn test_tessellate_circle() {
        let path = Path::circle(Point::new(50.0, 50.0), 25.0);
        let result = tessellate_fill(&path, &Color::BLUE.into());

        assert!(!result.is_empty());
    }

    #[test]
    fn test_tessellate_stroke() {
        let path = Path::line(Point::new(0.0, 0.0), Point::new(100.0, 100.0));
        let result = tessellate_stroke(&path, &Stroke::new(3.0), &Color::BLACK.into());

        assert!(!result.is_empty());
    }
}
