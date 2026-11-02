//! The SDF builder: shapes drawn as SDF primitives, antialiased exactly by
//! their distance field whatever the target's sample count.

use blinc_core::{
    Brush, CornerRadius, DrawContext, Point, Rect, SdfBuilder, Shadow, ShapeId, Stroke, Transform,
};

use super::GpuPaintContext;

/// SDF builder that directly emits GPU primitives
pub(super) struct GpuSdfBuilder<'a, 'b> {
    ctx: &'a mut GpuPaintContext<'b>,
    shapes: Vec<SdfShapeData>,
}

#[derive(Clone, Debug)]
enum SdfShapeData {
    Rect {
        rect: Rect,
        corner_radius: CornerRadius,
    },
    Circle {
        center: Point,
        radius: f32,
    },
    Ellipse {
        center: Point,
        radii: (f32, f32),
    },
    /// A segment `width` wide with round ends: a capsule.
    Line {
        from: Point,
        to: Point,
        width: f32,
    },
}

impl<'a, 'b> GpuSdfBuilder<'a, 'b> {
    pub(super) fn new(ctx: &'a mut GpuPaintContext<'b>) -> Self {
        Self {
            ctx,
            shapes: Vec::new(),
        }
    }

    fn add_shape(&mut self, shape: SdfShapeData) -> ShapeId {
        let id = ShapeId(self.shapes.len() as u32);
        self.shapes.push(shape);
        id
    }
}

impl<'a, 'b> SdfBuilder for GpuSdfBuilder<'a, 'b> {
    fn rect(&mut self, rect: Rect, corner_radius: CornerRadius) -> ShapeId {
        self.add_shape(SdfShapeData::Rect {
            rect,
            corner_radius,
        })
    }

    fn circle(&mut self, center: Point, radius: f32) -> ShapeId {
        self.add_shape(SdfShapeData::Circle { center, radius })
    }

    fn ellipse(&mut self, center: Point, radii: blinc_core::Vec2) -> ShapeId {
        self.add_shape(SdfShapeData::Ellipse {
            center,
            radii: (radii.x, radii.y),
        })
    }

    fn line(&mut self, from: Point, to: Point, width: f32) -> ShapeId {
        self.add_shape(SdfShapeData::Line { from, to, width })
    }

    fn arc(
        &mut self,
        _center: Point,
        _radius: f32,
        _start: f32,
        _end: f32,
        _width: f32,
    ) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn quad_bezier(&mut self, _p0: Point, _p1: Point, _p2: Point, _width: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn union(&mut self, _a: ShapeId, _b: ShapeId) -> ShapeId {
        // Boolean operations would require more complex SDF evaluation
        ShapeId(self.shapes.len() as u32)
    }

    fn subtract(&mut self, _a: ShapeId, _b: ShapeId) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn intersect(&mut self, _a: ShapeId, _b: ShapeId) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn smooth_union(&mut self, _a: ShapeId, _b: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn smooth_subtract(&mut self, _a: ShapeId, _b: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn smooth_intersect(&mut self, _a: ShapeId, _b: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn round(&mut self, _shape: ShapeId, _radius: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn outline(&mut self, _shape: ShapeId, _width: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn offset(&mut self, _shape: ShapeId, _distance: f32) -> ShapeId {
        ShapeId(self.shapes.len() as u32)
    }

    fn fill(&mut self, shape: ShapeId, brush: Brush) {
        if let Some(shape_data) = self.shapes.get(shape.0 as usize) {
            match shape_data.clone() {
                SdfShapeData::Rect {
                    rect,
                    corner_radius,
                } => {
                    self.ctx.fill_rect(rect, corner_radius, brush);
                }
                SdfShapeData::Circle { center, radius } => {
                    self.ctx.fill_circle(center, radius, brush);
                }
                SdfShapeData::Ellipse { center, radii } => {
                    // Ellipse would need its own primitive type
                    // For now, approximate with the larger radius
                    let radius = radii.0.max(radii.1);
                    self.ctx.fill_circle(center, radius, brush);
                }
                SdfShapeData::Line { from, to, width } => {
                    let (rect, radius) = self.along(from, to, width);
                    self.ctx.fill_rect(rect, radius, brush);
                    self.ctx.pop_transform();
                }
            }
        }
    }

    fn stroke(&mut self, shape: ShapeId, stroke: &Stroke, brush: Brush) {
        if let Some(shape_data) = self.shapes.get(shape.0 as usize) {
            match shape_data.clone() {
                SdfShapeData::Rect {
                    rect,
                    corner_radius,
                } => {
                    self.ctx.stroke_rect(rect, corner_radius, stroke, brush);
                }
                SdfShapeData::Circle { center, radius } => {
                    self.ctx.stroke_circle(center, radius, stroke, brush);
                }
                SdfShapeData::Ellipse { center, radii } => {
                    let radius = radii.0.max(radii.1);
                    self.ctx.stroke_circle(center, radius, stroke, brush);
                }
                SdfShapeData::Line { from, to, width } => {
                    let (rect, radius) = self.along(from, to, width);
                    self.ctx.stroke_rect(rect, radius, stroke, brush);
                    self.ctx.pop_transform();
                }
            }
        }
    }

    fn shadow(&mut self, shape: ShapeId, shadow: Shadow) {
        if let Some(shape_data) = self.shapes.get(shape.0 as usize) {
            match shape_data.clone() {
                SdfShapeData::Rect {
                    rect,
                    corner_radius,
                } => {
                    self.ctx.draw_shadow(rect, corner_radius, shadow);
                }
                SdfShapeData::Circle { center, radius } => {
                    let rect = Rect::new(
                        center.x - radius,
                        center.y - radius,
                        radius * 2.0,
                        radius * 2.0,
                    );
                    self.ctx.draw_shadow(rect, radius.into(), shadow);
                }
                SdfShapeData::Ellipse { center, radii } => {
                    let rect = Rect::new(
                        center.x - radii.0,
                        center.y - radii.1,
                        radii.0 * 2.0,
                        radii.1 * 2.0,
                    );
                    self.ctx.draw_shadow(rect, CornerRadius::default(), shadow);
                }
                SdfShapeData::Line { from, to, width } => {
                    let (rect, radius) = self.along(from, to, width);
                    self.ctx.draw_shadow(rect, radius, shadow);
                    self.ctx.pop_transform();
                }
            }
        }
    }
}

impl GpuSdfBuilder<'_, '_> {
    /// A capsule is a rounded rect whose radius is half its width, laid along
    /// the segment: pushes the transform to the segment's frame and gives
    /// the rect in it. The caller draws, then pops the transform.
    fn along(&mut self, from: Point, to: Point, width: f32) -> (Rect, CornerRadius) {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let length = (dx * dx + dy * dy).sqrt();
        let r = width / 2.0;
        self.ctx.push_transform(Transform::Affine2D(
            blinc_core::Affine2D::translation((from.x + to.x) / 2.0, (from.y + to.y) / 2.0)
                .then(&blinc_core::Affine2D::rotation(dy.atan2(dx))),
        ));
        (
            Rect::new(-length / 2.0 - r, -r, length + width, width),
            CornerRadius::uniform(r),
        )
    }
}

#[cfg(test)]
mod tests {
    use blinc_core::{Color, DrawContext, Point};

    use super::GpuPaintContext;

    #[test]
    fn a_line_is_one_rotated_rounded_rect() {
        let mut ctx = GpuPaintContext::new(400.0, 400.0);
        ctx.sdf_build(&mut |b| {
            let line = b.line(Point::new(100.0, 100.0), Point::new(160.0, 180.0), 4.0);
            b.fill(line, Color::BLUE.into());
        });
        let batch = ctx.batch();
        assert_eq!(batch.primitive_count(), 1);
        let p = &batch.primitives[0];
        // 100 long, plus a radius at each end; centred on the midpoint.
        let [x, y, w, h] = p.bounds;
        assert!(
            (w - 104.0).abs() < 1e-3 && (h - 4.0).abs() < 1e-3,
            "{:?}",
            p.bounds
        );
        assert!((x + w / 2.0 - 130.0).abs() < 1e-3 && (y + h / 2.0 - 140.0).abs() < 1e-3);
        assert_eq!(p.corner_radius, [2.0; 4]);
        // Turned to the segment's angle: (sin, cos) of atan2(80, 60).
        assert!((p.rotation[0] - 0.8).abs() < 1e-3 && (p.rotation[1] - 0.6).abs() < 1e-3);
    }
}
