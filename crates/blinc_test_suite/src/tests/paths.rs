//! Path rendering tests
//!
//! Tests for vector paths: lines, curves, complex shapes

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, Path, Point, Rect, Stroke};

/// Create the paths test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("paths");

    // Simple line path
    suite.add("line_path", |ctx| {
        let c = ctx.ctx();

        let path = Path::line(Point::new(50.0, 50.0), Point::new(200.0, 150.0));
        c.stroke_path(&path, &Stroke::new(3.0), Color::BLACK.into());
    });

    // Rectangle path
    suite.add("rect_path", |ctx| {
        let c = ctx.ctx();

        let path = Path::rect(Rect::new(100.0, 100.0, 150.0, 100.0));
        c.fill_path(&path, Color::BLUE.into());
        c.stroke_path(&path, &Stroke::new(2.0), Color::BLACK.into());
    });

    // Circle path
    suite.add("circle_path", |ctx| {
        let c = ctx.ctx();

        let path = Path::circle(Point::new(200.0, 150.0), 80.0);
        c.fill_path(&path, Color::RED.into());
        c.stroke_path(&path, &Stroke::new(2.0), Color::BLACK.into());
    });

    // Rounded rectangle path
    suite.add("rounded_rect_path", |ctx| {
        let c = ctx.ctx();

        let path = Path::rounded_rect(Rect::new(100.0, 100.0, 200.0, 120.0), 20.0);
        c.fill_path(&path, Color::GREEN.into());
        c.stroke_path(&path, &Stroke::new(2.0), Color::BLACK.into());
    });

    // Per-corner radius rounded rect
    suite.add("per_corner_rounded_rect", |ctx| {
        let c = ctx.ctx();

        let corner_radius = blinc_core::CornerRadius {
            top_left: 0.0,
            top_right: 20.0,
            bottom_right: 40.0,
            bottom_left: 10.0,
        };

        let path = Path::rounded_rect(Rect::new(100.0, 100.0, 200.0, 120.0), corner_radius);
        c.fill_path(&path, Color::PURPLE.into());
        c.stroke_path(&path, &Stroke::new(2.0), Color::BLACK.into());
    });

    // Triangle using path builder
    suite.add("triangle_path", |ctx| {
        let c = ctx.ctx();

        let path = Path::new()
            .move_to(200.0, 50.0)
            .line_to(300.0, 200.0)
            .line_to(100.0, 200.0)
            .close();

        c.fill_path(&path, Color::rgba(1.0, 0.5, 0.0, 1.0).into());
        c.stroke_path(&path, &Stroke::new(3.0), Color::BLACK.into());
    });

    // Star shape
    suite.add("star_path", |ctx| {
        let c = ctx.ctx();

        let center = Point::new(200.0, 150.0);
        let outer_r = 80.0;
        let inner_r = 35.0;
        let points = 5;

        let mut path = Path::new();
        for i in 0..(points * 2) {
            let angle = (i as f32 * std::f32::consts::PI / points as f32) - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { outer_r } else { inner_r };
            let x = center.x + r * angle.cos();
            let y = center.y + r * angle.sin();

            if i == 0 {
                path = path.move_to(x, y);
            } else {
                path = path.line_to(x, y);
            }
        }
        path = path.close();

        c.fill_path(&path, Color::YELLOW.into());
        c.stroke_path(&path, &Stroke::new(2.0), Color::rgba(0.8, 0.6, 0.0, 1.0).into());
    });

    // Quadratic bezier curve
    suite.add("quad_bezier", |ctx| {
        let c = ctx.ctx();

        let path = Path::new()
            .move_to(50.0, 200.0)
            .quad_to(200.0, 50.0, 350.0, 200.0);

        c.stroke_path(&path, &Stroke::new(4.0), Color::BLUE.into());

        // Draw control point marker
        c.fill_circle(Point::new(200.0, 50.0), 6.0, Color::RED.into());
        c.fill_circle(Point::new(50.0, 200.0), 6.0, Color::GREEN.into());
        c.fill_circle(Point::new(350.0, 200.0), 6.0, Color::GREEN.into());
    });

    // Cubic bezier curve
    suite.add("cubic_bezier", |ctx| {
        let c = ctx.ctx();

        let path = Path::new()
            .move_to(50.0, 150.0)
            .cubic_to(100.0, 50.0, 300.0, 250.0, 350.0, 150.0);

        c.stroke_path(&path, &Stroke::new(4.0), Color::PURPLE.into());

        // Draw control point markers
        c.fill_circle(Point::new(100.0, 50.0), 5.0, Color::RED.into());
        c.fill_circle(Point::new(300.0, 250.0), 5.0, Color::RED.into());
        c.fill_circle(Point::new(50.0, 150.0), 5.0, Color::GREEN.into());
        c.fill_circle(Point::new(350.0, 150.0), 5.0, Color::GREEN.into());
    });

    // Path with curves - heart shape
    suite.add("heart_shape", |ctx| {
        let c = ctx.ctx();

        let cx = 200.0;
        let cy = 180.0;
        let scale = 1.5;

        let path = Path::new()
            .move_to(cx, cy - 20.0 * scale)
            .cubic_to(
                cx - 40.0 * scale,
                cy - 60.0 * scale,
                cx - 80.0 * scale,
                cy - 20.0 * scale,
                cx - 80.0 * scale,
                cy + 10.0 * scale,
            )
            .cubic_to(
                cx - 80.0 * scale,
                cy + 50.0 * scale,
                cx,
                cy + 80.0 * scale,
                cx,
                cy + 80.0 * scale,
            )
            .cubic_to(
                cx,
                cy + 80.0 * scale,
                cx + 80.0 * scale,
                cy + 50.0 * scale,
                cx + 80.0 * scale,
                cy + 10.0 * scale,
            )
            .cubic_to(
                cx + 80.0 * scale,
                cy - 20.0 * scale,
                cx + 40.0 * scale,
                cy - 60.0 * scale,
                cx,
                cy - 20.0 * scale,
            )
            .close();

        c.fill_path(&path, Color::RED.into());
    });

    // Multiple paths
    suite.add("multiple_paths", |ctx| {
        let c = ctx.ctx();

        // Triangle
        let p1 = Path::new()
            .move_to(100.0, 50.0)
            .line_to(150.0, 130.0)
            .line_to(50.0, 130.0)
            .close();
        c.fill_path(&p1, Color::RED.into());

        // Square
        let p2 = Path::rect(Rect::new(180.0, 50.0, 80.0, 80.0));
        c.fill_path(&p2, Color::GREEN.into());

        // Pentagon
        let mut p3 = Path::new();
        for i in 0..5 {
            let angle = (i as f32 * std::f32::consts::TAU / 5.0) - std::f32::consts::FRAC_PI_2;
            let x = 340.0 + 40.0 * angle.cos();
            let y = 90.0 + 40.0 * angle.sin();
            if i == 0 {
                p3 = p3.move_to(x, y);
            } else {
                p3 = p3.line_to(x, y);
            }
        }
        p3 = p3.close();
        c.fill_path(&p3, Color::BLUE.into());
    });

    // Path bounds test
    suite.add("path_bounds", |ctx| {
        let c = ctx.ctx();

        let path = Path::new()
            .move_to(150.0, 100.0)
            .line_to(250.0, 80.0)
            .line_to(280.0, 180.0)
            .line_to(180.0, 220.0)
            .line_to(120.0, 160.0)
            .close();

        // Get and draw bounds
        let bounds = path.bounds();
        c.stroke_rect(bounds, 0.0.into(), &Stroke::new(1.0), Color::rgba(0.5, 0.5, 0.5, 1.0).into());

        // Draw the path
        c.fill_path(&path, Color::BLUE.with_alpha(0.5).into());
        c.stroke_path(&path, &Stroke::new(2.0), Color::BLUE.into());
    });

    suite
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::TestHarness;

    #[test]
    #[ignore] // Requires GPU
    fn run_paths_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
