//! iOS application runner
//!
//! Provides a unified API for running Blinc applications on iOS.
//!
//! # Example
//!
//! ```ignore
//! use blinc_app::prelude::*;
//! use blinc_app::ios::IOSApp;
//!
//! // Called from your Swift/Objective-C app delegate
//! IOSApp::run_with_metal_layer(metal_layer, width, height, scale, |ctx| {
//!     div().w(ctx.width).h(ctx.height)
//!         .bg([0.1, 0.1, 0.15, 1.0])
//!         .flex_center()
//!         .child(text("Hello iOS!").size(48.0))
//! }).unwrap();
//! ```
//!
//! # iOS Integration
//!
//! Unlike Android where Blinc can run as a native activity, on iOS you must
//! integrate Blinc into your existing UIKit application. The typical flow is:
//!
//! 1. Create a `UIView` subclass with `CAMetalLayer` as its layer class
//! 2. Set up a `CADisplayLink` for frame callbacks
//! 3. Call `IOSApp::render_frame()` on each display link callback
//! 4. Forward touch events to `IOSApp::handle_touch()`

// All `extern "C" fn blinc_*` exports in this module take raw `*mut`
// pointers from the Swift caller and dereference them. Each one
// documents its safety contract in a `# Safety` section: "must be a
// valid pointer returned by `blinc_create_context`". `clippy::not_unsafe_ptr_arg_deref`
// would have us promote every export to `unsafe extern "C" fn`, which
// is technically more accurate but doesn't change anything for the C
// caller (Swift) and forces every Swift call site to wrap them. We
// prefer the documented-contract approach used by every other Rust
// FFI library targeting Swift / Obj-C.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use blinc_animation::AnimationScheduler;
use blinc_core::context_state::{BlincContextState, HookState, SharedHookState};
use blinc_core::reactive::{ReactiveGraph, SignalId};
use blinc_layout::event_router::MouseButton;
use blinc_layout::overlay_state::OverlayContext;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{OverlayManager, overlay_manager};
use blinc_platform::assets::set_global_asset_loader;
use blinc_platform_ios::{Gesture, GestureDetector, IOSAssetLoader, IOSWakeProxy, TouchPhase};

use crate::app::BlincApp;
use crate::error::{BlincError, Result};
use crate::windowed::{
    RefDirtyFlag, SharedAnimationScheduler, SharedElementRegistry, SharedReactiveGraph,
    SharedReadyCallbacks, WindowedContext,
};

// =============================================================================
// Soft keyboard FFI — runtime-resolved via dlsym
// =============================================================================

/// Cached lookup of `blinc_ios_show_keyboard`. See the call site
/// in `build_frame` for the rationale on why these are resolved
/// at runtime instead of via a strong `extern "C"` reference.
fn keyboard_show_fn() -> Option<extern "C" fn()> {
    use std::sync::OnceLock;
    static FN: OnceLock<Option<extern "C" fn()>> = OnceLock::new();
    *FN.get_or_init(|| unsafe { lookup_extern_fn(b"blinc_ios_show_keyboard\0") })
}

/// Cached lookup of `blinc_ios_hide_keyboard`. Symmetric with
/// `keyboard_show_fn`.
fn keyboard_hide_fn() -> Option<extern "C" fn()> {
    use std::sync::OnceLock;
    static FN: OnceLock<Option<extern "C" fn()>> = OnceLock::new();
    *FN.get_or_init(|| unsafe { lookup_extern_fn(b"blinc_ios_hide_keyboard\0") })
}

/// Look up a C symbol in the global namespace via
/// `dlsym(RTLD_DEFAULT, ...)`. Returns `None` if the symbol
/// isn't present in the linked binary (e.g. user iOS app didn't
/// copy `BlincNativeBridge.swift` from
/// `extensions/blinc_platform_ios/templates/`).
///
/// `name` MUST be a null-terminated byte string. The caller's
/// fixed-string usage (`b"blinc_ios_show_keyboard\0"`) ensures
/// that property at compile time.
///
/// # Safety
///
/// Caller must guarantee that `name` is a valid C string and
/// that the symbol — if found — actually has the function
/// signature it's transmuted to. We only call this from
/// `keyboard_show_fn` / `keyboard_hide_fn`, both of which
/// transmute to `extern "C" fn()` and the templates `@_cdecl`
/// declarations match exactly.
unsafe fn lookup_extern_fn(name: &[u8]) -> Option<extern "C" fn()> {
    unsafe extern "C" {
        fn dlsym(handle: *mut std::ffi::c_void, symbol: *const i8) -> *mut std::ffi::c_void;
    }
    // `RTLD_DEFAULT` on Apple platforms is the magic value
    // `(void *) -2`. We can't import the constant from libc
    // without pulling in the libc crate as a dependency for one
    // value, so we hard-code it. The value is documented in
    // `dlfcn.h` and stable across all macOS / iOS / tvOS / watchOS
    // releases. Linux uses `(void *) 0` for the same constant,
    // but this entire module is iOS-only so the Linux value is
    // irrelevant here.
    const RTLD_DEFAULT: *mut std::ffi::c_void = -2isize as *mut std::ffi::c_void;
    debug_assert!(name.last() == Some(&0), "name must be null-terminated");
    // SAFETY: caller-asserted invariants on `name` (null-terminated
    // and points at a valid exported symbol); `RTLD_DEFAULT` is the
    // documented sentinel. Edition 2024 makes `unsafe fn` bodies
    // explicit-only, so the dlsym + transmute calls now need their
    // own `unsafe` block.
    let sym = unsafe { dlsym(RTLD_DEFAULT, name.as_ptr() as *const i8) };
    if sym.is_null() {
        None
    } else {
        // SAFETY: `sym` is non-null, points at a function
        // exported by the linked binary, and the caller has
        // committed to the function having signature
        // `extern "C" fn()`.
        unsafe { Some(std::mem::transmute::<*mut std::ffi::c_void, extern "C" fn()>(sym)) }
    }
}

/// iOS application runner
///
/// Provides methods for running a Blinc application on iOS with Metal rendering.
/// Unlike desktop or Android, iOS apps must integrate Blinc into their existing
/// UIKit application lifecycle.
pub struct IOSApp;

impl IOSApp {
    /// Initialize the iOS asset loader
    fn init_asset_loader() {
        let loader = IOSAssetLoader::new();
        let _ = set_global_asset_loader(Box::new(loader));
    }

    /// Initialize the theme system
    fn init_theme() {
        use blinc_theme::{
            ThemeState, detect_system_color_scheme, platform_theme_bundle, set_redraw_callback,
        };

        // Only initialize if not already initialized
        if ThemeState::try_get().is_none() {
            let bundle = platform_theme_bundle();
            let scheme = detect_system_color_scheme();
            ThemeState::init(bundle, scheme);
        }

        // Set up the redraw callback
        set_redraw_callback(|| {
            tracing::debug!("Theme changed - requesting full rebuild");
            blinc_layout::widgets::request_full_rebuild();
        });
    }

    /// Create a new Blinc context for iOS rendering
    ///
    /// This sets up all the shared state needed for Blinc rendering.
    /// Call this once when your app starts, then use the returned
    /// `IOSRenderContext` for rendering frames.
    ///
    /// # Arguments
    ///
    /// * `width` - Physical width in pixels
    /// * `height` - Physical height in pixels
    /// * `scale_factor` - Display scale factor (UIScreen.scale)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let render_ctx = IOSApp::create_context(
    ///     screen_width,
    ///     screen_height,
    ///     UIScreen.mainScreen.scale,
    /// )?;
    /// ```
    pub fn create_context(width: u32, height: u32, scale_factor: f64) -> Result<IOSRenderContext> {
        tracing::info!(
            "IOSApp::create_context: {}x{} physical pixels, scale_factor={}",
            width,
            height,
            scale_factor
        );

        let logical_width = width as f32 / scale_factor as f32;
        let logical_height = height as f32 / scale_factor as f32;
        tracing::info!(
            "IOSApp::create_context: {:.1}x{:.1} logical points",
            logical_width,
            logical_height
        );

        // Initialize the asset loader
        Self::init_asset_loader();

        // Initialize the text measurer
        blinc_layout::init_text_measurer();

        // Initialize the theme system
        Self::init_theme();

        // Initialize the native bridge state if it isn't already.
        // The Rust side of `text_edit::haptic_*` and
        // `show_edit_menu` / `hide_edit_menu` go through
        // `blinc_core::native_bridge::native_call`, which panics if
        // `NativeBridgeState::init()` was never called. Each helper
        // also has its own `bridge_ready` guard so they no-op when
        // no platform adapter is registered, but the bridge state
        // itself still has to exist for the `is_initialized` check
        // to return without panicking.
        //
        // Initializing here means the iOS runner ALWAYS has a bridge
        // state, even if no Swift code calls
        // `blinc_set_native_call_fn` to register an adapter — in
        // that case the helpers fall through their `bridge_ready`
        // checks and produce no haptics / no edit menu, but the
        // touch handler doesn't crash.
        if !blinc_core::native_bridge::NativeBridgeState::is_initialized() {
            blinc_core::native_bridge::NativeBridgeState::init();
        }

        // Shared state — reactive graph is the process-global one so
        // bare `signal()` / `effect()` / `computed()` interop with
        // `State<T>` / `Stateful::deps`.
        let ref_dirty_flag: RefDirtyFlag = Arc::new(AtomicBool::new(false));
        let reactive: SharedReactiveGraph = blinc_core::reactive::global_graph();
        let hooks: SharedHookState = Arc::new(Mutex::new(HookState::new()));

        // Initialize global context state singleton
        if !BlincContextState::is_initialized() {
            #[allow(clippy::type_complexity)]
            let stateful_callback: Arc<dyn Fn(&[SignalId]) + Send + Sync> =
                Arc::new(|signal_ids| {
                    blinc_layout::check_stateful_deps(signal_ids);
                });
            BlincContextState::init_with_callback(
                Arc::clone(&reactive),
                Arc::clone(&hooks),
                Arc::clone(&ref_dirty_flag),
                stateful_callback.clone(),
            );
            blinc_core::reactive::set_stateful_deps_notifier(move |ids| stateful_callback(ids));
        }

        // Animation scheduler with wake proxy
        let mut scheduler = AnimationScheduler::new();

        // Set up wake proxy for iOS
        let wake_proxy = IOSWakeProxy::new();
        let wake_proxy_clone = wake_proxy.clone();
        scheduler.set_wake_callback(move || wake_proxy_clone.wake());

        scheduler.start_background();
        let animations: SharedAnimationScheduler = Arc::new(Mutex::new(scheduler));

        // Set global scheduler handle
        {
            let scheduler_handle = animations.lock().unwrap().handle();
            blinc_animation::set_global_scheduler(scheduler_handle);
        }

        // Element registry for query API
        let element_registry: SharedElementRegistry =
            Arc::new(blinc_layout::selector::ElementRegistry::new());

        // Set up query callback
        {
            let registry_for_query = Arc::clone(&element_registry);
            let query_callback: blinc_core::QueryCallback = Arc::new(move |id: &str| {
                registry_for_query.get(id).map(|node_id| node_id.to_raw())
            });
            BlincContextState::get().set_query_callback(query_callback);
        }

        // Set up bounds callback
        {
            let registry_for_bounds = Arc::clone(&element_registry);
            let bounds_callback: blinc_core::BoundsCallback =
                Arc::new(move |id: &str| registry_for_bounds.get_bounds(id));
            BlincContextState::get().set_bounds_callback(bounds_callback);
        }

        // Store element registry in BlincContextState
        BlincContextState::get()
            .set_element_registry(Arc::clone(&element_registry) as blinc_core::AnyElementRegistry);

        // Ready callbacks
        let ready_callbacks: SharedReadyCallbacks = Arc::new(Mutex::new(Vec::new()));

        // Overlay manager
        let overlays: OverlayManager = overlay_manager();
        if !OverlayContext::is_initialized() {
            OverlayContext::init(Arc::clone(&overlays));
        }

        // Connect theme animation to scheduler
        blinc_theme::ThemeState::get().set_scheduler(&animations);

        // Render state and motion states
        let shared_motion_states = blinc_layout::create_shared_motion_states();

        // Set up motion state callback
        {
            let motion_states_for_callback = Arc::clone(&shared_motion_states);
            let motion_callback: blinc_core::MotionStateCallback = Arc::new(move |key: &str| {
                motion_states_for_callback
                    .read()
                    .ok()
                    .and_then(|states| states.get(key).copied())
                    .unwrap_or(blinc_core::MotionAnimationState::NotFound)
            });
            BlincContextState::get().set_motion_state_callback(motion_callback);
        }

        // Calculate logical dimensions
        let logical_width = width as f32 / scale_factor as f32;
        let logical_height = height as f32 / scale_factor as f32;

        // Set viewport size
        BlincContextState::get().set_viewport_size(logical_width, logical_height);

        // Fetch UIKit safe area insets (notch, status bar, home indicator).
        // Must happen on the main thread — the runner already is by the
        // time the UIApplicationDelegate triggers context creation.
        let safe_area = blinc_platform_ios::app::get_safe_area_insets();

        // Create windowed context
        let windowed_ctx = WindowedContext::new_ios(
            logical_width,
            logical_height,
            scale_factor,
            width as f32,
            height as f32,
            true, // focused
            safe_area,
            Arc::clone(&animations),
            Arc::clone(&ref_dirty_flag),
            Arc::clone(&reactive),
            Arc::clone(&hooks),
            Arc::clone(&overlays),
            Arc::clone(&element_registry),
            Arc::clone(&ready_callbacks),
        );

        // Initialize render state
        let mut render_state = blinc_layout::RenderState::new(Arc::clone(&animations));
        render_state.set_shared_motion_states(Arc::clone(&shared_motion_states));

        Ok(IOSRenderContext {
            windowed_ctx,
            render_state,
            render_tree: None,
            ref_dirty_flag,
            animations,
            ready_callbacks,
            wake_proxy,
            rebuild_count: 0,
            last_touch_pos: None,
            is_scrolling: false,
            gesture_detector: GestureDetector::new(),
            last_frame_time_ms: 0,
            last_applied_keyboard_inset: 0.0,
            last_focus_tap_generation: 0,
        })
    }

    /// iOS system font paths
    pub fn system_font_paths() -> &'static [&'static str] {
        blinc_platform_ios::system_font_paths()
    }
}

/// iOS render context
///
/// Holds all the state needed to render Blinc UI on iOS.
/// Create this once and reuse it for each frame.
pub struct IOSRenderContext {
    /// Windowed context for UI building
    pub windowed_ctx: WindowedContext,
    /// Render state for animations
    render_state: blinc_layout::RenderState,
    /// Render tree (created on first render)
    render_tree: Option<RenderTree>,
    /// Dirty flag for reactive updates
    ref_dirty_flag: RefDirtyFlag,
    /// Animation scheduler
    animations: SharedAnimationScheduler,
    /// Ready callbacks
    ready_callbacks: SharedReadyCallbacks,
    /// Wake proxy for animation thread
    wake_proxy: IOSWakeProxy,
    /// Number of rebuilds
    rebuild_count: u64,
    /// Touch tracking for scroll delta calculation
    /// Stores (x, y) of last touch position
    last_touch_pos: Option<(f32, f32)>,
    /// Whether currently scrolling (touch drag in progress)
    is_scrolling: bool,
    /// Gesture detector for touch gestures
    gesture_detector: GestureDetector,
    /// Last frame time for CSS animation delta calculation
    last_frame_time_ms: u64,
    /// Keyboard inset value applied last frame, used to detect changes so
    /// `scroll_focused_text_input_above_keyboard` only runs when the inset
    /// actually moves (otherwise we'd re-clamp the scroll offset every
    /// vsync tick, fighting any user pan).
    last_applied_keyboard_inset: f32,
    /// Last value of `text_input::focus_tap_generation()` we processed.
    /// The widget bumps that counter on every `on_mouse_down` that lands
    /// on a text input (or text area), regardless of whether the tap
    /// transitions focus state. We use it as a "user just tapped an
    /// input" signal that catches re-taps and same-frame focus swaps —
    /// the things `take_keyboard_state_change` misses because it only
    /// fires on `0 → 1` / `1 → 0` focus-count transitions.
    last_focus_tap_generation: u64,
}

impl IOSRenderContext {
    /// Check if a frame needs to be rendered
    ///
    /// Returns true if:
    /// - Reactive state changed (dirty flag)
    /// - Stateful elements need redraw (ButtonState changes, etc.)
    /// - Animations are active
    /// - Wake was requested by animation thread
    /// - A text-input long-press timer is armed (so the runner
    ///   tick polls `fire_long_press_timer_if_due` while the user
    ///   holds their finger still on a text input)
    pub fn needs_render(&self) -> bool {
        let dirty = self.ref_dirty_flag.load(Ordering::SeqCst);
        let wake_requested = self.wake_proxy.take_wake_request();
        let animations_active = self
            .animations
            .lock()
            .map(|sched| sched.has_active_animations())
            .unwrap_or(false);

        // Check if stateful elements need incremental updates (visual state changes)
        let has_stateful_updates = blinc_layout::peek_needs_redraw();
        let has_pending_rebuilds = blinc_layout::has_pending_subtree_rebuilds();

        // Check if CSS animations are active
        let css_animating = self
            .render_tree
            .as_ref()
            .map(|tree| !tree.css_animations_empty())
            .unwrap_or(false);

        // Long-press timer armed — the user is touching a text input
        // and waiting for the 500 ms long-press deadline to fire.
        // Without this, no events come in while the finger is still
        // and the timer never gets polled.
        let long_press_pending = blinc_layout::widgets::text_input::is_long_press_armed();

        dirty
            || wake_requested
            || animations_active
            || has_stateful_updates
            || has_pending_rebuilds
            || css_animating
            || long_press_pending
    }
    /// Update the window size
    ///
    /// Call this when the view's bounds change.
    /// `width` and `height` are in physical pixels.
    pub fn update_size(&mut self, width: u32, height: u32, scale_factor: f64) {
        let physical_width = width as f32;
        let physical_height = height as f32;
        let logical_width = physical_width / scale_factor as f32;
        let logical_height = physical_height / scale_factor as f32;

        // Only mark dirty if something actually changed
        let changed = (self.windowed_ctx.width - logical_width).abs() > 0.1
            || (self.windowed_ctx.height - logical_height).abs() > 0.1
            || (self.windowed_ctx.scale_factor - scale_factor).abs() > 0.001;

        self.windowed_ctx.width = logical_width;
        self.windowed_ctx.height = logical_height;
        self.windowed_ctx.physical_width = physical_width;
        self.windowed_ctx.physical_height = physical_height;
        self.windowed_ctx.scale_factor = scale_factor;

        BlincContextState::get().set_viewport_size(logical_width, logical_height);

        // Mark dirty to trigger rebuild with new dimensions
        if changed {
            tracing::debug!(
                "iOS update_size: {:.1}x{:.1} logical ({:.0}x{:.0} physical) @ {:.1}x scale",
                logical_width,
                logical_height,
                physical_width,
                physical_height,
                scale_factor
            );
            self.ref_dirty_flag.store(true, Ordering::SeqCst);
        }
    }

    /// Tick scroll physics - must be called every frame for scroll to work
    ///
    /// Returns true if scroll is animating and needs another frame.
    /// Call this before `build_ui` or `render_frame`.
    pub fn tick_scroll(&mut self) -> bool {
        if let Some(ref mut tree) = self.render_tree {
            let current_time = blinc_layout::prelude::elapsed_ms();
            let ticking = tree.tick_scroll_physics(current_time);
            // OR in `process_pending_scroll_refs`'s return so a freshly-fired
            // `scroll_to_with_options` keeps the redraw chain alive on its
            // own frame.
            let just_started = tree.process_pending_scroll_refs();
            ticking || just_started
        } else {
            false
        }
    }

    /// Build and layout the UI tree
    ///
    /// Call this before rendering each frame.
    pub fn build_ui<F, E>(&mut self, ui_builder: F)
    where
        F: FnOnce(&mut WindowedContext) -> E,
        E: ElementBuilder + 'static,
    {
        // Clear dirty flag
        self.ref_dirty_flag.swap(false, Ordering::SeqCst);

        // Tick scroll physics first
        self.tick_scroll();

        // Tick animations
        if let Ok(mut sched) = self.animations.lock() {
            sched.tick();
        }

        // Tick OverlayStack + ToastTray each frame so motion / TTL
        // state advances. Mirrors `windowed.rs:4937-4949` and the
        // matching block in `web.rs::run_one_frame`. Without this,
        // overlays pushed via `OverlayStack::present()` etc. never
        // animate in or auto-dismiss.
        let now = blinc_layout::prelude::elapsed_ms();
        {
            use blinc_layout::overlay_state::{overlay_stack, toast_tray};
            if let Ok(mut s) = overlay_stack().lock() {
                s.set_viewport_with_scale(
                    self.windowed_ctx.width,
                    self.windowed_ctx.height,
                    self.windowed_ctx.scale_factor as f32,
                );
                s.update(now);
            }
            if let Ok(mut t) = toast_tray().lock() {
                t.update(now);
            }
        }

        // Drain CSS queued by `ThemeBundle::with_css` →
        // `ThemeState::init` (and DSL/plugin `queue_stylesheet`
        // calls) before the tree is built so `cn_bundle()` styles
        // land on the first frame. Same pattern as
        // `windowed.rs:5228-5234`.
        {
            let queued = blinc_core::BlincContextState::get().drain_stylesheets();
            for css in queued {
                self.windowed_ctx.add_css(&css);
            }
        }

        // Build UI — compose the user UI with all overlay surfaces
        // (legacy + new OverlayStack + ToastTray). Without these
        // children the corresponding `.show()` calls would push
        // content into managers that never reach the tree.
        let element = {
            let user_ui = ui_builder(&mut self.windowed_ctx);
            let ctx = &mut self.windowed_ctx;
            let overlay_layer = ctx.overlay_manager.build_overlay_layer();
            let stack_layer = {
                use blinc_layout::overlay_state::overlay_stack;
                match overlay_stack().lock() {
                    Ok(mut s) => {
                        s.set_viewport_with_scale(ctx.width, ctx.height, ctx.scale_factor as f32);
                        s.build_overlay_layer()
                    }
                    Err(_) => blinc_layout::div::Div::new(),
                }
            };
            let viewport = (ctx.width, ctx.height);
            let tray_layer = {
                use blinc_layout::overlay_state::toast_tray;
                toast_tray()
                    .lock()
                    .ok()
                    .map(|t| t.build_tray_layer(viewport))
                    .unwrap_or_default()
            };
            blinc_layout::div::Div::new()
                .w(ctx.width)
                .h(ctx.height)
                .relative()
                .child(user_ui)
                .child(overlay_layer)
                .child(stack_layer)
                .child(tray_layer)
        };

        // Clear stale Stateful base_render_props updaters before rebuild
        blinc_layout::clear_stateful_base_updaters();
        blinc_layout::click_outside::clear_click_outside_handlers();

        // Create or update render tree
        if self.render_tree.is_none() {
            // First time: create tree
            tracing::debug!(
                "iOS build_ui: Creating tree with scale_factor={}, layout={:.1}x{:.1}",
                self.windowed_ctx.scale_factor,
                self.windowed_ctx.width,
                self.windowed_ctx.height
            );
            let mut tree = RenderTree::from_element(&element);
            tree.set_scale_factor(self.windowed_ctx.scale_factor as f32);
            if let Some(ref stylesheet) = self.windowed_ctx.stylesheet {
                tree.set_stylesheet_arc(stylesheet.clone());
            }
            tree.apply_all_stylesheet_styles();
            tree.compute_layout(self.windowed_ctx.width, self.windowed_ctx.height);
            tree.update_flip_bounds();
            tree.start_all_css_animations();
            self.render_tree = Some(tree);
        } else if let Some(ref mut tree) = self.render_tree {
            // Full rebuild
            tree.clear_dirty();
            *tree = RenderTree::from_element(&element);
            tree.set_scale_factor(self.windowed_ctx.scale_factor as f32);
            if let Some(ref stylesheet) = self.windowed_ctx.stylesheet {
                tree.set_stylesheet_arc(stylesheet.clone());
            }
            tree.apply_all_stylesheet_styles();
            tree.compute_layout(self.windowed_ctx.width, self.windowed_ctx.height);
            tree.update_flip_bounds();
            tree.start_all_css_animations();
        }

        // Tick and apply CSS animations/transitions synchronously with rendering
        if let Some(ref mut tree) = self.render_tree {
            let current_time = blinc_layout::prelude::elapsed_ms();
            let dt_ms = if self.last_frame_time_ms > 0 {
                (current_time - self.last_frame_time_ms) as f32
            } else {
                16.0
            };
            {
                let store = tree.css_anim_store();
                let mut s = store.lock().unwrap();
                s.tick(dt_ms);
            }
            let flip_active = tree.tick_flip_animations(dt_ms);
            let css_active = tree.css_has_active() || flip_active;
            if tree.stylesheet().is_some() {
                tree.apply_stylesheet_state_styles(&self.windowed_ctx.event_router);
            }
            if css_active || !tree.css_transitions_empty() || tree.has_active_flip_animations() {
                tree.apply_all_css_animation_props();
                tree.apply_all_css_transition_props();
                tree.apply_flip_animation_props();
                if tree.apply_animated_layout_props() {
                    tree.compute_layout(self.windowed_ctx.width, self.windowed_ctx.height);
                    tree.update_flip_bounds();
                }
            }
            self.last_frame_time_ms = current_time;
        }

        // Increment rebuild count
        self.rebuild_count += 1;

        // Execute ready callbacks after first rebuild
        if self.rebuild_count == 1 {
            if let Ok(mut callbacks) = self.ready_callbacks.lock() {
                for callback in callbacks.drain(..) {
                    callback();
                }
            }
        }
    }

    /// Get the render tree for rendering
    ///
    /// Returns None if build_ui hasn't been called yet.
    pub fn render_tree(&self) -> Option<&RenderTree> {
        self.render_tree.as_ref()
    }

    /// Get the render state for motion animations
    pub fn render_state(&self) -> &blinc_layout::RenderState {
        &self.render_state
    }

    /// Handle text input from the soft keyboard.
    ///
    /// Broadcasts a `TEXT_INPUT` event for each character in
    /// `text` to all focused text-input handlers in the tree.
    /// The handlers internally check `is_focused()` so the event
    /// only lands on the active text widget.
    ///
    /// Called from Swift via the `blinc_ios_handle_text_input`
    /// FFI when `BlincKeyboardHelper`'s hidden `UITextField`
    /// captures a keystroke through its
    /// `shouldChangeCharactersIn` delegate. Without this path
    /// the iOS soft keyboard pops up correctly but typed
    /// characters never reach the Rust text-input widget — the
    /// keyboard is purely visual.
    pub fn handle_text_input(&mut self, text: &str) {
        let tree = match &mut self.render_tree {
            Some(t) => t,
            None => {
                tracing::debug!("[Blinc] iOS handle_text_input: no render tree");
                return;
            }
        };
        for c in text.chars() {
            tree.broadcast_text_input_event(c, false, false, false, false);
        }
    }

    /// Handle a key-down event from the soft keyboard.
    ///
    /// Used for non-character keys the iOS keyboard sends
    /// (Backspace, Return, …). The `shouldChangeCharactersIn`
    /// delegate detects backspace via `range.length > 0 &&
    /// string.isEmpty`; the Swift side then calls
    /// `blinc_ios_handle_key_down(ctx, 8)` which maps to the
    /// same `key_code = 8` desktop dispatches for the Backspace
    /// key.
    pub fn handle_key_down(&mut self, key_code: u32) {
        self.handle_key_down_with_modifiers(key_code, 0);
    }

    /// Handle a key-down event with explicit modifier flags.
    ///
    /// Same as [`handle_key_down`] but lets the caller mark the event
    /// as Cmd/Ctrl/Alt/Shift held. The native edit menu uses this to
    /// dispatch synthesized `Cmd+X / Cmd+C / Cmd+V / Cmd+A` events when
    /// the user picks Cut / Copy / Paste / Select All from the
    /// `UIMenuController` — those land in the existing Cmd-shortcut
    /// branch of every Blinc text-editable widget's `on_key_down`
    /// handler, which already handles clipboard ops and select-all.
    ///
    /// `modifiers` is a bitmask:
    ///   - bit 0 (0x01): shift
    ///   - bit 1 (0x02): ctrl
    ///   - bit 2 (0x04): alt
    ///   - bit 3 (0x08): meta (Cmd on macOS, Win on Windows)
    pub fn handle_key_down_with_modifiers(&mut self, key_code: u32, modifiers: u32) {
        let tree = match &mut self.render_tree {
            Some(t) => t,
            None => {
                tracing::debug!("[Blinc] iOS handle_key_down: no render tree");
                return;
            }
        };
        let shift = modifiers & 0x01 != 0;
        let ctrl = modifiers & 0x02 != 0;
        let alt = modifiers & 0x04 != 0;
        let meta = modifiers & 0x08 != 0;
        tree.broadcast_key_event(
            blinc_core::events::event_types::KEY_DOWN,
            key_code,
            shift,
            ctrl,
            alt,
            meta,
        );
    }

    /// Update the soft-keyboard inset (height in logical points / pixels).
    ///
    /// Pushed from `BlincKeyboardHelper` in
    /// `BlincNativeBridge.swift` whenever UIKit posts a
    /// `UIKeyboardWillChangeFrameNotification`. The Swift side already
    /// intersects the keyboard frame with the key window's bounds and
    /// converts to UIKit points (which equal Blinc's logical pixels), so
    /// this method just stashes the value on the shared `WindowedContext`
    /// and triggers a redraw so the next-frame layout pass picks it up.
    ///
    /// On hide events the Swift side computes a zero intersection and
    /// passes `0.0`, which collapses the inset and lets the UI return
    /// to its full-viewport layout.
    pub fn handle_keyboard_inset(&mut self, inset: f32) {
        let clamped = inset.max(0.0);
        if (self.windowed_ctx.keyboard_inset - clamped).abs() < 0.5 {
            // Sub-pixel diff — ignore. Avoids redraw spam during
            // mid-animation `WillChangeFrame` events that fire every
            // ~16 ms while the keyboard slides in.
            return;
        }
        tracing::debug!(
            "[Blinc] iOS keyboard inset: {} -> {}",
            self.windowed_ctx.keyboard_inset,
            clamped
        );
        self.windowed_ctx.keyboard_inset = clamped;
        // The frame loop polls `take_keyboard_state_change` and other
        // dirty flags every tick on iOS, so the next vsync tick picks
        // this up automatically.
    }

    /// Handle a touch event
    ///
    /// Call this from your UIView's touch handling methods.
    /// Touch coordinates should be in logical points (not physical pixels).
    ///
    /// # Example (Swift)
    ///
    /// ```swift
    /// override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
    ///     for touch in touches {
    ///         let point = touch.location(in: self)
    ///         blinc_handle_touch(context, 0, Float(point.x), Float(point.y), 0) // 0 = began
    ///     }
    /// }
    /// ```
    pub fn handle_touch(&mut self, touch: blinc_platform_ios::Touch) {
        use blinc_layout::tree::LayoutNodeId;

        // Pending event structure for deferred dispatch
        #[derive(Clone, Default)]
        struct PendingEvent {
            node_id: LayoutNodeId,
            event_type: u32,
        }

        let gesture = self.gesture_detector.process(&touch);
        let active_touches = self.gesture_detector.active_touch_count();

        // Forward touch force/pressure and touch count to pointer query
        match touch.phase {
            TouchPhase::Began | TouchPhase::Moved => {
                self.windowed_ctx.pointer_query.set_pressure(touch.force);
                self.windowed_ctx
                    .pointer_query
                    .set_touch_count(active_touches as u32);
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.windowed_ctx.pointer_query.set_pressure(0.0);
                self.windowed_ctx
                    .pointer_query
                    .set_touch_count(active_touches as u32);
            }
        }

        let tree = match &self.render_tree {
            Some(t) => t,
            None => {
                tracing::debug!("[Blinc] iOS handle_touch: No render tree yet, ignoring touch");
                return;
            }
        };

        // Touch coordinates are already in logical points on iOS
        let lx = touch.x;
        let ly = touch.y;

        // Log tree info for debugging
        if let Some(root) = tree.root() {
            if let Some(bounds) = tree.layout().get_bounds(root, (0.0, 0.0)) {
                tracing::trace!(
                    "[Blinc] iOS Touch at ({:.1}, {:.1}) - tree root bounds: ({:.1}, {:.1}, {:.1}x{:.1})",
                    lx,
                    ly,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height
                );
            } else {
                tracing::debug!("[Blinc] iOS Touch: tree root has no bounds!");
            }
        } else {
            tracing::debug!("[Blinc] iOS Touch: tree has no root!");
        }

        // Collect pending events via callback
        let mut pending_events: Vec<PendingEvent> = Vec::new();

        // Set up callback to collect events
        self.windowed_ctx.event_router.set_event_callback({
            let events = &mut pending_events as *mut Vec<PendingEvent>;
            move |node, event_type| {
                // SAFETY: This callback is only used within this scope
                unsafe {
                    (*events).push(PendingEvent {
                        node_id: node,
                        event_type,
                    });
                }
            }
        });

        // Track scroll info for dispatch after regular event handling
        let mut scroll_info: Option<(f32, f32, f32, f32)> = None;
        let mut touch_ended = false;

        // Route touch event through event router
        match touch.phase {
            TouchPhase::Began => {
                tracing::trace!("[Blinc] iOS Touch BEGAN at ({:.1}, {:.1})", lx, ly);
                // Mark this event as touch input so editable widgets
                // can branch their drag / double-tap logic for mobile
                // semantics (drag = move cursor + haptic, double-tap
                // = native edit menu). Sticky between events; the
                // desktop / web runners flip this back to false on
                // mouse_down. See `widgets::text_input::is_touch_input`.
                blinc_layout::widgets::text_input::set_touch_input(true);
                // Blur any focused text inputs BEFORE processing
                // mouse down. Mirrors the desktop runner's
                // behavior at [`windowed.rs:2913`](crate::windowed):
                // tapping anywhere globally clears focus, and the
                // text input that gets tapped re-focuses itself
                // via its own `on_mouse_down` handler. The
                // resulting focus-count drop fires
                // `take_keyboard_state_change()` on the next
                // frame, which the runner forwards to
                // `blinc_ios_hide_keyboard` — so tapping outside
                // an input also dismisses the soft keyboard.
                blinc_layout::widgets::blur_all_text_inputs();
                self.windowed_ctx
                    .event_router
                    .on_mouse_down(tree, lx, ly, MouseButton::Left);
                // Initialize touch tracking for scroll
                if active_touches == 1 {
                    self.last_touch_pos = Some((lx, ly));
                    self.is_scrolling = false;
                } else {
                    self.last_touch_pos = None;
                    self.is_scrolling = false;
                }
            }
            TouchPhase::Moved => {
                self.windowed_ctx.event_router.on_mouse_move(tree, lx, ly);

                // Calculate scroll delta from touch movement
                // Touch: dragging down = positive delta = content scrolls up (shows below)
                if active_touches == 1 {
                    if let Some((prev_x, prev_y)) = self.last_touch_pos {
                        let delta_x = lx - prev_x;
                        let delta_y = ly - prev_y;

                        // Only dispatch scroll if there's actual movement
                        // Small threshold to avoid jitter
                        if delta_x.abs() > 0.5 || delta_y.abs() > 0.5 {
                            self.is_scrolling = true;
                            // Store scroll info for dispatch after event loop
                            scroll_info = Some((lx, ly, delta_x, delta_y));
                            tracing::trace!("Touch scroll: delta=({:.1}, {:.1})", delta_x, delta_y);
                        }
                    }

                    // Update last touch position
                    self.last_touch_pos = Some((lx, ly));
                } else {
                    self.last_touch_pos = None;
                    self.is_scrolling = false;
                }
            }
            TouchPhase::Ended => {
                tracing::trace!("[Blinc] iOS Touch ENDED at ({:.1}, {:.1})", lx, ly);
                // Cancel any armed text-input long-press timer.
                // Lifting the finger before the 500 ms deadline
                // means the user wasn't trying to long-press.
                blinc_layout::widgets::text_input::cancel_long_press_timer();
                self.windowed_ctx
                    .event_router
                    .on_mouse_up(tree, lx, ly, MouseButton::Left);
                // On touch devices, finger lift means pointer leaves too
                // This transitions ButtonState from Hovered back to Idle
                self.windowed_ctx.event_router.on_mouse_leave(tree);

                // Mark touch ended for scroll physics
                if self.is_scrolling {
                    touch_ended = true;
                }
                // Clear touch tracking
                self.last_touch_pos = None;
                self.is_scrolling = false;
            }
            TouchPhase::Cancelled => {
                tracing::trace!("[Blinc] iOS Touch CANCELLED");
                blinc_layout::widgets::text_input::cancel_long_press_timer();
                self.windowed_ctx.event_router.on_mouse_leave(tree);
                // Clear touch tracking on cancel too
                self.last_touch_pos = None;
                if self.is_scrolling {
                    touch_ended = true;
                }
                self.is_scrolling = false;
            }
        }

        // Clear callback
        self.windowed_ctx.event_router.clear_event_callback();

        tracing::trace!(
            "[Blinc] iOS Touch: collected {} pending events",
            pending_events.len()
        );

        // Dispatch collected events to the tree
        if !pending_events.is_empty() {
            tracing::trace!("[Blinc] iOS dispatching {} events", pending_events.len());

            if let Some(ref mut tree) = self.render_tree {
                let router = &self.windowed_ctx.event_router;
                for event in pending_events {
                    // Get bounds for local coordinate calculation
                    let (bounds_x, bounds_y, bounds_width, bounds_height) = router
                        .get_node_bounds(event.node_id)
                        .unwrap_or((0.0, 0.0, 0.0, 0.0));
                    let local_x = lx - bounds_x;
                    let local_y = ly - bounds_y;

                    tree.dispatch_event_full(
                        event.node_id,
                        event.event_type,
                        lx,
                        ly,
                        local_x,
                        local_y,
                        bounds_x,
                        bounds_y,
                        bounds_width,
                        bounds_height,
                        0.0,   // drag_delta_x
                        0.0,   // drag_delta_y
                        1.0,   // pinch_scale
                        false, // shift
                        false, // ctrl
                        false, // alt
                        false, // meta
                    );
                }
            }
            // Stateful elements will call request_redraw() internally when state changes
            // The needs_render() check will pick this up for the next frame
        }

        if let Some(Gesture::Pinch { scale, center }) = gesture {
            if let Some(ref mut tree) = self.render_tree {
                let router = &mut self.windowed_ctx.event_router;
                if let Some(hit) = router.hit_test(tree, center.0, center.1) {
                    tree.dispatch_pinch_chain(&hit, center.0, center.1, scale);
                    self.wake_proxy.wake();
                }
            }
        }

        // Dispatch scroll events (touch scrolling)
        // NOTE: Do NOT set ref_dirty_flag here - that triggers full UI rebuild!
        // Scroll just updates internal offset and needs redraw, not rebuild.
        // The wake_proxy and animation system handle continuous redraw.
        if let Some((mouse_x, mouse_y, delta_x, delta_y)) = scroll_info {
            if let Some(ref mut tree) = self.render_tree {
                let router = &mut self.windowed_ctx.event_router;
                // Hit test to get node chain for nested scroll dispatch
                if let Some(hit) = router.hit_test(tree, mouse_x, mouse_y) {
                    tracing::debug!(
                        "Dispatching scroll: hit={:?}, delta=({:.1}, {:.1})",
                        hit.node,
                        delta_x,
                        delta_y
                    );
                    tree.dispatch_scroll_chain(
                        hit.node,
                        &hit.ancestors,
                        mouse_x,
                        mouse_y,
                        delta_x,
                        delta_y,
                    );
                    // Wake to trigger redraw (NOT rebuild)
                    self.wake_proxy.wake();
                }
            }
        }

        // Handle touch end - notify scroll physics for bounce/momentum
        if touch_ended {
            if let Some(ref mut tree) = self.render_tree {
                tracing::debug!("Touch ended - notifying scroll physics");
                tree.on_scroll_end();
                // Wake to trigger redraw for bounce animation (NOT rebuild)
                self.wake_proxy.wake();
            }
        }
    }

    /// Set focus state
    pub fn set_focused(&mut self, focused: bool) {
        self.windowed_ctx.focused = focused;
    }
}

// =============================================================================
// Rust UI Builder Registration
// =============================================================================

use std::sync::OnceLock;

/// Type for Rust UI builder function that directly creates/updates the render tree
type RustUIBuilder =
    Box<dyn Fn(&mut WindowedContext, Option<&mut RenderTree>) -> RenderTree + Send + Sync>;

/// Global storage for Rust UI builder
static RUST_UI_BUILDER: OnceLock<RustUIBuilder> = OnceLock::new();

/// Register a Rust UI builder function
///
/// This is called from the example app's iOS entry point to register
/// the UI builder closure. The closure should return an ElementBuilder,
/// which will be converted to a RenderTree.
///
/// # Example
///
/// ```ignore
/// #[cfg(target_os = "ios")]
/// #[no_mangle]
/// pub extern "C" fn ios_app_init() {
///     blinc_app::ios::register_rust_ui_builder(|ctx| {
///         my_app_ui(ctx)
///     });
/// }
/// ```
pub fn register_rust_ui_builder<F, E>(builder: F)
where
    F: Fn(&mut WindowedContext) -> E + Send + Sync + 'static,
    E: ElementBuilder + 'static,
{
    let boxed_builder: RustUIBuilder = Box::new(move |ctx, _existing_tree| {
        blinc_layout::clear_stateful_base_updaters();
        blinc_layout::click_outside::clear_click_outside_handlers();
        let element = builder(ctx);
        let mut tree = RenderTree::from_element(&element);
        tree.set_scale_factor(ctx.scale_factor as f32);
        if let Some(ref stylesheet) = ctx.stylesheet {
            tree.set_stylesheet_arc(stylesheet.clone());
        }
        tree.apply_all_stylesheet_styles();
        tree.compute_layout(ctx.width, ctx.height);
        tree.update_flip_bounds();
        tree.start_all_css_animations();
        tree
    });
    let _ = RUST_UI_BUILDER.set(boxed_builder);
}

/// Get the registered Rust UI builder
fn get_rust_ui_builder() -> Option<&'static RustUIBuilder> {
    RUST_UI_BUILDER.get()
}

// =============================================================================
// C FFI for Swift/Objective-C Integration
// =============================================================================

/// Type alias for UI builder function pointer
///
/// The function receives the WindowedContext pointer and should build/update the UI.
/// It's called each frame when rendering is needed.
///
/// Example Rust implementation:
/// ```ignore
/// #[no_mangle]
/// pub extern "C" fn my_app_build_ui(ctx: *mut WindowedContext) {
///     if ctx.is_null() { return; }
///     let ctx = unsafe { &mut *ctx };
///     // Use ctx.width, ctx.height, etc. to build UI
/// }
/// ```
pub type UIBuilderFn = extern "C" fn(ctx: *mut WindowedContext);

/// Stored UI builder for FFI
static mut UI_BUILDER: Option<UIBuilderFn> = None;

/// Register a UI builder function (C FFI for Swift/Rust interop)
///
/// The builder function will be called each frame to build the UI.
/// Call this once during initialization before any rendering.
///
/// # Safety
/// The function pointer must remain valid for the lifetime of the application.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_set_ui_builder(builder: UIBuilderFn) {
    unsafe {
        UI_BUILDER = Some(builder);
    }
}

/// Get the registered UI builder (internal use)
fn get_ui_builder() -> Option<UIBuilderFn> {
    unsafe { UI_BUILDER }
}

/// Build a frame using the registered UI builder (C FFI for Swift)
///
/// This handles both incremental updates (prop changes, subtree rebuilds) and
/// full rebuilds. Call this each frame when blinc_needs_render() is true.
///
/// The function:
/// 1. First processes incremental updates (prop changes from stateful elements)
/// 2. Only does a full rebuild if the dirty flag is set (State::set_rebuild)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
/// A UI builder must have been registered via `blinc_set_ui_builder` or `register_rust_ui_builder`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_build_frame(ctx: *mut IOSRenderContext) {
    if ctx.is_null() {
        return;
    }

    unsafe {
        let ctx = &mut *ctx;

        // Tick animations
        if let Ok(sched) = ctx.animations.lock() {
            sched.tick();
        }

        // Soft keyboard: show/hide based on text widget focus.
        //
        // The implementation lives in `BlincNativeBridge.swift`
        // (at `extensions/blinc_platform_ios/templates/`), which
        // user iOS apps copy into their Xcode project on init.
        // Apps that don't include the bridge — common during
        // bring-up, or for headless / read-only apps that don't
        // need text input — would otherwise get a hard linker
        // error from the strong `extern "C"` reference:
        //
        //     Undefined symbols for architecture arm64:
        //       "_blinc_ios_show_keyboard", referenced from:
        //         _blinc_build_frame in libblinc...
        //       "_blinc_ios_hide_keyboard", referenced from:
        //         _blinc_build_frame in libblinc...
        //
        // To keep the rlib self-linkable regardless of whether
        // the user copied the Swift template, we resolve the
        // symbols at *runtime* via `dlsym(RTLD_DEFAULT, ...)`.
        // The rlib has no link-time dependency on the Swift
        // bridge — only on libc's `dlsym`, which is always
        // present on iOS — and the lookup is cached in a
        // `OnceLock` so the cost is paid exactly once per
        // process. If the symbol is found, we call it; if not,
        // we silently no-op (text input still works at the
        // model level, the soft keyboard just doesn't pop up).
        // Show / hide the soft keyboard when focus crosses the global
        // 0 / 1 boundary. `take_keyboard_state_change` returns
        // `Some(true)` on the first text input gaining focus and
        // `Some(false)` when the last focused input loses it; it does
        // NOT fire on re-taps or focus-swaps between two inputs.
        // That's fine for show/hide signaling — the keyboard is
        // already up in those cases.
        if let Some(show) = blinc_layout::widgets::text_input::take_keyboard_state_change() {
            if show {
                if let Some(f) = keyboard_show_fn() {
                    f();
                }
            } else if let Some(f) = keyboard_hide_fn() {
                f();
            }
        }

        // Long-press timer poll. Editable widgets arm this from
        // their `on_mouse_down` handlers when `is_touch_input()`
        // is true; the user holding their finger still for 500 ms
        // (with no drift past 10 px) fires the helper, which calls
        // `show_edit_menu` with PASTE available — matching the iOS
        // UITextField long-press-to-paste UX. The timer is
        // cancelled by `on_drag` drift detection (above) and by
        // the `TouchPhase::Ended` / `Cancelled` handlers in
        // `handle_touch`.
        blinc_layout::widgets::text_input::fire_long_press_timer_if_due();

        // Soft-keyboard inset → scroll the focused text input above the
        // keyboard so the user can see what they're typing. Driven by
        // `WindowedContext.keyboard_inset`, which is updated by Swift via
        // `blinc_ios_set_keyboard_inset` whenever UIKit posts
        // `UIKeyboardWillChangeFrameNotification`.
        //
        // We need TWO independent triggers because the focus-count
        // signal `take_keyboard_state_change` is not enough on its own:
        //
        //   1. **Inset change** — keyboard slides in or out, hardware
        //      keyboard attach / detach. Caught by diffing
        //      `current_inset` against `last_applied_keyboard_inset`.
        //
        //   2. **Tap-on-text-input generation bump** — the user tapped
        //      a text input (any of them, including re-tapping the
        //      same one) and we should re-evaluate whether the focused
        //      input is currently obscured. `take_keyboard_state_change`
        //      misses this because it only fires on `0 → 1` / `1 → 0`
        //      focus-count transitions; re-tapping a focused input
        //      stays at count = 1 the whole time. We use a separate
        //      `focus_tap_generation` counter that bumps in the
        //      `text_input` widget's `on_mouse_down` handler.
        let current_inset = ctx.windowed_ctx.keyboard_inset;
        let current_tap_gen = blinc_layout::widgets::text_input::focus_tap_generation();
        let inset_changed = (current_inset - ctx.last_applied_keyboard_inset).abs() > 0.5;
        let tap_changed = current_tap_gen != ctx.last_focus_tap_generation;
        let needs_scroll_pass = inset_changed || (tap_changed && current_inset > 0.0);

        if needs_scroll_pass {
            let viewport_h = ctx.windowed_ctx.height;
            if let Some(ref mut tree) = ctx.render_tree {
                let scrolled =
                    tree.scroll_focused_text_input_above_keyboard(viewport_h, current_inset);
                if scrolled {
                    // Force a redraw on the next frame so the new
                    // scroll offset is reflected in the rendered output.
                    blinc_layout::request_redraw();
                }
            }
            ctx.last_applied_keyboard_inset = current_inset;
            ctx.last_focus_tap_generation = current_tap_gen;
        }

        // PHASE 1: Process incremental updates (prop changes, subtree rebuilds)
        // This avoids full rebuild for simple state changes like ButtonState
        let has_stateful_updates = blinc_layout::take_needs_redraw();
        let has_pending_rebuilds = blinc_layout::has_pending_subtree_rebuilds();
        let has_prop_updates = blinc_layout::has_pending_partial_prop_updates();

        if has_stateful_updates || has_pending_rebuilds || has_prop_updates {
            // Drain the unified property channel
            // ([[project-reactive-architecture-v2]]).
            let prop_updates = blinc_layout::take_pending_partial_prop_updates();
            let mut prop_effects = blinc_layout::SideEffects::default();
            if let Some(ref mut tree) = ctx.render_tree {
                for upd in prop_updates {
                    prop_effects = prop_effects.or(upd.effects);
                    if let Some(write) = upd.render_write {
                        tree.update_render_props(upd.node_id, |p| write(p));
                    }
                    if let Some(write) = upd.layout_write {
                        if let Some(mut style) = tree.layout_tree.get_style(upd.node_id) {
                            write(&mut style);
                            tree.layout_tree.set_style(upd.node_id, style);
                        }
                    }
                }
            }

            // Process subtree rebuilds
            let mut needs_layout = prop_effects.needs_layout;
            if let Some(ref mut tree) = ctx.render_tree {
                needs_layout |= tree.process_pending_subtree_rebuilds();
            }

            if needs_layout {
                if let Some(ref mut tree) = ctx.render_tree {
                    tree.apply_stylesheet_layout_overrides();
                    tree.compute_layout(ctx.windowed_ctx.width, ctx.windowed_ctx.height);
                    tree.apply_flip_transitions();
                    tree.update_flip_bounds();
                }
            }
        }

        // PHASE 2: Check if full rebuild is needed
        let needs_rebuild = ctx.ref_dirty_flag.swap(false, Ordering::SeqCst);
        let no_tree_yet = ctx.render_tree.is_none();

        if !needs_rebuild && !no_tree_yet {
            // No full rebuild needed - incremental updates already applied
            return;
        }

        // PHASE 3: Full rebuild using UI builder (required on first load or when dirty)
        if let Some(rust_builder) = get_rust_ui_builder() {
            // The builder creates the RenderTree for us
            let tree = rust_builder(&mut ctx.windowed_ctx, ctx.render_tree.as_mut());
            ctx.render_tree = Some(tree);
        } else if let Some(builder) = get_ui_builder() {
            builder(&mut ctx.windowed_ctx as *mut WindowedContext);
        }
    }
}

/// Create an iOS render context (C FFI for Swift)
///
/// # Arguments
/// * `width` - Physical width in pixels
/// * `height` - Physical height in pixels
/// * `scale_factor` - Display scale factor (UIScreen.scale)
///
/// # Returns
/// Pointer to the render context, or null on failure
///
/// # Safety
/// The returned pointer must be freed with `blinc_destroy_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_create_context(
    width: u32,
    height: u32,
    scale_factor: f64,
) -> *mut IOSRenderContext {
    match IOSApp::create_context(width, height, scale_factor) {
        Ok(ctx) => Box::into_raw(Box::new(ctx)),
        Err(e) => {
            tracing::error!("Failed to create iOS render context: {}", e);
            std::ptr::null_mut()
        }
    }
}

/// Check if a frame needs to be rendered (C FFI for Swift)
///
/// Returns true if reactive state changed, animations are active,
/// or a wake was requested.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_needs_render(ctx: *mut IOSRenderContext) -> bool {
    if ctx.is_null() {
        return false;
    }
    unsafe { (*ctx).needs_render() }
}

/// Update the window size (C FFI for Swift)
///
/// Call this when the view's bounds change.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_update_size(
    ctx: *mut IOSRenderContext,
    width: u32,
    height: u32,
    scale_factor: f64,
) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).update_size(width, height, scale_factor);
    }
}

/// Handle a touch event (C FFI for Swift)
///
/// # Arguments
/// * `ctx` - Render context pointer
/// * `touch_id` - Unique touch identifier (from UITouch)
/// * `x` - X position in logical points
/// * `y` - Y position in logical points
/// * `phase` - Touch phase: 0=began, 1=moved, 2=ended, 3=cancelled
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_handle_touch(
    ctx: *mut IOSRenderContext,
    touch_id: u64,
    x: f32,
    y: f32,
    phase: i32,
) {
    tracing::trace!(
        "[Blinc FFI] blinc_handle_touch called: x={}, y={}, phase={}",
        x,
        y,
        phase
    );

    if ctx.is_null() {
        tracing::debug!("[Blinc FFI] blinc_handle_touch: ctx is NULL!");
        return;
    }

    let touch_phase = match phase {
        0 => TouchPhase::Began,
        1 => TouchPhase::Moved,
        2 => TouchPhase::Ended,
        _ => TouchPhase::Cancelled,
    };

    let touch = blinc_platform_ios::Touch::new(touch_id, x, y, touch_phase);
    unsafe {
        (*ctx).handle_touch(touch);
    }
    tracing::trace!("[Blinc FFI] blinc_handle_touch completed");
}

/// Forward characters typed on the iOS soft keyboard to the
/// focused text-input widget.
///
/// Called from Swift's `BlincKeyboardHelper.shouldChangeCharactersIn`
/// delegate every time the user types a character. The text is
/// a UTF-8 C string (NUL-terminated). For backspace, see
/// [`blinc_ios_handle_key_down`] — UITextField reports
/// deletions as `(range.length > 0, replacementString.isEmpty)`,
/// which Swift detects and forwards via the key-down path
/// instead.
///
/// # Arguments
/// * `ctx` - Render context pointer
/// * `text` - UTF-8 NUL-terminated string with the typed
///   character(s). Almost always a single character, but a
///   multi-character payload (e.g. autocorrect insertion) is
///   handled by broadcasting one TEXT_INPUT event per char.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
/// `text` must be a valid NUL-terminated UTF-8 string for the
/// duration of the call.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_ios_handle_text_input(
    ctx: *mut IOSRenderContext,
    text: *const std::os::raw::c_char,
) {
    if ctx.is_null() || text.is_null() {
        return;
    }
    let c_str = unsafe { std::ffi::CStr::from_ptr(text) };
    let text = match c_str.to_str() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("[Blinc FFI] blinc_ios_handle_text_input: invalid UTF-8: {e}");
            return;
        }
    };
    unsafe {
        (*ctx).handle_text_input(text);
    }
}

/// Forward a key-down event from the iOS soft keyboard.
///
/// Used for non-character keys the keyboard sends — primarily
/// Backspace (key code 8) and Return (key code 13). Swift's
/// `shouldChangeCharactersIn` detects backspace via
/// `range.length > 0 && replacementString.isEmpty` and calls
/// this with `key_code = 8`. Return is detected via the
/// `textFieldShouldReturn` delegate.
///
/// Key codes match the desktop runner's table at
/// [`windowed.rs:3052`](crate::windowed) (8 = Backspace,
/// 13 = Enter, 27 = Escape, 37/39 = ←/→, …) so the same
/// `text_input` widget handlers fire on every platform.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_ios_handle_key_down(ctx: *mut IOSRenderContext, key_code: u32) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).handle_key_down(key_code);
    }
}

/// Forward a key-down event with explicit modifier flags.
///
/// Same as [`blinc_ios_handle_key_down`] but lets the Swift caller mark
/// the event as Cmd / Ctrl / Alt / Shift held. The native edit menu
/// uses this to dispatch synthesized `Cmd+X / Cmd+C / Cmd+V / Cmd+A`
/// events when the user picks Cut / Copy / Paste / Select All from
/// `UIMenuController` — the modifier bits route the event into the
/// existing Cmd-shortcut branch of every Blinc text-editable widget's
/// `on_key_down` handler, which already handles clipboard ops and
/// select-all.
///
/// `modifiers` is a bitmask:
///   - bit 0 (0x01): shift
///   - bit 1 (0x02): ctrl
///   - bit 2 (0x04): alt
///   - bit 3 (0x08): meta (Cmd on macOS)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_ios_handle_key_down_with_modifiers(
    ctx: *mut IOSRenderContext,
    key_code: u32,
    modifiers: u32,
) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).handle_key_down_with_modifiers(key_code, modifiers);
    }
}

/// Update the keyboard inset (height of the soft keyboard) in
/// logical points / pixels.
///
/// Pushed from `BlincKeyboardHelper` in `BlincNativeBridge.swift`
/// whenever UIKit posts a `UIKeyboardWillChangeFrameNotification`
/// or `UIKeyboardWillHideNotification`. Swift already intersects
/// the keyboard's reported screen frame with the key window's
/// bounds and converts to UIKit points, so the value here is
/// directly comparable to `WindowedContext.height`. Pass `0.0`
/// when the keyboard is hidden.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_ios_set_keyboard_inset(ctx: *mut IOSRenderContext, inset: f32) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).handle_keyboard_inset(inset);
    }
}

/// Handle a touch event with force/pressure (C FFI for Swift)
///
/// Same as `blinc_handle_touch` but includes force pressure from 3D Touch / Haptic Touch.
/// Use this for pressure-sensitive interactions via `env(pointer-pressure)` in CSS.
///
/// # Arguments
/// * `ctx` - Render context pointer
/// * `touch_id` - Unique touch identifier
/// * `x` - X position in logical points
/// * `y` - Y position in logical points
/// * `phase` - Touch phase: 0=began, 1=moved, 2=ended, 3=cancelled
/// * `force` - Normalized force (0.0-1.0), from `UITouch.force / UITouch.maximumPossibleForce`
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_handle_touch_with_force(
    ctx: *mut IOSRenderContext,
    touch_id: u64,
    x: f32,
    y: f32,
    phase: i32,
    force: f32,
) {
    if ctx.is_null() {
        return;
    }

    let touch_phase = match phase {
        0 => TouchPhase::Began,
        1 => TouchPhase::Moved,
        2 => TouchPhase::Ended,
        _ => TouchPhase::Cancelled,
    };

    let touch = blinc_platform_ios::Touch::with_force(touch_id, x, y, touch_phase, force);
    unsafe {
        (*ctx).handle_touch(touch);
    }
}

/// Set the focus state (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_set_focused(ctx: *mut IOSRenderContext, focused: bool) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).set_focused(focused);
    }
}

/// Destroy the render context (C FFI for Swift)
///
/// Frees all resources associated with the context.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`,
/// and must not be used after this call.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_destroy_context(ctx: *mut IOSRenderContext) {
    if !ctx.is_null() {
        unsafe {
            drop(Box::from_raw(ctx));
        }
    }
}

/// Tick animations (C FFI for Swift)
///
/// Call this each frame before building UI. Returns true if any animations
/// are active (meaning you should continue rendering).
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_tick_animations(ctx: *mut IOSRenderContext) -> bool {
    if ctx.is_null() {
        return false;
    }
    unsafe {
        let ctx = &mut *ctx;
        let motion_active = if let Ok(mut sched) = ctx.animations.lock() {
            sched.tick()
        } else {
            false
        };

        // Tick and apply CSS animations/transitions synchronously with rendering
        let css_active = if let Some(ref mut tree) = ctx.render_tree {
            let current_time = blinc_layout::prelude::elapsed_ms();
            let dt_ms = if ctx.last_frame_time_ms > 0 {
                (current_time - ctx.last_frame_time_ms) as f32
            } else {
                16.0
            };
            {
                let store = tree.css_anim_store();
                let mut s = store.lock().unwrap();
                s.tick(dt_ms);
            }
            let flip_active = tree.tick_flip_animations(dt_ms);
            let active = tree.css_has_active() || flip_active;
            if tree.stylesheet().is_some() {
                tree.apply_stylesheet_state_styles(&ctx.windowed_ctx.event_router);
            }
            if active || !tree.css_transitions_empty() || tree.has_active_flip_animations() {
                tree.apply_all_css_animation_props();
                tree.apply_all_css_transition_props();
                tree.apply_flip_animation_props();
                if tree.apply_animated_layout_props() {
                    tree.compute_layout(ctx.windowed_ctx.width, ctx.windowed_ctx.height);
                    tree.update_flip_bounds();
                }
            }
            ctx.last_frame_time_ms = current_time;
            active
        } else {
            false
        };

        motion_active || css_active
    }
}

/// Get the logical width for UI layout (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_get_width(ctx: *mut IOSRenderContext) -> f32 {
    if ctx.is_null() {
        return 0.0;
    }
    unsafe { (*ctx).windowed_ctx.width }
}

/// Get the logical height for UI layout (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_get_height(ctx: *mut IOSRenderContext) -> f32 {
    if ctx.is_null() {
        return 0.0;
    }
    unsafe { (*ctx).windowed_ctx.height }
}

/// Get the scale factor (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_get_scale_factor(ctx: *mut IOSRenderContext) -> f64 {
    if ctx.is_null() {
        return 1.0;
    }
    unsafe { (*ctx).windowed_ctx.scale_factor }
}

/// Get a pointer to the WindowedContext for UI building (C FFI for Swift)
///
/// Use this to pass to a Rust UI builder function.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
/// The returned pointer is only valid while `ctx` is valid.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_get_windowed_context(ctx: *mut IOSRenderContext) -> *mut WindowedContext {
    if ctx.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { &mut (*ctx).windowed_ctx as *mut WindowedContext }
}

/// Mark the context as needing a rebuild (C FFI for Swift)
///
/// Call this when external state changes that should trigger a UI update.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_mark_dirty(ctx: *mut IOSRenderContext) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).ref_dirty_flag.store(true, Ordering::SeqCst);
    }
}

/// Clear the dirty flag (C FFI for Swift)
///
/// Call this after processing a rebuild.
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_clear_dirty(ctx: *mut IOSRenderContext) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        (*ctx).ref_dirty_flag.store(false, Ordering::SeqCst);
    }
}

/// Get the physical width in pixels (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_get_physical_width(ctx: *mut IOSRenderContext) -> u32 {
    if ctx.is_null() {
        return 0;
    }
    unsafe {
        let ctx = &*ctx;
        (ctx.windowed_ctx.width * ctx.windowed_ctx.scale_factor as f32) as u32
    }
}

/// Get the physical height in pixels (C FFI for Swift)
///
/// # Safety
/// `ctx` must be a valid pointer returned by `blinc_create_context`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_get_physical_height(ctx: *mut IOSRenderContext) -> u32 {
    if ctx.is_null() {
        return 0;
    }
    unsafe {
        let ctx = &*ctx;
        (ctx.windowed_ctx.height * ctx.windowed_ctx.scale_factor as f32) as u32
    }
}

// =============================================================================
// GPU Rendering (C FFI for Swift)
// =============================================================================

/// GPU renderer state for iOS
pub struct IOSGpuRenderer {
    /// The Blinc application (includes renderer, text context, image context)
    app: BlincApp,
    /// The wgpu surface
    surface: wgpu::Surface<'static>,
    /// Surface configuration
    surface_config: wgpu::SurfaceConfiguration,
    /// Render context reference
    render_ctx: *mut IOSRenderContext,
}

/// Initialize the GPU renderer with a CAMetalLayer (C FFI for Swift)
///
/// # Arguments
/// * `ctx` - Render context pointer from `blinc_create_context`
/// * `metal_layer` - Pointer to CAMetalLayer (from UIView.layer)
/// * `width` - Drawable width in pixels
/// * `height` - Drawable height in pixels
///
/// # Returns
/// Pointer to GPU renderer, or null on failure
///
/// # Safety
/// * `ctx` must be a valid pointer returned by `blinc_create_context`
/// * `metal_layer` must be a valid pointer to a CAMetalLayer
#[unsafe(no_mangle)]
pub extern "C" fn blinc_init_gpu(
    ctx: *mut IOSRenderContext,
    metal_layer: *mut std::ffi::c_void,
    width: u32,
    height: u32,
) -> *mut IOSGpuRenderer {
    use blinc_gpu::{GpuRenderer, RendererConfig, TextRenderingContext};

    if ctx.is_null() || metal_layer.is_null() {
        tracing::error!("blinc_init_gpu: null context or metal_layer");
        return std::ptr::null_mut();
    }

    let config = crate::BlincConfig::default();

    let renderer_config = RendererConfig {
        max_primitives: config.max_primitives,
        max_glass_primitives: config.max_glass_primitives,
        max_glyphs: config.max_glyphs,
        sample_count: 1,
        texture_format: None,
        unified_text_rendering: true,
        ..RendererConfig::default()
    };

    // Create wgpu instance with Metal backend
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });

    // Create surface from CAMetalLayer
    // CoreAnimationLayer takes a raw *mut c_void pointer
    let surface_target = wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(metal_layer);
    let surface = match unsafe { instance.create_surface_unsafe(surface_target) } {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("blinc_init_gpu: failed to create surface: {}", e);
            return std::ptr::null_mut();
        }
    };

    // Create renderer
    let renderer = match pollster::block_on(async {
        GpuRenderer::with_instance_and_surface(instance, &surface, renderer_config).await
    }) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("blinc_init_gpu: failed to create renderer: {}", e);
            return std::ptr::null_mut();
        }
    };

    let device = renderer.device_arc();
    let queue = renderer.queue_arc();

    let mut text_ctx = TextRenderingContext::new(device.clone(), queue.clone());

    // Load iOS system fonts into the registry
    // Note: The FontRegistry already tries to load from KNOWN_FONT_PATHS,
    // but we also load from system_font_paths() to ensure fonts are available.
    let mut fonts_loaded = 0;
    for font_path in IOSApp::system_font_paths() {
        let path = std::path::Path::new(font_path);
        tracing::debug!("Checking font path: {}", font_path);
        if path.exists() {
            match std::fs::read(path) {
                Ok(data) => {
                    tracing::info!("Loading font from: {} ({} bytes)", font_path, data.len());
                    // Use load_font_data_to_registry to add to the font registry
                    // (not load_font_data which only sets the default font)
                    let loaded = text_ctx.load_font_data_to_registry(data);
                    if loaded > 0 {
                        tracing::info!("Successfully loaded {} faces from: {}", loaded, font_path);
                        fonts_loaded += loaded;
                    } else {
                        tracing::warn!("No faces loaded from font {}", font_path);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read font file {}: {}", font_path, e);
                }
            }
        } else {
            tracing::debug!("Font path does not exist: {}", font_path);
        }
    }
    tracing::info!("Loaded {} font faces total", fonts_loaded);

    // Preload common fonts with iOS family names
    // Note: iOS uses ".SF UI" family names (with leading dot for system fonts)
    text_ctx.preload_fonts(&[
        ".SF UI",         // iOS system font
        ".SF UI Text",    // iOS system font (text)
        ".SF UI Display", // iOS system font (display)
        "Helvetica",      // Helvetica
        "Helvetica Neue", // Helvetica Neue
        "Avenir",         // Avenir
        "Avenir Next",    // Avenir Next
        "Menlo",          // Monospace
        "Courier New",    // Courier
    ]);
    text_ctx.preload_generic_styles(blinc_gpu::GenericFont::SansSerif, &[400, 700], false);
    tracing::info!("Font preloading complete, {} fonts loaded", fonts_loaded);

    // Create RenderContext with text rendering support
    let render_context =
        crate::context::RenderContext::new(renderer, text_ctx, device, queue, config.sample_count);
    let app = BlincApp::from_context(render_context, config);

    // Configure surface with the format the renderer selected
    let format = app.texture_format();
    let surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(app.device(), &surface_config);

    tracing::info!(
        "blinc_init_gpu: GPU initialized ({}x{}, format: {:?})",
        width,
        height,
        format
    );

    Box::into_raw(Box::new(IOSGpuRenderer {
        app,
        surface,
        surface_config,
        render_ctx: ctx,
    }))
}

/// Resize the GPU surface (C FFI for Swift)
///
/// Call this when the Metal layer's drawable size changes.
///
/// # Safety
/// `gpu` must be a valid pointer returned by `blinc_init_gpu`.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_gpu_resize(gpu: *mut IOSGpuRenderer, width: u32, height: u32) {
    if gpu.is_null() {
        return;
    }

    unsafe {
        let gpu = &mut *gpu;
        if width > 0
            && height > 0
            && (gpu.surface_config.width != width || gpu.surface_config.height != height)
        {
            gpu.surface_config.width = width;
            gpu.surface_config.height = height;
            gpu.surface.configure(gpu.app.device(), &gpu.surface_config);
            tracing::debug!("blinc_gpu_resize: {}x{}", width, height);
        }
    }
}

/// Render a frame (C FFI for Swift)
///
/// This builds the UI if needed and renders to the current surface.
/// Call this from your CADisplayLink callback when `blinc_needs_render()` is true.
///
/// # Returns
/// true if frame was rendered successfully, false on error
///
/// # Safety
/// * `gpu` must be a valid pointer returned by `blinc_init_gpu`
/// * Must be called on the main thread
#[unsafe(no_mangle)]
pub extern "C" fn blinc_render_frame(gpu: *mut IOSGpuRenderer) -> bool {
    if gpu.is_null() {
        return false;
    }

    unsafe {
        let gpu = &mut *gpu;
        let ctx = match gpu.render_ctx.as_mut() {
            Some(c) => c,
            None => return false,
        };

        // Get surface texture
        let surface_texture = match gpu.surface.get_current_texture() {
            Ok(st) => st,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // Reconfigure surface and try again
                gpu.surface.configure(gpu.app.device(), &gpu.surface_config);
                match gpu.surface.get_current_texture() {
                    Ok(st) => st,
                    Err(e) => {
                        tracing::error!("blinc_render_frame: surface error: {:?}", e);
                        return false;
                    }
                }
            }
            Err(e) => {
                tracing::error!("blinc_render_frame: surface error: {:?}", e);
                return false;
            }
        };

        // Get render tree
        let tree = match ctx.render_tree.as_ref() {
            Some(t) => t,
            None => {
                surface_texture.present();
                return true; // No tree yet, just present empty frame
            }
        };

        // Render
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        if let Err(e) = gpu.app.render_tree_with_motion(
            tree,
            &ctx.render_state,
            &view,
            gpu.surface_config.width,
            gpu.surface_config.height,
        ) {
            tracing::error!("blinc_render_frame: render error: {}", e);
            surface_texture.present();
            return false;
        }

        surface_texture.present();
        true
    }
}

/// Destroy the GPU renderer (C FFI for Swift)
///
/// # Safety
/// `gpu` must be a valid pointer returned by `blinc_init_gpu`,
/// and must not be used after this call.
#[unsafe(no_mangle)]
pub extern "C" fn blinc_destroy_gpu(gpu: *mut IOSGpuRenderer) {
    if !gpu.is_null() {
        unsafe {
            drop(Box::from_raw(gpu));
        }
    }
}

/// Load a bundled font from the app bundle (C FFI for Swift)
///
/// Call this after `blinc_init_gpu` to load fonts from the app bundle.
/// The font will be added to the font registry and available for text rendering.
///
/// # Arguments
/// * `gpu` - GPU renderer pointer from `blinc_init_gpu`
/// * `path` - Path to the font file (null-terminated C string)
///
/// # Returns
/// Number of font faces loaded (0 on failure)
///
/// # Safety
/// * `gpu` must be a valid pointer returned by `blinc_init_gpu`
/// * `path` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub extern "C" fn blinc_load_bundled_font(
    gpu: *mut IOSGpuRenderer,
    path: *const std::ffi::c_char,
) -> u32 {
    if gpu.is_null() || path.is_null() {
        return 0;
    }

    unsafe {
        let gpu = &mut *gpu;
        let path_str = match std::ffi::CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => {
                tracing::error!("blinc_load_bundled_font: invalid path string");
                return 0;
            }
        };

        tracing::info!("Loading bundled font from: {}", path_str);

        let path = std::path::Path::new(path_str);
        if !path.exists() {
            tracing::error!(
                "blinc_load_bundled_font: font file does not exist: {}",
                path_str
            );
            return 0;
        }

        match std::fs::read(path) {
            Ok(data) => {
                tracing::info!("Read {} bytes from bundled font", data.len());
                let loaded = gpu.app.load_font_data_to_registry(data);
                tracing::info!("Loaded {} font faces from bundled font", loaded);
                loaded as u32
            }
            Err(e) => {
                tracing::error!("blinc_load_bundled_font: failed to read font: {}", e);
                0
            }
        }
    }
}

// ============================================================================
// Deep link handling
// ============================================================================

/// C FFI entry point: called by Swift when the app receives a deep link URL.
///
/// Auto-dispatches to the router registered via `RouterBuilder::build()`.
/// No user setup required — just build a router and deep links work.
///
/// Wire in Swift AppDelegate:
/// ```swift
/// func application(_ app: UIApplication, open url: URL, options: ...) -> Bool {
///     blinc_ios_handle_deep_link(url.absoluteString)
///     return true
/// }
/// ```
#[unsafe(no_mangle)]
pub extern "C" fn blinc_ios_handle_deep_link(uri: *const std::ffi::c_char) {
    if uri.is_null() {
        return;
    }
    let uri_str = unsafe { std::ffi::CStr::from_ptr(uri) };
    if let Ok(uri) = uri_str.to_str() {
        tracing::info!("iOS deep link received: {}", uri);
        blinc_router::dispatch_deep_link(uri);
    }
}

/// C FFI: receive stream data from native side (camera frames, audio buffers).
///
/// Wire in Swift:
/// ```swift
/// @_cdecl("blinc_dispatch_stream_data")
/// public func blinc_dispatch_stream_data(
///     streamId: UInt64,
///     dataPtr: UnsafePointer<UInt8>,
///     dataLen: UInt64
/// ) { ... }
/// ```
#[unsafe(no_mangle)]
pub extern "C" fn blinc_dispatch_stream_data(stream_id: u64, data_ptr: *const u8, data_len: u64) {
    if data_ptr.is_null() || data_len == 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data_ptr, data_len as usize) };
    blinc_core::native_bridge::dispatch_stream_data(
        stream_id,
        blinc_core::native_bridge::NativeValue::Bytes(bytes.to_vec()),
    );
}
