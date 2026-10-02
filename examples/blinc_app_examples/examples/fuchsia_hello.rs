//! Fuchsia Hello World Example
//!
//! no-web: this example targets Fuchsia OS specifically and uses the
//! `fuchsia` runner feature. It is not cross-platform in the same
//! sense as the other examples. (The `no-web:` marker tells
//! `tools/build-web-examples` to skip this file during discovery.)
//!
//! A simple Blinc application for Fuchsia OS demonstrating
//! basic UI rendering with touch interactions.
//!
//! # Building
//!
//! ```bash
//! # Add Fuchsia target
//! rustup target add x86_64-unknown-fuchsia
//!
//! # Build for Fuchsia
//! cargo build --example fuchsia_hello --target x86_64-unknown-fuchsia --features fuchsia
//! ```
//!
//! # Running in Fuchsia Emulator
//!
//! The Fuchsia platform lives under `extensions/archived/`.

#[cfg(target_os = "fuchsia")]
use blinc_app::fuchsia::FuchsiaApp;
#[cfg(target_os = "fuchsia")]
use blinc_app::prelude::*;

#[cfg(target_os = "fuchsia")]
fn main() {
    FuchsiaApp::run(|ctx| {
        // Counter state
        let count = ctx.state("count", 0i32);

        // Button press handler
        let on_increment = ctx.callback({
            let count = count.clone();
            move |_| {
                count.update(|n| *n + 1);
            }
        });

        let on_decrement = ctx.callback({
            let count = count.clone();
            move |_| {
                count.update(|n| *n - 1);
            }
        });

        // Main UI
        div()
            .w(ctx.width)
            .h(ctx.height)
            .bg([0.1, 0.1, 0.15, 1.0])
            .flex_col()
            .items_center()
            .justify_center()
            .gap(24.0)
            .child(
                // Title
                text("Hello Fuchsia!")
                    .size(48.0)
                    .color([0.9, 0.9, 0.95, 1.0]),
            )
            .child(
                // Counter display
                div()
                    .bg([0.2, 0.2, 0.25, 1.0])
                    .rounded(16.0)
                    .px(32.0)
                    .py(16.0)
                    .child(
                        text(format!("Count: {}", count.get()))
                            .size(32.0)
                            .color([0.95, 0.95, 1.0, 1.0]),
                    ),
            )
            .child(
                // Button row
                div()
                    .flex_row()
                    .gap(16.0)
                    .child(
                        // Decrement button
                        div()
                            .bg([0.8, 0.3, 0.3, 1.0])
                            .rounded(12.0)
                            .px(24.0)
                            .py(12.0)
                            .on_click(on_decrement)
                            .child(text("-").size(28.0).color([1.0, 1.0, 1.0, 1.0])),
                    )
                    .child(
                        // Increment button
                        div()
                            .bg([0.3, 0.7, 0.4, 1.0])
                            .rounded(12.0)
                            .px(24.0)
                            .py(12.0)
                            .on_click(on_increment)
                            .child(text("+").size(28.0).color([1.0, 1.0, 1.0, 1.0])),
                    ),
            )
            .child(
                // Platform info
                text("Running on Fuchsia OS")
                    .size(14.0)
                    .color([0.5, 0.5, 0.6, 1.0]),
            )
    })
    .expect("Failed to run Fuchsia app");
}

#[cfg(not(target_os = "fuchsia"))]
fn main() {
    eprintln!("This example can only run on Fuchsia OS.");
    eprintln!(
        "Build with: cargo build --example fuchsia_hello --target x86_64-unknown-fuchsia --features fuchsia"
    );
    std::process::exit(1);
}
