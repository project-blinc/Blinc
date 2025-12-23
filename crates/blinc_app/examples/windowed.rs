//! Windowed Application Example
//!
//! This example demonstrates how to create a windowed Blinc application
//! using the platform abstraction layer with a colorful music-player style background.
//!
//! Run with: cargo run -p blinc_app --example windowed --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};

fn main() -> Result<()> {
    // Initialize tracing for logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Configure the window
    let config = WindowConfig {
        title: "Blinc Windowed Example".to_string(),
        width: 800,
        height: 600,
        resizable: true,
        ..Default::default()
    };

    // Run the windowed application
    WindowedApp::run(config, |ctx| build_ui(ctx))
}

/// Build the UI based on the current window context
fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Scale factor based on window size (baseline 800x600)
    let scale_x = ctx.width / 800.0;
    let scale_y = ctx.height / 600.0;
    let scale = (scale_x + scale_y) / 2.0; // Average scale

    // Root container with purple background
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.4, 0.2, 0.6, 1.0))
        // Background color blobs layer
        .child(build_blobs(ctx.width, ctx.height, scale))
        // Main content layer
        .child(build_content(ctx))
}

/// Build the colorful background blobs, scaled to window size
fn build_blobs(width: f32, height: f32, scale: f32) -> impl ElementBuilder {
    div()
        .w(width)
        .h(height)
        .absolute()
        // Large pink blob - top right
        .child(
            blob(320.0 * scale, Color::rgba(1.0, 0.4, 0.6, 0.5))
                .top(-40.0 * scale)
                .right(60.0 * scale),
        )
        // Blue blob - bottom left
        .child(
            blob(380.0 * scale, Color::rgba(0.3, 0.5, 1.0, 0.45))
                .bottom(-60.0 * scale)
                .left(40.0 * scale),
        )
        // Purple blob - center left
        .child(
            blob(260.0 * scale, Color::rgba(0.6, 0.3, 0.9, 0.4))
                .top(150.0 * scale)
                .left(120.0 * scale),
        )
        // Orange blob - top left
        .child(
            blob(200.0 * scale, Color::rgba(1.0, 0.6, 0.2, 0.5))
                .top(30.0 * scale)
                .left(-30.0 * scale),
        )
        // Cyan blob - bottom right
        .child(
            blob(240.0 * scale, Color::rgba(0.2, 0.8, 0.9, 0.45))
                .bottom(80.0 * scale)
                .right(100.0 * scale),
        )
        // Yellow blob - top center
        .child(
            blob(180.0 * scale, Color::rgba(1.0, 0.85, 0.2, 0.45))
                .top(60.0 * scale)
                .left(300.0 * scale),
        )
        // Green blob - middle right
        .child(
            blob(220.0 * scale, Color::rgba(0.3, 0.9, 0.5, 0.4))
                .top(200.0 * scale)
                .right(-20.0 * scale),
        )
        // Magenta blob - bottom center
        .child(
            blob(280.0 * scale, Color::rgba(0.9, 0.2, 0.7, 0.35))
                .bottom(-50.0 * scale)
                .left(350.0 * scale),
        )
        // Small coral blob - center
        .child(
            blob(150.0 * scale, Color::rgba(1.0, 0.5, 0.4, 0.5))
                .top(280.0 * scale)
                .left(450.0 * scale),
        )
        // Small teal blob - top right area
        .child(
            blob(140.0 * scale, Color::rgba(0.2, 0.7, 0.7, 0.45))
                .top(120.0 * scale)
                .right(200.0 * scale),
        )
        // Lavender blob - left side
        .child(
            blob(170.0 * scale, Color::rgba(0.7, 0.5, 1.0, 0.4))
                .top(350.0 * scale)
                .left(20.0 * scale),
        )
        // Light blue blob - bottom area
        .child(
            blob(190.0 * scale, Color::rgba(0.4, 0.7, 1.0, 0.4))
                .bottom(150.0 * scale)
                .left(200.0 * scale),
        )
}

/// Create a circular blob with given size and color
fn blob(size: f32, color: Color) -> Div {
    div()
        .w(size)
        .h(size)
        .bg(color)
        .rounded(size / 2.0)
        .absolute()
}

/// Build the main content layer
fn build_content(ctx: &WindowedContext) -> impl ElementBuilder {
     let scale_x = ctx.width / 800.0;
    let scale_y = ctx.height / 600.0;
    let scale = (scale_x + scale_y) / 2.0; // Average scale
    div()
        .w(ctx.width)
        .h(ctx.height)
        .flex_col()
        .items_center()
        .justify_center()
        .gap(24.0)
        // Glass card with welcome message
        .child(
            div()
                .glass()
                .shadow_xl()
                .rounded(56.0)
                .p(40.0)
                .flex_col()
                .items_center()
                .justify_center()
                .gap(16.0)
                .child(text("Welcome to Blinc").text_center().size(64.0).color(Color::WHITE))
                .child(
                    text("A modern UI framework for Rust")
                    .text_center()
                        .size(32.0)
                        .color(Color::WHITE),
                ),
        )
        // Info panel showing window state
        .child(
            div()
                .glass()
                .shadow_lg()
                .rounded(56.0)
                .p(20.0)
                .flex_row()
                .gap(24.0)
                .child(info_item(
                    "Size",
                    &format!("{}x{}", ctx.width as u32, ctx.height as u32),
                ))
                .child(info_item("Scale", &format!("{:.1}x", ctx.scale_factor)))
                .child(info_item("Focus", if ctx.focused { "Yes" } else { "No" })),
        )
        // Feature cards row
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .items_center()
                .child(feature_card(
                    "Glass Effects",
                    Color::rgba(1.0, 0.4, 0.6, 0.8),
                ))
                .child(feature_card(
                    "Flexbox Layout",
                    Color::rgba(0.3, 0.5, 1.0, 0.8),
                ))
                .child(feature_card(
                    "GPU Rendered",
                    Color::rgba(0.6, 0.3, 0.9, 0.8),
                )),
        )
}

/// Create an info item with label and value
fn info_item(label: &str, value: &str) -> impl ElementBuilder {
    div()
        .flex_col()
        .items_center()
        .gap(4.0)
        .child(
            text(label)
                .size(24.0)
                .color(Color::WHITE),
        )
        .child(
            text(value)
                .size(32.0)
                .color(Color::WHITE),
        )
}

/// Create a feature card with a colored accent
fn feature_card(label: &str, accent: Color) -> impl ElementBuilder {
    div()
        .flex()
        .w_fit()
        .p(4.0)
        .bg(accent)
        .shadow_md()
        .rounded(14.0)
        .flex_col()
        .items_center()
        .justify_center()
        .child(text(label).align(TextAlign::Center).size(24.0).color(Color::WHITE))
}
