//! Minimal text positioning test
//!
//! Tests that text is correctly centered within parent containers.
//!
//! Run with: cargo run -p blinc_app --example text_position_test --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::Color;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let config = WindowConfig {
        title: "Text Position Test".to_string(),
        width: 600,
        height: 400,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Use logical pixel values for consistent testing
    // The scale factor is applied at render time
    let scale = ctx.scale_factor;
    tracing::info!(
        "Window: {}x{}, scale_factor: {}",
        ctx.width,
        ctx.height,
        scale
    );

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
        .flex_col()
        .gap(20.0)
        .p(40.0)
        .items_center() // Center all children horizontally
        // Test 1: Simple centered text
        .child(
            div()
                .w(400.0)
                .h(60.0)
                .bg(Color::rgba(0.2, 0.2, 0.3, 1.0))
                .rounded(8.0)
                .items_center()
                .justify_center()
                .child(
                    text("Centered Text (items_center + justify_center)")
                        .size(16.0)
                        .color(Color::WHITE),
                ),
        )
        // Test 2: Centered text with text_center()
        .child(
            div()
                .w(400.0)
                .h(60.0)
                .bg(Color::rgba(0.3, 0.2, 0.2, 1.0))
                .rounded(8.0)
                .items_center()
                .justify_center()
                .child(
                    text("Centered + text_center()")
                        .size(16.0)
                        .color(Color::WHITE)
                        .text_center(),
                ),
        )
        // Test 3: Button-like container with v_center
        .child(
            div()
                .w(200.0)
                .h(50.0)
                .bg(Color::rgba(0.3, 0.5, 0.8, 1.0))
                .rounded(8.0)
                .items_center()
                .justify_center()
                .child(
                    text("Button (v_center)")
                        .size(18.0)
                        .color(Color::WHITE)
                        .v_center(),
                ),
        )
        // Test 4: Left-aligned text (default)
        .child(
            div()
                .w(400.0)
                .h(60.0)
                .bg(Color::rgba(0.2, 0.3, 0.2, 1.0))
                .rounded(8.0)
                .items_center()
                // No justify_center - text box at left, but vertically centered
                .child(
                    text("Left-aligned (items_center only)")
                        .size(16.0)
                        .color(Color::WHITE),
                ),
        )
}
