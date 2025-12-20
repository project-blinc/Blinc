//! SDF Builder tests
//!
//! Tests for the SDF (Signed Distance Field) builder API

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, Point, Rect, Stroke};

/// Create the SDF test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("sdf");

    // Basic SDF rect
    suite.add("sdf_basic_rect", |ctx| {
        let c = ctx.ctx();

        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(100.0, 100.0, 200.0, 100.0), 12.0.into());
            sdf.fill(shape, Color::BLUE.into());
        });
    });

    // SDF with stroke
    suite.add("sdf_stroke", |ctx| {
        let c = ctx.ctx();

        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(100.0, 100.0, 200.0, 100.0), 12.0.into());
            sdf.stroke(shape, &Stroke::new(2.0), Color::BLUE.into());
        });
    });

    // SDF fill and stroke combined
    suite.add("sdf_fill_and_stroke", |ctx| {
        let c = ctx.ctx();

        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(100.0, 100.0, 200.0, 100.0), 12.0.into());
            sdf.fill(shape, Color::rgba(0.9, 0.95, 1.0, 1.0).into());
            sdf.stroke(shape, &Stroke::new(2.0), Color::BLUE.into());
        });
    });

    // SDF circle
    suite.add("sdf_circle", |ctx| {
        let c = ctx.ctx();

        c.sdf_build(&mut |sdf| {
            let shape = sdf.circle(Point::new(200.0, 150.0), 60.0);
            sdf.fill(shape, Color::GREEN.into());
        });
    });

    // SDF with shadow
    suite.add("sdf_with_shadow", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(100.0, 100.0, 200.0, 100.0), 12.0.into());
            sdf.shadow(shape, blinc_core::Shadow::new(4.0, 4.0, 10.0, Color::BLACK.with_alpha(0.3)));
            sdf.fill(shape, Color::WHITE.into());
        });
    });

    // Multiple SDF operations
    suite.add("sdf_multiple_shapes", |ctx| {
        let c = ctx.ctx();

        // First shape
        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(50.0, 100.0, 120.0, 80.0), 8.0.into());
            sdf.fill(shape, Color::RED.into());
        });

        // Second shape
        c.sdf_build(&mut |sdf| {
            let shape = sdf.circle(Point::new(280.0, 140.0), 50.0);
            sdf.fill(shape, Color::BLUE.into());
        });

        // Third shape
        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(150.0, 180.0, 100.0, 60.0), 4.0.into());
            sdf.fill(shape, Color::GREEN.into());
        });
    });

    // SDF button-like element
    suite.add("sdf_button", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(125.0, 120.0, 150.0, 40.0), 8.0.into());
            sdf.shadow(shape, blinc_core::Shadow::new(0.0, 2.0, 4.0, Color::BLACK.with_alpha(0.2)));
            sdf.fill(shape, Color::BLUE.into());
        });
    });

    // SDF card with layered shadows
    suite.add("sdf_card_layered", |ctx| {
        let c = ctx.ctx();

        // Light background to see shadows
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.92, 0.92, 0.94, 1.0).into(),
        );

        // Soft outer shadow
        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(100.0, 75.0, 200.0, 150.0), 16.0.into());
            sdf.shadow(shape, blinc_core::Shadow::new(0.0, 12.0, 24.0, Color::BLACK.with_alpha(0.1)));
        });

        // Sharper close shadow
        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(100.0, 75.0, 200.0, 150.0), 16.0.into());
            sdf.shadow(shape, blinc_core::Shadow::new(0.0, 4.0, 8.0, Color::BLACK.with_alpha(0.15)));
        });

        // Card fill
        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(100.0, 75.0, 200.0, 150.0), 16.0.into());
            sdf.fill(shape, Color::WHITE.into());
        });
    });

    // SDF with various corner radii
    suite.add("sdf_corner_radii", |ctx| {
        let c = ctx.ctx();

        // Sharp corners
        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(30.0, 50.0, 100.0, 60.0), 0.0.into());
            sdf.fill(shape, Color::RED.into());
        });

        // Small radius
        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(150.0, 50.0, 100.0, 60.0), 4.0.into());
            sdf.fill(shape, Color::GREEN.into());
        });

        // Medium radius
        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(270.0, 50.0, 100.0, 60.0), 12.0.into());
            sdf.fill(shape, Color::BLUE.into());
        });

        // Large radius (pill-like)
        c.sdf_build(&mut |sdf| {
            let shape = sdf.rect(Rect::new(90.0, 180.0, 220.0, 60.0), 30.0.into());
            sdf.fill(shape, Color::rgba(0.5, 0.3, 0.8, 1.0).into());
        });
    });

    // SDF strokes with different widths
    suite.add("sdf_stroke_widths", |ctx| {
        let c = ctx.ctx();

        let widths = [1.0, 2.0, 4.0, 8.0];
        for (i, width) in widths.iter().enumerate() {
            let y = 50.0 + i as f32 * 60.0;
            c.sdf_build(&mut |sdf| {
                let shape = sdf.rect(Rect::new(100.0, y, 200.0, 40.0), 8.0.into());
                sdf.stroke(shape, &Stroke::new(*width), Color::BLUE.into());
            });
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
    fn run_sdf_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
