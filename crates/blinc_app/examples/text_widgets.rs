//! Text Input Widgets Example
//!
//! Demonstrates ready-to-use text input and text area elements using the layout API.
//!
//! Run with: cargo run -p blinc_app --example text_widgets --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::Color;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Text Input Demo".to_string(),
        width: 900,
        height: 700,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create text input states that persist across rebuilds
    // use_state_keyed returns State<T> which wraps the value; we store SharedTextInputState directly
    let username_state = ctx.use_state_keyed("username", || {
        text_input_state_with_placeholder("Enter username")
    });
    let email_state = ctx.use_state_keyed("email", || {
        text_input_state_with_placeholder("you@example.com")
    });
    let password_state = ctx.use_state_keyed("password", || {
        let mut s = TextInputState::with_placeholder("Enter password");
        s.masked = true;
        std::sync::Arc::new(std::sync::Mutex::new(s))
    });
    let message_state = ctx.use_state_keyed("message", || {
        text_area_state_with_placeholder("Write your message here...")
    });

    scroll()
        .w(ctx.width)
        .h(ctx.height)
        .direction(ScrollDirection::Vertical)
        .child(
            div()
                .w_full()
                .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
                .flex_col()
                .p(10.0)
                .gap(10.0)
                .overflow_clip()
                .justify_center()
                .items_center()
                // Title
                .child(
                    h1("Text Input Elements")
                        .size(48.0)
                        .text_center()
                        .weight(FontWeight::Bold)
                        .color(Color::WHITE),
                )
                // Subtitle
                .child(
                    h2("Ready-to-use text_input() and text_area() from blinc_layout")
                        .text_center()
                        .size(20.0)
                        .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
                )
                // Content row
                .child(
                    div()
                        .flex_row()
                        .w_full()
                        .justify_center()
                        .gap(20.0)
                        .flex_grow()
                        // Left column: Text input examples
                        .child(build_input_section(
                            &username_state.get(),
                            &email_state.get(),
                            &password_state.get(),
                        ))
                        // Right column: Form with text area
                        .child(build_form_section(ctx, &message_state.get())),
                ),
        )
}

/// Build the text input examples section
fn build_input_section(
    username: &SharedTextInputState,
    email: &SharedTextInputState,
    password: &SharedTextInputState,
) -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(4.0)
        .w_fit()
        // Section header
        .child(
            text("TextInput Element")
                .size(28.0)
                .weight(FontWeight::SemiBold)
                .color(Color::rgba(0.4, 0.8, 1.0, 1.0)),
        )
        // Description
        .child(
            text("text_input() provides a ready-to-use styled input.")
                .size(16.0)
                .color(Color::rgba(0.6, 0.6, 0.6, 1.0)),
        )
        // Username field
        .child(build_labeled_input("Username", username))
        // Email field
        .child(build_labeled_input("Email", email))
        // Password field
        .child(build_labeled_input("Password", password))
        // Show current values
        .child(build_values_display(username, email))
}

/// Build a labeled text input
fn build_labeled_input(label: &str, state: &SharedTextInputState) -> impl ElementBuilder {
    div()
        .w_full()
        .flex_col()
        .gap(1.0)
        .child(
            text(label)
                .size(16.0)
                .weight(FontWeight::Medium)
                .color(Color::rgba(0.8, 0.8, 0.8, 1.0)),
        )
        .child(text_input(state).text_size(12.0))
}

/// Display current input values
fn build_values_display(
    username: &SharedTextInputState,
    email: &SharedTextInputState,
) -> impl ElementBuilder {
    let username_val = username.lock().unwrap().value.clone();
    let email_val = email.lock().unwrap().value.clone();

    div()
        .flex_col()
        .w_full()
        .mt(10.0)
        .gap(8.0)
        .p(4.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 0.8))
        .rounded(12.0)
        .child(
            text("Current Values")
                .size(14.0)
                .weight(FontWeight::Medium)
                .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
        )
        .child(
            text(&format!(
                "Username: {}",
                if username_val.is_empty() {
                    "(empty)"
                } else {
                    &username_val
                }
            ))
            .size(13.0)
            .color(Color::rgba(0.5, 0.8, 0.5, 1.0)),
        )
        .child(
            text(&format!(
                "Email: {}",
                if email_val.is_empty() {
                    "(empty)"
                } else {
                    &email_val
                }
            ))
            .size(13.0)
            .color(Color::rgba(0.5, 0.8, 0.5, 1.0)),
        )
}

/// Build the form section with text area
fn build_form_section(ctx: &WindowedContext, message: &SharedTextAreaState) -> impl ElementBuilder {
    let button_state = ctx.use_state_for("submit_button", ButtonState::Idle);

    div()
        .flex_col()
        .gap(16.0)
        .w_fit()
        // Section header
        .child(
            h3("TextArea Element")
                .weight(FontWeight::SemiBold)
                .color(Color::rgba(0.4, 1.0, 0.8, 1.0)),
        )
        // Form card
        .child(
            div()
                .glass() // Disabled for memory testing
                // .bg(Color::rgba(0.2, 0.2, 0.25, 0.9))
                .shadow_lg()
                .rounded(16.0)
                .p(16.0)
                .flex_col()
                .gap(10.0)
                .justify_center()
                // Form title
                .child(
                    h4("Contact Form")
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                // Message field
                .child(
                    div()
                        .flex_col()
                        .gap(1.0)
                        .child(
                            span("Message")
                                .weight(FontWeight::Medium)
                                .color(Color::rgba(0.9, 0.9, 0.9, 1.0)),
                        )
                        .child(text_area(message).w(352.0).rows(4)),
                )
                // Submit button using stateful API
                .child(
                    stateful(button_state)
                        .w_full()
                        .h(44.0)
                        .rounded(8.0)
                        .items_center()
                        .justify_center()
                        .on_state(|state, div| {
                            let (bg, scale) = match state {
                                ButtonState::Idle => (Color::rgba(0.3, 0.6, 1.0, 1.0), 1.0),
                                ButtonState::Hovered => (Color::rgba(0.4, 0.7, 1.0, 1.0), 1.0),
                                ButtonState::Pressed => (Color::rgba(0.25, 0.5, 0.9, 1.0), 0.97),
                                ButtonState::Disabled => (Color::rgba(0.3, 0.3, 0.35, 0.5), 1.0),
                            };

                            // Use setter methods to update visual properties
                            // This preserves layout properties (items_center, justify_center, etc.)
                            div.set_bg(bg);
                            div.set_rounded(8.0);
                            div.set_shadow(Shadow::new(
                                0.0,
                                4.0,
                                12.0,
                                Color::rgba(0.3, 0.5, 1.0, 0.3),
                            ));
                            div.set_transform(Transform::scale(scale, scale));
                        })
                        .on_click(|_| {
                            tracing::info!("Form submitted!");
                        })
                        .child(label("Submit").color(Color::WHITE).v_center()),
                ),
        )
        // Show message preview
        .child(build_message_preview(message))
}

/// Display message preview
fn build_message_preview(message: &SharedTextAreaState) -> impl ElementBuilder {
    let msg_val = message.lock().unwrap().value();

    div()
        .w_full()
        .flex_col()
        .gap(8.0)
        .p(16.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 0.8))
        .rounded(12.0)
        .child(
            text("Message Preview")
                .size(14.0)
                .weight(FontWeight::Medium)
                .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
        )
        .child(
            text(&if msg_val.is_empty() {
                "(empty)".to_string()
            } else {
                msg_val
            })
            .size(13.0)
            .color(Color::rgba(0.5, 0.8, 0.5, 1.0)),
        )
}
