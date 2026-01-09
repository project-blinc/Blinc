//! Path tessellation for GPU rendering
//!
//! Converts vector paths into GPU-renderable triangle meshes using lyon.

use blinc_core::{Brush, Color, Gradient, Path, PathCommand, Point, Stroke, Vec2};
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
    pub position: [f32; 2],        // 8 bytes, offset 0
    pub color: [f32; 4],           // 16 bytes, offset 8 (start color for gradients)
    pub end_color: [f32; 4],       // 16 bytes, offset 24 (end color for gradients)
    pub uv: [f32; 2], // 8 bytes, offset 40, UV coordinates for gradient sampling (0-1 range)
    pub gradient_params: [f32; 4], // 16 bytes, offset 48, gradient parameters (linear: x1,y1,x2,y2; radial: cx,cy,r,0)
    pub gradient_type: u32,        // 4 bytes, offset 64, 0 = solid, 1 = linear, 2 = radial
    pub edge_distance: f32,        // 4 bytes, offset 68, distance to nearest edge for AA
    pub _padding: [u32; 2],        // 8 bytes, offset 72, Padding for 16-byte alignment
}
// Total: 80 bytes

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

// ============================================================================
// Edge Distance Infrastructure for Anti-Aliasing
// ============================================================================

/// A line segment representing a flattened path edge
#[derive(Clone, Copy, Debug)]
struct EdgeSegment {
    start: Point,
    end: Point,
}

/// Collection of edge segments from a flattened path
struct PathEdges {
    segments: Vec<EdgeSegment>,
}

impl PathEdges {
    /// Build edge segments from a path, flattening curves to the given tolerance
    fn from_path(path: &Path, tolerance: f32) -> Self {
        let mut segments = Vec::new();
        let mut current = Point::new(0.0, 0.0);
        let mut first_point: Option<Point> = None;

        for cmd in path.commands() {
            match cmd {
                PathCommand::MoveTo(p) => {
                    current = *p;
                    first_point = Some(*p);
                }
                PathCommand::LineTo(p) => {
                    segments.push(EdgeSegment {
                        start: current,
                        end: *p,
                    });
                    current = *p;
                }
                PathCommand::QuadTo { control, end } => {
                    flatten_quad(current, *control, *end, tolerance, &mut segments);
                    current = *end;
                }
                PathCommand::CubicTo {
                    control1,
                    control2,
                    end,
                } => {
                    flatten_cubic(
                        current,
                        *control1,
                        *control2,
                        *end,
                        tolerance,
                        &mut segments,
                    );
                    current = *end;
                }
                PathCommand::ArcTo {
                    radii,
                    rotation,
                    large_arc,
                    sweep,
                    end,
                } => {
                    // Convert arc to cubic beziers, then flatten
                    let cubics =
                        arc_to_cubics(current, *radii, *rotation, *large_arc, *sweep, *end);
                    let mut prev = current;
                    for (ctrl1, ctrl2, end_pt) in cubics {
                        flatten_cubic(prev, ctrl1, ctrl2, end_pt, tolerance, &mut segments);
                        prev = end_pt;
                    }
                    current = *end;
                }
                PathCommand::Close => {
                    if let Some(first) = first_point {
                        if (current.x - first.x).abs() > 0.001
                            || (current.y - first.y).abs() > 0.001
                        {
                            segments.push(EdgeSegment {
                                start: current,
                                end: first,
                            });
                        }
                        current = first;
                    }
                    first_point = None;
                }
            }
        }

        Self { segments }
    }

    /// Compute minimum distance from a point to any edge segment
    fn distance_to_edges(&self, p: Point) -> f32 {
        self.segments
            .iter()
            .map(|seg| point_to_segment_distance(p, seg.start, seg.end))
            .fold(f32::MAX, f32::min)
    }
}

/// Compute perpendicular distance from point p to line segment a-b
fn point_to_segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let ab_x = b.x - a.x;
    let ab_y = b.y - a.y;
    let ap_x = p.x - a.x;
    let ap_y = p.y - a.y;

    let len_sq = ab_x * ab_x + ab_y * ab_y;
    if len_sq < 0.0001 {
        // Degenerate segment - return distance to point a
        return (ap_x * ap_x + ap_y * ap_y).sqrt();
    }

    // Project p onto line ab, clamped to segment
    let t = ((ap_x * ab_x + ap_y * ab_y) / len_sq).clamp(0.0, 1.0);

    // Closest point on segment
    let closest_x = a.x + t * ab_x;
    let closest_y = a.y + t * ab_y;

    let dx = p.x - closest_x;
    let dy = p.y - closest_y;
    (dx * dx + dy * dy).sqrt()
}

/// Flatten a quadratic bezier curve into line segments
fn flatten_quad(p0: Point, p1: Point, p2: Point, tolerance: f32, out: &mut Vec<EdgeSegment>) {
    // Check if chord distance is within tolerance
    let mid = Point::new(
        0.25 * p0.x + 0.5 * p1.x + 0.25 * p2.x,
        0.25 * p0.y + 0.5 * p1.y + 0.25 * p2.y,
    );
    let chord_mid = Point::new((p0.x + p2.x) * 0.5, (p0.y + p2.y) * 0.5);
    let dist = ((mid.x - chord_mid.x).powi(2) + (mid.y - chord_mid.y).powi(2)).sqrt();

    if dist <= tolerance {
        out.push(EdgeSegment { start: p0, end: p2 });
    } else {
        // Subdivide at t=0.5 using de Casteljau
        let p01 = Point::new((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
        let p12 = Point::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
        let p012 = Point::new((p01.x + p12.x) * 0.5, (p01.y + p12.y) * 0.5);

        flatten_quad(p0, p01, p012, tolerance, out);
        flatten_quad(p012, p12, p2, tolerance, out);
    }
}

/// Flatten a cubic bezier curve into line segments
fn flatten_cubic(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    tolerance: f32,
    out: &mut Vec<EdgeSegment>,
) {
    // Check convex hull distance for flatness
    let d1 = point_to_line_distance(p1, p0, p3);
    let d2 = point_to_line_distance(p2, p0, p3);

    if d1 <= tolerance && d2 <= tolerance {
        out.push(EdgeSegment { start: p0, end: p3 });
    } else {
        // De Casteljau subdivision at t=0.5
        let p01 = midpoint(p0, p1);
        let p12 = midpoint(p1, p2);
        let p23 = midpoint(p2, p3);
        let p012 = midpoint(p01, p12);
        let p123 = midpoint(p12, p23);
        let p0123 = midpoint(p012, p123);

        flatten_cubic(p0, p01, p012, p0123, tolerance, out);
        flatten_cubic(p0123, p123, p23, p3, tolerance, out);
    }
}

/// Compute midpoint of two points
fn midpoint(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

/// Compute perpendicular distance from point p to infinite line through a and b
fn point_to_line_distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.0001 {
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len
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
                let cubics =
                    arc_to_cubics(current_point, *radii, *rotation, *large_arc, *sweep, *end);

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

/// Compute the bounding box of a path
fn compute_path_bounds(path: &Path) -> (f32, f32, f32, f32) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for cmd in path.commands() {
        let points: Vec<Point> = match cmd {
            PathCommand::MoveTo(p) => vec![*p],
            PathCommand::LineTo(p) => vec![*p],
            PathCommand::QuadTo { control, end } => vec![*control, *end],
            PathCommand::CubicTo {
                control1,
                control2,
                end,
            } => vec![*control1, *control2, *end],
            PathCommand::ArcTo { end, .. } => vec![*end],
            PathCommand::Close => vec![],
        };

        for p in points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
    }

    if min_x == f32::MAX {
        (0.0, 0.0, 1.0, 1.0)
    } else {
        (min_x, min_y, max_x, max_y)
    }
}

/// Brush type for path rendering
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathBrushType {
    /// Solid color fill
    #[default]
    Solid,
    /// Linear gradient (2-stop fast path or multi-stop texture)
    LinearGradient,
    /// Radial gradient (2-stop fast path or multi-stop texture)
    RadialGradient,
    /// Image texture fill
    Image,
    /// Glass/blur effect
    Glass,
}

/// Extended brush information for path rendering
#[derive(Clone, Debug)]
pub struct PathBrushInfo {
    /// Type of brush
    pub brush_type: PathBrushType,
    /// Gradient type for vertex shader: 0=solid, 1=linear, 2=radial
    pub gradient_type: u32,
    /// Start color (or solid color)
    pub start_color: Color,
    /// End color (for 2-stop gradients)
    pub end_color: Color,
    /// Gradient parameters: linear (x1,y1,x2,y2), radial (cx,cy,r,0)
    pub gradient_params: [f32; 4],
    /// Whether gradient has >2 stops (needs texture lookup)
    pub needs_gradient_texture: bool,
    /// Gradient stops for multi-stop gradients (for texture rasterization)
    pub gradient_stops: Option<Vec<blinc_core::GradientStop>>,
    /// Image source path for image brushes
    pub image_source: Option<String>,
    /// Image tint color
    pub image_tint: Color,
    /// Glass parameters: (blur, saturation, tint_strength, opacity)
    pub glass_params: [f32; 4],
    /// Glass tint color
    pub glass_tint: Color,
}

impl Default for PathBrushInfo {
    fn default() -> Self {
        Self {
            brush_type: PathBrushType::Solid,
            gradient_type: 0,
            start_color: Color::BLACK,
            end_color: Color::BLACK,
            gradient_params: [0.0, 0.0, 1.0, 1.0],
            needs_gradient_texture: false,
            gradient_stops: None,
            image_source: None,
            image_tint: Color::WHITE,
            glass_params: [20.0, 1.0, 0.5, 0.9],
            glass_tint: Color::rgba(1.0, 1.0, 1.0, 0.3),
        }
    }
}

/// Extract comprehensive brush info for path rendering
pub fn extract_brush_info(brush: &Brush) -> PathBrushInfo {
    match brush {
        Brush::Solid(color) => PathBrushInfo {
            brush_type: PathBrushType::Solid,
            gradient_type: 0,
            start_color: *color,
            end_color: *color,
            ..Default::default()
        },
        Brush::Glass(style) => PathBrushInfo {
            brush_type: PathBrushType::Glass,
            gradient_type: 0,
            start_color: style.tint,
            end_color: style.tint,
            glass_params: [style.blur, style.saturation, 0.5, style.tint.a],
            glass_tint: style.tint,
            ..Default::default()
        },
        Brush::Image(img) => PathBrushInfo {
            brush_type: PathBrushType::Image,
            gradient_type: 0,
            start_color: img.tint,
            end_color: img.tint,
            image_source: Some(img.source.clone()),
            image_tint: img.tint,
            ..Default::default()
        },
        Brush::Gradient(gradient) => {
            let stops = gradient.stops();
            let start_color = gradient.first_color();
            let end_color = gradient.last_color();
            let needs_texture = stops.len() > 2;

            match gradient {
                Gradient::Linear { start, end, stops, .. } => {
                    tracing::debug!(
                        "Linear gradient: start=({}, {}), end=({}, {}), stops={}, colors=({:?} -> {:?})",
                        start.x,
                        start.y,
                        end.x,
                        end.y,
                        stops.len(),
                        start_color,
                        end_color
                    );
                    PathBrushInfo {
                        brush_type: PathBrushType::LinearGradient,
                        gradient_type: 1,
                        start_color,
                        end_color,
                        gradient_params: [start.x, start.y, end.x, end.y],
                        needs_gradient_texture: needs_texture,
                        gradient_stops: if needs_texture { Some(stops.clone()) } else { None },
                        ..Default::default()
                    }
                }
                Gradient::Radial { center, radius, stops, .. } => {
                    tracing::debug!(
                        "Radial gradient: center=({}, {}), radius={}, stops={}, colors=({:?} -> {:?})",
                        center.x,
                        center.y,
                        radius,
                        stops.len(),
                        start_color,
                        end_color
                    );
                    PathBrushInfo {
                        brush_type: PathBrushType::RadialGradient,
                        gradient_type: 2,
                        start_color,
                        end_color,
                        gradient_params: [center.x, center.y, *radius, 0.0],
                        needs_gradient_texture: needs_texture,
                        gradient_stops: if needs_texture { Some(stops.clone()) } else { None },
                        ..Default::default()
                    }
                }
                Gradient::Conic { center, start_angle, stops, .. } => {
                    // Treat conic as radial for now
                    PathBrushInfo {
                        brush_type: PathBrushType::RadialGradient,
                        gradient_type: 2,
                        start_color,
                        end_color,
                        gradient_params: [center.x, center.y, 100.0, *start_angle],
                        needs_gradient_texture: needs_texture,
                        gradient_stops: if needs_texture { Some(stops.clone()) } else { None },
                        ..Default::default()
                    }
                }
            }
        }
    }
}

/// Extract gradient info from brush (legacy function for backward compatibility)
fn extract_gradient_info(brush: &Brush) -> (u32, Color, Color, [f32; 4]) {
    let info = extract_brush_info(brush);
    (info.gradient_type, info.start_color, info.end_color, info.gradient_params)
}

/// Tessellate a path for filling
pub fn tessellate_fill(path: &Path, brush: &Brush) -> TessellatedPath {
    let events = path_to_lyon_events(path);

    if events.is_empty() {
        return TessellatedPath::new();
    }

    let (gradient_type, start_color, end_color, gradient_params) = extract_gradient_info(brush);
    let (min_x, min_y, max_x, max_y) = compute_path_bounds(path);
    let bounds_width = (max_x - min_x).max(1.0);
    let bounds_height = (max_y - min_y).max(1.0);

    // Build edge segments for anti-aliasing edge distance computation
    let edges = PathEdges::from_path(path, 0.1);

    let mut geometry: VertexBuffers<PathVertex, u32> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    // Lower tolerance = more triangles = smoother curves (at cost of more vertices)
    let options = FillOptions::default().with_tolerance(0.025);

    let result = tessellator.tessellate(
        events.iter().cloned(),
        &options,
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
            let pos = vertex.position();

            // Compute edge distance for anti-aliasing
            let edge_distance = edges.distance_to_edges(Point::new(pos.x, pos.y));

            // Compute UV based on position in bounding box
            // For ObjectBoundingBox gradients, UV is used directly as gradient parameter
            let u = (pos.x - min_x) / bounds_width;
            let v = (pos.y - min_y) / bounds_height;

            // For gradients, we pass start/end colors and gradient params
            // The shader computes the gradient based on UV and gradient direction
            PathVertex {
                position: pos.to_array(),
                color: [start_color.r, start_color.g, start_color.b, start_color.a],
                end_color: [end_color.r, end_color.g, end_color.b, end_color.a],
                uv: [u, v],
                gradient_params,
                gradient_type,
                edge_distance,
                _padding: [0, 0],
            }
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
    let events = path_to_lyon_events(path);

    if events.is_empty() {
        return TessellatedPath::new();
    }

    let (gradient_type, start_color, end_color, gradient_params) = extract_gradient_info(brush);
    let (min_x, min_y, max_x, max_y) = compute_path_bounds(path);
    let bounds_width = (max_x - min_x).max(1.0);
    let bounds_height = (max_y - min_y).max(1.0);

    let mut geometry: VertexBuffers<PathVertex, u32> = VertexBuffers::new();
    let mut tessellator = StrokeTessellator::new();

    let half_width = stroke.width * 0.5;

    let mut options = StrokeOptions::default()
        .with_line_width(stroke.width)
        .with_tolerance(0.025);

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
        &mut BuffersBuilder::new(&mut geometry, |vertex: StrokeVertex| {
            let pos = vertex.position();
            let pos_on_path = vertex.position_on_path();

            // Compute edge distance: distance from vertex to stroke edge
            // Lyon gives us the position on the centerline path, so we can
            // compute how far this vertex is from the center
            let dx = pos.x - pos_on_path.x;
            let dy = pos.y - pos_on_path.y;
            let dist_from_center = (dx * dx + dy * dy).sqrt();
            // Edge distance is how far we are from the edge (positive = inside)
            let edge_distance = half_width - dist_from_center;

            // Compute UV based on position in bounding box
            let u = (pos.x - min_x) / bounds_width;
            let v = (pos.y - min_y) / bounds_height;

            // For gradients, we pass start/end colors and gradient params
            PathVertex {
                position: pos.to_array(),
                color: [start_color.r, start_color.g, start_color.b, start_color.a],
                end_color: [end_color.r, end_color.g, end_color.b, end_color.a],
                uv: [u, v],
                gradient_params,
                gradient_type,
                edge_distance,
                _padding: [0, 0],
            }
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
        Brush::Glass(style) => {
            // Glass effects are not supported on tessellated paths
            // Return the tint color as a fallback
            style.tint
        }
        Brush::Image(img) => {
            // Image backgrounds are not supported on tessellated paths
            // Return the tint color as a fallback
            img.tint
        }
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
