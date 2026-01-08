//! Windowed Application Example
//!
//! This example demonstrates how to create a windowed Blinc application
//! using the platform abstraction layer with a colorful music-player style background.
//!
//! Features demonstrated:
//! - `Stateful<S>` with `on_state` callback for reactive state management
//! - Window resize/focus events via context properties
//! - Image element with hover glow effect
//!
//! Run with: cargo run -p blinc_app --example windowed --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::{Shadow, Transform};
use blinc_layout::stateful::ButtonState;

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
    // Note: State is managed at the component level using keyed use_state
    WindowedApp::run(config, |ctx| build_ui(ctx))
}

/// Build the UI based on the current window context
fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Scale factor based on window size (baseline 800x600)
    let scale_x = ctx.width / 800.0;
    let scale_y = ctx.height / 600.0;
    let scale = (scale_x + scale_y) / 2.0;

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
    let width = ctx.width;
    let height = ctx.height;
    let scale_factor = ctx.scale_factor;
    let focused = ctx.focused;

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
                .child(
                    text("Welcome to Blinc")
                        .weight(FontWeight::ExtraBold)
                        .text_center()
                        .size(64.0)
                        .color(Color::WHITE),
                )
                .child(
                    text("A modern UI framework for Rust")
                        .text_center()
                        .size(32.0)
                        .color(Color::WHITE),
                ),
        )
        // Info panel showing window state with event-driven updates
        .child(build_info_panel(width, height, scale_factor, focused))
        // Feature cards row - each card uses ctx.use_state() for reactive hover effects
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .items_center()
                .child(feature_card(
                    ctx,
                    "Glass Effects",
                    Color::rgba(1.0, 0.4, 0.6, 0.8),
                ))
                .child(feature_card(
                    ctx,
                    "Flexbox Layout",
                    Color::rgba(0.3, 0.5, 1.0, 0.8),
                ))
                .child(feature_card(
                    ctx,
                    "GPU Rendered",
                    Color::rgba(0.6, 0.3, 0.9, 0.8),
                )),
        )
        // Image showcase card with hover effect
        .child(build_image_showcase(ctx))
}

/// Build the info panel with window state (responds to resize/focus events)
fn build_info_panel(
    width: f32,
    height: f32,
    scale_factor: f64,
    focused: bool,
) -> impl ElementBuilder {
    // Focus indicator color changes based on window focus state
    let focus_color = if focused {
        Color::rgba(0.3, 1.0, 0.5, 1.0) // Green when focused
    } else {
        Color::rgba(1.0, 0.5, 0.3, 1.0) // Orange when not focused
    };

    let focus_text = if focused { "Yes" } else { "No" };

    div()
        .glass()
        .shadow_lg()
        .rounded(56.0)
        .p(20.0)
        .flex_row()
        .gap(24.0)
        // Size info - updates on window resize
        .child(
            div()
                .flex_col()
                .items_center()
                .gap(4.0)
                .child(text("Size").bold().size(24.0).color(Color::WHITE))
                .child(
                    text(&format!("{}x{}", width as u32, height as u32))
                        .size(30.0)
                        .color(Color::WHITE),
                ),
        )
        // Scale info
        .child(info_item("Scale", &format!("{:.1}x", scale_factor)))
        // Focus info - updates on window focus/blur events with visual feedback
        .child(
            div()
                .flex_col()
                .items_center()
                .gap(4.0)
                .child(text("Focus").bold().size(24.0).color(Color::WHITE))
                .child(
                    div()
                        .flex_row()
                        .items_center()
                        .gap(8.0)
                        .child(
                            // Focus indicator dot
                            div().w(12.0).h(12.0).rounded(6.0).bg(focus_color),
                        )
                        .child(text(focus_text).size(30.0).color(Color::WHITE)),
                ),
        )
}

/// Create an info item with label and value
fn info_item(label: &str, value: &str) -> impl ElementBuilder {
    div()
        .flex_col()
        .items_center()
        .gap(4.0)
        .child(text(label).bold().size(24.0).color(Color::WHITE))
        .child(text(value).size(30.0).color(Color::WHITE))
}

/// Create a feature card with reactive hover effects using Stateful<ButtonState>
///
/// Uses the new stateful::<S>() API for automatic key generation.
fn feature_card(_ctx: &WindowedContext, label: &str, accent: Color) -> impl ElementBuilder {
    let label_owned = label.to_string();
    let label_for_click = label.to_string();

    stateful::<ButtonState>()
        .initial(ButtonState::Idle)
        .on_state(move |ctx| {
            let state = ctx.state();
            let (bg, shadow, rounded, transform) = match state {
                ButtonState::Idle => (
                    accent,
                    Shadow::new(0.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.2)),
                    14.0,
                    Transform::default(),
                ),
                ButtonState::Hovered => {
                    let hover_color = Color::rgba(
                        (accent.r * 1.15).min(1.0),
                        (accent.g * 1.15).min(1.0),
                        (accent.b * 1.15).min(1.0),
                        accent.a,
                    );
                    (
                        hover_color,
                        Shadow::new(0.0, 8.0, 16.0, Color::rgba(0.0, 0.0, 0.0, 0.35)),
                        16.0,
                        Transform::scale(1.05, 1.05),
                    )
                }
                ButtonState::Pressed => {
                    let press_color =
                        Color::rgba(accent.r * 0.85, accent.g * 0.85, accent.b * 0.85, accent.a);
                    (
                        press_color,
                        Shadow::new(0.0, 1.0, 2.0, Color::rgba(0.0, 0.0, 0.0, 0.15)),
                        14.0,
                        Transform::scale(0.95, 0.95),
                    )
                }
                ButtonState::Disabled => (
                    Color::GRAY,
                    Shadow::new(0.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.2)),
                    14.0,
                    Transform::default(),
                ),
            };

            div()
                .w_fit()
                .p(4.0)
                .flex_col()
                .rounded(rounded)
                .items_center()
                .justify_center()
                .bg(bg)
                .shadow(shadow)
                .transform(transform)
                .child(
                    text(&label_owned)
                        .text_center()
                        .size(24.0)
                        .color(Color::WHITE)
                        .v_center(),
                )
        })
        .on_click(move |_| tracing::info!("'{}' clicked!", label_for_click))
}

/// Build the image showcase card with hover effect using Stateful<ButtonState>
fn build_image_showcase(_ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .glass()
        .shadow_xl()
        .rounded(40.0)
        .p(16.0)
        .flex_row()
        .items_center()
        .gap(16.0)
        // Image container with hover effect using stateful
        .child(
            stateful::<ButtonState>()
                .initial(ButtonState::Idle)
                .on_state(|ctx| {
                    let state = ctx.state();
                    let (shadow, transform) = match state {
                        ButtonState::Idle | ButtonState::Disabled => (
                            Shadow::new(0.0, 4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.2)),
                            Transform::default(),
                        ),
                        ButtonState::Hovered | ButtonState::Pressed => (
                            Shadow::new(0.0, 12.0, 24.0, Color::rgba(0.4, 0.6, 1.0, 0.5)),
                            Transform::scale(1.03, 1.03),
                        ),
                    };

                    div()
                        .shadow(shadow)
                        .transform(transform)
                        .child(
                            img("crates/blinc_app/examples/assets/original-c4197a5bf25a4356aa2bac6f82073eb2.webp")
                                .w(120.0 * 4.0)
                                .h(80.0 * 4.0)
                                .cover()
                                .rounded(12.0)
                        )
                }),
        )
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(text("Image Support").bold().size(40.0).color(Color::WHITE))
                .child(text("CSS-style object-fit").size(30.0).color(Color::rgba(1.0, 1.0, 1.0, 0.7)))
                .child(text("Hover for glow effect!").size(24.0).color(Color::rgba(0.4, 0.8, 1.0, 0.9)))
                .child(text("Art By JASIM: https://dribbble.com/jasimillustration").size(24.0).color(Color::rgba(1.0, 1.0, 1.0, 0.7)))
        )
}
