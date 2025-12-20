//! Opacity stack tests
//!
//! Tests for opacity stacking and alpha blending

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, Point, Rect};

/// Create the opacity test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("opacity");

    // Basic opacity
    suite.add("opacity_basic", |ctx| {
        let c = ctx.ctx();

        // Full opacity
        c.fill_rect(
            Rect::new(50.0, 50.0, 100.0, 100.0),
            8.0.into(),
            Color::BLUE.into(),
        );

        // 50% opacity
        c.push_opacity(0.5);
        c.fill_rect(
            Rect::new(100.0, 100.0, 100.0, 100.0),
            8.0.into(),
            Color::RED.into(),
        );
        c.pop_opacity();

        // Full opacity again
        c.fill_rect(
            Rect::new(150.0, 150.0, 100.0, 100.0),
            8.0.into(),
            Color::GREEN.into(),
        );
    });

    // Stacked opacity
    suite.add("opacity_stacked", |ctx| {
        let c = ctx.ctx();

        // Draw background
        c.fill_rect(
            Rect::new(50.0, 50.0, 300.0, 200.0),
            0.0.into(),
            Color::WHITE.into(),
        );

        // Nested opacity: 0.5 * 0.5 = 0.25
        c.push_opacity(0.5);
        c.fill_rect(
            Rect::new(100.0, 100.0, 100.0, 100.0),
            8.0.into(),
            Color::BLUE.into(),
        );

        c.push_opacity(0.5);
        c.fill_rect(
            Rect::new(150.0, 100.0, 100.0, 100.0),
            8.0.into(),
            Color::BLUE.into(),
        );
        c.pop_opacity();

        c.pop_opacity();

        // Reference: same blue at 0.25 opacity
        c.push_opacity(0.25);
        c.fill_rect(
            Rect::new(200.0, 100.0, 100.0, 100.0),
            8.0.into(),
            Color::BLUE.into(),
        );
        c.pop_opacity();
    });

    // Opacity gradient effect
    suite.add("opacity_gradient_effect", |ctx| {
        let c = ctx.ctx();

        for i in 0..10 {
            let opacity = (i + 1) as f32 / 10.0;
            let x = 50.0 + i as f32 * 35.0;

            c.push_opacity(opacity);
            c.fill_rect(
                Rect::new(x, 100.0, 30.0, 200.0),
                4.0.into(),
                Color::BLUE.into(),
            );
            c.pop_opacity();
        }
    });

    // Opacity with overlapping shapes
    suite.add("opacity_overlapping", |ctx| {
        let c = ctx.ctx();

        // Background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::WHITE.into(),
        );

        // Three overlapping circles at 50% opacity
        c.push_opacity(0.5);
        c.fill_circle(Point::new(150.0, 150.0), 80.0, Color::RED.into());
        c.fill_circle(Point::new(200.0, 150.0), 80.0, Color::GREEN.into());
        c.fill_circle(Point::new(175.0, 200.0), 80.0, Color::BLUE.into());
        c.pop_opacity();
    });

    // Opacity with color alpha
    suite.add("opacity_with_color_alpha", |ctx| {
        let c = ctx.ctx();

        // Background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::WHITE.into(),
        );

        // Color with 50% alpha
        c.fill_rect(
            Rect::new(50.0, 100.0, 80.0, 80.0),
            8.0.into(),
            Color::BLUE.with_alpha(0.5).into(),
        );

        // Push 50% opacity with full color
        c.push_opacity(0.5);
        c.fill_rect(
            Rect::new(150.0, 100.0, 80.0, 80.0),
            8.0.into(),
            Color::BLUE.into(),
        );
        c.pop_opacity();

        // Combined: 50% opacity * 50% color alpha = 25%
        c.push_opacity(0.5);
        c.fill_rect(
            Rect::new(250.0, 100.0, 80.0, 80.0),
            8.0.into(),
            Color::BLUE.with_alpha(0.5).into(),
        );
        c.pop_opacity();
    });

    // Current opacity query
    suite.add("current_opacity", |ctx| {
        let c = ctx.ctx();

        assert!((c.current_opacity() - 1.0).abs() < 0.001);

        c.push_opacity(0.5);
        assert!((c.current_opacity() - 0.5).abs() < 0.001);

        c.push_opacity(0.5);
        assert!((c.current_opacity() - 0.25).abs() < 0.001);

        c.pop_opacity();
        assert!((c.current_opacity() - 0.5).abs() < 0.001);

        c.pop_opacity();
        assert!((c.current_opacity() - 1.0).abs() < 0.001);

        // Draw indicator that test passed
        c.fill_rect(
            Rect::new(100.0, 100.0, 100.0, 100.0),
            8.0.into(),
            Color::GREEN.into(),
        );
    });

    // Fade in/out animation effect
    suite.add("fade_animation", |ctx| {
        let c = ctx.ctx();

        // Simulate 10 frames of fade
        for frame in 0..10 {
            let opacity = (frame + 1) as f32 / 10.0;
            let x = 50.0 + frame as f32 * 35.0;

            c.push_opacity(opacity);
            c.fill_circle(Point::new(x + 15.0, 150.0), 15.0, Color::PURPLE.into());
            c.pop_opacity();
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
    fn run_opacity_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
