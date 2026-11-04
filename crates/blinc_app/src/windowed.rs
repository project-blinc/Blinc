//! Windowed application runner
//!
//! Provides a unified API for running windowed Blinc applications across
//! desktop and Mobile platforms.
//!
//! # Example
//!
//! ```ignore
//! use blinc_app::prelude::*;
//! use blinc_app::windowed::WindowedApp;
//!
//! fn main() -> Result<()> {
//!     WindowedApp::run(WindowConfig::default(), |ctx| {
//!         // Build your UI using reactive signals
//!         let count = ctx.use_signal(0);
//!         let doubled = ctx.use_derived(move |cx| cx.get(count).unwrap_or(0) * 2);
//!
//!         div().w_full().h_full()
//!             .flex_center()
//!             .child(text(&format!("Count: {}", ctx.get(count).unwrap_or(0))).size(48.0))
//!     })
//! }
//! ```

use std::hash::Hash;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use blinc_animation::{
    AnimatedTimeline, AnimatedValue, AnimationContext, AnimationScheduler, SchedulerHandle,
    SharedAnimatedTimeline, SharedAnimatedValue, SpringConfig,
};
use blinc_core::context_state::{BlincContextState, HookState, SharedHookState, StateKey};
use blinc_core::reactive::{Derived, ReactiveGraph, Signal, SignalId, State, StatefulDepsCallback};
use blinc_layout::overlay_state::{OverlayContext, get_overlay_manager};
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{OverlayManager, OverlayManagerExt, overlay_manager};
use blinc_platform::{
    ControlFlow, Event, EventLoop, InputEvent, Key, KeyState, LifecycleEvent, MouseEvent, Platform,
    TouchEvent, Window, WindowConfig, WindowEvent,
};

use crate::app::BlincApp;
use crate::error::{BlincError, Result};

/// Shared animation scheduler for the application (thread-safe)
pub type SharedAnimationScheduler = Arc<Mutex<AnimationScheduler>>;

mod app_init;
mod context;
mod context_display;
mod context_query;
mod context_reactive;
mod desktop_loop;
mod fps;
mod helpers;
mod pending;
mod window;

pub use self::context::WindowedContext;
use self::desktop_loop::DesktopLoop;
#[allow(unused_imports)]
use self::fps::*;
#[allow(unused_imports)]
pub use self::helpers::*;
#[allow(unused_imports)]
pub use self::pending::*;
#[allow(unused_imports)]
pub use self::window::*;

// SharedAnimatedValue and SharedAnimatedTimeline are re-exported from blinc_animation

#[cfg(all(feature = "windowed", not(target_os = "android")))]
use blinc_platform_desktop::DesktopPlatform;

/// Shared dirty flag type for element refs
pub type RefDirtyFlag = Arc<AtomicBool>;

/// Shared reactive graph for the application (thread-safe)
pub type SharedReactiveGraph = Arc<Mutex<ReactiveGraph>>;

/// Shared element registry for query API (thread-safe)
pub type SharedElementRegistry = Arc<blinc_layout::selector::ElementRegistry>;

/// Callback type for on_ready handlers
pub type ReadyCallback = Box<dyn FnOnce() + Send + Sync>;

/// Shared storage for ready callbacks
pub type SharedReadyCallbacks = Arc<Mutex<Vec<ReadyCallback>>>;

/// UI builder function for a window. Called each frame to produce the UI tree.
/// Returns a `Div` (the root element type for all Blinc UIs).
pub type WindowBuilder = Box<dyn FnMut(&mut WindowedContext) -> Div + Send>;

/// Windowed application runner
///
/// Provides a simple way to run a Blinc application in a window
/// with automatic event handling and rendering.
pub struct WindowedApp;

impl WindowedApp {
    /// Run a windowed Blinc application on desktop platforms
    ///
    /// This is the main entry point for desktop applications. It creates
    /// a window, sets up GPU rendering, and runs the event loop.
    ///
    /// # Arguments
    ///
    /// * `config` - Window configuration (title, size, etc.)
    /// * `ui_builder` - Function that builds the UI tree given the window context
    ///
    /// # Example
    ///
    /// ```ignore
    /// WindowedApp::run(WindowConfig::default(), |ctx| {
    ///     div()
    ///         .w(ctx.width).h(ctx.height)
    ///         .bg([0.1, 0.1, 0.15, 1.0])
    ///         .flex_center()
    ///         .child(
    ///             div().glass().rounded(16.0).p(24.0)
    ///                 .child(text("Hello Blinc!").size(32.0))
    ///         )
    /// })
    /// ```
    ///
    /// # Named-fn signature (edition 2024)
    ///
    /// On edition 2024, your `build_ui` function needs `+ use<>` on
    /// its return type:
    /// ```ignore
    /// fn build_ui(ctx: &mut WindowedContext) -> impl Element + use<> {
    ///     /* … */
    /// }
    /// WindowedApp::run(config, build_ui)?;
    /// ```
    ///
    /// **Why `+ use<>` is required and not just a workaround.** The
    /// runner stores the built tree across frames and incrementally
    /// updates it — every element inside the tree therefore has to be
    /// `'static` (it can't borrow from `ctx`, which is re-borrowed
    /// mutably on every frame). Edition 2024 changed RPIT capture
    /// rules so that `-> impl Element` in a free fn *implicitly*
    /// captures input lifetimes (it means `-> impl Element + use<'_>`),
    /// which the compiler reads as "may borrow from `ctx`". `+ use<>`
    /// explicitly says "captures nothing", restoring `'static`.
    ///
    /// This is the right signature for every UI builder in practice —
    /// they construct new owned elements from values read out of
    /// `ctx`, never references into it.
    ///
    /// Edition 2021 callers don't need the annotation (free-fn RPIT
    /// didn't capture by default there), but adding it is harmless
    /// and forward-compatible with the 2024 migration.
    ///
    /// See `gotcha-run-with-theme-hrtb-fnmut` in dev memory for
    /// reproduction, full error shapes, and why a closure wrap
    /// (`|cx| build_ui(cx)`) and a `Box` wrap don't help — both inherit
    /// the named fn's RPIT capture and produce the same error in a
    /// different shape.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    pub fn run<F, E>(config: WindowConfig, ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        Self::run_desktop(config, ui_builder)
    }

    /// Run a windowed app with an explicit theme bundle.
    ///
    /// Equivalent to `ThemeState::init(bundle, scheme)` immediately
    /// followed by [`Self::run`], but lets the caller install the
    /// theme without having to do it from inside the UI builder
    /// (where `init_theme`'s platform-default has already run and
    /// the user's bundle has to fight that path).
    ///
    /// Pass any value that implements `Into<ThemeBundle>` — the
    /// built-in [`BlincTheme::bundle()`](blinc_theme::BlincTheme::bundle)
    /// or a custom bundle built by your app.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_theme::{BlincTheme, ColorScheme};
    ///
    /// WindowedApp::run_with_theme(
    ///     WindowConfig::default(),
    ///     BlincTheme::bundle(),
    ///     ColorScheme::Dark,
    ///     |ctx| my_ui(ctx),
    /// )
    /// ```
    ///
    /// # Named-fn signature (edition 2024)
    ///
    /// Same requirement as [`Self::run`]: on edition 2024 your
    /// `build_ui` function needs `+ use<>` on the return type, e.g.
    /// `fn build_ui(ctx: &mut WindowedContext) -> impl Element + use<>`.
    /// See [`Self::run`]'s doc for the soundness rationale.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    pub fn run_with_theme<F, E>(
        config: WindowConfig,
        bundle: blinc_theme::ThemeBundle,
        scheme: blinc_theme::ColorScheme,
        ui_builder: F,
    ) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        blinc_theme::ThemeState::init(bundle, scheme);
        Self::run_desktop(config, ui_builder)
    }

    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn run_desktop<F, E>(config: WindowConfig, mut ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        // Initialize the platform asset loader for cross-platform asset loading
        Self::init_asset_loader();

        // Initialize the text measurer for accurate text layout
        blinc_layout::init_text_measurer();

        // Initialize the theme system with platform detection
        Self::init_theme();

        let platform = DesktopPlatform::new().map_err(|e| BlincError::Platform(e.to_string()))?;
        let primary_transparent = config.transparent;
        let primary_max_frame_latency = config.max_frame_latency.clamp(1, 3);
        // Snapshot the animation FPS policy before `config` moves into
        // the event loop. Reconcile the two paths:
        //
        // * Legacy `animation_fps_cap: Option<u32>` — when set, wins
        //   unconditionally (treated as `AnimationFps::Fixed(N)`). Lets
        //   existing apps keep working without code changes.
        //
        // * New `animation_fps: AnimationFps`:
        //   - `Fixed(N)`     → cap at N, framework never overrides.
        //   - `Refresh`      → no cap, vsync-paced animations.
        //   - `Adaptive`     → framework picks a starting cap from a
        //                       static heuristic (GPU class proxy via
        //                       core count + total RAM) and adjusts at
        //                       runtime based on observed frame times.
        //
        // `animation_fps_cap_atomic` is shared with the Phase 4
        // adaptation logic below so the adapter can mutate the cap
        // at runtime; the per-frame paint gate and the scheduler
        // both read through it.
        let initial_animation_fps_cap: Option<u32> = match config.animation_fps_cap {
            Some(n) => Some(n),
            None => match config.animation_fps {
                blinc_platform::AnimationFps::Fixed(n) => Some(n),
                blinc_platform::AnimationFps::Refresh => None,
                blinc_platform::AnimationFps::Adaptive => Some(detect_initial_fps_cap()),
            },
        };
        let animation_fps_is_adaptive = config.animation_fps_cap.is_none()
            && matches!(config.animation_fps, blinc_platform::AnimationFps::Adaptive);
        let animation_fps_cap = initial_animation_fps_cap;
        tracing::info!(
            "animation_fps: source={} cap={:?} adaptive={}",
            if config.animation_fps_cap.is_some() {
                "legacy_animation_fps_cap"
            } else {
                match config.animation_fps {
                    blinc_platform::AnimationFps::Adaptive => "Adaptive",
                    blinc_platform::AnimationFps::Fixed(_) => "Fixed",
                    blinc_platform::AnimationFps::Refresh => "Refresh",
                }
            },
            animation_fps_cap,
            animation_fps_is_adaptive,
        );
        // Atomic cell that all per-frame paths read for the current
        // effective cap (0 = no cap / refresh-paced). The dynamic
        // adapter writes through this so changes flow without
        // recompiling closures.
        let animation_fps_cap_atomic = Arc::new(std::sync::atomic::AtomicU32::new(
            animation_fps_cap.unwrap_or(0),
        ));
        // Adapter only spun up in Adaptive mode. `Fixed` / `Refresh`
        // and legacy `Some(n)` skip it — the framework respects what
        // the app picked.
        let fps_adapter: Option<Arc<Mutex<FpsAdapter>>> = if animation_fps_is_adaptive {
            Some(Arc::new(Mutex::new(FpsAdapter::new(
                animation_fps_cap.unwrap_or(60),
            ))))
        } else {
            None
        };
        // Capture animation_thread_mode pre-move (config gets moved
        // into `create_event_loop_with_config` below). Drives whether
        // the AnimationScheduler spawns its bg thread or relies on
        // the main thread's per-frame `tick()` call (Phase 3) to
        // advance springs / keyframes / timelines / tick_callbacks.
        let animation_thread_mode = config.animation_thread_mode;
        let event_loop = platform
            .create_event_loop_with_config(config)
            .map_err(|e| BlincError::Platform(e.to_string()))?;

        // Get a wake proxy to allow the animation thread to wake up the event loop
        let wake_proxy = event_loop.wake_proxy();
        // Clone for the open_window callback
        let wake_proxy_for_windows = event_loop.wake_proxy();

        // Clone for the redraw-chain pacing path. When `animation_fps_cap`
        // is set, the chain calls `wake_at` on this proxy instead of
        // `request_redraw`, so the platform shim's lazy timer thread
        // delivers the next frame after the configured delay.
        let wake_proxy_for_pacing = event_loop.wake_proxy();

        // Frame-dirty flag. The OS sends `Event::Frame` at vsync to focused
        // windows whether or not we asked for a redraw, which means a
        // statically-rendered focused UI was burning CPU re-rendering an
        // identical scene every ~16 ms. We now skip the entire frame
        // handler when this flag is `false` at frame entry. Any mutation
        // we care about — input event, lifecycle event, scheduler wake
        // (set by the wake callback below), end-of-frame signal indicating
        // ongoing work — flips it back to `true`. Initial value is `true`
        // so the first frame always renders.
        let frame_dirty = Arc::new(AtomicBool::new(true));
        let frame_dirty_for_wake = Arc::clone(&frame_dirty);

        // If the `hot-reload` feature is on AND we're a child of
        // `dx serve --hot-patch`, spawn the websocket client that
        // sends our ASLR offset to the dev-server and applies
        // incoming jump-table patches. The wake closure is what lets
        // the patch thread nudge the event loop out of
        // `ControlFlow::Wait` after a patch lands — without it the
        // render tree only refreshes on the next natural redraw
        // (mouse move, focus change, etc.), which is not what the
        // user wants from a hot-reload signal.
        //
        // The closure also flips `frame_dirty` because Event::Frame
        // returns early when both `frame_dirty` and `peek_needs_redraw`
        // are false. Without that flip, `wake_proxy.wake()` reaches
        // the event loop and `request_redraw()` fires, but the
        // resulting Event::Frame bails before our rebuild check at
        // line ~4244 ever runs — the patch lands silently and the
        // window doesn't update until something else happens to
        // dirty the frame.
        //
        // No-op when the dx env vars aren't set, which is every
        // normal `cargo run` invocation, so the call is safe to make
        // unconditionally here.
        #[cfg(feature = "hot-reload")]
        {
            let wp = event_loop.wake_proxy();
            let frame_dirty_hr = Arc::clone(&frame_dirty);
            crate::hot_reload::connect(move || {
                frame_dirty_hr.store(true, Ordering::Release);
                wp.wake();
            });
        }

        // Cross-thread mirror of the renderer's `visible_anim_active`
        // flag. The end-of-frame chain (main thread) writes the
        // current frame's value here; the scheduler's wake callback
        // (bg thread) reads it. When `false`, the scheduler's
        // periodic ticks for off-screen-only animations don't kick
        // the main thread — the chain dies until input or scroll
        // brings the animation back into view. Starts `true` so the
        // very first scheduler activity wakes the main thread to
        // render the initial frame.
        let visible_anim_for_wake = Arc::new(AtomicBool::new(true));
        let visible_anim_for_wake_cb = Arc::clone(&visible_anim_for_wake);

        // Shared dirty flag for element refs
        let ref_dirty_flag: RefDirtyFlag = Arc::new(AtomicBool::new(false));
        // Process-global reactive graph + dirty flag — shared with bare
        // `signal()` / `effect()` / `computed()` callers in user code so
        // dependency tracking spans both paths.
        let reactive: SharedReactiveGraph = blinc_core::reactive::global_graph();
        // Shared hook state for use_state persistence
        let hooks: SharedHookState = Arc::new(Mutex::new(HookState::new()));

        // Initialize global context state singleton (if not already initialized)
        // This allows components to create internal state without context parameters
        if !BlincContextState::is_initialized() {
            #[allow(clippy::type_complexity)]
            let stateful_callback: std::sync::Arc<dyn Fn(&[SignalId]) + Send + Sync> =
                Arc::new(|signal_ids| {
                    blinc_layout::check_stateful_deps(signal_ids);
                });
            BlincContextState::init_with_callback(
                Arc::clone(&reactive),
                Arc::clone(&hooks),
                Arc::clone(&ref_dirty_flag),
                stateful_callback,
            );
        }

        // Outside the init guard on purpose. Anything that builds a
        // widget before the window opens initialises the context state
        // itself -- `BlincDsl` does, through `ensure_context_state` --
        // and the guard above then skips, leaving no notifier. A bare
        // `Signal::set` would mark the signal dirty and reach no
        // `Stateful`, so a bound widget only caught up when some
        // unrelated interaction rebuilt it.
        {
            #[allow(clippy::type_complexity)]
            let notifier: std::sync::Arc<dyn Fn(&[SignalId]) + Send + Sync> =
                Arc::new(|signal_ids| {
                    blinc_layout::check_stateful_deps(signal_ids);
                });
            blinc_core::reactive::set_stateful_deps_notifier(move |ids| notifier(ids));
        }

        // Shared animation scheduler for spring/keyframe animations
        // Runs on background thread so animations continue even when window loses focus
        let mut scheduler = AnimationScheduler::new();
        // The wake callback gets installed in either thread mode.
        // Used by:
        //   * Bg thread (when `start_background()` is called below) —
        //     fires on idle→active edge so the main thread wakes up
        //     and starts rendering.
        //   * `notify_active()` paths inside the scheduler
        //     (`add_spring` / `add_keyframe` / etc.) — fires whenever
        //     an animation gets registered. Necessary in
        //     `AnimationThreadMode::Main` for off-thread registrations
        //     (custom timer thread, async task) to trigger a render;
        //     redundant-but-harmless in `Background` since the bg
        //     thread also fires it on its idle→active edge.
        // Marks `frame_dirty` so the resulting Event::Frame actually
        // renders.
        let _ = visible_anim_for_wake_cb; // wake gate moved into scheduler edge trigger
        scheduler.set_wake_callback(move || {
            frame_dirty_for_wake.store(true, Ordering::Release);
            wake_proxy.wake();
        });
        // Spawn the bg ticking thread only in `Background` mode. In
        // `Main` (default), the windowed app's Phase 3 calls
        // `RenderState::tick` which in turn calls
        // `AnimationScheduler::tick` synchronously — eliminating phase
        // jitter and removing one thread from the runtime. The
        // `tick()` method itself is a no-op when a bg thread is
        // running, so the two paths are mutually exclusive.
        match animation_thread_mode {
            blinc_platform::AnimationThreadMode::Background => {
                // wasm32 has no background thread — animations tick via
                // requestAnimationFrame — so this is a no-op there.
                #[cfg(not(target_arch = "wasm32"))]
                scheduler.start_background();
            }
            blinc_platform::AnimationThreadMode::Main => {
                // No bg thread. Main-thread tick handles it.
            }
        }
        let animations: SharedAnimationScheduler = Arc::new(Mutex::new(scheduler));

        // Set global scheduler handle for StateContext and component access
        {
            let scheduler_handle = animations.lock().unwrap().handle();
            blinc_animation::set_global_scheduler(scheduler_handle);
            // Default look of the layout widgets, ahead of the theme's own CSS.
            blinc_layout::ua_styles::install();
        }

        // Shared CSS animation/transition store. CSS ticking happens
        // synchronously on the main thread (Phase 3 of the frame loop)
        // to avoid phase jitter; the bg scheduler thread does not drive
        // it. Once a CSS animation/transition is live, the main thread
        // self-perpetuates via `request_redraw()` at the end of the
        // frame as long as `css_needs_redraw` is true (see Phase 5).
        // No keep-alive scheduler callback is needed — the bg thread
        // can stay parked while only CSS work is in flight.
        let css_anim_store = Arc::new(Mutex::new(blinc_layout::CssAnimationStore::new()));

        // Shared element registry for query API
        let element_registry: SharedElementRegistry =
            Arc::new(blinc_layout::selector::ElementRegistry::new());

        // Set up query callback in BlincContextState so components can query elements globally
        {
            let registry_for_query = Arc::clone(&element_registry);
            let query_callback: blinc_core::QueryCallback = Arc::new(move |id: &str| {
                registry_for_query.get(id).map(|node_id| node_id.to_raw())
            });
            BlincContextState::get().set_query_callback(query_callback);
        }

        // Set up bounds callback for ElementHandle.bounds()
        {
            let registry_for_bounds = Arc::clone(&element_registry);
            let bounds_callback: blinc_core::BoundsCallback =
                Arc::new(move |id: &str| registry_for_bounds.get_bounds(id));
            BlincContextState::get().set_bounds_callback(bounds_callback);
        }

        // Store element registry in BlincContextState for global query() function
        // Cast to Arc<dyn Any + Send + Sync> for type-erased storage
        BlincContextState::get()
            .set_element_registry(Arc::clone(&element_registry) as blinc_core::AnyElementRegistry);

        // Shared storage for on_ready callbacks
        let ready_callbacks: SharedReadyCallbacks = Arc::new(Mutex::new(Vec::new()));

        // Set up continuous redraw callback for text widget cursor animation
        // This bridges text widgets (which track focus) with the animation scheduler (which drives redraws)
        {
            let animations_for_callback = Arc::clone(&animations);
            blinc_layout::widgets::set_continuous_redraw_callback(move |enabled| {
                if let Ok(scheduler) = animations_for_callback.lock() {
                    scheduler.set_continuous_redraw(enabled);
                }
            });
        }

        // Connect theme animation to the animation scheduler
        // This enables smooth color transitions when switching between light/dark mode
        blinc_theme::ThemeState::get().set_scheduler(&animations);

        // Render state: dynamic properties that update every frame without tree rebuild
        // This includes cursor blink, animated colors, hover states, etc.
        let mut render_state: Option<blinc_layout::RenderState> = None;

        // Shared motion states for query API access
        // This allows components to query motion animation state via query_motion()
        let shared_motion_states = blinc_layout::create_shared_motion_states();

        // Set up motion state callback in BlincContextState
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

        // Overlay manager for modals, dialogs, toasts, etc.
        let overlays: OverlayManager = overlay_manager();

        // Initialize overlay context singleton for component access
        if !OverlayContext::is_initialized() {
            OverlayContext::init(Arc::clone(&overlays));
        }

        // Primary window state
        let mut ws = WindowState::new(
            Arc::clone(&css_anim_store),
            Arc::clone(&shared_motion_states),
        );
        ws.transparent = primary_transparent;
        // Track primary window ID once known
        let mut primary_wid: Option<blinc_platform::WindowId> = None;
        // Secondary windows (opened via ctx.open_window())
        let mut secondary_windows: std::collections::HashMap<
            blinc_platform::WindowId,
            WindowState,
        > = std::collections::HashMap::new();
        // UI builders for secondary windows (queued via open_window)
        // For now secondary windows get a blank UI — full UI builder support is future work

        let desktop_loop = DesktopLoop {
            platform,
            primary_max_frame_latency,
            animation_fps_cap,
            animation_fps_cap_atomic,
            fps_adapter,
            wake_proxy_for_windows,
            wake_proxy_for_pacing,
            frame_dirty,
            visible_anim_for_wake,
            ref_dirty_flag,
            reactive,
            hooks,
            animations,
            css_anim_store,
            element_registry,
            ready_callbacks,
            shared_motion_states,
            overlays,
            ws,
            primary_wid,
            secondary_windows,
            ui_builder,
            _builder_output: std::marker::PhantomData,
        };
        let mut desktop_loop = desktop_loop;
        event_loop
            .run(move |event, window| desktop_loop.handle(event, window))
            .map_err(|e| BlincError::Platform(e.to_string()))?;

        Ok(())
    }

    /// Placeholder for non-windowed builds
    #[cfg(not(feature = "windowed"))]
    pub fn run<F, E>(_config: WindowConfig, _ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder + 'static,
    {
        Err(BlincError::Platform(
            "Windowed feature not enabled. Add 'windowed' feature to blinc_app".to_string(),
        ))
    }
}
