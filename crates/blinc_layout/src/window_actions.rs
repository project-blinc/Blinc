//! Window action callbacks for layout elements.
//!
//! These are registered by the platform runner and callable from element
//! event handlers (e.g., `.drag_region()` on Div for custom title bars).
//!
//! The callbacks target the **currently focused** window. When focus changes
//! between windows, the platform runner updates the active callbacks.

use std::sync::Mutex;

type ActionCallback = Box<dyn Fn() + Send + Sync>;

struct WindowActions {
    drag: Option<ActionCallback>,
    minimize: Option<ActionCallback>,
    maximize: Option<ActionCallback>,
    close: Option<ActionCallback>,
}

static ACTIONS: Mutex<WindowActions> = Mutex::new(WindowActions {
    drag: None,
    minimize: None,
    maximize: None,
    close: None,
});

/// Set all window action callbacks for the currently focused window.
///
/// Called by the platform runner when:
/// - The primary window initializes
/// - Focus changes to a different window
/// - A new secondary window is created
pub fn set_active_window_actions(
    drag: impl Fn() + Send + Sync + 'static,
    minimize: impl Fn() + Send + Sync + 'static,
    maximize: impl Fn() + Send + Sync + 'static,
    close: impl Fn() + Send + Sync + 'static,
) {
    if let Ok(mut actions) = ACTIONS.lock() {
        actions.drag = Some(Box::new(drag));
        actions.minimize = Some(Box::new(minimize));
        actions.maximize = Some(Box::new(maximize));
        actions.close = Some(Box::new(close));
    }
}

/// Start a window drag operation (for custom title bars)
pub fn drag_window() {
    if let Ok(actions) = ACTIONS.lock()
        && let Some(ref f) = actions.drag
    {
        f();
    }
}

/// Minimize the window
pub fn minimize_window() {
    if let Ok(actions) = ACTIONS.lock()
        && let Some(ref f) = actions.minimize
    {
        f();
    }
}

/// Maximize/restore the window
pub fn maximize_window() {
    if let Ok(actions) = ACTIONS.lock()
        && let Some(ref f) = actions.maximize
    {
        f();
    }
}

/// Close the window
pub fn close_window() {
    if let Ok(actions) = ACTIONS.lock()
        && let Some(ref f) = actions.close
    {
        f();
    }
}
