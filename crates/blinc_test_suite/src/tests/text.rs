//! Text rendering tests
//!
//! Tests for text drawing capabilities
//! Note: Actual text rendering requires font infrastructure which may not be fully implemented

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, FontWeight, Point, Rect, TextStyle};

/// Create the text test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("text");

    // Basic text
    suite.add("text_basic", |ctx| {
        let c = ctx.ctx();

        c.draw_text(
            "Hello, Blinc!",
            Point::new(50.0, 100.0),
            &TextStyle::new(24.0).with_color(Color::BLACK),
        );
    });

    // Different font sizes
    suite.add("text_sizes", |ctx| {
        let c = ctx.ctx();

        let sizes = [12.0, 16.0, 20.0, 24.0, 32.0, 48.0];
        for (i, size) in sizes.iter().enumerate() {
            c.draw_text(
                &format!("Size {}", size),
                Point::new(50.0, 50.0 + i as f32 * 50.0),
                &TextStyle::new(*size).with_color(Color::BLACK),
            );
        }
    });

    // Different font weights
    suite.add("text_weights", |ctx| {
        let c = ctx.ctx();

        let weights = [
            FontWeight::Thin,
            FontWeight::Light,
            FontWeight::Regular,
            FontWeight::Medium,
            FontWeight::Bold,
            FontWeight::Black,
        ];
        let names = ["Thin", "Light", "Regular", "Medium", "Bold", "Black"];

        for (i, (weight, name)) in weights.iter().zip(names.iter()).enumerate() {
            c.draw_text(
                *name,
                Point::new(50.0, 50.0 + i as f32 * 40.0),
                &TextStyle::new(20.0)
                    .with_color(Color::BLACK)
                    .with_weight(*weight),
            );
        }
    });

    // Colored text
    suite.add("text_colors", |ctx| {
        let c = ctx.ctx();

        let colors = [
            ("Red", Color::RED),
            ("Green", Color::GREEN),
            ("Blue", Color::BLUE),
            ("Purple", Color::PURPLE),
        ];

        for (i, (name, color)) in colors.iter().enumerate() {
            c.draw_text(
                *name,
                Point::new(50.0, 50.0 + i as f32 * 40.0),
                &TextStyle::new(24.0).with_color(*color),
            );
        }
    });

    // Text with background
    suite.add("text_with_background", |ctx| {
        let c = ctx.ctx();

        // Draw background
        c.fill_rect(
            Rect::new(40.0, 90.0, 200.0, 40.0),
            8.0.into(),
            Color::rgba(0.9, 0.9, 0.9, 1.0).into(),
        );

        // Draw text on top
        c.draw_text(
            "Text on background",
            Point::new(50.0, 120.0),
            &TextStyle::new(20.0).with_color(Color::BLACK),
        );
    });

    // Text with opacity
    suite.add("text_opacity", |ctx| {
        let c = ctx.ctx();

        for i in 0..5 {
            let opacity = (i + 1) as f32 * 0.2;
            c.push_opacity(opacity);
            c.draw_text(
                &format!("Opacity {:.0}%", opacity * 100.0),
                Point::new(50.0, 50.0 + i as f32 * 40.0),
                &TextStyle::new(20.0).with_color(Color::BLACK),
            );
            c.pop_opacity();
        }
    });

    // Lorem ipsum paragraph
    suite.add("text_paragraph", |ctx| {
        let c = ctx.ctx();

        let lines = [
            "Lorem ipsum dolor sit amet, consectetur",
            "adipiscing elit. Sed do eiusmod tempor",
            "incididunt ut labore et dolore magna",
            "aliqua. Ut enim ad minim veniam, quis",
            "nostrud exercitation ullamco laboris.",
        ];

        for (i, line) in lines.iter().enumerate() {
            c.draw_text(
                *line,
                Point::new(50.0, 50.0 + i as f32 * 24.0),
                &TextStyle::new(16.0).with_color(Color::BLACK),
            );
        }
    });

    suite
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::TestHarness;

    #[test]
    #[ignore] // Requires GPU + font infrastructure
    fn run_text_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
