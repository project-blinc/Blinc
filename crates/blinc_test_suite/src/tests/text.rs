//! Text rendering tests
//!
//! Tests for text drawing capabilities using the blinc_text rendering pipeline.

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, Rect};

/// Helper to convert Color to [f32; 4]
fn color_to_array(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}

/// Create the text test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("text");

    // Basic text
    suite.add("text_basic", |ctx| {
        ctx.draw_text("Hello, Blinc!", 50.0, 100.0, 24.0, color_to_array(Color::BLACK));
    });

    // Different font sizes
    suite.add("text_sizes", |ctx| {
        let sizes = [12.0, 16.0, 20.0, 24.0, 32.0, 48.0];
        for (i, size) in sizes.iter().enumerate() {
            ctx.draw_text(
                &format!("Size {}", size),
                50.0,
                50.0 + i as f32 * 50.0,
                *size,
                color_to_array(Color::BLACK),
            );
        }
    });

    // Font weight labels (actual weights require multiple font files)
    suite.add("text_weights", |ctx| {
        let names = ["Thin", "Light", "Regular", "Medium", "Bold", "Black"];

        for (i, name) in names.iter().enumerate() {
            ctx.draw_text(
                *name,
                50.0,
                50.0 + i as f32 * 40.0,
                20.0,
                color_to_array(Color::BLACK),
            );
        }
    });

    // Colored text
    suite.add("text_colors", |ctx| {
        let colors = [
            ("Red", Color::RED),
            ("Green", Color::GREEN),
            ("Blue", Color::BLUE),
            ("Purple", Color::PURPLE),
        ];

        for (i, (name, color)) in colors.iter().enumerate() {
            ctx.draw_text(
                *name,
                50.0,
                50.0 + i as f32 * 40.0,
                24.0,
                color_to_array(*color),
            );
        }
    });

    // Text with background
    suite.add("text_with_background", |ctx| {
        // Draw background first
        ctx.ctx().fill_rect(
            Rect::new(40.0, 90.0, 220.0, 40.0),
            8.0.into(),
            Color::rgba(0.9, 0.9, 0.9, 1.0).into(),
        );

        // Draw text on top
        ctx.draw_text(
            "Text on background",
            50.0,
            120.0,
            20.0,
            color_to_array(Color::BLACK),
        );
    });

    // Text with different opacities
    suite.add("text_opacity", |ctx| {
        for i in 0..5 {
            let opacity = (i + 1) as f32 * 0.2;
            ctx.draw_text(
                &format!("Opacity {:.0}%", opacity * 100.0),
                50.0,
                50.0 + i as f32 * 40.0,
                20.0,
                [0.0, 0.0, 0.0, opacity], // Black with varying alpha
            );
        }
    });

    // Lorem ipsum paragraph
    suite.add("text_paragraph", |ctx| {
        let lines = [
            "Lorem ipsum dolor sit amet, consectetur",
            "adipiscing elit. Sed do eiusmod tempor",
            "incididunt ut labore et dolore magna",
            "aliqua. Ut enim ad minim veniam, quis",
            "nostrud exercitation ullamco laboris.",
        ];

        for (i, line) in lines.iter().enumerate() {
            ctx.draw_text(
                *line,
                50.0,
                50.0 + i as f32 * 24.0,
                16.0,
                color_to_array(Color::BLACK),
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
    #[ignore] // Requires GPU
    fn run_text_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
