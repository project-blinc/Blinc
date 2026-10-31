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
//! Run with: cargo run -p blinc_app_examples --example overlay_demo

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;

#[cfg(not(target_arch = "wasm32"))]
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

    blinc_app::windowed::WindowedApp::run(config, build_ui)
}

/// Button colours by state, as a stylesheet: the default look comes from the
/// theme, these classes restyle it.
const BUTTON_CSS: &str = r#"
.demo-blue, .demo-green, .demo-red, .demo-ghost, .demo-delete {
    transition: background-color 150ms ease;
}
.demo-blue { background: #4d80ff; }
.demo-blue:hover { background: #6699ff; }
.demo-blue:active { background: #4066e6; }
.demo-green { background: #4db366; }
.demo-green:hover { background: #66cc80; }
.demo-green:active { background: #40995a; }
.demo-red { background: #cc664d; }
.demo-red:hover { background: #e68066; }
.demo-red:active { background: #b3593f; }
.demo-ghost { background: rgba(255, 255, 255, 0.1); color: rgba(255, 255, 255, 0.7); }
.demo-ghost:hover { background: rgba(255, 255, 255, 0.2); }
.demo-delete { background: #e64d4d; }
.demo-delete:hover { background: #ff6666; }
"#;

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    ctx.add_css(BUTTON_CSS);
    let overlay_mgr = ctx.overlay_manager();

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
        // Button row - using real button() widgets
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .child(
                    button("Open Modal")
                        .key("modal_btn")
                        .class("demo-blue")
                        .rounded(8.0)
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
                    button("Show Toast")
                        .key("toast_btn")
                        .class("demo-green")
                        .rounded(8.0)
                        .on_click({
                            let mgr = overlay_mgr.clone();
                            move |_| {
                                tracing::info!("Showing toast...");
                                mgr.toast()
                                    .corner(Corner::TopRight)
                                    .duration_ms(3000)
                                    .content(toast_content)
                                    .show();
                            }
                        }),
                )
                .child(
                    button("Open Dialog")
                        .key("dialog_btn")
                        .class("demo-red")
                        .rounded(8.0)
                        .on_click({
                            let mgr = overlay_mgr.clone();
                            move |_| {
                                tracing::info!("Opening dialog...");
                                let mgr_for_content = mgr.clone();
                                mgr.dialog()
                                    .content(move || dialog_content(mgr_for_content.clone()))
                                    .show();
                            }
                        }),
                ),
        )
        // Instructions
        .child(
            div()
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

fn instruction(text_content: &str) -> impl ElementBuilder + use<> {
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

/// A button for overlay content. The overlay is built again each time it
/// opens, so the key is the label, not where it was made.
fn overlay_button(label: &str) -> blinc_layout::widgets::Button {
    button(label).key(format!("overlay_btn:{label}"))
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
                .child(overlay_button("Cancel").class("demo-ghost").on_click({
                    let mgr = mgr.clone();
                    move |_| mgr.close_top()
                }))
                .child(overlay_button("Confirm").class("demo-green").on_click({
                    let mgr = mgr.clone();
                    move |_| {
                        tracing::info!("Confirmed!");
                        mgr.close_top();
                    }
                })),
        )
}

fn toast_content() -> Div {
    div()
        .p(4.0)
        .rounded(16.0)
        .bg(Color::rgba(0.2, 0.2, 0.25, 0.95))
        .shadow_lg()
        .flex_row()
        .gap(8.0)
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
                .color(Color::WHITE)
                .v_center(),
        )
}

fn dialog_content(mgr: OverlayManager) -> Div {
    div()
        .w(400.0)
        .h_fit()
        .p_px(16.0)
        .rounded(12.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .shadow_xl()
        .flex_col()
        .gap_px(16.0)
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
                .w_full()
                .mt(8.0)
                .flex_row()
                .gap_px(4.0)
                .justify_between()
                .content_center()
                .child(overlay_button("Cancel").class("demo-ghost").on_click({
                    let mgr = mgr.clone();
                    move |_| mgr.close_top()
                }))
                .child(overlay_button("Delete").class("demo-delete").on_click({
                    let mgr = mgr.clone();
                    move |_| {
                        tracing::info!("Deleted!");
                        mgr.close_top();
                    }
                })),
        )
}
