//! Stateful API Demo
//!
//! This example demonstrates the new stateful::<S>() API with:
//! - `ctx.event()` - Access triggering event in state callbacks
//! - `ctx.use_signal()` - Scoped signals for local state
//! - `ctx.use_animated_value()` - Spring-animated values
//!
//! Run with: cargo run -p blinc_app --example stateful_demo --features windowed

use blinc_animation::SpringConfig;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::events::event_types;
use blinc_core::Transform;
use blinc_layout::stateful::ButtonState;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Stateful API Demo".to_string(),
        width: 800,
        height: 600,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .flex_col()
        .items_center()
        .justify_center()
        .gap(4.0)
        // Title
        .child(
            text("Stateful API Demo")
                .size(48.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        // Description
        .child(
            text("Click the button to increment. Watch the spring animation!")
                .size(20.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.7)),
        )
        // Counter button using new stateful API
        .child(counter_button())
        // Event info display
        .child(event_info_display())
}

/// A counter button demonstrating ctx.use_signal() and ctx.use_spring()
fn counter_button() -> impl ElementBuilder {
    stateful::<ButtonState>()
        .on_state(|ctx| {
            // Scoped signal - persists across rebuilds, keyed to this stateful
            // Automatically registered as dependency - on_state re-runs when count changes
            let count = ctx.use_signal("count", || 0i32);

            // Declarative spring animation - specify target, get current value
            let target_scale = match ctx.state() {
                ButtonState::Idle => 1.0,
                ButtonState::Hovered => 1.08,
                ButtonState::Pressed => 0.95,
                ButtonState::Disabled => 1.0,
            };
            let current_scale = ctx.use_spring("scale", target_scale, SpringConfig::snappy());

            // Handle click via ctx.event()
            if let Some(event) = ctx.event() {
                if event.event_type == event_types::POINTER_UP {
                    count.update(|n| n + 1);
                    tracing::info!("Counter incremented to {}", count.get());
                }
            }

            // Background color based on state
            let bg = match ctx.state() {
                ButtonState::Idle => Color::rgba(0.3, 0.5, 0.9, 1.0),
                ButtonState::Hovered => Color::rgba(0.4, 0.6, 1.0, 1.0),
                ButtonState::Pressed => Color::rgba(0.25, 0.4, 0.8, 1.0),
                ButtonState::Disabled => Color::GRAY,
            };

            div()
                .w(200.0)
                .h(80.0)
                .bg(bg)
                .rounded(16.0)
                .flex_col()
                .items_center()
                .justify_center()
                .gap(4.0)
                .cursor_pointer()
                .transform(Transform::scale(current_scale, current_scale))
                .child(
                    text(&format!("{}", count.get()))
                        .size(36.0)
                        .weight(FontWeight::Bold)
                        .color(Color::WHITE).pointer_events_none(),
                )
                .child(
                    text("Click me!")
                        .size(14.0)
                        .color(Color::rgba(1.0, 1.0, 1.0, 0.8)).pointer_events_none(),
                )
        })
}

/// Display showing event information via ctx.event()
fn event_info_display() -> impl ElementBuilder {
    stateful::<ButtonState>()
        .on_state(|ctx| {
            // Track last event info using scoped signal
            // Automatically registered as dependency - on_state re-runs when it changes
            let last_event = ctx.use_signal("last_event", || "None".to_string());

            // Update event info when we receive an event
            if let Some(event) = ctx.event() {
                let event_name = match event.event_type {
                    event_types::POINTER_ENTER => "POINTER_ENTER",
                    event_types::POINTER_LEAVE => "POINTER_LEAVE",
                    event_types::POINTER_DOWN => "POINTER_DOWN",
                    event_types::POINTER_UP => "POINTER_UP",
                    event_types::POINTER_MOVE => "POINTER_MOVE",
                    _ => "Unknown",
                };
                last_event.set(format!(
                    "{} at ({:.0}, {:.0})",
                    event_name, event.local_x, event.local_y
                ));
            }

            let state_name = match ctx.state() {
                ButtonState::Idle => "Idle",
                ButtonState::Hovered => "Hovered",
                ButtonState::Pressed => "Pressed",
                ButtonState::Disabled => "Disabled",
            };

            let bg = match ctx.state() {
                ButtonState::Idle => Color::rgba(0.15, 0.15, 0.2, 1.0),
                ButtonState::Hovered => Color::rgba(0.2, 0.2, 0.28, 1.0),
                ButtonState::Pressed => Color::rgba(0.12, 0.12, 0.16, 1.0),
                ButtonState::Disabled => Color::rgba(0.1, 0.1, 0.12, 0.5),
            };

            div()
                .w(400.0)
                .p(20.0)
                .bg(bg)
                .rounded(12.0)
                .flex_col()
                .gap(12.0)
                .cursor_pointer()
                // State row
                .child(
                    div()
                        .flex_row()
                        .justify_between()
                        .child(
                            text("State:")
                                .size(16.0)
                                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
                        )
                        .child(
                            text(state_name)
                                .size(16.0)
                                .weight(FontWeight::SemiBold)
                                .color(Color::WHITE),
                        ),
                )
                // Last event row
                .child(
                    div()
                        .flex_row()
                        .justify_between()
                        .child(
                            text("Last Event:")
                                .size(16.0)
                                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
                        )
                        .child(
                            text(&last_event.get())
                                .size(16.0)
                                .weight(FontWeight::SemiBold)
                                .color(Color::rgba(0.4, 0.8, 1.0, 1.0)),
                        ),
                )
                // Instructions
                .child(
                    text("Hover over this panel to see events")
                        .size(14.0)
                        .color(Color::rgba(1.0, 1.0, 1.0, 0.4))
                        .text_center(),
                )
        })
}
