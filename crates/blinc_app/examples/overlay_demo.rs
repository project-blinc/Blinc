//! Overlay System Demo
//!
//! This example demonstrates the overlay infrastructure for modals, dialogs,
//! context menus, and toast notifications.
//!
//! Features demonstrated:
//! - Modal dialogs with backdrop
//! - Toast notifications in corners
//! - Context menus at cursor position
//! - Overlay manager accessed via `ctx.overlay_manager()`
//!
//! Run with: cargo run -p blinc_app --example overlay_demo --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_layout::stateful::{ButtonState, SharedState};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc Overlay Demo".to_string(),
        width: 900,
        height: 700,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {
    let overlay_mgr = ctx.overlay_manager();

    // Create button states via context for persistence across rebuilds
    let modal_btn = ctx.use_state_for("modal_btn", ButtonState::Idle);
    let toast_btn = ctx.use_state_for("toast_btn", ButtonState::Idle);
    let dialog_btn = ctx.use_state_for("dialog_btn", ButtonState::Idle);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .flex_col()
        .items_center()
        .justify_center()
        .gap(10.0)
        // Title
        .child(
            text("Overlay System Demo")
                .size(48.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        .child(
            text("Click buttons to open different overlay types")
                .size(20.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
        )
        // Button row - using real button() widgets with context-managed state
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .child(
                    button(modal_btn, "Open Modal")
                        .rounded(8.0)
                        .bg_color(Color::rgba(0.3, 0.5, 1.0, 1.0))
                        .hover_color(Color::rgba(0.4, 0.6, 1.0, 1.0))
                        .pressed_color(Color::rgba(0.25, 0.4, 0.9, 1.0))
                        .on_click({
                            let mgr = overlay_mgr.clone();
                            move |_| {
                                tracing::info!("Opening modal...");
                                let mgr_for_content = mgr.clone();
                                mgr.modal()
                                    .content(move || modal_content(mgr_for_content.clone()))
                                    .show();
                            }
                        }),
                )
                .child(
                    button(toast_btn, "Show Toast")
                        .rounded(8.0)
                        .bg_color(Color::rgba(0.3, 0.7, 0.4, 1.0))
                        .hover_color(Color::rgba(0.4, 0.8, 0.5, 1.0))
                        .pressed_color(Color::rgba(0.25, 0.6, 0.35, 1.0))
                        .on_click({
                            let mgr = overlay_mgr.clone();
                            move |_| {
                                tracing::info!("Showing toast...");
                                mgr.toast()
                                    .corner(Corner::TopRight)
                                    .duration_ms(3000)
                                    .content(|| toast_content())
                                    .show();
                            }
                        }),
                )
                .child(
                    button(dialog_btn, "Open Dialog")
                        .rounded(8.0)
                        .bg_color(Color::rgba(0.8, 0.4, 0.3, 1.0))
                        .hover_color(Color::rgba(0.9, 0.5, 0.4, 1.0))
                        .pressed_color(Color::rgba(0.7, 0.35, 0.25, 1.0))
                        .on_click({
                            let mgr = overlay_mgr.clone();
                            move |_| {
                                tracing::info!("Opening dialog...");
                                let mgr_for_content = mgr.clone();
                                mgr.dialog()
                                    .size(350.0, 180.0)
                                    .content(move || dialog_content(mgr_for_content.clone()))
                                    .show();
                            }
                        }),
                ),
        )
        // Instructions
        .child(
            div()
                .mt(20.0)
                .p(20.0)
                .rounded(12.0)
                .bg(Color::rgba(1.0, 1.0, 1.0, 0.05))
                .flex_col()
                .gap(8.0)
                .child(
                    text("Instructions:")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(instruction(
                    "Click 'Open Modal' to show a centered modal with backdrop",
                ))
                .child(instruction(
                    "Click 'Show Toast' to display a notification (auto-dismisses after 3s)",
                ))
                .child(instruction(
                    "Click 'Open Dialog' to show a confirmation dialog",
                ))
                .child(instruction(
                    "Press Escape or click backdrop to dismiss modals",
                )),
        )
}

fn instruction(text_content: &str) -> impl ElementBuilder {
    div()
        .flex_row()
        .gap(8.0)
        .child(
            text("\u{2022}")
                .size(16.0)
                .color(Color::rgba(0.4, 0.8, 1.0, 0.8)),
        )
        .child(
            text(text_content)
                .size(16.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.7)),
        )
}

/// Create an ephemeral button for overlay content (no context persistence needed)
fn overlay_button(label: &str) -> blinc_layout::widgets::Button {
    use blinc_layout::stateful::StatefulInner;
    use std::sync::{Arc, Mutex};

    let state: SharedState<ButtonState> =
        Arc::new(Mutex::new(StatefulInner::new(ButtonState::Idle)));
    button(state, label)
}

fn modal_content(mgr: OverlayManager) -> Div {
    div()
        .w_fit()
        .h_fit()
        .p(10.0)
        .rounded(16.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .shadow_xl()
        .flex_col()
        .gap(10.0)
        .justify_center()
        .child(
            text("Modal Dialog")
                .size(28.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        .child(
            text("This is a modal dialog. Click the backdrop or press Escape to dismiss.")
                .size(16.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.7)),
        )
        .child(
            div()
                .w_fit()
                .flex_row()
                .gap(5.0)
                .justify_end()
                .child(
                    overlay_button("Cancel")
                        .bg_color(Color::rgba(1.0, 1.0, 1.0, 0.1))
                        .hover_color(Color::rgba(1.0, 1.0, 1.0, 0.2))
                        .text_color(Color::rgba(1.0, 1.0, 1.0, 0.7))
                        .on_click({
                            let mgr = mgr.clone();
                            move |_| mgr.close_top()
                        }),
                )
                .child(
                    overlay_button("Confirm")
                        .bg_color(Color::rgba(0.3, 0.7, 0.4, 1.0))
                        .hover_color(Color::rgba(0.4, 0.8, 0.5, 1.0))
                        .on_click({
                            let mgr = mgr.clone();
                            move |_| {
                                tracing::info!("Confirmed!");
                                mgr.close_top();
                            }
                        }),
                ),
        )
}

fn toast_content() -> Div {
    div()
        .px(20.0)
        .py(12.0)
        .rounded(8.0)
        .bg(Color::rgba(0.2, 0.2, 0.25, 0.95))
        .shadow_lg()
        .flex_row()
        .gap(12.0)
        .items_center()
        .child(
            div()
                .w(24.0)
                .h(24.0)
                .rounded(12.0)
                .bg(Color::rgba(0.3, 0.8, 0.5, 1.0))
                .items_center()
                .justify_center()
                .child(text("\u{2713}").size(14.0).color(Color::WHITE)),
        )
        .child(
            text("Action completed successfully!")
                .size(14.0)
                .color(Color::WHITE),
        )
}

fn dialog_content(mgr: OverlayManager) -> Div {
    div()
        .w(350.0)
        .p(28.0)
        .rounded(12.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .shadow_xl()
        .flex_col()
        .gap(16.0)
        .child(
            text("Confirm Action")
                .size(24.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        .child(
            text("Are you sure you want to proceed? Click backdrop or Escape to cancel.")
                .size(15.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.7)),
        )
        .child(
            div()
                .mt(8.0)
                .flex_row()
                .gap(12.0)
                .justify_end()
                .child(
                    overlay_button("Cancel")
                        .bg_color(Color::rgba(1.0, 1.0, 1.0, 0.1))
                        .hover_color(Color::rgba(1.0, 1.0, 1.0, 0.2))
                        .text_color(Color::rgba(1.0, 1.0, 1.0, 0.7))
                        .on_click({
                            let mgr = mgr.clone();
                            move |_| mgr.close_top()
                        }),
                )
                .child(
                    overlay_button("Delete")
                        .bg_color(Color::rgba(0.9, 0.3, 0.3, 1.0))
                        .hover_color(Color::rgba(1.0, 0.4, 0.4, 1.0))
                        .on_click({
                            let mgr = mgr.clone();
                            move |_| {
                                tracing::info!("Deleted!");
                                mgr.close_top();
                            }
                        }),
                ),
        )
}
