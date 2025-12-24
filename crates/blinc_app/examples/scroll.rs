//! Scroll Container Example
//!
//! This example demonstrates the scroll widget with webkit-style
//! bounce physics, glass clipping, and scroll event handling.
//!
//! Features demonstrated:
//! - `scroll()` container with bounce physics
//! - Glass elements clipping properly inside scroll
//! - Scroll event handling with delta reporting
//! - Spring animation for edge bounce
//!
//! Run with: cargo run -p blinc_app --example scroll --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};

fn main() -> Result<()> {
    // Initialize tracing for logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Configure the window
    let config = WindowConfig {
        title: "Blinc Scroll Example".to_string(),
        width: 800,
        height: 600,
        resizable: true,
        ..Default::default()
    };

    // Run the windowed application
    WindowedApp::run(config, |ctx| build_ui(ctx))
}

/// Build the main UI with a scroll container
fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
        .flex_col()
        .items_center()
        .p(20.0)
        .gap(20.0)
        // Title
        .child(
            text("Scroll Container Demo")
                .size(48.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        // Instructions
        .child(
            text("Scroll with mouse wheel or trackpad - bounce physics at edges!")
                .size(20.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.7)),
        )
        // Scroll container
        .child(build_scroll_container(ctx))
}

/// Build the scroll container with content
fn build_scroll_container(ctx: &WindowedContext) -> impl ElementBuilder {
    // Calculate scroll viewport size
    let viewport_width = ctx.width - 80.0;
    let viewport_height = ctx.height - 200.0;

    // The scroll container with default bounce physics
    scroll()
        .w(viewport_width)
        .h(viewport_height)
        .rounded(24.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .on_scroll(|e| {
            tracing::info!(
                "Scroll delta: ({:.1}, {:.1})",
                e.scroll_delta_x,
                e.scroll_delta_y
            );
        })
        // Scrollable content
        .child(build_scroll_content())
}

/// Build the scrollable content (cards list)
fn build_scroll_content() -> impl ElementBuilder {
    div()
        .w_full()
        .flex_col()
        .p(20.0)
        .gap(16.0)
        // Add many cards to create scrollable content
        .child(content_card("Glass Cards", "These glass cards demonstrate that blur effects clip properly inside the scroll container.", Color::rgba(0.4, 0.6, 1.0, 0.3)))
        .child(content_card("Bounce Physics", "Scroll past the edges to see webkit-style spring bounce animation bring it back.", Color::rgba(1.0, 0.4, 0.6, 0.3)))
        .child(content_card("Momentum Scrolling", "Release while scrolling to see momentum-based deceleration.", Color::rgba(0.4, 1.0, 0.6, 0.3)))
        .child(simple_card("Card 4", "More content to scroll through..."))
        .child(simple_card("Card 5", "Keep scrolling!"))
        .child(content_card("State Machine", "The scroll uses a FSM with states: Idle, Scrolling, Decelerating, Bouncing.", Color::rgba(0.8, 0.4, 1.0, 0.3)))
        .child(simple_card("Card 7", "Almost there..."))
        .child(simple_card("Card 8", "A bit more content."))
        .child(content_card("Configurable", "Bounce can be disabled, spring stiffness adjusted, and friction tuned.", Color::rgba(1.0, 0.8, 0.2, 0.3)))
        .child(simple_card("Card 10", "Getting close to the end!"))
        .child(simple_card("Card 11", "One more card..."))
        .child(content_card("End of Content", "You've reached the bottom! Scroll up or bounce back down.", Color::rgba(0.2, 0.8, 0.8, 0.3)))
}

/// Build a glass content card with title, description, and accent color
fn content_card(title: &str, description: &str, accent: Color) -> impl ElementBuilder {
    div()
        .w_full()
        .glass()
        .rounded(16.0)
        .p(20.0)
        .flex_col()
        .gap(8.0)
        // Accent bar at top
        .child(
            div()
                .w_full()
                .h(4.0)
                .bg(accent)
                .rounded(2.0),
        )
        // Title
        .child(
            text(title)
                .size(24.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        // Description
        .child(
            text(description)
                .size(16.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.8)),
        )
}

/// Build a simple card without glass effect
fn simple_card(title: &str, description: &str) -> impl ElementBuilder {
    div()
        .w_full()
        .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
        .rounded(12.0)
        .p(16.0)
        .flex_col()
        .gap(4.0)
        .child(
            text(title)
                .size(20.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
        .child(
            text(description)
                .size(14.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
        )
}
