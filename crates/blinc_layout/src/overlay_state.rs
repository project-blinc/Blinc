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

// `clippy::missing_const_for_thread_local` mis-fires on nightly clippy
// 0.1.96+ when the initializer is already wrapped in `const { ... }` —
// see `motion.rs` for the same workaround.
#![allow(clippy::missing_const_for_thread_local)]

use std::cell::Cell;
use std::sync::{Arc, Mutex, OnceLock};

use crate::widgets::overlay::OverlayManager;
use crate::widgets::overlay_stack::OverlayStack;
use crate::widgets::toast_tray::ToastTray;

// Phase 2 of the overlay-stack refactor (see OVERLAY_STACK_DESIGN.md).
// The new `OverlayStack` and `ToastTray` singletons live alongside the
// legacy `OverlayManager` during the migration. Once every consumer is
// ported (Phase 4) the legacy singleton + module will be deleted.

pub type SharedOverlayStack = Arc<Mutex<OverlayStack>>;
pub type SharedToastTray = Arc<Mutex<ToastTray>>;

static OVERLAY_STACK: OnceLock<SharedOverlayStack> = OnceLock::new();
static TOAST_TRAY: OnceLock<SharedToastTray> = OnceLock::new();

/// Get the global overlay stack, creating it lazily on first access.
///
/// Unlike the legacy `OverlayContext::init()` which panics if called twice,
/// this lazy initializer is safe to call from anywhere. The stack is shared
/// across the entire process — every cn widget pushes / closes through this
/// singleton, every windowed-runner queries it for `has_blocking_overlay()`
/// and `build_overlay_layer()`.
pub fn overlay_stack() -> SharedOverlayStack {
    OVERLAY_STACK
        .get_or_init(|| Arc::new(Mutex::new(OverlayStack::new())))
        .clone()
}

/// Get the global toast tray. Lazy initializer, see `overlay_stack()`.
pub fn toast_tray() -> SharedToastTray {
    TOAST_TRAY
        .get_or_init(|| Arc::new(Mutex::new(ToastTray::new())))
        .clone()
}

// =========================================================================
// Subtree-rebuild helper
// =========================================================================

/// Generic "is this overlay surface dirty? then rebuild its subtree" pass.
/// Used by the windowed runner so each surface (overlay stack, toast tray,
/// future surfaces) doesn't accumulate its own copy-pasted dirty-check +
/// registry-lookup + queue_subtree_rebuild block.
///
/// Returns `true` if a rebuild was queued.
///
/// ```ignore
/// rebuild_overlay_subtree_if_dirty(
///     &element_registry,
///     OVERLAY_STACK_LAYER_ID,
///     overlay_stack().lock().map(|s| s.take_dirty()).unwrap_or(false),
///     || overlay_stack().lock().ok().map(|s| s.build_overlay_layer())
///         .unwrap_or_else(crate::div::Div::new),
/// );
/// ```
pub fn rebuild_overlay_subtree_if_dirty(
    registry: &crate::prelude::ElementRegistry,
    layer_id: &str,
    dirty: bool,
    build: impl FnOnce() -> crate::div::Div,
) -> bool {
    if !dirty {
        return false;
    }
    let Some(node_id) = registry.get(layer_id) else {
        tracing::trace!(
            target: "blinc_layout::overlay_state",
            "Overlay surface '{}' dirty but layer node not yet in registry — will mount on next full UI build",
            layer_id,
        );
        return false;
    };
    // The layer is built afresh: what its overlays create goes when the layer is rebuilt next.
    let owner = blinc_core::owner::Owner::default();
    let layer = owner.run(build);
    crate::stateful::queue_subtree_rebuild_owned(node_id, layer, owner);
    true
}

/// Global overlay context instance
static OVERLAY_CONTEXT: OnceLock<OverlayContext> = OnceLock::new();

// Thread-local flag indicating if we're currently rendering closing overlay content
//
// DEPRECATED: This mechanism is being replaced by explicit `MotionHandle.exit()` calls.
// Motion exit should be triggered explicitly via `query_motion(key).exit()` instead of
// relying on this flag captured at construction time.
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
