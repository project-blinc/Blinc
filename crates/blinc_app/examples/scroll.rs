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
//! - Toggle between vertical and horizontal scroll directions
//! - Using reactive state system (`ctx.use_state`) for state persistence
//!
//! Run with: cargo run -p blinc_app --example scroll --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_layout::prelude::{Scroll, ScrollPhysics, SharedScrollPhysics};
use std::sync::{Arc, Mutex};

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

    // Run the windowed application - state is managed via ctx.use_state
    WindowedApp::run(config, |ctx| build_ui(ctx))
}

/// Build the main UI with a scroll container
fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Use reactive state for direction - persists across rebuilds, keyed by string
    let direction_state = ctx.use_state_keyed("scroll_direction", || ScrollDirection::Vertical);
    let current_direction = direction_state.get();

    // Use reactive state for physics - persists across rebuilds
    let physics_state = ctx.use_state_keyed("scroll_physics", || {
        Arc::new(Mutex::new(ScrollPhysics::default())) as SharedScrollPhysics
    });
    let physics = physics_state.get();

    // Ensure physics direction matches current direction
    // (handle direction changes)
    {
        let mut p = physics.lock().unwrap();
        if p.config.direction != current_direction {
            p.set_direction(current_direction);
        }
    }

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
        // Direction toggle button
        .child(build_direction_toggle(ctx, current_direction))
        // Scroll container with shared physics
        .child(build_scroll_container(ctx, current_direction, physics))
}

/// Build the direction toggle button
fn build_direction_toggle(ctx: &WindowedContext, current: ScrollDirection) -> impl ElementBuilder {
    // Get the direction state to update it on click
    let direction_state = ctx.use_state_keyed("scroll_direction", || ScrollDirection::Vertical);

    let label = match current {
        ScrollDirection::Vertical => "Vertical",
        ScrollDirection::Horizontal => "Horizontal",
        ScrollDirection::Both => "Both",
    };

    div()
        .flex_row()
        .gap(12.0)
        .items_center()
        .child(
            text("Direction:")
                .size(18.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.8)),
        )
        .child(
            div()
                .px(20.0)
                .py(10.0)
                .rounded(12.0)
                .bg(Color::rgba(0.3, 0.5, 1.0, 0.8))
                .on_click(move |_| {
                    let current = direction_state.get();
                    let next = match current {
                        ScrollDirection::Vertical => ScrollDirection::Horizontal,
                        ScrollDirection::Horizontal => ScrollDirection::Both,
                        ScrollDirection::Both => ScrollDirection::Vertical,
                    };
                    direction_state.set(next);
                    tracing::info!("Switched to {:?} scroll", next);
                })
                .child(
                    text(label)
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                ),
        )
}

/// Build the scroll container with content
fn build_scroll_container(
    ctx: &WindowedContext,
    direction: ScrollDirection,
    physics: SharedScrollPhysics,
) -> impl ElementBuilder {
    // Calculate scroll viewport size
    let viewport_width = ctx.width - 80.0;
    let viewport_height = ctx.height - 260.0;

    // Update viewport dimensions in physics
    {
        let mut p = physics.lock().unwrap();
        p.viewport_width = viewport_width;
        p.viewport_height = viewport_height;
    }

    // The scroll container with shared physics for state persistence
    Scroll::with_physics(physics)
        .w(viewport_width)
        .h(viewport_height)
        .rounded(24.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .direction(direction)
        .on_scroll(|e| {
            tracing::info!(
                "Scroll delta: ({:.1}, {:.1})",
                e.scroll_delta_x,
                e.scroll_delta_y
            );
        })
        // Scrollable content - layout differs based on direction
        .child(build_scroll_content(direction))
}

/// Build the scrollable content (cards list)
fn build_scroll_content(direction: ScrollDirection) -> impl ElementBuilder {
    let is_horizontal = matches!(direction, ScrollDirection::Horizontal);

    let container = div().p(20.0).gap(16.0);

    let container = if is_horizontal {
        container.flex_row().h_full()
    } else {
        container.w_full().flex_col()
    };

    // Add many cards to create scrollable content
    container
        .child(content_card(
            "Glass Cards",
            "These glass cards demonstrate that blur effects clip properly inside the scroll container.",
            Color::rgba(0.4, 0.6, 1.0, 0.3),
            is_horizontal,
        ))
        .child(content_card(
            "Bounce Physics",
            "Scroll past the edges to see webkit-style spring bounce animation bring it back.",
            Color::rgba(1.0, 0.4, 0.6, 0.3),
            is_horizontal,
        ))
        .child(content_card(
            "Momentum Scrolling",
            "Release while scrolling to see momentum-based deceleration.",
            Color::rgba(0.4, 1.0, 0.6, 0.3),
            is_horizontal,
        ))
        .child(simple_card("Card 4", "More content to scroll through...", is_horizontal))
        .child(simple_card("Card 5", "Keep scrolling!", is_horizontal))
        .child(content_card(
            "State Machine",
            "The scroll uses a FSM with states: Idle, Scrolling, Decelerating, Bouncing.",
            Color::rgba(0.8, 0.4, 1.0, 0.3),
            is_horizontal,
        ))
        .child(simple_card("Card 7", "Almost there...", is_horizontal))
        .child(simple_card("Card 8", "A bit more content.", is_horizontal))
        .child(content_card(
            "Configurable",
            "Bounce can be disabled, spring stiffness adjusted, and friction tuned.",
            Color::rgba(1.0, 0.8, 0.2, 0.3),
            is_horizontal,
        ))
        .child(simple_card("Card 10", "Getting close to the end!", is_horizontal))
        .child(simple_card("Card 11", "One more card...", is_horizontal))
        .child(content_card(
            "End of Content",
            "You've reached the end! Scroll back or bounce.",
            Color::rgba(0.2, 0.8, 0.8, 0.3),
            is_horizontal,
        ))
}

/// Build a glass content card with title, description, and accent color
fn content_card(
    title: &str,
    description: &str,
    accent: Color,
    is_horizontal: bool,
) -> impl ElementBuilder {
    let card = div().glass().rounded(16.0).p(20.0).flex_col().gap(8.0);

    let card = if is_horizontal {
        // Fixed width, no shrinking, full height in horizontal mode
        card.w(280.0).h_full().flex_shrink_0()
    } else {
        card.w_full()
    };

    card
        // Accent bar at top
        .child(div().w_full().h(4.0).bg(accent).rounded(2.0))
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
fn simple_card(title: &str, description: &str, is_horizontal: bool) -> impl ElementBuilder {
    let card = div()
        .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
        .rounded(12.0)
        .p(16.0)
        .flex_col()
        .gap(4.0);

    let card = if is_horizontal {
        // Fixed width, no shrinking, full height in horizontal mode
        card.w(200.0).h_full().flex_shrink_0()
    } else {
        card.w_full()
    };

    card.child(
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
