//! Shape rendering tests
//!
//! Tests for basic shape primitives: rectangles, circles, rounded rects

use crate::runner::TestSuite;
use blinc_core::{Color, CornerRadius, DrawContext, Point, Rect};

/// Create the shapes test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("shapes");

    // Basic rectangle
    suite.add("fill_rect_basic", |ctx| {
        ctx.ctx().fill_rect(
            Rect::new(100.0, 100.0, 200.0, 150.0),
            CornerRadius::ZERO,
            Color::BLUE.into(),
        );
    });

    // Rectangle with uniform corner radius
    suite.add("fill_rect_rounded_uniform", |ctx| {
        ctx.ctx().fill_rect(
            Rect::new(100.0, 100.0, 200.0, 150.0),
            16.0.into(),
            Color::RED.into(),
        );
    });

    // Rectangle with per-corner radius
    suite.add("fill_rect_rounded_per_corner", |ctx| {
        ctx.ctx().fill_rect(
            Rect::new(100.0, 100.0, 200.0, 150.0),
            CornerRadius {
                top_left: 0.0,
                top_right: 16.0,
                bottom_right: 32.0,
                bottom_left: 8.0,
            },
            Color::GREEN.into(),
        );
    });

    // Circle
    suite.add("fill_circle_basic", |ctx| {
        ctx.ctx()
            .fill_circle(Point::new(200.0, 200.0), 80.0, Color::PURPLE.into());
    });

    // Stroke rectangle
    suite.add("stroke_rect_basic", |ctx| {
        ctx.ctx().stroke_rect(
            Rect::new(100.0, 100.0, 200.0, 150.0),
            CornerRadius::ZERO,
            &blinc_core::Stroke::new(4.0),
            Color::BLACK.into(),
        );
    });

    // Stroke rounded rectangle
    suite.add("stroke_rect_rounded", |ctx| {
        ctx.ctx().stroke_rect(
            Rect::new(100.0, 100.0, 200.0, 150.0),
            12.0.into(),
            &blinc_core::Stroke::new(3.0),
            Color::BLUE.into(),
        );
    });

    // Stroke circle
    suite.add("stroke_circle_basic", |ctx| {
        ctx.ctx().stroke_circle(
            Point::new(200.0, 200.0),
            80.0,
            &blinc_core::Stroke::new(5.0),
            Color::RED.into(),
        );
    });

    // Multiple shapes
    suite.add("multiple_shapes", |ctx| {
        let c = ctx.ctx();

        // Background
        c.fill_rect(
            Rect::new(50.0, 50.0, 300.0, 200.0),
            8.0.into(),
            Color::rgba(0.9, 0.9, 0.9, 1.0).into(),
        );

        // Red circle
        c.fill_circle(Point::new(120.0, 120.0), 40.0, Color::RED.into());

        // Blue rectangle
        c.fill_rect(
            Rect::new(180.0, 100.0, 100.0, 80.0),
            4.0.into(),
            Color::BLUE.into(),
        );

        // Green rounded rect
        c.fill_rect(
            Rect::new(100.0, 180.0, 120.0, 60.0),
            16.0.into(),
            Color::GREEN.into(),
        );
    });

    // Overlapping shapes (testing z-order)
    suite.add("overlapping_shapes", |ctx| {
        let c = ctx.ctx();

        // Draw three overlapping circles
        c.fill_circle(Point::new(150.0, 150.0), 80.0, Color::RED.into());
        c.fill_circle(Point::new(200.0, 150.0), 80.0, Color::GREEN.into());
        c.fill_circle(Point::new(175.0, 200.0), 80.0, Color::BLUE.into());
    });

    // Small shapes (anti-aliasing test)
    suite.add("small_shapes", |ctx| {
        let c = ctx.ctx();

        for i in 0..10 {
            let size = (i + 1) as f32 * 2.0;
            let x = 50.0 + i as f32 * 30.0;

            // Small rects
            c.fill_rect(
                Rect::new(x, 50.0, size, size),
                (size / 4.0).into(),
                Color::BLUE.into(),
            );

            // Small circles
            c.fill_circle(
                Point::new(x + size / 2.0, 100.0),
                size / 2.0,
                Color::RED.into(),
            );
        }
    });

    // Grid of shapes
    suite.add("shape_grid", |ctx| {
        let c = ctx.ctx();
        let colors = [Color::RED, Color::GREEN, Color::BLUE, Color::PURPLE];

        for row in 0..4 {
            for col in 0..4 {
                let x = 50.0 + col as f32 * 80.0;
                let y = 50.0 + row as f32 * 80.0;
                let color = colors[(row + col) % 4];

                if (row + col) % 2 == 0 {
                    c.fill_rect(Rect::new(x, y, 60.0, 60.0), 8.0.into(), color.into());
                } else {
                    c.fill_circle(Point::new(x + 30.0, y + 30.0), 30.0, color.into());
                }
            }
        }
    });

    suite
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::TestHarness;

    #[test]
    #[ignore] // Requires GPU
    fn run_shapes_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
