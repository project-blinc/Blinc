//! Gradient rendering tests
//!
//! Tests for linear, radial, and conic gradients

use crate::runner::TestSuite;
use blinc_core::{Brush, Color, DrawContext, Gradient, GradientStop, Point, Rect};

/// Create the gradients test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("gradients");

    // Linear gradient - horizontal
    suite.add("linear_horizontal", |ctx| {
        let c = ctx.ctx();

        let gradient = Gradient::linear(
            Point::new(100.0, 150.0),
            Point::new(300.0, 150.0),
            Color::RED,
            Color::BLUE,
        );

        c.fill_rect(
            Rect::new(100.0, 100.0, 200.0, 100.0),
            8.0.into(),
            Brush::Gradient(gradient),
        );
    });

    // Linear gradient - vertical
    suite.add("linear_vertical", |ctx| {
        let c = ctx.ctx();

        let gradient = Gradient::linear(
            Point::new(200.0, 100.0),
            Point::new(200.0, 200.0),
            Color::GREEN,
            Color::YELLOW,
        );

        c.fill_rect(
            Rect::new(100.0, 100.0, 200.0, 100.0),
            8.0.into(),
            Brush::Gradient(gradient),
        );
    });

    // Linear gradient - diagonal
    suite.add("linear_diagonal", |ctx| {
        let c = ctx.ctx();

        let gradient = Gradient::linear(
            Point::new(100.0, 100.0),
            Point::new(300.0, 200.0),
            Color::PURPLE,
            Color::rgba(1.0, 0.5, 0.0, 1.0), // Orange
        );

        c.fill_rect(
            Rect::new(100.0, 100.0, 200.0, 100.0),
            8.0.into(),
            Brush::Gradient(gradient),
        );
    });

    // Linear gradient with multiple stops
    suite.add("linear_multi_stop", |ctx| {
        let c = ctx.ctx();

        let gradient = Gradient::linear_with_stops(
            Point::new(100.0, 150.0),
            Point::new(400.0, 150.0),
            vec![
                GradientStop::new(0.0, Color::RED),
                GradientStop::new(0.25, Color::YELLOW),
                GradientStop::new(0.5, Color::GREEN),
                GradientStop::new(0.75, Color::BLUE),
                GradientStop::new(1.0, Color::PURPLE),
            ],
        );

        c.fill_rect(
            Rect::new(100.0, 100.0, 300.0, 100.0),
            8.0.into(),
            Brush::Gradient(gradient),
        );
    });

    // Radial gradient - basic
    suite.add("radial_basic", |ctx| {
        let c = ctx.ctx();

        let gradient = Gradient::radial(Point::new(200.0, 150.0), 100.0, Color::WHITE, Color::BLUE);

        c.fill_rect(
            Rect::new(100.0, 50.0, 200.0, 200.0),
            8.0.into(),
            Brush::Gradient(gradient),
        );
    });

    // Radial gradient with multi-stop (sun effect)
    suite.add("radial_sun", |ctx| {
        let c = ctx.ctx();

        let gradient = Gradient::radial_with_stops(
            Point::new(200.0, 150.0),
            100.0,
            vec![
                GradientStop::new(0.0, Color::rgba(1.0, 1.0, 0.5, 1.0)), // Light yellow
                GradientStop::new(0.3, Color::YELLOW),
                GradientStop::new(0.6, Color::rgba(1.0, 0.5, 0.0, 1.0)), // Orange
                GradientStop::new(1.0, Color::RED),
            ],
        );

        c.fill_circle(Point::new(200.0, 150.0), 100.0, Brush::Gradient(gradient));
    });

    // Conic gradient
    suite.add("conic_basic", |ctx| {
        let c = ctx.ctx();

        let gradient = Gradient::conic(Point::new(200.0, 150.0), Color::RED, Color::BLUE);

        c.fill_circle(Point::new(200.0, 150.0), 100.0, Brush::Gradient(gradient));
    });

    // Gradient in rounded rect
    suite.add("gradient_rounded_rect", |ctx| {
        let c = ctx.ctx();

        let gradient = Gradient::linear(
            Point::new(100.0, 100.0),
            Point::new(300.0, 100.0),
            Color::rgba(0.2, 0.6, 1.0, 1.0), // Light blue
            Color::rgba(0.8, 0.2, 0.8, 1.0), // Purple
        );

        c.fill_rect(
            Rect::new(100.0, 100.0, 200.0, 100.0),
            24.0.into(),
            Brush::Gradient(gradient),
        );
    });

    // Multiple gradients
    suite.add("multiple_gradients", |ctx| {
        let c = ctx.ctx();

        // Horizontal gradient
        let g1 = Gradient::linear(
            Point::new(50.0, 75.0),
            Point::new(150.0, 75.0),
            Color::RED,
            Color::YELLOW,
        );
        c.fill_rect(
            Rect::new(50.0, 50.0, 100.0, 50.0),
            8.0.into(),
            Brush::Gradient(g1),
        );

        // Radial gradient
        let g2 = Gradient::radial(Point::new(275.0, 75.0), 40.0, Color::WHITE, Color::GREEN);
        c.fill_rect(
            Rect::new(225.0, 50.0, 100.0, 50.0),
            8.0.into(),
            Brush::Gradient(g2),
        );

        // Vertical gradient
        let g3 = Gradient::linear(
            Point::new(100.0, 150.0),
            Point::new(100.0, 250.0),
            Color::BLUE,
            Color::PURPLE,
        );
        c.fill_rect(
            Rect::new(50.0, 150.0, 100.0, 100.0),
            8.0.into(),
            Brush::Gradient(g3),
        );
    });

    // Gradient opacity
    suite.add("gradient_opacity", |ctx| {
        let c = ctx.ctx();

        // Background
        c.fill_rect(
            Rect::new(50.0, 50.0, 300.0, 200.0),
            0.0.into(),
            Color::rgba(0.2, 0.2, 0.2, 1.0).into(),
        );

        // Gradient with opacity
        c.push_opacity(0.7);
        let gradient = Gradient::linear(
            Point::new(100.0, 100.0),
            Point::new(300.0, 100.0),
            Color::WHITE,
            Color::RED,
        );
        c.fill_rect(
            Rect::new(100.0, 100.0, 200.0, 100.0),
            16.0.into(),
            Brush::Gradient(gradient),
        );
        c.pop_opacity();
    });

    suite
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::TestHarness;

    #[test]
    #[ignore] // Requires GPU
    fn run_gradients_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
