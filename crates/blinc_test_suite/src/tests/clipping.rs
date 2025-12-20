//! Clipping tests
//!
//! Tests for clip regions (rect, rounded rect, circle, path)

use crate::runner::TestSuite;
use blinc_core::{ClipShape, Color, DrawContext, Point, Rect, CornerRadius, Vec2};

/// Create the clipping test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("clipping");

    // Rect clip
    suite.add("clip_rect", |ctx| {
        let c = ctx.ctx();

        // Push a rectangular clip region
        c.push_clip(ClipShape::Rect(Rect::new(100.0, 100.0, 200.0, 100.0)));

        // Draw a larger rect that should be clipped
        c.fill_rect(
            Rect::new(50.0, 50.0, 300.0, 200.0),
            0.0.into(),
            Color::BLUE.into(),
        );

        c.pop_clip();

        // Draw border to show clip region
        c.stroke_rect(
            Rect::new(100.0, 100.0, 200.0, 100.0),
            0.0.into(),
            &blinc_core::Stroke::new(1.0),
            Color::RED.into(),
        );
    });

    // Rounded rect clip
    suite.add("clip_rounded_rect", |ctx| {
        let c = ctx.ctx();

        c.push_clip(ClipShape::RoundedRect {
            rect: Rect::new(100.0, 100.0, 200.0, 100.0),
            corner_radius: 20.0.into(),
        });

        // Draw gradient that gets clipped
        c.fill_rect(
            Rect::new(50.0, 50.0, 300.0, 200.0),
            0.0.into(),
            Color::BLUE.into(),
        );

        c.pop_clip();
    });

    // Circle clip
    suite.add("clip_circle", |ctx| {
        let c = ctx.ctx();

        c.push_clip(ClipShape::Circle {
            center: Point::new(200.0, 150.0),
            radius: 80.0,
        });

        // Draw checkerboard pattern that gets clipped
        let size = 20.0;
        for row in 0..15 {
            for col in 0..20 {
                let x = col as f32 * size;
                let y = row as f32 * size;
                let color = if (row + col) % 2 == 0 {
                    Color::WHITE
                } else {
                    Color::BLACK
                };
                c.fill_rect(Rect::new(x, y, size, size), 0.0.into(), color.into());
            }
        }

        c.pop_clip();

        // Show circle outline
        c.stroke_circle(
            Point::new(200.0, 150.0),
            80.0,
            &blinc_core::Stroke::new(1.0),
            Color::RED.into(),
        );
    });

    // Ellipse clip
    suite.add("clip_ellipse", |ctx| {
        let c = ctx.ctx();

        c.push_clip(ClipShape::Ellipse {
            center: Point::new(200.0, 150.0),
            radii: Vec2::new(100.0, 60.0),
        });

        // Draw stripes that get clipped
        for i in 0..20 {
            let x = i as f32 * 20.0;
            let color = if i % 2 == 0 {
                Color::BLUE
            } else {
                Color::rgba(0.2, 0.4, 0.8, 1.0)
            };
            c.fill_rect(Rect::new(x, 0.0, 20.0, 300.0), 0.0.into(), color.into());
        }

        c.pop_clip();
    });

    // Nested clips
    suite.add("clip_nested", |ctx| {
        let c = ctx.ctx();

        // Outer clip
        c.push_clip(ClipShape::Rect(Rect::new(50.0, 50.0, 300.0, 200.0)));

        // Fill background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.9, 0.9, 0.95, 1.0).into(),
        );

        // Inner clip
        c.push_clip(ClipShape::Circle {
            center: Point::new(200.0, 150.0),
            radius: 70.0,
        });

        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::BLUE.into(),
        );

        c.pop_clip(); // Inner
        c.pop_clip(); // Outer
    });

    // Clip with transform
    suite.add("clip_with_transform", |ctx| {
        let c = ctx.ctx();

        // Apply transform first
        c.push_transform(blinc_core::Transform::translate(200.0, 150.0));
        c.push_transform(blinc_core::Transform::rotate(0.3));

        // Clip in transformed space
        c.push_clip(ClipShape::Rect(Rect::new(-75.0, -50.0, 150.0, 100.0)));

        // Draw content
        c.fill_rect(
            Rect::new(-100.0, -100.0, 200.0, 200.0),
            0.0.into(),
            Color::GREEN.into(),
        );

        c.pop_clip();
        c.pop_transform();
        c.pop_transform();
    });

    // Multiple disjoint clips (simulated with separate operations)
    suite.add("clip_multiple_shapes", |ctx| {
        let c = ctx.ctx();

        // First clip region
        c.push_clip(ClipShape::Circle {
            center: Point::new(120.0, 150.0),
            radius: 60.0,
        });

        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::RED.into(),
        );

        c.pop_clip();

        // Second clip region
        c.push_clip(ClipShape::Circle {
            center: Point::new(280.0, 150.0),
            radius: 60.0,
        });

        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::BLUE.into(),
        );

        c.pop_clip();
    });

    // Card with clipped image area
    suite.add("clip_card_ui", |ctx| {
        let c = ctx.ctx();

        let card_rect = Rect::new(100.0, 50.0, 200.0, 200.0);
        let image_rect = Rect::new(100.0, 50.0, 200.0, 120.0);

        // Draw card background
        c.fill_rect(card_rect, 16.0.into(), Color::WHITE.into());

        // Clip for image area (top rounded corners only)
        c.push_clip(ClipShape::RoundedRect {
            rect: image_rect,
            corner_radius: CornerRadius::new(16.0, 16.0, 0.0, 0.0),
        });

        // Placeholder image (gradient)
        c.fill_rect(
            image_rect,
            0.0.into(),
            Color::rgba(0.3, 0.5, 0.8, 1.0).into(),
        );

        c.pop_clip();

        // Card border
        c.stroke_rect(
            card_rect,
            16.0.into(),
            &blinc_core::Stroke::new(1.0),
            Color::rgba(0.8, 0.8, 0.8, 1.0).into(),
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
    fn run_clipping_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
