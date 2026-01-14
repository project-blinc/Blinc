//! Example
//!
//! A Blinc UI application with desktop, Android, and iOS support.

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::reactive::State;

/// Counter button with stateful hover/press states
fn counter_button(label: &str, count: State<i32>, delta: i32) -> impl ElementBuilder {
    let label = label.to_string();

    let count = count.clone();
    stateful::<ButtonState>()
        .on_state(move |ctx| {
            let bg = match ctx.state() {
                ButtonState::Idle => Color::rgba(0.3, 0.3, 0.4, 1.0),
                ButtonState::Hovered => Color::rgba(0.4, 0.4, 0.5, 1.0),
                ButtonState::Pressed => Color::rgba(0.2, 0.2, 0.3, 1.0),
                ButtonState::Disabled => Color::rgba(0.2, 0.2, 0.2, 0.5),
            };

            div()
                .w(80.0)
                .h(50.0)
                .rounded(8.0)
                .bg(bg)
                .items_center()
                .justify_center()
                .cursor(CursorStyle::Pointer)
                .child(text(&label).size(24.0).color(Color::WHITE))
        })
        .on_click(move |_| {
            // Use set_rebuild to trigger a full UI rebuild when state changes
            // This ensures iOS re-renders (incremental updates require Stateful pattern)
            count.set_rebuild(count.get() + delta);
        })
}

/// Counter display that reacts to count changes
fn counter_display(count: State<i32>) -> impl ElementBuilder {
    stateful::<NoState>()
        .deps([count.signal_id()])
        .on_state(move |_ctx| {
            div().child(
                text(format!("Count: {}", count.get()))
                    .size(48.0)
                    .color(Color::rgba(0.4, 0.8, 1.0, 1.0)),
            )
        })
}

/// Main application UI
fn app_ui(ctx: &mut WindowedContext) -> impl ElementBuilder {
    let count = ctx.use_state_keyed("count", || 0i32);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
        .flex_col()
        .items_center()
        .justify_center()
        .gap(20.0)
        .child(text("Blinc Mobile Example").size(32.0).color(Color::WHITE).no_wrap())
        .child(counter_display(count.clone()))
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .child(counter_button("-", count.clone(), -1))
                .child(counter_button("+", count.clone(), 1)),
        )
}

// =============================================================================
// Desktop Entry Point
// =============================================================================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc Mobile Example".to_string(),
        width: 400,
        height: 600,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| app_ui(ctx))
}

// =============================================================================
// Android Entry Point
// =============================================================================

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    use android_logger::Config;
    use log::LevelFilter;

    android_logger::init_once(
        Config::default()
            .with_max_level(LevelFilter::Info)
            .with_tag("example"),
    );

    log::info!("Starting example on Android");

    blinc_app::AndroidApp::run(app, |ctx| app_ui(ctx)).expect("Failed to run Android app");
}

#[cfg(target_os = "android")]
fn main() {}

// =============================================================================
// iOS Entry Point
// =============================================================================

#[cfg(target_os = "ios")]
fn main() {}

/// iOS initialization function - called from Swift during app launch
///
/// This registers the Rust UI builder so that each frame can build the UI.
/// Must be called before blinc_create_context.
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn ios_app_init() {
    // Initialize tracing to stderr (will show in Xcode console)
    use std::io::Write;
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::stderr)
        .try_init();

    eprintln!("[Blinc] ios_app_init called - registering UI builder");

    // Register our UI builder
    blinc_app::ios::register_rust_ui_builder(|ctx| app_ui(ctx));

    eprintln!("[Blinc] UI builder registered");
}
