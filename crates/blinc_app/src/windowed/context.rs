//! The context passed to UI builders: its state and constructors.

use crate::EventRouter;
use crate::windowed::{
    RefDirtyFlag, SharedAnimationScheduler, SharedElementRegistry, SharedReactiveGraph,
    SharedReadyCallbacks,
};
use blinc_animation::AnimationScheduler;
use blinc_core::context_state::SharedHookState;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{OverlayManager, overlay_manager};
use blinc_platform::{Window, WindowConfig};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

mod display;
mod query;
mod reactive;

/// Context passed to the UI builder function
pub struct WindowedContext {
    /// Current window width in logical pixels (for UI layout)
    ///
    /// This is the width you should use when building UI. It automatically
    /// accounts for DPI scaling, so elements sized to `ctx.width` will
    /// fill the window regardless of display scale factor.
    pub width: f32,
    /// Current window height in logical pixels (for UI layout)
    pub height: f32,
    /// Current scale factor (physical / logical)
    pub scale_factor: f64,
    /// Safe area insets (top, right, bottom, left) in logical pixels.
    /// On mobile: notch, status bar, home indicator.
    /// On desktop: all zeros.
    pub safe_area: (f32, f32, f32, f32),
    /// Soft-keyboard inset, in **logical** pixels — height in pixels of
    /// the area at the bottom of the screen currently obscured by an
    /// on-screen keyboard. Zero when the keyboard is hidden.
    ///
    /// Updated by the platform runner from native keyboard events:
    ///
    ///   - iOS: parsed from
    ///     `UIKeyboardWillChangeFrameNotification.userInfo[UIKeyboardFrameEndUserInfoKey]`
    ///     in `BlincKeyboardHelper`, pushed via the
    ///     `blinc_ios_set_keyboard_inset` FFI export.
    ///   - Android: read from
    ///     `WindowInsets.Type.ime().bottom` in
    ///     `BlincNativeBridge.kt`, dispatched into Rust through the
    ///     `keyboard.set_inset` native-bridge handler.
    ///   - Desktop / web / Fuchsia: always zero.
    ///
    /// The text-input refocus path consumes this to scroll the focused
    /// input above the keyboard when it appears, mirroring the iOS UIKit
    /// `UIScrollView.contentInset` adjustment dance.
    pub keyboard_inset: f32,
    /// Physical window width (for internal use)
    pub(crate) physical_width: f32,
    /// Physical window height (for internal use)
    pub(crate) physical_height: f32,
    /// Whether the window is focused
    pub focused: bool,
    /// Number of completed UI rebuilds (0 = first build in progress)
    ///
    /// Use `is_ready()` to check if the UI has been built at least once.
    /// This is useful for triggering animations after motion bindings are registered.
    pub rebuild_count: u32,
    /// Event router for input event handling
    pub event_router: EventRouter,
    /// Animation scheduler for spring/keyframe animations
    pub animations: SharedAnimationScheduler,
    /// Shared dirty flag for element refs - when set, triggers UI rebuild
    pub(super) ref_dirty_flag: RefDirtyFlag,
    /// Reactive graph for signal-based state management
    pub(super) reactive: SharedReactiveGraph,
    /// Hook state for call-order based signal persistence
    pub(super) hooks: SharedHookState,
    /// Overlay manager for modals, dialogs, toasts, etc.
    pub(crate) overlay_manager: OverlayManager,
    /// Whether overlays were visible last frame (for triggering rebuilds)
    pub(crate) had_visible_overlays: bool,
    /// Element registry for query API (shared with RenderTree)
    pub(super) element_registry: SharedElementRegistry,
    /// Callbacks to run after UI is ready (motion bindings registered)
    pub(super) ready_callbacks: SharedReadyCallbacks,
    /// CSS stylesheet for automatic style application (hover, animations, base styles)
    /// Multiple stylesheets cascade — later rules override earlier ones.
    pub stylesheet: Option<Arc<blinc_layout::css_parser::Stylesheet>>,
    /// Raw CSS source strings, preserved for reparsing on theme changes.
    /// Each entry corresponds to one `add_css()` call, in order.
    pub(super) css_sources: Vec<String>,
    pub(super) css_overrides: Vec<(String, String)>,
    /// Continuous pointer query state (per-element pointer tracking)
    pub pointer_query: blinc_layout::pointer_query::PointerQueryState,
    /// Callback to request opening a new window (set by desktop runner)
    pub(super) open_window_fn: Option<Arc<dyn Fn(WindowConfig) + Send + Sync>>,
    /// Per-window close callback (sends CloseWindow command for THIS window)
    pub(super) close_fn: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Per-window drag callback (starts OS drag for THIS window)
    pub(super) drag_fn: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Per-window minimize callback
    pub(super) minimize_fn: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Per-window maximize callback
    pub(super) maximize_fn: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl WindowedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_window<W: Window>(
        window: &W,
        event_router: EventRouter,
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        overlay_mgr: OverlayManager,
        element_registry: SharedElementRegistry,
        ready_callbacks: SharedReadyCallbacks,
    ) -> Self {
        // Get physical size (actual surface pixels) and scale factor
        let (physical_width, physical_height) = window.size();
        let scale_factor = window.scale_factor();

        // Compute logical size (what users work with in their UI code)
        // This ensures elements sized with ctx.width/height fill the window
        // regardless of DPI, and font sizes appear consistent across displays
        let logical_width = physical_width as f32 / scale_factor as f32;
        let logical_height = physical_height as f32 / scale_factor as f32;

        // Publish to the shared context so CSS viewport units (`vh` /
        // `vw`) resolve. iOS and web already did this; desktop did not,
        // so `height: 100vh` in a stylesheet parsed against a zero
        // viewport and was dropped.
        if blinc_core::context_state::BlincContextState::is_initialized() {
            blinc_core::context_state::BlincContextState::get()
                .set_viewport_size(logical_width, logical_height);
        }

        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            safe_area: window.safe_area_insets(),
            keyboard_inset: 0.0,
            physical_width: physical_width as f32,
            physical_height: physical_height as f32,
            focused: window.is_focused(),
            rebuild_count: 0,
            event_router,
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
            stylesheet: None,
            css_sources: Vec::new(),
            css_overrides: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// Create a WindowedContext for Android
    ///
    /// This is used by the Android runner since it doesn't have a Window trait implementation.
    #[cfg(all(feature = "android", target_os = "android"))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_android(
        logical_width: f32,
        logical_height: f32,
        scale_factor: f64,
        physical_width: f32,
        physical_height: f32,
        focused: bool,
        safe_area: (f32, f32, f32, f32),
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        overlay_mgr: OverlayManager,
        element_registry: SharedElementRegistry,
        ready_callbacks: SharedReadyCallbacks,
    ) -> Self {
        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            safe_area,
            keyboard_inset: 0.0,
            physical_width,
            physical_height,
            focused,
            rebuild_count: 0,
            event_router: EventRouter::new(),
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
            stylesheet: None,
            css_sources: Vec::new(),
            css_overrides: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// Create a WindowedContext for iOS
    ///
    /// This is used by the iOS runner since it doesn't have a Window trait implementation.
    #[cfg(all(feature = "ios", target_os = "ios"))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_ios(
        logical_width: f32,
        logical_height: f32,
        scale_factor: f64,
        physical_width: f32,
        physical_height: f32,
        focused: bool,
        safe_area: (f32, f32, f32, f32),
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        overlay_mgr: OverlayManager,
        element_registry: SharedElementRegistry,
        ready_callbacks: SharedReadyCallbacks,
    ) -> Self {
        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            safe_area,
            keyboard_inset: 0.0,
            physical_width,
            physical_height,
            focused,
            rebuild_count: 0,
            event_router: EventRouter::new(),
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
            stylesheet: None,
            css_sources: Vec::new(),
            css_overrides: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// Create a WindowedContext for the web target.
    ///
    /// Mirrors [`Self::new_android`] / [`Self::new_ios`] / [`Self::new_fuchsia`]:
    /// the web runner extracts canvas dimensions and `devicePixelRatio`
    /// from the browser before calling, instead of going through the
    /// `Window` trait (which requires `raw-window-handle` types that
    /// `HtmlCanvasElement` doesn't implement).
    ///
    /// Wired into the `web` feature so this constructor is invisible to
    /// non-wasm builds. The shared / animation / overlay parameters are
    /// the same as the other `new_*` constructors so the wasm runner
    /// can build the same `WindowedContext` shape every other platform
    /// gets.
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_web(
        logical_width: f32,
        logical_height: f32,
        scale_factor: f64,
        physical_width: f32,
        physical_height: f32,
        focused: bool,
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        overlay_mgr: OverlayManager,
        element_registry: SharedElementRegistry,
        ready_callbacks: SharedReadyCallbacks,
    ) -> Self {
        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            safe_area: (0.0, 0.0, 0.0, 0.0),
            keyboard_inset: 0.0,
            physical_width,
            physical_height,
            focused,
            rebuild_count: 0,
            event_router: EventRouter::new(),
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
            stylesheet: None,
            css_sources: Vec::new(),
            css_overrides: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// A context with no window, for building and rendering a UI offscreen:
    /// profiling, tests and exports. It gets its own animation scheduler,
    /// overlay manager and element registry; the reactive graph is the
    /// process-wide one, as in a window.
    pub fn headless(width: f32, height: f32) -> Self {
        Self::headless_with_scale(width, height, 1.0)
    }

    /// A headless context whose layout uses logical pixels at the given density.
    pub fn headless_with_scale(width: f32, height: f32, scale_factor: f64) -> Self {
        assert!(
            width.is_finite()
                && height.is_finite()
                && width > 0.0
                && height > 0.0
                && scale_factor.is_finite()
                && scale_factor > 0.0
        );
        Self {
            width,
            height,
            scale_factor,
            safe_area: (0.0, 0.0, 0.0, 0.0),
            keyboard_inset: 0.0,
            physical_width: (width * scale_factor as f32).round().max(1.0),
            physical_height: (height * scale_factor as f32).round().max(1.0),
            focused: true,
            rebuild_count: 0,
            event_router: EventRouter::new(),
            animations: Arc::new(Mutex::new(AnimationScheduler::new())),
            ref_dirty_flag: Arc::new(AtomicBool::new(false)),
            reactive: blinc_core::reactive::global_graph(),
            hooks: Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
            overlay_manager: blinc_layout::widgets::overlay::overlay_manager(),
            had_visible_overlays: false,
            element_registry: Arc::new(blinc_layout::selector::ElementRegistry::new()),
            ready_callbacks: Arc::new(Mutex::new(Vec::new())),
            stylesheet: None,
            css_sources: Vec::new(),
            css_overrides: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// Create a WindowedContext for Fuchsia
    ///
    /// This is used by the Fuchsia runner since it doesn't have a Window trait implementation.
    #[cfg(all(feature = "fuchsia", target_os = "fuchsia"))]
    pub(crate) fn new_fuchsia(
        logical_width: f32,
        logical_height: f32,
        scale_factor: f64,
        physical_width: f32,
        physical_height: f32,
        focused: bool,
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
        overlay_mgr: OverlayManager,
        element_registry: SharedElementRegistry,
        ready_callbacks: SharedReadyCallbacks,
    ) -> Self {
        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            safe_area: (0.0, 0.0, 0.0, 0.0),
            keyboard_inset: 0.0,
            physical_width,
            physical_height,
            focused,
            rebuild_count: 0,
            event_router: EventRouter::new(),
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
            stylesheet: None,
            css_sources: Vec::new(),
            css_overrides: Vec::new(),
            pointer_query: blinc_layout::pointer_query::PointerQueryState::new(),
            open_window_fn: None,
            close_fn: None,
            drag_fn: None,
            minimize_fn: None,
            maximize_fn: None,
        }
    }

    /// Update context from window (preserving event router, dirty flag, and reactive graph)
    pub(super) fn update_from_window<W: Window>(&mut self, window: &W) {
        let (physical_width, physical_height) = window.size();
        let scale_factor = window.scale_factor();

        self.physical_width = physical_width as f32;
        self.physical_height = physical_height as f32;
        self.width = physical_width as f32 / scale_factor as f32;
        self.height = physical_height as f32 / scale_factor as f32;
        self.scale_factor = scale_factor;
        self.focused = window.is_focused();
    }
}
