//! Transform stack tests
//!
//! Tests for 2D transforms: translation, rotation, scale, composition

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, Point, Rect, Transform};

/// Create the transforms test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("transforms");

    // Basic translation
    suite.add("translate_basic", |ctx| {
        let c = ctx.ctx();

        // Draw a rect at origin
        c.fill_rect(
            Rect::new(0.0, 0.0, 50.0, 50.0),
            4.0.into(),
            Color::RED.into(),
        );

        // Translate and draw another
        c.push_transform(Transform::translate(100.0, 100.0));
        c.fill_rect(
            Rect::new(0.0, 0.0, 50.0, 50.0),
            4.0.into(),
            Color::BLUE.into(),
        );
        c.pop_transform();

        // Third rect should be at origin again
        c.fill_rect(
            Rect::new(60.0, 0.0, 50.0, 50.0),
            4.0.into(),
            Color::GREEN.into(),
        );
    });

    // Basic scale
    suite.add("scale_basic", |ctx| {
        let c = ctx.ctx();

        // Original size
        c.fill_rect(
            Rect::new(50.0, 50.0, 50.0, 50.0),
            4.0.into(),
            Color::RED.into(),
        );

        // Scaled 2x
        c.push_transform(Transform::scale(2.0, 2.0));
        c.fill_rect(
            Rect::new(100.0, 50.0, 50.0, 50.0),
            4.0.into(),
            Color::BLUE.into(),
        );
        c.pop_transform();

        // Scaled 0.5x
        c.push_transform(Transform::scale(0.5, 0.5));
        c.fill_rect(
            Rect::new(400.0, 100.0, 100.0, 100.0),
            4.0.into(),
            Color::GREEN.into(),
        );
        c.pop_transform();
    });

    // Basic rotation
    suite.add("rotate_basic", |ctx| {
        let c = ctx.ctx();
        let center = Point::new(200.0, 200.0);

        // Draw rotated rectangles around center
        for i in 0..8 {
            let angle = i as f32 * std::f32::consts::PI / 4.0;

            c.push_transform(Transform::translate(center.x, center.y));
            c.push_transform(Transform::rotate(angle));
            c.push_transform(Transform::translate(-25.0, -100.0));

            let hue = i as f32 / 8.0;
            let color = Color::rgba(
                (hue * 6.0).sin().abs(),
                ((hue * 6.0 + 2.0).sin()).abs(),
                ((hue * 6.0 + 4.0).sin()).abs(),
                1.0,
            );

            c.fill_rect(Rect::new(0.0, 0.0, 50.0, 80.0), 8.0.into(), color.into());

            c.pop_transform();
            c.pop_transform();
            c.pop_transform();
        }
    });

    // Transform composition
    suite.add("transform_composition", |ctx| {
        let c = ctx.ctx();

        // Translate then scale
        c.push_transform(Transform::translate(100.0, 100.0));
        c.push_transform(Transform::scale(2.0, 2.0));
        c.fill_rect(
            Rect::new(0.0, 0.0, 30.0, 30.0),
            4.0.into(),
            Color::RED.into(),
        );
        c.pop_transform();
        c.pop_transform();

        // Scale then translate (different result)
        c.push_transform(Transform::scale(2.0, 2.0));
        c.push_transform(Transform::translate(150.0, 50.0));
        c.fill_rect(
            Rect::new(0.0, 0.0, 30.0, 30.0),
            4.0.into(),
            Color::BLUE.into(),
        );
        c.pop_transform();
        c.pop_transform();
    });

    // Nested transforms
    suite.add("nested_transforms", |ctx| {
        let c = ctx.ctx();

        // Outer translation
        c.push_transform(Transform::translate(200.0, 200.0));

        for i in 0..5 {
            // Each iteration: rotate and shrink
            c.push_transform(Transform::rotate(std::f32::consts::PI / 10.0));
            c.push_transform(Transform::scale(0.85, 0.85));

            let alpha = 1.0 - i as f32 * 0.15;
            c.fill_rect(
                Rect::new(-50.0, -50.0, 100.0, 100.0),
                8.0.into(),
                Color::BLUE.with_alpha(alpha).into(),
            );
        }

        // Pop all inner transforms
        for _ in 0..10 {
            c.pop_transform();
        }

        c.pop_transform(); // Outer translation
    });

    // Transform with circles
    suite.add("transform_circles", |ctx| {
        let c = ctx.ctx();

        // Draw a circle, then transformed copies
        c.fill_circle(Point::new(100.0, 100.0), 30.0, Color::RED.into());

        c.push_transform(Transform::translate(200.0, 0.0));
        c.fill_circle(Point::new(100.0, 100.0), 30.0, Color::GREEN.into());
        c.pop_transform();

        c.push_transform(Transform::translate(0.0, 200.0));
        c.fill_circle(Point::new(100.0, 100.0), 30.0, Color::BLUE.into());
        c.pop_transform();
    });

    // Spiral pattern using transforms
    suite.add("spiral_pattern", |ctx| {
        let c = ctx.ctx();
        let center = Point::new(300.0, 300.0);

        c.push_transform(Transform::translate(center.x, center.y));

        for i in 0..60 {
            let angle = i as f32 * 0.15;
            let scale = 0.97_f32.powi(i as i32);
            let offset = i as f32 * 2.0;

            c.push_transform(Transform::rotate(angle));
            c.push_transform(Transform::scale(scale, scale));

            let hue = (i % 12) as f32 / 12.0;
            let color = Color::rgba(
                (hue * std::f32::consts::TAU).sin() * 0.5 + 0.5,
                ((hue + 0.33) * std::f32::consts::TAU).sin() * 0.5 + 0.5,
                ((hue + 0.66) * std::f32::consts::TAU).sin() * 0.5 + 0.5,
                0.8,
            );

            c.fill_rect(
                Rect::new(offset, -5.0, 40.0, 10.0),
                3.0.into(),
                color.into(),
            );

            c.pop_transform();
            c.pop_transform();
        }

        c.pop_transform();
    });

    // Current transform query
    suite.add("current_transform", |ctx| {
        let c = ctx.ctx();

        let t1 = c.current_transform();
        assert!(t1.is_2d(), "Initial transform should be 2D");

        c.push_transform(Transform::translate(50.0, 50.0));
        let _t2 = c.current_transform();

        c.push_transform(Transform::scale(2.0, 2.0));
        let _t3 = c.current_transform();

        c.pop_transform();
        c.pop_transform();

        // Draw markers at each transform state
        c.fill_rect(Rect::new(0.0, 0.0, 20.0, 20.0), 4.0.into(), Color::RED.into());
    });

    suite
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::TestHarness;

    #[test]
    #[ignore] // Requires GPU
    fn run_transforms_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
