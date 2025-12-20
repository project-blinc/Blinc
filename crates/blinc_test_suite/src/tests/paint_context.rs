//! PaintContext tests
//!
//! Tests for the blinc_paint PaintContext convenience API

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, Point, Rect, Stroke};
// Note: We test through GpuPaintContext which implements DrawContext

/// Create the PaintContext test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("paint_context");

    // Basic fill_rect_xywh
    suite.add("paint_fill_rect_xywh", |ctx| {
        let c = ctx.ctx();

        // Use the GpuPaintContext's fill_rect since we don't have direct
        // access to PaintContext convenience methods here
        c.fill_rect(
            Rect::new(100.0, 100.0, 200.0, 100.0),
            0.0.into(),
            Color::BLUE.into(),
        );
    });

    // Stroke rect
    suite.add("paint_stroke_rect_xywh", |ctx| {
        let c = ctx.ctx();

        c.stroke_rect(
            Rect::new(100.0, 100.0, 200.0, 100.0),
            0.0.into(),
            &Stroke::new(2.0),
            Color::BLUE.into(),
        );
    });

    // Rounded rect
    suite.add("paint_rounded_rect", |ctx| {
        let c = ctx.ctx();

        c.fill_rect(
            Rect::new(100.0, 100.0, 200.0, 100.0),
            12.0.into(),
            Color::GREEN.into(),
        );
    });

    // Circle using fill_circle
    suite.add("paint_circle", |ctx| {
        let c = ctx.ctx();

        c.fill_circle(Point::new(200.0, 150.0), 60.0, Color::RED.into());
    });

    // Stroke circle
    suite.add("paint_stroke_circle", |ctx| {
        let c = ctx.ctx();

        c.stroke_circle(
            Point::new(200.0, 150.0),
            60.0,
            &Stroke::new(3.0),
            Color::BLUE.into(),
        );
    });

    // Transform convenience: translate
    suite.add("paint_translate", |ctx| {
        let c = ctx.ctx();

        c.push_transform(blinc_core::Transform::translate(100.0, 50.0));
        c.fill_rect(
            Rect::new(0.0, 0.0, 150.0, 100.0),
            8.0.into(),
            Color::BLUE.into(),
        );
        c.pop_transform();
    });

    // Transform convenience: scale
    suite.add("paint_scale", |ctx| {
        let c = ctx.ctx();

        c.push_transform(blinc_core::Transform::translate(200.0, 150.0));
        c.push_transform(blinc_core::Transform::scale(2.0, 1.5));
        c.fill_rect(
            Rect::new(-50.0, -30.0, 100.0, 60.0),
            8.0.into(),
            Color::GREEN.into(),
        );
        c.pop_transform();
        c.pop_transform();
    });

    // Transform convenience: rotate
    suite.add("paint_rotate", |ctx| {
        let c = ctx.ctx();

        c.push_transform(blinc_core::Transform::translate(200.0, 150.0));
        c.push_transform(blinc_core::Transform::rotate(0.3));
        c.fill_rect(
            Rect::new(-75.0, -40.0, 150.0, 80.0),
            8.0.into(),
            Color::RED.into(),
        );
        c.pop_transform();
        c.pop_transform();
    });

    // Combined transforms
    suite.add("paint_combined_transforms", |ctx| {
        let c = ctx.ctx();

        c.push_transform(blinc_core::Transform::translate(200.0, 150.0));
        c.push_transform(blinc_core::Transform::rotate(0.5));
        c.push_transform(blinc_core::Transform::scale(1.5, 1.0));

        c.fill_rect(
            Rect::new(-60.0, -30.0, 120.0, 60.0),
            4.0.into(),
            Color::BLUE.into(),
        );

        c.pop_transform();
        c.pop_transform();
        c.pop_transform();
    });

    // Multiple shapes in sequence
    suite.add("paint_multiple_shapes", |ctx| {
        let c = ctx.ctx();

        // Background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.95, 0.95, 0.95, 1.0).into(),
        );

        // Red rect
        c.fill_rect(
            Rect::new(50.0, 50.0, 100.0, 80.0),
            8.0.into(),
            Color::RED.into(),
        );

        // Green circle
        c.fill_circle(Point::new(250.0, 90.0), 40.0, Color::GREEN.into());

        // Blue rounded rect
        c.fill_rect(
            Rect::new(150.0, 160.0, 120.0, 80.0),
            16.0.into(),
            Color::BLUE.into(),
        );

        // Yellow circle
        c.fill_circle(
            Point::new(100.0, 200.0),
            30.0,
            Color::rgba(1.0, 0.8, 0.0, 1.0).into(),
        );
    });

    // Strokes with different styles
    suite.add("paint_stroke_styles", |ctx| {
        let c = ctx.ctx();

        // Thin stroke
        c.stroke_rect(
            Rect::new(50.0, 50.0, 100.0, 60.0),
            8.0.into(),
            &Stroke::new(1.0),
            Color::BLACK.into(),
        );

        // Medium stroke
        c.stroke_rect(
            Rect::new(180.0, 50.0, 100.0, 60.0),
            8.0.into(),
            &Stroke::new(3.0),
            Color::BLACK.into(),
        );

        // Thick stroke
        c.stroke_rect(
            Rect::new(50.0, 150.0, 100.0, 60.0),
            8.0.into(),
            &Stroke::new(6.0),
            Color::BLACK.into(),
        );

        // Very thick stroke
        c.stroke_rect(
            Rect::new(180.0, 150.0, 100.0, 60.0),
            8.0.into(),
            &Stroke::new(10.0),
            Color::BLACK.into(),
        );
    });

    suite
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::TestHarness;

    #[test]
    #[ignore] // Requires GPU
    fn run_paint_context_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
