//! Shadow rendering tests
//!
//! Tests for drop shadows with blur and spread

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, Point, Rect, Shadow};

/// Create the shadows test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("shadows");

    // Basic shadow
    suite.add("shadow_basic", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        // Draw shadow first
        c.draw_shadow(
            Rect::new(100.0, 100.0, 150.0, 100.0),
            12.0.into(),
            Shadow::new(4.0, 4.0, 10.0, Color::BLACK.with_alpha(0.3)),
        );

        // Then draw the shape
        c.fill_rect(
            Rect::new(100.0, 100.0, 150.0, 100.0),
            12.0.into(),
            Color::WHITE.into(),
        );
    });

    // Large blur shadow
    suite.add("shadow_large_blur", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        c.draw_shadow(
            Rect::new(100.0, 100.0, 150.0, 100.0),
            12.0.into(),
            Shadow::new(8.0, 8.0, 30.0, Color::BLACK.with_alpha(0.4)),
        );

        c.fill_rect(
            Rect::new(100.0, 100.0, 150.0, 100.0),
            12.0.into(),
            Color::WHITE.into(),
        );
    });

    // Shadow with spread
    suite.add("shadow_with_spread", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        c.draw_shadow(
            Rect::new(100.0, 100.0, 150.0, 100.0),
            12.0.into(),
            Shadow {
                offset_x: 0.0,
                offset_y: 4.0,
                blur: 10.0,
                spread: 5.0,
                color: Color::BLACK.with_alpha(0.3),
            },
        );

        c.fill_rect(
            Rect::new(100.0, 100.0, 150.0, 100.0),
            12.0.into(),
            Color::WHITE.into(),
        );
    });

    // Colored shadow
    suite.add("shadow_colored", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        // Blue shadow
        c.draw_shadow(
            Rect::new(50.0, 100.0, 100.0, 80.0),
            8.0.into(),
            Shadow::new(6.0, 6.0, 15.0, Color::BLUE.with_alpha(0.5)),
        );
        c.fill_rect(
            Rect::new(50.0, 100.0, 100.0, 80.0),
            8.0.into(),
            Color::rgba(0.8, 0.9, 1.0, 1.0).into(),
        );

        // Red shadow
        c.draw_shadow(
            Rect::new(180.0, 100.0, 100.0, 80.0),
            8.0.into(),
            Shadow::new(6.0, 6.0, 15.0, Color::RED.with_alpha(0.5)),
        );
        c.fill_rect(
            Rect::new(180.0, 100.0, 100.0, 80.0),
            8.0.into(),
            Color::rgba(1.0, 0.9, 0.9, 1.0).into(),
        );
    });

    // Inner shadow effect (negative spread)
    suite.add("shadow_inner_effect", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        // Draw the shape first for inner shadow effect
        c.fill_rect(
            Rect::new(100.0, 100.0, 150.0, 100.0),
            12.0.into(),
            Color::WHITE.into(),
        );

        // Inner shadow simulation with negative offset
        c.draw_shadow(
            Rect::new(100.0, 100.0, 150.0, 100.0),
            12.0.into(),
            Shadow {
                offset_x: 3.0,
                offset_y: 3.0,
                blur: 8.0,
                spread: -2.0,
                color: Color::BLACK.with_alpha(0.25),
            },
        );
    });

    // Multiple shadows (layered)
    suite.add("shadow_layered", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        let rect = Rect::new(100.0, 100.0, 150.0, 100.0);
        let radius = 12.0;

        // Outer soft shadow
        c.draw_shadow(
            rect,
            radius.into(),
            Shadow::new(0.0, 20.0, 40.0, Color::BLACK.with_alpha(0.15)),
        );

        // Medium shadow
        c.draw_shadow(
            rect,
            radius.into(),
            Shadow::new(0.0, 10.0, 20.0, Color::BLACK.with_alpha(0.2)),
        );

        // Close sharp shadow
        c.draw_shadow(
            rect,
            radius.into(),
            Shadow::new(0.0, 4.0, 8.0, Color::BLACK.with_alpha(0.25)),
        );

        // The shape
        c.fill_rect(rect, radius.into(), Color::WHITE.into());
    });

    // Different offset directions
    suite.add("shadow_directions", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        let offsets = [
            (0.0, -8.0, "Top"),
            (8.0, 0.0, "Right"),
            (0.0, 8.0, "Bottom"),
            (-8.0, 0.0, "Left"),
        ];

        for (i, (ox, oy, _name)) in offsets.iter().enumerate() {
            let x = 80.0 + (i % 2) as f32 * 160.0;
            let y = 80.0 + (i / 2) as f32 * 140.0;

            c.draw_shadow(
                Rect::new(x, y, 100.0, 80.0),
                8.0.into(),
                Shadow::new(*ox, *oy, 12.0, Color::BLACK.with_alpha(0.3)),
            );

            c.fill_rect(Rect::new(x, y, 100.0, 80.0), 8.0.into(), Color::WHITE.into());
        }
    });

    // Shadow on circle
    suite.add("shadow_circle", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        // Approximate circle shadow with rounded rect
        let center = Point::new(200.0, 150.0);
        let radius = 60.0;
        let shadow_rect = Rect::new(center.x - radius, center.y - radius, radius * 2.0, radius * 2.0);

        c.draw_shadow(
            shadow_rect,
            radius.into(),
            Shadow::new(6.0, 6.0, 20.0, Color::BLACK.with_alpha(0.4)),
        );

        c.fill_circle(center, radius, Color::WHITE.into());
    });

    // Card UI pattern
    suite.add("shadow_card_ui", |ctx| {
        let c = ctx.ctx();

        // Background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.95, 0.95, 0.97, 1.0).into(),
        );

        // Card with shadow
        let card_rect = Rect::new(50.0, 50.0, 300.0, 180.0);

        c.draw_shadow(
            card_rect,
            16.0.into(),
            Shadow {
                offset_x: 0.0,
                offset_y: 4.0,
                blur: 16.0,
                spread: -2.0,
                color: Color::BLACK.with_alpha(0.12),
            },
        );

        c.fill_rect(card_rect, 16.0.into(), Color::WHITE.into());

        // Card content area
        c.fill_rect(
            Rect::new(60.0, 60.0, 280.0, 80.0),
            8.0.into(),
            Color::rgba(0.9, 0.95, 1.0, 1.0).into(),
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
    fn run_shadows_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
