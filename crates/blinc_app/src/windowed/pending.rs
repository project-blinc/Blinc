//! Requests to open additional windows, queued until the event loop drains them.

use crate::windowed::{WindowBuilder, WindowedContext};
use blinc_layout::Div;
use blinc_layout::prelude::*;
use blinc_platform::WindowConfig;
use std::sync::{Arc, Mutex};

/// Pending window request: config + optional UI builder
pub(super) struct PendingWindowRequest {
    pub(super) config: WindowConfig,
    pub(super) builder: Option<WindowBuilder>,
}

/// Queue of pending window requests (builder closures waiting to be picked up
/// by the event loop after AppCommand::CreateWindow fires).
pub(super) static PENDING_WINDOW_BUILDERS: std::sync::OnceLock<Mutex<Vec<PendingWindowRequest>>> =
    std::sync::OnceLock::new();

pub(super) fn pending_builders() -> &'static Mutex<Vec<PendingWindowRequest>> {
    PENDING_WINDOW_BUILDERS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Global callback for sending CreateWindow command to the event loop.
pub(super) static OPEN_WINDOW_FN: std::sync::OnceLock<Arc<dyn Fn(WindowConfig) + Send + Sync>> =
    std::sync::OnceLock::new();

/// Global callback for initiating a window drag operation (custom title bars).
pub(super) static DRAG_WINDOW_FN: std::sync::OnceLock<Arc<dyn Fn() + Send + Sync>> =
    std::sync::OnceLock::new();

/// Start a window drag operation (for custom title bars).
///
/// Call this from a mouse-down handler on a draggable element.
/// The OS takes over and the window follows the cursor until release.
pub fn drag_window() {
    if let Some(f) = DRAG_WINDOW_FN.get() {
        f();
    }
}

/// Open a new window with a UI builder from anywhere in the application.
///
/// The builder closure is called each frame to produce the window's UI.
///
/// # Example
/// ```ignore
/// use blinc_app::windowed::open_window_with;
///
/// open_window_with(
///     WindowConfig::new("Settings").size(400, 300),
///     |ctx| {
///         Box::new(div()
///             .w(ctx.width).h(ctx.height)
///             .bg(Color::rgb(0.1, 0.1, 0.15))
///             .child(text("Settings Window").size(24.0).color(Color::WHITE)))
///     },
/// );
/// ```
pub fn open_window_with<F>(config: WindowConfig, builder: F)
where
    F: FnMut(&mut WindowedContext) -> Div + Send + 'static,
{
    // Queue the builder so the event loop can pick it up
    pending_builders()
        .lock()
        .unwrap()
        .push(PendingWindowRequest {
            config: config.clone(),
            builder: Some(Box::new(builder)),
        });

    // Send the CreateWindow command to the event loop
    if let Some(f) = OPEN_WINDOW_FN.get() {
        f(config);
    } else {
        tracing::warn!("open_window_with() called before app initialization");
    }
}

/// Open a new window with a default blank UI.
///
/// For windows with custom UI, use `open_window_with()` instead.
pub fn open_window(config: WindowConfig) {
    // Queue without builder (uses default UI)
    pending_builders()
        .lock()
        .unwrap()
        .push(PendingWindowRequest {
            config: config.clone(),
            builder: None,
        });

    if let Some(f) = OPEN_WINDOW_FN.get() {
        f(config);
    } else {
        tracing::warn!("open_window() called before app initialization");
    }
}
