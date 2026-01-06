//! Global overlay context singleton
//!
//! OverlayContext provides a global singleton for accessing the overlay manager
//! without requiring explicit context parameters.
//!
//! This enables components like Select to create dropdowns via overlay:
//!
//! ```ignore
//! use blinc_layout::overlay_state::get_overlay_manager;
//!
//! // In a component:
//! let mgr = get_overlay_manager();
//! mgr.dropdown()
//!     .at(x, y)
//!     .content(|| dropdown_content)
//!     .show();
//! ```
//!
//! # Initialization
//!
//! The singleton must be initialized by the app layer before use:
//!
//! ```ignore
//! // In WindowedApp::run()
//! OverlayContext::init(overlay_manager);
//! ```

use std::cell::Cell;
use std::sync::OnceLock;

use crate::widgets::overlay::OverlayManager;

/// Global overlay context instance
static OVERLAY_CONTEXT: OnceLock<OverlayContext> = OnceLock::new();

/// Thread-local flag indicating if we're currently rendering closing overlay content
///
/// DEPRECATED: This mechanism is being replaced by explicit `MotionHandle.exit()` calls.
/// Motion exit should be triggered explicitly via `query_motion(key).exit()` instead of
/// relying on this flag captured at construction time.
thread_local! {
    static OVERLAY_CLOSING: Cell<bool> = const { Cell::new(false) };
}

/// Check if we're currently rendering overlay content that is closing
///
/// DEPRECATED: Use `query_motion(key).exit()` to explicitly trigger motion exit instead.
/// This flag-based mechanism doesn't work correctly because the flag resets after
/// `build_content()` returns, breaking multi-frame exit animations.
#[deprecated(
    since = "0.1.0",
    note = "Use query_motion(key).exit() to explicitly trigger motion exit"
)]
pub fn is_overlay_closing() -> bool {
    OVERLAY_CLOSING.with(|c| c.get())
}

/// Set the overlay closing flag (call before/after building closing overlay content)
///
/// DEPRECATED: Use `query_motion(key).exit()` to explicitly trigger motion exit instead.
/// This flag-based mechanism doesn't work correctly because the flag resets after
/// `build_content()` returns, breaking multi-frame exit animations.
#[deprecated(
    since = "0.1.0",
    note = "Use query_motion(key).exit() to explicitly trigger motion exit"
)]
pub fn set_overlay_closing(closing: bool) {
    OVERLAY_CLOSING.with(|c| c.set(closing));
}

/// Global overlay context singleton
///
/// Provides access to the overlay manager without requiring explicit context parameters.
/// Named `OverlayContext` to avoid conflict with `OverlayState` FSM enum.
pub struct OverlayContext {
    /// The overlay manager instance
    manager: OverlayManager,
}

impl OverlayContext {
    /// Initialize the global overlay context (call once at app startup)
    ///
    /// # Panics
    ///
    /// Panics if called more than once.
    pub fn init(manager: OverlayManager) {
        let state = OverlayContext { manager };

        if OVERLAY_CONTEXT.set(state).is_err() {
            panic!("OverlayContext::init() called more than once");
        }
    }

    /// Get the global overlay context instance
    ///
    /// # Panics
    ///
    /// Panics if `init()` has not been called.
    pub fn get() -> &'static OverlayContext {
        OVERLAY_CONTEXT
            .get()
            .expect("OverlayContext not initialized. Call OverlayContext::init() at app startup.")
    }

    /// Try to get the global overlay context (returns None if not initialized)
    pub fn try_get() -> Option<&'static OverlayContext> {
        OVERLAY_CONTEXT.get()
    }

    /// Check if the overlay context has been initialized
    pub fn is_initialized() -> bool {
        OVERLAY_CONTEXT.get().is_some()
    }

    /// Get the overlay manager
    pub fn overlay_manager(&self) -> OverlayManager {
        std::sync::Arc::clone(&self.manager)
    }
}

// =========================================================================
// Convenience Free Functions
// =========================================================================

/// Get the global overlay manager
///
/// This is a convenience wrapper around `OverlayContext::get().overlay_manager()`.
///
/// # Panics
///
/// Panics if `OverlayContext::init()` has not been called.
///
/// # Example
///
/// ```ignore
/// use blinc_layout::overlay_state::get_overlay_manager;
///
/// let mgr = get_overlay_manager();
/// mgr.dropdown()
///     .at(x, y)
///     .content(|| dropdown_content)
///     .show();
/// ```
pub fn get_overlay_manager() -> OverlayManager {
    OverlayContext::get().overlay_manager()
}
