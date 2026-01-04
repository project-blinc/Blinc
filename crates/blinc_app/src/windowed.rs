//! Windowed application runner
//!
//! Provides a unified API for running windowed Blinc applications across
//! desktop and Android platforms.
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
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use blinc_animation::{
    AnimatedTimeline, AnimatedValue, AnimationContext, AnimationScheduler, SchedulerHandle,
    SharedAnimatedTimeline, SharedAnimatedValue, SpringConfig,
};
use blinc_core::context_state::{BlincContextState, HookState, SharedHookState, StateKey};
use blinc_core::reactive::{Derived, ReactiveGraph, Signal, SignalId, State, StatefulDepsCallback};
use blinc_layout::overlay_state::OverlayContext;
use blinc_layout::prelude::*;
use blinc_layout::widgets::overlay::{overlay_manager, OverlayManager, OverlayManagerExt};
use blinc_platform::{
    ControlFlow, Event, EventLoop, InputEvent, Key, KeyState, LifecycleEvent, MouseEvent, Platform,
    TouchEvent, Window, WindowConfig, WindowEvent,
};

use crate::app::BlincApp;
use crate::error::{BlincError, Result};

/// Shared animation scheduler for the application (thread-safe)
pub type SharedAnimationScheduler = Arc<Mutex<AnimationScheduler>>;

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
    /// Physical window width (for internal use)
    physical_width: f32,
    /// Physical window height (for internal use)
    physical_height: f32,
    /// Whether the window is focused
    pub focused: bool,
    /// Number of completed UI rebuilds (0 = first build in progress)
    ///
    /// Use `is_ready()` to check if the UI has been built at least once.
    /// This is useful for triggering animations after motion bindings are registered.
    pub rebuild_count: u32,
    /// Event router for input event handling
    pub event_router: EventRouter,
    /// Event router for overlay content (modals, dropdowns, etc.)
    pub overlay_event_router: EventRouter,
    /// Animation scheduler for spring/keyframe animations
    pub animations: SharedAnimationScheduler,
    /// Shared dirty flag for element refs - when set, triggers UI rebuild
    ref_dirty_flag: RefDirtyFlag,
    /// Reactive graph for signal-based state management
    reactive: SharedReactiveGraph,
    /// Hook state for call-order based signal persistence
    hooks: SharedHookState,
    /// Overlay manager for modals, dialogs, toasts, etc.
    overlay_manager: OverlayManager,
    /// Cached overlay tree for event routing (rebuilt when overlays change)
    overlay_tree: Option<blinc_layout::RenderTree>,
    /// Whether overlays were visible last frame (for stable motion cleanup)
    had_visible_overlays: bool,
    /// Element registry for query API (shared with RenderTree)
    element_registry: SharedElementRegistry,
    /// Callbacks to run after UI is ready (motion bindings registered)
    ready_callbacks: SharedReadyCallbacks,
}

impl WindowedContext {
    fn from_window<W: Window>(
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

        Self {
            width: logical_width,
            height: logical_height,
            scale_factor,
            physical_width: physical_width as f32,
            physical_height: physical_height as f32,
            focused: window.is_focused(),
            rebuild_count: 0,
            event_router,
            overlay_event_router: EventRouter::new(),
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
            overlay_manager: overlay_mgr,
            overlay_tree: None,
            had_visible_overlays: false,
            element_registry,
            ready_callbacks,
        }
    }

    /// Update context from window (preserving event router, dirty flag, and reactive graph)
    fn update_from_window<W: Window>(&mut self, window: &W) {
        let (physical_width, physical_height) = window.size();
        let scale_factor = window.scale_factor();

        self.physical_width = physical_width as f32;
        self.physical_height = physical_height as f32;
        self.width = physical_width as f32 / scale_factor as f32;
        self.height = physical_height as f32 / scale_factor as f32;
        self.scale_factor = scale_factor;
        self.focused = window.is_focused();
    }

    // =========================================================================
    // DPI-Related Helpers
    // =========================================================================

    /// Get the physical window width (for advanced use cases)
    ///
    /// Most users should use `ctx.width` which is in logical pixels.
    /// Physical dimensions are only needed when directly interfacing
    /// with GPU surfaces or platform-specific code.
    pub fn physical_width(&self) -> f32 {
        self.physical_width
    }

    /// Get the physical window height (for advanced use cases)
    pub fn physical_height(&self) -> f32 {
        self.physical_height
    }

    /// Check if the UI is ready (has completed at least one rebuild)
    ///
    /// This is useful for triggering animations after the first UI build,
    /// when motion bindings have been registered with the renderer.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn my_component(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let progress = ctx.use_animated_value_for("progress", 0.0, SpringConfig::gentle());
    ///
    ///     // Only trigger animation after UI is ready
    ///     let triggered = ctx.use_state_keyed("triggered", || false);
    ///     if ctx.is_ready() && !triggered.get() {
    ///         triggered.set(true);
    ///         progress.lock().unwrap().set_target(100.0);
    ///     }
    ///
    ///     // ... build UI ...
    /// }
    /// ```
    pub fn is_ready(&self) -> bool {
        self.rebuild_count > 0
    }

    /// Register a callback to run once after the UI is ready
    ///
    /// The callback will be executed after the first UI rebuild completes,
    /// when motion bindings have been registered with the renderer.
    /// This is the recommended way to trigger initial animations.
    ///
    /// Callbacks are executed once and then discarded. If `is_ready()` is
    /// already true, the callback will run on the next frame.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn my_component(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let progress = ctx.use_animated_value_for("progress", 0.0, SpringConfig::gentle());
    ///
    ///     // Register animation to trigger when UI is ready
    ///     let progress_clone = progress.clone();
    ///     ctx.on_ready(move || {
    ///         if let Ok(mut value) = progress_clone.lock() {
    ///             value.set_target(100.0);
    ///         }
    ///     });
    ///
    ///     // ... build UI ...
    /// }
    /// ```
    /// Register a callback to run once when the UI is ready (context-level).
    ///
    /// **Note:** For element-specific callbacks, prefer using the query API:
    /// ```ignore
    /// ctx.query_element("my-element").on_ready(|bounds| {
    ///     // Triggered once after element is laid out
    /// });
    /// ```
    /// The query-based approach uses stable string IDs that survive tree rebuilds.
    ///
    /// This context-level callback runs after the first rebuild completes.
    /// If called after the UI is already ready, executes immediately.
    pub fn on_ready<F>(&self, callback: F)
    where
        F: FnOnce() + Send + Sync + 'static,
    {
        // If already ready, execute immediately
        if self.rebuild_count > 0 {
            callback();
            return;
        }
        // Otherwise queue for execution after first rebuild
        if let Ok(mut callbacks) = self.ready_callbacks.lock() {
            callbacks.push(Box::new(callback));
        }
    }

    // =========================================================================
    // Reactive Signal API
    // =========================================================================

    /// Create a persistent state value that survives across UI rebuilds (keyed)
    ///
    /// This creates component-level state identified by a unique string key.
    /// Returns a `State<T>` with direct `.get()` and `.set()` methods.
    ///
    /// For stateful UI elements with `StateTransitions`, prefer `use_state(initial)`
    /// which auto-keys by source location.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn my_button(ctx: &WindowedContext, id: &str) -> impl ElementBuilder {
    ///     // Each button gets its own hover state, keyed by id
    ///     let hovered = ctx.use_state_keyed(id, || false);
    ///
    ///     div()
    ///         .bg(if hovered.get() { Color::RED } else { Color::BLUE })
    ///         .on_hover_enter({
    ///             let hovered = hovered.clone();
    ///             move |_| hovered.set(true)
    ///         })
    ///         .on_hover_leave({
    ///             let hovered = hovered.clone();
    ///             move |_| hovered.set(false)
    ///         })
    /// }
    /// ```
    pub fn use_state_keyed<T, F>(&self, key: &str, init: F) -> State<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        use blinc_core::reactive::SignalId;

        let state_key = StateKey::from_string::<T>(key);
        let mut hooks = self.hooks.lock().unwrap();

        // Check if we have an existing signal with this key
        let signal = if let Some(raw_id) = hooks.get(&state_key) {
            // Reconstruct the signal from stored ID
            let signal_id = SignalId::from_raw(raw_id);
            Signal::from_id(signal_id)
        } else {
            // First time - create a new signal and store it
            let signal = self.reactive.lock().unwrap().create_signal(init());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            signal
        };

        // Create callback for stateful deps notification
        let callback: StatefulDepsCallback = Arc::new(|signal_ids| {
            blinc_layout::check_stateful_deps(signal_ids);
        });

        State::with_stateful_callback(
            signal,
            Arc::clone(&self.reactive),
            Arc::clone(&self.ref_dirty_flag),
            callback,
        )
    }

    /// Create a persistent signal that survives across UI rebuilds (keyed)
    ///
    /// Unlike `use_signal()` which creates a new signal each call, this method
    /// persists the signal using a unique string key. Use this for simple
    /// reactive values that need to survive rebuilds.
    ///
    /// For FSM-based state with `StateTransitions`, use `use_state_keyed()` instead.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let current_index = ctx.use_signal_keyed("current_index", || 0usize);
    ///
    /// // Read the value
    /// let index = ctx.get(current_index).unwrap_or(0);
    ///
    /// // Set the value (in an event handler)
    /// ctx.set(current_index, 1);
    /// ```
    pub fn use_signal_keyed<T, F>(&self, key: &str, init: F) -> Signal<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        use blinc_core::reactive::SignalId;

        let state_key = StateKey::from_string::<T>(key);
        let mut hooks = self.hooks.lock().unwrap();

        // Check if we have an existing signal with this key
        if let Some(raw_id) = hooks.get(&state_key) {
            // Reconstruct the signal from stored ID
            let signal_id = SignalId::from_raw(raw_id);
            Signal::from_id(signal_id)
        } else {
            // First time - create a new signal and store it
            let signal = self.reactive.lock().unwrap().create_signal(init());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            signal
        }
    }

    /// Create a persistent ScrollRef for programmatic scroll control
    ///
    /// This creates a ScrollRef that survives across UI rebuilds. Use `.bind()`
    /// on a scroll widget to connect it, then call methods like `.scroll_to()`
    /// to programmatically control scrolling.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let scroll_ref = ctx.use_scroll_ref("my_scroll");
    ///
    ///     div()
    ///         .child(
    ///             scroll()
    ///                 .bind(&scroll_ref)
    ///                 .child(items.iter().map(|i| div().id(format!("item-{}", i.id))))
    ///         )
    ///         .child(
    ///             button("Scroll to item 5").on_click({
    ///                 let scroll_ref = scroll_ref.clone();
    ///                 move |_| scroll_ref.scroll_to("item-5")
    ///             })
    ///         )
    /// }
    /// ```
    pub fn use_scroll_ref(&self, key: &str) -> blinc_layout::selector::ScrollRef {
        use blinc_core::reactive::SignalId;
        use blinc_layout::selector::{ScrollRef, SharedScrollRefInner, TriggerCallback};

        // Create a unique key for the scroll ref's inner state
        let state_key =
            StateKey::from_string::<SharedScrollRefInner>(&format!("scroll_ref:{}", key));
        let mut hooks = self.hooks.lock().unwrap();

        // Check if we have an existing signal with this key
        let (signal_id, inner) = if let Some(raw_id) = hooks.get(&state_key) {
            // Reconstruct the signal ID and get the inner state from the reactive graph
            let signal_id = SignalId::from_raw(raw_id);
            let inner = self
                .reactive
                .lock()
                .unwrap()
                .get_untracked(Signal::<SharedScrollRefInner>::from_id(signal_id))
                .unwrap_or_else(ScrollRef::new_inner);
            (signal_id, inner)
        } else {
            // First time - create a new inner state and store it in the reactive graph
            let new_inner = ScrollRef::new_inner();
            let signal = self
                .reactive
                .lock()
                .unwrap()
                .create_signal(Arc::clone(&new_inner));
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            (signal.id(), new_inner)
        };

        drop(hooks);

        // ScrollRef doesn't need to trigger rebuilds - scroll operations are processed
        // every frame by process_pending_scroll_refs()
        let noop_trigger: TriggerCallback = Arc::new(|| {});

        ScrollRef::with_inner(inner, signal_id, noop_trigger)
    }

    /// Create a new reactive signal with an initial value (low-level API)
    ///
    /// **Note**: Prefer `use_state` in most cases, as it automatically
    /// persists signals across rebuilds.
    ///
    /// This method always creates a new signal. Use this for advanced
    /// cases where you manage signal lifecycle manually.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = ctx.use_signal(0);
    ///
    /// // In an event handler:
    /// ctx.set(count, ctx.get(count).unwrap_or(0) + 1);
    /// ```
    pub fn use_signal<T: Send + 'static>(&self, initial: T) -> Signal<T> {
        self.reactive.lock().unwrap().create_signal(initial)
    }

    /// Get the current value of a signal
    ///
    /// This automatically tracks the signal as a dependency when called
    /// within a derived computation or effect.
    pub fn get<T: Clone + 'static>(&self, signal: Signal<T>) -> Option<T> {
        self.reactive.lock().unwrap().get(signal)
    }

    /// Set the value of a signal, triggering reactive updates
    ///
    /// This will automatically trigger a UI rebuild.
    pub fn set<T: Send + 'static>(&self, signal: Signal<T>, value: T) {
        self.reactive.lock().unwrap().set(signal, value);
        // Mark dirty to trigger rebuild
        self.ref_dirty_flag.store(true, Ordering::SeqCst);
    }

    /// Update a signal using a function
    ///
    /// This is useful for incrementing counters or modifying state based
    /// on the current value.
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.update(count, |n| n + 1);
    /// ```
    pub fn update<T: Clone + Send + 'static, F: FnOnce(T) -> T>(&self, signal: Signal<T>, f: F) {
        self.reactive.lock().unwrap().update(signal, f);
        self.ref_dirty_flag.store(true, Ordering::SeqCst);
    }

    /// Create a derived (computed) value
    ///
    /// Derived values are lazily computed and cached. They automatically
    /// track their signal dependencies and recompute when those signals change.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = ctx.use_signal(5);
    /// let doubled = ctx.use_derived(move |cx| cx.get(count).unwrap_or(0) * 2);
    ///
    /// assert_eq!(ctx.get_derived(doubled), Some(10));
    /// ```
    pub fn use_derived<T, F>(&self, compute: F) -> Derived<T>
    where
        T: Clone + Send + 'static,
        F: Fn(&ReactiveGraph) -> T + Send + 'static,
    {
        self.reactive.lock().unwrap().create_derived(compute)
    }

    /// Get the value of a derived computation
    pub fn get_derived<T: Clone + 'static>(&self, derived: Derived<T>) -> Option<T> {
        self.reactive.lock().unwrap().get_derived(derived)
    }

    /// Create an effect that runs when its dependencies change
    ///
    /// Effects are useful for side effects like logging, network requests,
    /// or syncing state with external systems.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = ctx.use_signal(0);
    ///
    /// ctx.use_effect(move |cx| {
    ///     let value = cx.get(count).unwrap_or(0);
    ///     println!("Count changed to: {}", value);
    /// });
    /// ```
    pub fn use_effect<F>(&self, run: F) -> blinc_core::reactive::Effect
    where
        F: FnMut(&ReactiveGraph) + Send + 'static,
    {
        self.reactive.lock().unwrap().create_effect(run)
    }

    /// Batch multiple signal updates into a single reactive update
    ///
    /// This is useful when updating multiple signals at once to avoid
    /// redundant recomputations.
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.batch(|g| {
    ///     g.set(x, 10);
    ///     g.set(y, 20);
    ///     g.set(z, 30);
    /// });
    /// // Only one UI rebuild triggered
    /// ```
    pub fn batch<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ReactiveGraph) -> R,
    {
        let result = self.reactive.lock().unwrap().batch(f);
        self.ref_dirty_flag.store(true, Ordering::SeqCst);
        result
    }

    /// Get the shared reactive graph for advanced usage
    ///
    /// This is useful when you need to pass the graph to closures or
    /// store it for later use.
    pub fn reactive(&self) -> SharedReactiveGraph {
        Arc::clone(&self.reactive)
    }

    /// Create a new DivRef that will trigger rebuilds when modified
    ///
    /// Use this to create refs that can be mutated in event handlers.
    /// When you call `.borrow_mut()` or `.with_mut()` on the returned ref,
    /// the UI will automatically rebuild when the mutation completes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let card_ref = ctx.create_ref::<Div>();
    ///
    /// div()
    ///     .child(
    ///         div()
    ///             .bind(&card_ref)
    ///             .on_hover_enter({
    ///                 let r = card_ref.clone();
    ///                 move |_| {
    ///                     // This automatically triggers a rebuild
    ///                     r.with_mut(|d| *d = d.swap().bg(Color::RED));
    ///                 }
    ///             })
    ///     )
    /// ```
    pub fn create_ref<T>(&self) -> ElementRef<T> {
        ElementRef::with_dirty_flag(Arc::clone(&self.ref_dirty_flag))
    }

    /// Create a new DivRef (convenience method)
    pub fn div_ref(&self) -> DivRef {
        self.create_ref::<Div>()
    }

    /// Get the shared dirty flag for manual state management
    ///
    /// Use this when you want to create your own state types that trigger
    /// UI rebuilds when modified. When you modify state, set this flag to true.
    ///
    /// # Example
    ///
    /// ```ignore
    /// struct MyState {
    ///     value: i32,
    ///     dirty_flag: RefDirtyFlag,
    /// }
    ///
    /// impl MyState {
    ///     fn set_value(&mut self, v: i32) {
    ///         self.value = v;
    ///         self.dirty_flag.store(true, Ordering::SeqCst);
    ///     }
    /// }
    /// ```
    pub fn dirty_flag(&self) -> RefDirtyFlag {
        Arc::clone(&self.ref_dirty_flag)
    }

    /// Get a handle to the animation scheduler for creating animated values
    ///
    /// Components use this handle to create `AnimatedValue`s that automatically
    /// register with the scheduler. The scheduler ticks all animations each frame
    /// and triggers UI rebuilds while animations are active.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_animation::{AnimatedValue, SpringConfig};
    ///
    /// let opacity = AnimatedValue::new(ctx.animations(), 1.0, SpringConfig::stiff());
    /// opacity.set_target(0.5); // Auto-registers and animates
    /// let current = opacity.get(); // Get interpolated value
    /// ```
    pub fn animation_handle(&self) -> SchedulerHandle {
        self.animations.lock().unwrap().handle()
    }

    /// Get the overlay manager for showing modals, dialogs, toasts, etc.
    ///
    /// The overlay manager provides a fluent API for creating overlays that
    /// render in a separate pass after the main UI tree, ensuring they always
    /// appear on top.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_layout::prelude::*;
    ///
    /// fn my_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let overlay_mgr = ctx.overlay_manager();
    ///
    ///     div()
    ///         .child(
    ///             button("Show Modal").on_click({
    ///                 let mgr = overlay_mgr.clone();
    ///                 move |_| {
    ///                     mgr.modal()
    ///                         .content(|| {
    ///                             div().p(20.0).bg(Color::WHITE)
    ///                                 .child(text("Hello from modal!"))
    ///                         })
    ///                         .show();
    ///                 }
    ///             })
    ///         )
    /// }
    /// ```
    pub fn overlay_manager(&self) -> OverlayManager {
        Arc::clone(&self.overlay_manager)
    }

    // =========================================================================
    // Query API
    // =========================================================================

    /// Query an element by ID and get an ElementHandle for programmatic manipulation
    ///
    /// Returns an `ElementHandle` for interacting with the element. The handle
    /// provides methods like `scroll_into_view()`, `focus()`, `click()`, `on_ready()`,
    /// and tree traversal.
    ///
    /// The handle works even if the element doesn't exist yet - operations like
    /// `on_ready()` will queue until the element is laid out. Use `handle.exists()`
    /// to check if the element currently exists.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Register on_ready callback (works before element exists):
    /// ctx.query("progress-bar").on_ready(|bounds| {
    ///     progress_anim.lock().unwrap().set_target(bounds.width * 0.75);
    /// });
    ///
    /// // In UI builder:
    /// div().id("progress-bar").child(...)
    ///
    /// // Later, interact with existing element:
    /// let handle = ctx.query("my-element");
    /// if handle.exists() {
    ///     handle.scroll_into_view();
    ///     handle.focus();
    /// }
    /// ```
    pub fn query(&self, id: &str) -> blinc_layout::selector::ElementHandle<()> {
        blinc_layout::selector::ElementHandle::new(id, self.element_registry.clone())
    }

    /// Get the shared element registry
    ///
    /// This provides access to the element registry for advanced query operations.
    pub fn element_registry(&self) -> &SharedElementRegistry {
        &self.element_registry
    }

    /// Create a persistent state for stateful UI elements
    ///
    /// This creates a `SharedState<S>` that survives across UI rebuilds.
    /// State is keyed automatically by source location using `#[track_caller]`.
    ///
    /// Use with `stateful()` for the cleanest API:
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_layout::prelude::*;
    ///
    /// fn my_button(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let handle = ctx.use_state(ButtonState::Idle);
    ///
    ///     stateful(handle)
    ///         .on_state(|state, div| {
    ///             match state {
    ///                 ButtonState::Hovered => { *div = div.swap().bg(Color::RED); }
    ///                 _ => { *div = div.swap().bg(Color::BLUE); }
    ///             }
    ///         })
    /// }
    /// ```
    #[track_caller]
    pub fn use_state<S>(&self, initial: S) -> blinc_layout::SharedState<S>
    where
        S: blinc_layout::StateTransitions + Clone + Send + 'static,
    {
        // Use caller location as the key
        let location = std::panic::Location::caller();
        let key = format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
        self.use_state_for(&key, initial)
    }

    /// Create a persistent state with an explicit key
    ///
    /// Use this for reusable components that are called multiple times
    /// from the same location (e.g., in a loop or when the same component
    /// function is called multiple times with different props).
    ///
    /// The key can be any type that implements `Hash` (strings, numbers, etc).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Reusable component - string key
    /// fn feature_card(ctx: &WindowedContext, id: &str) -> impl ElementBuilder {
    ///     let handle = ctx.use_state_for(id, ButtonState::Idle);
    ///     stateful(handle).on_state(|state, div| { ... })
    /// }
    ///
    /// // Or with numeric key in a loop
    /// for i in 0..3 {
    ///     let handle = ctx.use_state_for(i, ButtonState::Idle);
    ///     // ...
    /// }
    /// ```
    pub fn use_state_for<K, S>(&self, key: K, initial: S) -> blinc_layout::SharedState<S>
    where
        K: Hash,
        S: blinc_layout::StateTransitions + Clone + Send + 'static,
    {
        use blinc_core::reactive::SignalId;
        use blinc_layout::stateful::StatefulInner;

        // We store the SharedState<S> as a signal value
        let state_key = StateKey::new::<blinc_layout::SharedState<S>, _>(&key);
        let mut hooks = self.hooks.lock().unwrap();

        if let Some(raw_id) = hooks.get(&state_key) {
            // Existing state - get the SharedState from the signal
            let signal_id = SignalId::from_raw(raw_id);
            let signal: Signal<blinc_layout::SharedState<S>> = Signal::from_id(signal_id);
            self.reactive.lock().unwrap().get(signal).unwrap()
        } else {
            // New state - create SharedState and store in signal
            let shared_state: blinc_layout::SharedState<S> =
                Arc::new(Mutex::new(StatefulInner::new(initial)));
            let signal = self
                .reactive
                .lock()
                .unwrap()
                .create_signal(shared_state.clone());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            shared_state
        }
    }

    /// Create a persistent animated value using caller location as key
    ///
    /// The animated value survives UI rebuilds, preserving its current value
    /// and active spring animations. This is essential for continuous animations
    /// driven by state changes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Animated value persists across rebuilds
    /// let offset_y = ctx.use_animated_value(0.0, SpringConfig::wobbly());
    ///
    /// // Can be used in motion bindings
    /// motion().translate_y(offset_y.clone()).child(content)
    /// ```
    #[track_caller]
    pub fn use_animated_value(&self, initial: f32, config: SpringConfig) -> SharedAnimatedValue {
        let location = std::panic::Location::caller();
        let key = format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
        self.use_animated_value_for(&key, initial, config)
    }

    /// Create a persistent animated value with an explicit key
    ///
    /// Use this for reusable components or when creating multiple animated
    /// values at the same source location (e.g., in a loop).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Multiple animated values with unique keys
    /// for i in 0..3 {
    ///     let scale = ctx.use_animated_value_for(
    ///         format!("item_{}_scale", i),
    ///         1.0,
    ///         SpringConfig::snappy(),
    ///     );
    /// }
    /// ```
    pub fn use_animated_value_for<K: Hash>(
        &self,
        key: K,
        initial: f32,
        config: SpringConfig,
    ) -> SharedAnimatedValue {
        use blinc_core::reactive::SignalId;

        // Use a type marker for SharedAnimatedValue
        let state_key = StateKey::new::<SharedAnimatedValue, _>(&key);
        let mut hooks = self.hooks.lock().unwrap();

        if let Some(raw_id) = hooks.get(&state_key) {
            // Existing animated value - retrieve from signal
            let signal_id = SignalId::from_raw(raw_id);
            let signal: Signal<SharedAnimatedValue> = Signal::from_id(signal_id);
            self.reactive.lock().unwrap().get(signal).unwrap()
        } else {
            // New animated value - create and store in signal
            let animated_value: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
                self.animation_handle(),
                initial,
                config,
            )));
            let signal = self
                .reactive
                .lock()
                .unwrap()
                .create_signal(animated_value.clone());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            animated_value
        }
    }

    /// Create or retrieve a persistent animated timeline
    ///
    /// AnimatedTimeline provides keyframe-based animations that persist across
    /// UI rebuilds. Use this for timeline animations that need to survive
    /// layout changes and window resizes.
    ///
    /// The returned timeline is empty on first call - add keyframes using
    /// `timeline.add()` then call `start()`. Use `has_entries()` to check
    /// if the timeline needs configuration.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let timeline = ctx.use_animated_timeline();
    /// let entry_id = {
    ///     let mut t = timeline.lock().unwrap();
    ///     if !t.has_entries() {
    ///         let id = t.add(0, 2000, 0.0, 1.0);
    ///         t.start();
    ///         id
    ///     } else {
    ///         t.entry_ids().first().copied().unwrap()
    ///     }
    /// };
    /// ```
    #[track_caller]
    pub fn use_animated_timeline(&self) -> SharedAnimatedTimeline {
        let location = std::panic::Location::caller();
        let key = format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
        self.use_animated_timeline_for(&key)
    }

    /// Create or retrieve a persistent animated timeline with an explicit key
    ///
    /// Use this for reusable components or when creating multiple timelines
    /// at the same source location (e.g., in a loop).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Multiple timelines with unique keys
    /// for i in 0..3 {
    ///     let timeline = ctx.use_animated_timeline_for(format!("dot_{}", i));
    ///     // ...
    /// }
    /// ```
    pub fn use_animated_timeline_for<K: Hash>(&self, key: K) -> SharedAnimatedTimeline {
        use blinc_core::reactive::SignalId;

        // Use a type marker for SharedAnimatedTimeline
        let state_key = StateKey::new::<SharedAnimatedTimeline, _>(&key);
        let mut hooks = self.hooks.lock().unwrap();

        if let Some(raw_id) = hooks.get(&state_key) {
            // Existing timeline - retrieve from signal
            let signal_id = SignalId::from_raw(raw_id);
            let signal: Signal<SharedAnimatedTimeline> = Signal::from_id(signal_id);
            self.reactive.lock().unwrap().get(signal).unwrap()
        } else {
            // New timeline - create and store in signal
            let timeline: SharedAnimatedTimeline =
                Arc::new(Mutex::new(AnimatedTimeline::new(self.animation_handle())));
            let signal = self
                .reactive
                .lock()
                .unwrap()
                .create_signal(timeline.clone());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            timeline
        }
    }

    // =========================================================================
    // Theme API
    // =========================================================================

    /// Get the current color scheme (light or dark)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let scheme = ctx.color_scheme();
    /// match scheme {
    ///     ColorScheme::Light => println!("Light mode"),
    ///     ColorScheme::Dark => println!("Dark mode"),
    /// }
    /// ```
    pub fn color_scheme(&self) -> blinc_theme::ColorScheme {
        blinc_theme::ThemeState::get().scheme()
    }

    /// Set the color scheme (triggers smooth theme transition)
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.set_color_scheme(ColorScheme::Dark);
    /// ```
    pub fn set_color_scheme(&self, scheme: blinc_theme::ColorScheme) {
        blinc_theme::ThemeState::get().set_scheme(scheme);
    }

    /// Toggle between light and dark mode
    ///
    /// # Example
    ///
    /// ```ignore
    /// button("Toggle Theme").on_click(|ctx| {
    ///     ctx.toggle_color_scheme();
    /// })
    /// ```
    pub fn toggle_color_scheme(&self) {
        blinc_theme::ThemeState::get().toggle_scheme();
    }

    /// Get a color from the current theme
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_theme::ColorToken;
    ///
    /// let primary = ctx.theme_color(ColorToken::Primary);
    /// let bg = ctx.theme_color(ColorToken::Background);
    /// ```
    pub fn theme_color(&self, token: blinc_theme::ColorToken) -> blinc_core::Color {
        blinc_theme::ThemeState::get().color(token)
    }

    /// Get spacing from the current theme
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_theme::SpacingToken;
    ///
    /// let padding = ctx.theme_spacing(SpacingToken::Space4); // 16px
    /// ```
    pub fn theme_spacing(&self, token: blinc_theme::SpacingToken) -> f32 {
        blinc_theme::ThemeState::get().spacing_value(token)
    }

    /// Get border radius from the current theme
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_theme::RadiusToken;
    ///
    /// let radius = ctx.theme_radius(RadiusToken::Lg); // 8px
    /// ```
    pub fn theme_radius(&self, token: blinc_theme::RadiusToken) -> f32 {
        blinc_theme::ThemeState::get().radius(token)
    }
}

// =============================================================================
// BlincContext Implementation
// =============================================================================

impl blinc_core::BlincContext for WindowedContext {
    fn use_state_keyed<T, F>(&self, key: &str, init: F) -> State<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        // Delegate to the existing method
        WindowedContext::use_state_keyed(self, key, init)
    }

    fn use_signal_keyed<T, F>(&self, key: &str, init: F) -> Signal<T>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> T,
    {
        WindowedContext::use_signal_keyed(self, key, init)
    }

    fn use_signal<T: Send + 'static>(&self, initial: T) -> Signal<T> {
        WindowedContext::use_signal(self, initial)
    }

    fn get<T: Clone + 'static>(&self, signal: Signal<T>) -> Option<T> {
        WindowedContext::get(self, signal)
    }

    fn set<T: Send + 'static>(&self, signal: Signal<T>, value: T) {
        WindowedContext::set(self, signal, value)
    }

    fn update<T: Clone + Send + 'static, F: FnOnce(T) -> T>(&self, signal: Signal<T>, f: F) {
        WindowedContext::update(self, signal, f)
    }

    fn use_derived<T, F>(&self, compute: F) -> Derived<T>
    where
        T: Clone + Send + 'static,
        F: Fn(&ReactiveGraph) -> T + Send + 'static,
    {
        WindowedContext::use_derived(self, compute)
    }

    fn get_derived<T: Clone + 'static>(&self, derived: Derived<T>) -> Option<T> {
        WindowedContext::get_derived(self, derived)
    }

    fn batch<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ReactiveGraph) -> R,
    {
        WindowedContext::batch(self, f)
    }

    fn dirty_flag(&self) -> blinc_core::DirtyFlag {
        WindowedContext::dirty_flag(self)
    }

    fn request_rebuild(&self) {
        self.ref_dirty_flag.store(true, Ordering::SeqCst);
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn scale_factor(&self) -> f64 {
        self.scale_factor
    }
}

// =============================================================================
// AnimationContext Implementation
// =============================================================================

impl AnimationContext for WindowedContext {
    fn animation_handle(&self) -> SchedulerHandle {
        WindowedContext::animation_handle(self)
    }

    fn use_animated_value_for<K: Hash>(
        &self,
        key: K,
        initial: f32,
        config: SpringConfig,
    ) -> SharedAnimatedValue {
        WindowedContext::use_animated_value_for(self, key, initial, config)
    }

    fn use_animated_timeline_for<K: Hash>(&self, key: K) -> SharedAnimatedTimeline {
        WindowedContext::use_animated_timeline_for(self, key)
    }
}

/// Windowed application runner
///
/// Provides a simple way to run a Blinc application in a window
/// with automatic event handling and rendering.
pub struct WindowedApp;

impl WindowedApp {
    /// Initialize the platform asset loader
    ///
    /// On desktop, this sets up a filesystem-based loader.
    /// On Android, this would use the NDK AssetManager.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn init_asset_loader() {
        use blinc_platform::assets::{set_global_asset_loader, FilesystemAssetLoader};

        // Create a filesystem loader (uses current directory as base)
        let loader = FilesystemAssetLoader::new();

        // Try to set the global loader (ignore error if already set)
        let _ = set_global_asset_loader(Box::new(loader));
    }

    /// Initialize the theme system with platform detection
    ///
    /// This sets up the global ThemeState with:
    /// - Platform-appropriate theme bundle (macOS, Windows, Linux, etc.)
    /// - System color scheme detection (light/dark mode)
    /// - Redraw callback to trigger UI updates on theme changes
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn init_theme() {
        use blinc_theme::{
            detect_system_color_scheme, platform_theme_bundle, set_redraw_callback, ThemeState,
        };

        // Only initialize if not already initialized
        if ThemeState::try_get().is_none() {
            let bundle = platform_theme_bundle();
            let scheme = detect_system_color_scheme();
            ThemeState::init(bundle, scheme);
        }

        // Set up the redraw callback to trigger full UI rebuilds when theme changes
        // We use request_full_rebuild() to trigger all three phases:
        // 1. Tree rebuild - reconstruct UI with new theme values
        // 2. Layout recompute - recalculate flexbox layout
        // 3. Visual redraw - render the frame
        set_redraw_callback(|| {
            tracing::debug!("Theme changed - requesting full rebuild");
            blinc_layout::widgets::request_full_rebuild();
        });
    }

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
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    pub fn run<F, E>(config: WindowConfig, ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder,
    {
        Self::run_desktop(config, ui_builder)
    }

    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn run_desktop<F, E>(config: WindowConfig, mut ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder,
    {
        // Initialize the platform asset loader for cross-platform asset loading
        Self::init_asset_loader();

        // Initialize the text measurer for accurate text layout
        crate::text_measurer::init_text_measurer();

        // Initialize the theme system with platform detection
        Self::init_theme();

        let platform = DesktopPlatform::new().map_err(|e| BlincError::Platform(e.to_string()))?;
        let event_loop = platform
            .create_event_loop_with_config(config)
            .map_err(|e| BlincError::Platform(e.to_string()))?;

        // Get a wake proxy to allow the animation thread to wake up the event loop
        let wake_proxy = event_loop.wake_proxy();

        // We need to defer BlincApp creation until we have a window
        let mut app: Option<BlincApp> = None;
        let mut surface: Option<wgpu::Surface<'static>> = None;
        let mut surface_config: Option<wgpu::SurfaceConfiguration> = None;

        // Persistent context with event router
        let mut ctx: Option<WindowedContext> = None;
        // Persistent render tree for hit testing and dirty tracking
        let mut render_tree: Option<RenderTree> = None;
        // Track if we need to rebuild UI (e.g., after resize)
        let mut needs_rebuild = true;
        // Track if we need to relayout (e.g., after resize even if tree unchanged)
        let mut needs_relayout = false;
        // Shared dirty flag for element refs
        let ref_dirty_flag: RefDirtyFlag = Arc::new(AtomicBool::new(false));
        // Shared reactive graph for signal-based state management
        let reactive: SharedReactiveGraph = Arc::new(Mutex::new(ReactiveGraph::new()));
        // Shared hook state for use_state persistence
        let hooks: SharedHookState = Arc::new(Mutex::new(HookState::new()));

        // Initialize global context state singleton (if not already initialized)
        // This allows components to create internal state without context parameters
        if !BlincContextState::is_initialized() {
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

        // Shared animation scheduler for spring/keyframe animations
        // Runs on background thread so animations continue even when window loses focus
        let mut scheduler = AnimationScheduler::new();
        // Set up wake callback so animation thread can wake the event loop
        scheduler.set_wake_callback(move || wake_proxy.wake());
        scheduler.start_background();
        let animations: SharedAnimationScheduler = Arc::new(Mutex::new(scheduler));
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

        // Overlay manager for modals, dialogs, toasts, etc.
        let overlays: OverlayManager = overlay_manager();

        // Initialize overlay context singleton for component access
        if !OverlayContext::is_initialized() {
            OverlayContext::init(Arc::clone(&overlays));
        }

        event_loop
            .run(move |event, window| {
                match event {
                    Event::Lifecycle(LifecycleEvent::Resumed) => {
                        // Initialize GPU if not already done
                        if app.is_none() {
                            let winit_window = window.winit_window_arc();

                            match BlincApp::with_window(winit_window, None) {
                                Ok((blinc_app, surf)) => {
                                    let (width, height) = window.size();
                                    // Use the same texture format that the renderer's pipelines use
                                    let format = blinc_app.texture_format();
                                    let config = wgpu::SurfaceConfiguration {
                                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                        format,
                                        width,
                                        height,
                                        present_mode: wgpu::PresentMode::AutoVsync,
                                        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                                        view_formats: vec![],
                                        desired_maximum_frame_latency: 2,
                                    };
                                    surf.configure(&blinc_app.device(), &config);

                                    // Update text measurer with shared font registry for accurate measurement
                                    crate::text_measurer::init_text_measurer_with_registry(
                                        blinc_app.font_registry(),
                                    );

                                    surface = Some(surf);
                                    surface_config = Some(config);
                                    app = Some(blinc_app);

                                    // Initialize context with event router, animations, dirty flag, reactive graph, hooks, overlay manager, registry, and ready callbacks
                                    ctx = Some(WindowedContext::from_window(
                                        window,
                                        EventRouter::new(),
                                        Arc::clone(&animations),
                                        Arc::clone(&ref_dirty_flag),
                                        Arc::clone(&reactive),
                                        Arc::clone(&hooks),
                                        Arc::clone(&overlays),
                                        Arc::clone(&element_registry),
                                        Arc::clone(&ready_callbacks),
                                    ));

                                    // Set initial viewport size in BlincContextState
                                    if let Some(ref windowed_ctx) = ctx {
                                        BlincContextState::get().set_viewport_size(windowed_ctx.width, windowed_ctx.height);
                                    }

                                    // Initialize render state with the shared animation scheduler
                                    // RenderState handles dynamic properties (cursor blink, animations)
                                    // independently from tree structure changes
                                    render_state = Some(blinc_layout::RenderState::new(Arc::clone(&animations)));

                                    tracing::debug!("Blinc windowed app initialized");
                                }
                                Err(e) => {
                                    tracing::error!("Failed to initialize Blinc: {}", e);
                                    return ControlFlow::Exit;
                                }
                            }
                        }
                    }

                    Event::Window(WindowEvent::Resized { width, height }) => {
                        if let (Some(ref blinc_app), Some(ref surf), Some(ref mut config)) =
                            (&app, &surface, &mut surface_config)
                        {
                            if width > 0 && height > 0 {
                                config.width = width;
                                config.height = height;
                                surf.configure(&blinc_app.device(), config);
                                needs_rebuild = true;
                                needs_relayout = true;

                                // Dispatch RESIZE event to elements (use logical dimensions)
                                if let (Some(ref mut windowed_ctx), Some(ref tree)) =
                                    (&mut ctx, &render_tree)
                                {
                                    let logical_width = width as f32 / windowed_ctx.scale_factor as f32;
                                    let logical_height = height as f32 / windowed_ctx.scale_factor as f32;

                                    // Update windowed context dimensions - CRITICAL for layout computation
                                    // Without this, compute_layout uses stale dimensions
                                    windowed_ctx.width = logical_width;
                                    windowed_ctx.height = logical_height;
                                    windowed_ctx.physical_width = width as f32;
                                    windowed_ctx.physical_height = height as f32;

                                    // Update viewport size in BlincContextState for ElementHandle.is_visible()
                                    BlincContextState::get().set_viewport_size(logical_width, logical_height);

                                    windowed_ctx
                                        .event_router
                                        .on_window_resize(tree, logical_width, logical_height);

                                    // Clear layout bounds storages to force fresh calculations
                                    // This prevents stale cached bounds from influencing the new layout
                                    tree.clear_layout_bounds_storages();
                                }

                                // Request redraw to trigger relayout with new dimensions
                                window.request_redraw();
                            }
                        }
                    }

                    Event::Window(WindowEvent::Focused(focused)) => {
                        // Update context focus state
                        if let Some(ref mut windowed_ctx) = ctx {
                            windowed_ctx.focused = focused;

                            // Dispatch WINDOW_FOCUS or WINDOW_BLUR to the focused element
                            windowed_ctx.event_router.on_window_focus(focused);

                            // When window loses focus, blur all text inputs/areas
                            if !focused {
                                blinc_layout::widgets::blur_all_text_inputs();
                            }
                        }
                    }

                    Event::Window(WindowEvent::CloseRequested) => {
                        return ControlFlow::Exit;
                    }

                    // Handle input events
                    Event::Input(input_event) => {
                        // Pending event structure for deferred dispatch
                        #[derive(Clone)]
                        struct PendingEvent {
                            node_id: LayoutNodeId,
                            event_type: u32,
                            mouse_x: f32,
                            mouse_y: f32,
                            /// Local coordinates relative to element bounds
                            local_x: f32,
                            local_y: f32,
                            /// Absolute position of element bounds (top-left corner)
                            bounds_x: f32,
                            bounds_y: f32,
                            /// Computed bounds dimensions of the element
                            bounds_width: f32,
                            bounds_height: f32,
                            scroll_delta_x: f32,
                            scroll_delta_y: f32,
                            /// Drag delta for DRAG/DRAG_END events
                            drag_delta_x: f32,
                            drag_delta_y: f32,
                            key_char: Option<char>,
                            key_code: u32,
                            shift: bool,
                            ctrl: bool,
                            alt: bool,
                            meta: bool,
                        }

                        impl Default for PendingEvent {
                            fn default() -> Self {
                                Self {
                                    node_id: LayoutNodeId::default(),
                                    event_type: 0,
                                    mouse_x: 0.0,
                                    mouse_y: 0.0,
                                    local_x: 0.0,
                                    local_y: 0.0,
                                    bounds_x: 0.0,
                                    bounds_y: 0.0,
                                    bounds_width: 0.0,
                                    bounds_height: 0.0,
                                    scroll_delta_x: 0.0,
                                    scroll_delta_y: 0.0,
                                    drag_delta_x: 0.0,
                                    drag_delta_y: 0.0,
                                    key_char: None,
                                    key_code: 0,
                                    shift: false,
                                    ctrl: false,
                                    alt: false,
                                    meta: false,
                                }
                            }
                        }

                        // First phase: collect events using immutable borrow
                        let (pending_events, keyboard_events, pending_overlay_events, scroll_ended, gesture_ended, scroll_info) = if let (Some(ref mut windowed_ctx), Some(ref tree)) =
                            (&mut ctx, &render_tree)
                        {
                            let router = &mut windowed_ctx.event_router;

                            // Collect events from router
                            let mut pending_events: Vec<PendingEvent> = Vec::new();
                            // Separate collection for keyboard events (TEXT_INPUT)
                            let mut keyboard_events: Vec<PendingEvent> = Vec::new();
                            // Separate collection for overlay events
                            let mut pending_overlay_events: Vec<PendingEvent> = Vec::new();
                            // Track if scroll ended (momentum finished)
                            let mut scroll_ended = false;
                            // Track if gesture ended (finger lifted - may still have momentum)
                            let mut gesture_ended = false;
                            // Track scroll info for nested scroll dispatch (mouse_x, mouse_y, delta_x, delta_y)
                            let mut scroll_info: Option<(f32, f32, f32, f32)> = None;

                            // Set up callback to collect events
                            router.set_event_callback({
                                let events = &mut pending_events as *mut Vec<PendingEvent>;
                                move |node, event_type| {
                                    // SAFETY: This callback is only used within this scope
                                    unsafe {
                                        (*events).push(PendingEvent {
                                            node_id: node,
                                            event_type,
                                            ..Default::default()
                                        });
                                    }
                                }
                            });

                            // Set up callback to collect overlay events
                            windowed_ctx.overlay_event_router.set_event_callback({
                                let events = &mut pending_overlay_events as *mut Vec<PendingEvent>;
                                move |node, event_type| {
                                    // SAFETY: This callback is only used within this scope
                                    unsafe {
                                        (*events).push(PendingEvent {
                                            node_id: node,
                                            event_type,
                                            ..Default::default()
                                        });
                                    }
                                }
                            });

                            // Convert physical coordinates to logical for hit testing
                            let scale = windowed_ctx.scale_factor as f32;

                            match input_event {
                                InputEvent::Mouse(mouse_event) => match mouse_event {
                                    MouseEvent::Moved { x, y } => {
                                        // Convert physical to logical coordinates
                                        let lx = x / scale;
                                        let ly = y / scale;

                                        // Route to overlay tree first if visible
                                        if windowed_ctx.overlay_manager.has_visible_overlays() {
                                            // Build/update overlay tree for event routing
                                            // Use is_dirty() to peek without consuming (render phase will take it)
                                            if windowed_ctx.overlay_tree.is_none() || windowed_ctx.overlay_manager.is_dirty() {
                                                windowed_ctx.overlay_tree = windowed_ctx.overlay_manager.build_overlay_tree();
                                            }
                                            if let Some(ref overlay_tree) = windowed_ctx.overlay_tree {
                                                windowed_ctx.overlay_event_router.on_mouse_move(overlay_tree, lx, ly);
                                                // Update cursor from overlay if hovering over overlay content
                                                let overlay_cursor = overlay_tree
                                                    .get_cursor_at(&windowed_ctx.overlay_event_router, lx, ly)
                                                    .unwrap_or(CursorStyle::Default);
                                                if overlay_cursor != CursorStyle::Default {
                                                    window.set_cursor(convert_cursor_style(overlay_cursor));
                                                }
                                            }
                                        }

                                        router.on_mouse_move(tree, lx, ly);

                                        // Get drag delta from router (for DRAG events)
                                        let (drag_dx, drag_dy) = router.drag_delta();

                                        for event in pending_events.iter_mut() {
                                            event.mouse_x = lx;
                                            event.mouse_y = ly;
                                            // Populate drag delta for DRAG events
                                            if event.event_type == blinc_core::events::event_types::DRAG
                                                || event.event_type == blinc_core::events::event_types::DRAG_END
                                            {
                                                event.drag_delta_x = drag_dx;
                                                event.drag_delta_y = drag_dy;
                                            }
                                        }

                                        // Update overlay events with mouse position
                                        for event in pending_overlay_events.iter_mut() {
                                            event.mouse_x = lx;
                                            event.mouse_y = ly;
                                        }

                                        // Update cursor based on hovered element (only if overlay didn't set one)
                                        if !windowed_ctx.overlay_manager.has_visible_overlays() {
                                            let cursor = tree
                                                .get_cursor_at(router, lx, ly)
                                                .unwrap_or(CursorStyle::Default);
                                            window.set_cursor(convert_cursor_style(cursor));
                                        }
                                    }
                                    MouseEvent::ButtonPressed { button, x, y } => {
                                        let lx = x / scale;
                                        let ly = y / scale;
                                        let btn = convert_mouse_button(button);

                                        // Check for blocking overlay (Modal/Dialog with backdrop)
                                        // These block all clicks to the main UI - only modal content receives events
                                        if windowed_ctx.overlay_manager.has_blocking_overlay() {
                                            // Check if click is on backdrop (outside content) - dismisses if so
                                            let dismissed = windowed_ctx.overlay_manager.handle_click_at(lx, ly);
                                            // Route click to overlay content if not on backdrop
                                            if !dismissed {
                                                if let Some(ref overlay_tree) = windowed_ctx.overlay_tree {
                                                    windowed_ctx.overlay_event_router.on_mouse_down(overlay_tree, lx, ly, btn);
                                                }
                                            }
                                        } else {
                                            // Check for dismissable overlays (dropdowns, context menus)
                                            // If click is on backdrop (outside overlay content), dismiss and block the click
                                            // This prevents opening a new dropdown while dismissing the old one
                                            let overlay_dismissed = if windowed_ctx.overlay_manager.has_dismissable_overlay() {
                                                windowed_ctx.overlay_manager.handle_click_at(lx, ly)
                                            } else {
                                                false
                                            };

                                            // If overlay was dismissed, we don't process the click further
                                            // If not dismissed, route click to overlay content first (for dropdown items)
                                            if !overlay_dismissed {
                                                // Route click to overlay content if visible
                                                if windowed_ctx.overlay_manager.has_visible_overlays() {
                                                    if let Some(ref overlay_tree) = windowed_ctx.overlay_tree {
                                                        windowed_ctx.overlay_event_router.on_mouse_down(overlay_tree, lx, ly, btn);
                                                    }
                                                } else {
                                                    // No overlay visible - route to main tree
                                                    // Blur any focused text inputs BEFORE processing mouse down
                                                    // This mimics HTML behavior where clicking anywhere blurs inputs,
                                                    // and clicking on an input then re-focuses it via its own handler
                                                    blinc_layout::widgets::blur_all_text_inputs();

                                                    let _events = router.on_mouse_down(tree, lx, ly, btn);

                                                    let (local_x, local_y) = router.last_hit_local();
                                                    let (bounds_x, bounds_y) = router.last_hit_bounds_pos();
                                                    let (bounds_width, bounds_height) = router.last_hit_bounds();
                                                    for event in pending_events.iter_mut() {
                                                        event.mouse_x = lx;
                                                        event.mouse_y = ly;
                                                        event.local_x = local_x;
                                                        event.local_y = local_y;
                                                        event.bounds_x = bounds_x;
                                                        event.bounds_y = bounds_y;
                                                        event.bounds_width = bounds_width;
                                                        event.bounds_height = bounds_height;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    MouseEvent::ButtonReleased { button, x, y } => {
                                        let lx = x / scale;
                                        let ly = y / scale;
                                        let btn = convert_mouse_button(button);

                                        // Route mouse up to overlay tree if visible
                                        if windowed_ctx.overlay_manager.has_visible_overlays() {
                                            if let Some(ref overlay_tree) = windowed_ctx.overlay_tree {
                                                windowed_ctx.overlay_event_router.on_mouse_up(overlay_tree, lx, ly, btn);
                                            }
                                        }

                                        router.on_mouse_up(tree, lx, ly, btn);
                                        // Use the local coordinates from when the press started
                                        // (stored by on_mouse_down via last_hit_local)
                                        let (local_x, local_y) = router.last_hit_local();
                                        let (bounds_x, bounds_y) = router.last_hit_bounds_pos();
                                        let (bounds_width, bounds_height) = router.last_hit_bounds();
                                        for event in pending_events.iter_mut() {
                                            event.mouse_x = lx;
                                            event.mouse_y = ly;
                                            event.local_x = local_x;
                                            event.local_y = local_y;
                                            event.bounds_x = bounds_x;
                                            event.bounds_y = bounds_y;
                                            event.bounds_width = bounds_width;
                                            event.bounds_height = bounds_height;
                                        }
                                    }
                                    MouseEvent::Left => {
                                        // on_mouse_leave now emits POINTER_UP if there was a pressed target
                                        // This handles the case where mouse leaves window while dragging
                                        router.on_mouse_leave();
                                        // Reset cursor to default when mouse leaves window
                                        window.set_cursor(blinc_platform::Cursor::Default);
                                        // Events are collected via the callback set above
                                    }
                                    MouseEvent::Entered => {
                                        let (mx, my) = router.mouse_position();
                                        router.on_mouse_move(tree, mx, my);
                                        for event in pending_events.iter_mut() {
                                            event.mouse_x = mx;
                                            event.mouse_y = my;
                                        }

                                        // Update cursor based on hovered element
                                        let cursor = tree
                                            .get_cursor_at(router, mx, my)
                                            .unwrap_or(CursorStyle::Default);
                                        window.set_cursor(convert_cursor_style(cursor));
                                    }
                                },
                                InputEvent::Keyboard(kb_event) => {
                                    let mods = &kb_event.modifiers;

                                    // Extract character from key if applicable
                                    let key_char = match &kb_event.key {
                                        Key::Char(c) => Some(*c),
                                        Key::Space => Some(' '),
                                        Key::A => Some(if mods.shift { 'A' } else { 'a' }),
                                        Key::B => Some(if mods.shift { 'B' } else { 'b' }),
                                        Key::C => Some(if mods.shift { 'C' } else { 'c' }),
                                        Key::D => Some(if mods.shift { 'D' } else { 'd' }),
                                        Key::E => Some(if mods.shift { 'E' } else { 'e' }),
                                        Key::F => Some(if mods.shift { 'F' } else { 'f' }),
                                        Key::G => Some(if mods.shift { 'G' } else { 'g' }),
                                        Key::H => Some(if mods.shift { 'H' } else { 'h' }),
                                        Key::I => Some(if mods.shift { 'I' } else { 'i' }),
                                        Key::J => Some(if mods.shift { 'J' } else { 'j' }),
                                        Key::K => Some(if mods.shift { 'K' } else { 'k' }),
                                        Key::L => Some(if mods.shift { 'L' } else { 'l' }),
                                        Key::M => Some(if mods.shift { 'M' } else { 'm' }),
                                        Key::N => Some(if mods.shift { 'N' } else { 'n' }),
                                        Key::O => Some(if mods.shift { 'O' } else { 'o' }),
                                        Key::P => Some(if mods.shift { 'P' } else { 'p' }),
                                        Key::Q => Some(if mods.shift { 'Q' } else { 'q' }),
                                        Key::R => Some(if mods.shift { 'R' } else { 'r' }),
                                        Key::S => Some(if mods.shift { 'S' } else { 's' }),
                                        Key::T => Some(if mods.shift { 'T' } else { 't' }),
                                        Key::U => Some(if mods.shift { 'U' } else { 'u' }),
                                        Key::V => Some(if mods.shift { 'V' } else { 'v' }),
                                        Key::W => Some(if mods.shift { 'W' } else { 'w' }),
                                        Key::X => Some(if mods.shift { 'X' } else { 'x' }),
                                        Key::Y => Some(if mods.shift { 'Y' } else { 'y' }),
                                        Key::Z => Some(if mods.shift { 'Z' } else { 'z' }),
                                        Key::Num0 => Some(if mods.shift { ')' } else { '0' }),
                                        Key::Num1 => Some(if mods.shift { '!' } else { '1' }),
                                        Key::Num2 => Some(if mods.shift { '@' } else { '2' }),
                                        Key::Num3 => Some(if mods.shift { '#' } else { '3' }),
                                        Key::Num4 => Some(if mods.shift { '$' } else { '4' }),
                                        Key::Num5 => Some(if mods.shift { '%' } else { '5' }),
                                        Key::Num6 => Some(if mods.shift { '^' } else { '6' }),
                                        Key::Num7 => Some(if mods.shift { '&' } else { '7' }),
                                        Key::Num8 => Some(if mods.shift { '*' } else { '8' }),
                                        Key::Num9 => Some(if mods.shift { '(' } else { '9' }),
                                        Key::Minus => Some(if mods.shift { '_' } else { '-' }),
                                        Key::Equals => Some(if mods.shift { '+' } else { '=' }),
                                        Key::LeftBracket => Some(if mods.shift { '{' } else { '[' }),
                                        Key::RightBracket => Some(if mods.shift { '}' } else { ']' }),
                                        Key::Backslash => Some(if mods.shift { '|' } else { '\\' }),
                                        Key::Semicolon => Some(if mods.shift { ':' } else { ';' }),
                                        Key::Quote => Some(if mods.shift { '"' } else { '\'' }),
                                        Key::Comma => Some(if mods.shift { '<' } else { ',' }),
                                        Key::Period => Some(if mods.shift { '>' } else { '.' }),
                                        Key::Slash => Some(if mods.shift { '?' } else { '/' }),
                                        Key::Grave => Some(if mods.shift { '~' } else { '`' }),
                                        _ => None,
                                    };

                                    // Key code for special key handling (backspace, arrows, etc)
                                    let key_code = match &kb_event.key {
                                        Key::Backspace => 8,
                                        Key::Delete => 127,
                                        Key::Enter => 13,
                                        Key::Tab => 9,
                                        Key::Escape => 27,
                                        Key::Left => 37,
                                        Key::Right => 39,
                                        Key::Up => 38,
                                        Key::Down => 40,
                                        Key::Home => 36,
                                        Key::End => 35,
                                        _ => 0,
                                    };

                                    match kb_event.state {
                                        KeyState::Pressed => {
                                            // Handle Escape key for overlays first
                                            // If an overlay handles it, don't propagate further
                                            if kb_event.key == Key::Escape {
                                                if windowed_ctx.overlay_manager.handle_escape() {
                                                    // Escape was consumed by overlay, skip further processing
                                                    // (but continue collecting events for non-overlay targets)
                                                }
                                            }

                                            // Dispatch KEY_DOWN for all keys
                                            router.on_key_down(key_code);

                                            // For character-producing keys, dispatch TEXT_INPUT
                                            // We use broadcast dispatch so any focused text input can receive it
                                            if let Some(c) = key_char {
                                                // Don't send text input if ctrl/cmd is held (shortcuts)
                                                if !mods.ctrl && !mods.meta {
                                                    keyboard_events.push(PendingEvent {
                                                        event_type: blinc_core::events::event_types::TEXT_INPUT,
                                                        key_char: Some(c),
                                                        key_code,
                                                        shift: mods.shift,
                                                        ctrl: mods.ctrl,
                                                        alt: mods.alt,
                                                        meta: mods.meta,
                                                        ..Default::default()
                                                    });
                                                }
                                            }

                                            // For KEY_DOWN events with special keys (backspace, arrows)
                                            if key_code != 0 {
                                                keyboard_events.push(PendingEvent {
                                                    event_type: blinc_core::events::event_types::KEY_DOWN,
                                                    key_char: None,
                                                    key_code,
                                                    shift: mods.shift,
                                                    ctrl: mods.ctrl,
                                                    alt: mods.alt,
                                                    meta: mods.meta,
                                                    ..Default::default()
                                                });
                                            }
                                        }
                                        KeyState::Released => {
                                            router.on_key_up(key_code);
                                        }
                                    }
                                },
                                InputEvent::Touch(touch_event) => match touch_event {
                                    TouchEvent::Started { x, y, .. } => {
                                        let lx = x / scale;
                                        let ly = y / scale;
                                        router.on_mouse_down(tree, lx, ly, MouseButton::Left);
                                        let (local_x, local_y) = router.last_hit_local();
                                        let (bounds_x, bounds_y) = router.last_hit_bounds_pos();
                                        let (bounds_width, bounds_height) = router.last_hit_bounds();
                                        for event in pending_events.iter_mut() {
                                            event.mouse_x = lx;
                                            event.mouse_y = ly;
                                            event.local_x = local_x;
                                            event.local_y = local_y;
                                            event.bounds_x = bounds_x;
                                            event.bounds_y = bounds_y;
                                            event.bounds_width = bounds_width;
                                            event.bounds_height = bounds_height;
                                        }
                                    }
                                    TouchEvent::Moved { x, y, .. } => {
                                        let lx = x / scale;
                                        let ly = y / scale;
                                        router.on_mouse_move(tree, lx, ly);
                                        for event in pending_events.iter_mut() {
                                            event.mouse_x = lx;
                                            event.mouse_y = ly;
                                        }
                                    }
                                    TouchEvent::Ended { x, y, .. } => {
                                        let lx = x / scale;
                                        let ly = y / scale;
                                        router.on_mouse_up(tree, lx, ly, MouseButton::Left);
                                        for event in pending_events.iter_mut() {
                                            event.mouse_x = lx;
                                            event.mouse_y = ly;
                                        }
                                    }
                                    TouchEvent::Cancelled { .. } => {
                                        // Touch cancelled - treat like mouse leave
                                        // This will emit POINTER_UP if there was a pressed target
                                        router.on_mouse_leave();
                                    }
                                },
                                InputEvent::Scroll { delta_x, delta_y, phase } => {
                                    let (mx, my) = router.mouse_position();
                                    // Scroll deltas are also in physical pixels, convert to logical
                                    let ldx = delta_x;
                                    let ldy = delta_y;

                                    tracing::trace!(
                                        "InputEvent::Scroll received: pos=({:.1}, {:.1}) delta=({:.1}, {:.1}) phase={:?}",
                                        mx, my, ldx, ldy, phase
                                    );

                                    // Check if gesture ended (finger lifted from trackpad)
                                    // This happens before momentum ends
                                    if phase == blinc_platform::ScrollPhase::Ended {
                                        gesture_ended = true;
                                    }

                                    // Use nested scroll support - get hit result for smart dispatch
                                    // Store mouse position and delta for dispatch phase
                                    // We'll re-do hit test in dispatch phase since we need mutable borrow
                                    scroll_info = Some((mx, my, ldx, ldy));
                                }
                                InputEvent::ScrollEnd => {
                                    // Scroll momentum ended - full stop
                                    scroll_ended = true;
                                }
                            }

                            router.clear_event_callback();
                            windowed_ctx.overlay_event_router.clear_event_callback();
                            (pending_events, keyboard_events, pending_overlay_events, scroll_ended, gesture_ended, scroll_info)
                        } else {
                            (Vec::new(), Vec::new(), Vec::new(), false, false, None)
                        };

                        // Second phase: dispatch events with mutable borrow
                        // This automatically marks the tree dirty when handlers fire
                        if let Some(ref mut tree) = render_tree {
                            // IMPORTANT: Process gesture_ended BEFORE scroll delta dispatch
                            // When gesture ends while overscrolling, we start bounce which
                            // sets state to Bouncing. Then apply_scroll_delta will early-return
                            // and ignore the momentum delta that came with this same event.
                            if gesture_ended {
                                tree.on_gesture_end();
                                // Request redraw to animate bounce-back
                                window.request_redraw();
                            }

                            // Handle scroll with nested scroll support
                            // Skip scroll delta entirely if gesture just ended - the delta
                            // from the same event as gesture_ended is the last finger movement,
                            // not momentum, but we still want to ignore it for instant snap-back
                            //
                            // Also skip scroll when an overlay with backdrop is open to prevent
                            // background content from scrolling while dropdown/modal is visible.
                            let has_overlay_backdrop = ctx
                                .as_ref()
                                .map(|c| c.overlay_manager.has_blocking_overlay() || c.overlay_manager.has_dismissable_overlay())
                                .unwrap_or(false);

                            if let Some((mouse_x, mouse_y, delta_x, delta_y)) = scroll_info {
                                // Skip if gesture ended in this same event - go straight to bounce
                                if gesture_ended {
                                    tracing::trace!("Skipping scroll delta - gesture ended, bouncing");
                                } else if has_overlay_backdrop {
                                    // Skip scroll when overlay is visible to prevent background scrolling
                                    tracing::trace!("Skipping scroll delta - overlay with backdrop is visible");
                                } else {
                                    tracing::trace!(
                                        "Scroll dispatch: pos=({:.1}, {:.1}) delta=({:.1}, {:.1})",
                                        mouse_x, mouse_y, delta_x, delta_y
                                    );
                                    // Re-do hit test with mutable borrow to get ancestor chain
                                    // Then use dispatch_scroll_chain for proper nested scroll handling
                                    if let Some(ref mut windowed_ctx) = ctx {
                                        let router = &mut windowed_ctx.event_router;
                                        if let Some(hit) = router.hit_test(tree, mouse_x, mouse_y) {
                                            tracing::trace!(
                                                "Hit: node={:?}, ancestors={:?}",
                                                hit.node, hit.ancestors
                                            );
                                            tree.dispatch_scroll_chain(
                                                hit.node,
                                                &hit.ancestors,
                                                mouse_x,
                                                mouse_y,
                                                delta_x,
                                                delta_y,
                                            );
                                        }
                                    }
                                }
                            }

                            // Dispatch mouse/touch events (scroll is handled above with nested support)
                            for event in pending_events {
                                // Skip scroll events - already handled with nested scroll support
                                if event.event_type == blinc_core::events::event_types::SCROLL {
                                    continue;
                                }
                                tree.dispatch_event_full(
                                    event.node_id,
                                    event.event_type,
                                    event.mouse_x,
                                    event.mouse_y,
                                    event.local_x,
                                    event.local_y,
                                    event.bounds_x,
                                    event.bounds_y,
                                    event.bounds_width,
                                    event.bounds_height,
                                    event.drag_delta_x,
                                    event.drag_delta_y,
                                );
                            }

                            // Dispatch overlay events to the overlay tree
                            if !pending_overlay_events.is_empty() {
                                if let Some(ref mut windowed_ctx) = ctx {
                                    if let Some(ref mut overlay_tree) = windowed_ctx.overlay_tree {
                                        for event in pending_overlay_events {
                                            // Skip scroll events
                                            if event.event_type == blinc_core::events::event_types::SCROLL {
                                                continue;
                                            }
                                            overlay_tree.dispatch_event_full(
                                                event.node_id,
                                                event.event_type,
                                                event.mouse_x,
                                                event.mouse_y,
                                                event.local_x,
                                                event.local_y,
                                                event.bounds_x,
                                                event.bounds_y,
                                                event.bounds_width,
                                                event.bounds_height,
                                                event.drag_delta_x,
                                                event.drag_delta_y,
                                            );
                                        }
                                    }
                                }
                            }

                            // Dispatch keyboard events
                            // Use broadcast instead of bubbling to handle focus correctly after tree rebuilds.
                            // Text inputs track their own focus state internally via `s.visual.is_focused()`,
                            // so broadcasting to all handlers is safe - only the focused one will process.
                            for event in keyboard_events {
                                if event.event_type == blinc_core::events::event_types::TEXT_INPUT {
                                    if let Some(c) = event.key_char {
                                        // Broadcast to all text input handlers
                                        // Each handler checks its own focus state internally
                                        tree.broadcast_text_input_event(
                                            c,
                                            event.shift,
                                            event.ctrl,
                                            event.alt,
                                            event.meta,
                                        );
                                    }
                                } else {
                                    // Broadcast KEY_DOWN to all key handlers
                                    tree.broadcast_key_event(
                                        event.event_type,
                                        event.key_code,
                                        event.shift,
                                        event.ctrl,
                                        event.alt,
                                        event.meta,
                                    );
                                }
                            }

                            // If scroll momentum ended, notify scroll physics
                            if scroll_ended {
                                tree.on_scroll_end();
                                // Request redraw to animate bounce-back
                                window.request_redraw();
                            }
                        }
                    }

                    Event::Frame => {
                        if let (
                            Some(ref mut blinc_app),
                            Some(ref surf),
                            Some(ref config),
                            Some(ref mut windowed_ctx),
                            Some(ref mut rs),
                        ) = (&mut app, &surface, &surface_config, &mut ctx, &mut render_state)
                        {
                            // Get current frame
                            let frame = match surf.get_current_texture() {
                                Ok(f) => f,
                                Err(wgpu::SurfaceError::Lost) => {
                                    surf.configure(&blinc_app.device(), config);
                                    return ControlFlow::Continue;
                                }
                                Err(wgpu::SurfaceError::OutOfMemory) => {
                                    tracing::error!("Out of GPU memory");
                                    return ControlFlow::Exit;
                                }
                                Err(e) => {
                                    tracing::warn!("Surface error: {:?}", e);
                                    return ControlFlow::Continue;
                                }
                            };

                            let view = frame
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());

                            // Update context from window
                            windowed_ctx.update_from_window(window);

                            // Update viewport for lazy loading visibility checks
                            // Uses logical pixels (width/height) as that's what layout uses
                            rs.set_viewport_size(windowed_ctx.width, windowed_ctx.height);

                            // Clear overlays from previous frame (cursor, selection, focus ring)
                            // These are re-added during rendering if still active
                            rs.clear_overlays();

                            // =========================================================
                            // PHASE 1: Check if tree structure needs rebuild
                            // Only structural changes require tree rebuild
                            // =========================================================

                            // Check if event handlers marked anything dirty (auto-rebuild)
                            if let Some(ref tree) = render_tree {
                                if tree.needs_rebuild() {
                                    tracing::debug!("Rebuild triggered by: dirty_tracker");
                                    needs_rebuild = true;
                                }
                            }

                            // Check if element refs were modified (triggers rebuild)
                            if ref_dirty_flag.swap(false, Ordering::SeqCst) {
                                tracing::debug!("Rebuild triggered by: ref_dirty_flag (State::set)");
                                needs_rebuild = true;
                            }

                            // Check if text widgets requested a rebuild (focus/text changes)
                            if blinc_layout::widgets::take_needs_rebuild() {
                                tracing::debug!("Rebuild triggered by: text widget state change");
                                needs_rebuild = true;
                            }

                            // Check if a full relayout was requested (e.g., theme changes)
                            if blinc_layout::widgets::take_needs_relayout() {
                                tracing::debug!("Relayout triggered by: theme or global state change");
                                needs_relayout = true;
                            }

                            // Check if stateful elements requested a redraw (hover/press changes)
                            // Apply incremental prop updates without full rebuild
                            if blinc_layout::take_needs_redraw() {
                                tracing::debug!("Redraw requested by: stateful state change");

                                // Get all pending prop updates
                                let prop_updates = blinc_layout::take_pending_prop_updates();
                                let had_prop_updates = !prop_updates.is_empty();

                                // Apply prop updates to the appropriate tree
                                // IMPORTANT: Node IDs are local to each tree's SlotMap, so we must
                                // be careful not to apply updates to the wrong tree. When an overlay
                                // is visible, interactions are with the overlay, so apply to overlay tree.
                                // When no overlay, apply to main tree.
                                let has_overlay = windowed_ctx.overlay_tree.is_some();

                                // Apply prop updates to BOTH trees - nodes may exist in either
                                // The update_render_props silently ignores invalid node IDs
                                let had_prop_updates = !prop_updates.is_empty();
                                if let Some(ref mut tree) = render_tree {
                                    for (node_id, props) in &prop_updates {
                                        tree.update_render_props(*node_id, |p| *p = props.clone());
                                    }
                                }
                                if let Some(ref mut overlay_tree) = windowed_ctx.overlay_tree {
                                    for (node_id, props) in &prop_updates {
                                        overlay_tree.update_render_props(*node_id, |p| *p = props.clone());
                                    }
                                }

                                // Process subtree rebuilds for main tree
                                // This must happen even when overlay is visible, because
                                // Select/DropdownMenu buttons are in the main tree and need
                                // their children rebuilt when the value changes
                                if let Some(ref mut tree) = render_tree {
                                    // Process pending subtree rebuilds (structural changes)
                                    // Only recompute layout if subtree rebuilds occurred
                                    let had_subtree_rebuilds = blinc_layout::has_pending_subtree_rebuilds();
                                    tree.process_pending_subtree_rebuilds();

                                    // Recompute layout only if structural changes happened
                                    if had_subtree_rebuilds {
                                        tracing::debug!("Subtree rebuilds processed, recomputing layout");
                                        tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                        // Initialize motion animations for any new motion() containers
                                        // added during the subtree rebuild (e.g., tab content changes)
                                        tree.initialize_motion_animations(rs);
                                        // Process any motion replay requests queued during tree building
                                        rs.process_global_motion_replays();
                                    } else if had_prop_updates {
                                        tracing::trace!("Visual-only prop updates, skipping layout");
                                    }
                                }

                                // Request window redraw without rebuild
                                window.request_redraw();
                            }

                            // =========================================================
                            // PHASE 2: Build/rebuild tree only for structural changes
                            // This must happen BEFORE tick() so motion animations are available
                            // =========================================================

                            if needs_rebuild || render_tree.is_none() {
                                // Reset call counters for stable key generation
                                reset_call_counters();

                                // Build UI element tree
                                let ui = ui_builder(windowed_ctx);

                                // Use incremental update if we have an existing tree
                                // BUT: Skip incremental update during resize - do full rebuild instead
                                // This ensures parent constraints properly propagate to all children
                                if let Some(ref mut existing_tree) = render_tree {
                                    if needs_relayout {
                                        // Window resize: bypass incremental update, do full rebuild
                                        // This ensures proper constraint propagation from parents to children
                                        tracing::debug!("Window resize: full tree rebuild (bypassing incremental update)");

                                        // Clear layout bounds storages before rebuild
                                        existing_tree.clear_layout_bounds_storages();

                                        // Full rebuild: create new tree from element with shared registry
                                        // Pass registry to from_element_with_registry so IDs are registered during build
                                        let mut tree = RenderTree::from_element_with_registry(
                                            &ui,
                                            Arc::clone(&element_registry),
                                        );

                                        // Set animation scheduler for scroll bounce springs
                                        tree.set_animations(&windowed_ctx.animations);

                                        // Set DPI scale factor for HiDPI rendering
                                        tree.set_scale_factor(windowed_ctx.scale_factor as f32);

                                        // Compute layout with new viewport dimensions
                                        tree.compute_layout(windowed_ctx.width, windowed_ctx.height);

                                        // Initialize motion animations for any nodes wrapped in motion() containers
                                        tree.initialize_motion_animations(rs);
                                        // Process any motion replay requests queued during tree building
                                        rs.process_global_motion_replays();

                                        // Replace existing tree with fresh one
                                        *existing_tree = tree;

                                        // Clear relayout flag after full rebuild
                                        needs_relayout = false;
                                    } else {
                                        // Normal incremental update (no resize)
                                        use blinc_layout::UpdateResult;

                                        let update_result = existing_tree.incremental_update(&ui);

                                        match update_result {
                                            UpdateResult::NoChanges => {
                                                tracing::debug!("Incremental update: NoChanges - skipping rebuild");
                                            }
                                            UpdateResult::VisualOnly => {
                                                tracing::debug!("Incremental update: VisualOnly - skipping layout");
                                                // Props already updated in-place by incremental_update
                                            }
                                            UpdateResult::LayoutChanged => {
                                                // Layout changed - recompute layout
                                                tracing::debug!("Incremental update: LayoutChanged - recomputing layout");
                                                existing_tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                            }
                                            UpdateResult::ChildrenChanged => {
                                                // Children changed - subtrees were rebuilt in place
                                                tracing::debug!("Incremental update: ChildrenChanged - subtrees rebuilt");

                                                // Recompute layout since structure changed
                                                existing_tree.compute_layout(windowed_ctx.width, windowed_ctx.height);

                                                // Initialize motion animations for any new nodes wrapped in motion() containers
                                                existing_tree.initialize_motion_animations(rs);

                                                // Process any global motion replays that were queued during tree building
                                                rs.process_global_motion_replays();
                                            }
                                        }
                                    }
                                } else {
                                    // No existing tree - create new with shared registry
                                    let mut tree = RenderTree::from_element_with_registry(
                                        &ui,
                                        Arc::clone(&element_registry),
                                    );

                                    // Set animation scheduler for scroll bounce springs
                                    tree.set_animations(&windowed_ctx.animations);

                                    // Set DPI scale factor for HiDPI rendering
                                    tree.set_scale_factor(windowed_ctx.scale_factor as f32);

                                    // Compute layout in logical pixels
                                    tree.compute_layout(windowed_ctx.width, windowed_ctx.height);

                                    // Initialize motion animations for any nodes wrapped in motion() containers
                                    tree.initialize_motion_animations(rs);

                                    // Process any global motion replays that were queued during tree building
                                    rs.process_global_motion_replays();

                                    render_tree = Some(tree);
                                }

                                needs_rebuild = false;
                                let was_first_rebuild = windowed_ctx.rebuild_count == 0;
                                windowed_ctx.rebuild_count = windowed_ctx.rebuild_count.saturating_add(1);

                                // Execute on_ready callbacks after first rebuild
                                if was_first_rebuild {
                                    if let Ok(mut callbacks) = ready_callbacks.lock() {
                                        for callback in callbacks.drain(..) {
                                            callback();
                                        }
                                    }
                                }
                            }

                            // Note: on_ready callbacks are only executed after the FIRST rebuild
                            // (in the was_first_rebuild block above). Callbacks registered
                            // after the first rebuild are executed immediately since the UI
                            // is already ready at that point.

                            // =========================================================
                            // PHASE 3: Tick animations and dynamic render state
                            // This must happen AFTER tree rebuild so motions are initialized
                            // =========================================================

                            // Tick render state (handles cursor blink, color animations, etc.)
                            // This updates dynamic properties without touching tree structure
                            let current_time = elapsed_ms();
                            let _animations_active = rs.tick(current_time);

                            // Tick theme animation (handles color interpolation during theme transitions)
                            let theme_animating = blinc_theme::ThemeState::get().tick();

                            // Process pending scroll operations from ScrollRefs
                            if let Some(ref mut tree) = render_tree {
                                tree.process_pending_scroll_refs();
                            }

                            // Tick scroll physics for bounce-back animations
                            let scroll_animating = if let Some(ref mut tree) = render_tree {
                                tree.tick_scroll_physics(current_time)
                            } else {
                                false
                            };

                            // =========================================================
                            // PHASE 4: Render
                            // Combines stable tree structure with dynamic render state
                            // =========================================================

                            if let Some(ref tree) = render_tree {
                                // Render with motion animations
                                // Use physical pixel dimensions for the render surface
                                let result = blinc_app.render_tree_with_motion(
                                    tree,
                                    rs,
                                    &view,
                                    windowed_ctx.physical_width as u32,
                                    windowed_ctx.physical_height as u32,
                                );
                                if let Err(e) = result {
                                    tracing::error!("Render error: {}", e);
                                }
                            }

                            // =========================================================
                            // PHASE 4b: Render overlay tree (modals, toasts, etc.)
                            // Overlays render after main tree to ensure always-on-top
                            // =========================================================

                            // Update overlay manager viewport for positioning (with scale factor for HiDPI)
                            windowed_ctx.overlay_manager.set_viewport_with_scale(
                                windowed_ctx.width,
                                windowed_ctx.height,
                                windowed_ctx.scale_factor as f32,
                            );

                            // Update overlay states (Opening->Open transitions, auto-dismiss toasts)
                            windowed_ctx.overlay_manager.update(current_time);

                            // Build and render overlay tree if there are visible overlays
                            // Use render_overlay_tree_with_motion which does NOT clear the screen
                            let has_visible_overlays = windowed_ctx.overlay_manager.has_visible_overlays();

                            if has_visible_overlays {
                                // Begin tracking stable motion usage for this frame
                                rs.begin_stable_motion_frame();

                                // Only rebuild overlay tree if content changed (not just animation)
                                // This is critical for InstanceKey stability - rebuilding creates new UUIDs
                                let content_dirty = windowed_ctx.overlay_manager.take_dirty();
                                let _animation_dirty = windowed_ctx.overlay_manager.take_animation_dirty();

                                if content_dirty || windowed_ctx.overlay_tree.is_none() {
                                    // Content changed or no tree - full rebuild
                                    windowed_ctx.overlay_tree = windowed_ctx.overlay_manager.build_overlay_tree();
                                }
                                // Animation dirty just means we re-render the existing tree

                                if let Some(ref overlay_tree) = windowed_ctx.overlay_tree {
                                    // Initialize motion animations for overlay content
                                    overlay_tree.initialize_motion_animations(rs);

                                    // Process any global motion replays that were queued during overlay tree building
                                    rs.process_global_motion_replays();

                                    let result = blinc_app.render_overlay_tree_with_motion(
                                        overlay_tree,
                                        rs,
                                        &view,
                                        windowed_ctx.physical_width as u32,
                                        windowed_ctx.physical_height as u32,
                                    );
                                    if let Err(e) = result {
                                        tracing::error!("Overlay render error: {}", e);
                                    }
                                }

                                // Mark unused stable motions as Removed so they can restart
                                // when their overlay reopens
                                rs.end_stable_motion_frame();
                            } else if windowed_ctx.had_visible_overlays {
                                // Overlays just became invisible - mark all stable motions as unused
                                // so they restart their animations when overlays reopen
                                rs.begin_stable_motion_frame();
                                rs.end_stable_motion_frame();

                                // Clear cached overlay tree
                                windowed_ctx.overlay_tree = None;
                            }

                            // Track visibility for next frame
                            windowed_ctx.had_visible_overlays = has_visible_overlays;

                            frame.present();

                            // =========================================================
                            // PHASE 5: Request next frame if animations are active
                            // This ensures smooth animation without waiting for events
                            // =========================================================

                            // Check if background animation thread signaled that redraw is needed
                            // The background thread runs at 120fps and sets this flag when
                            // there are active animations (springs, keyframes, timelines)
                            let scheduler = windowed_ctx.animations.lock().unwrap();
                            let needs_animation_redraw = scheduler.take_needs_redraw();
                            drop(scheduler); // Release lock before request_redraw

                            // Check if text widgets need continuous redraws (cursor blink)
                            let needs_cursor_redraw = blinc_layout::widgets::take_needs_continuous_redraw();

                            // Check if motion animations are active (enter/exit animations)
                            let needs_motion_redraw = if let Some(ref rs) = render_state {
                                rs.has_active_motions()
                            } else {
                                false
                            };

                            // Check if overlays changed (modal opened/closed, toast appeared, etc.)
                            let needs_overlay_redraw = {
                                let mgr = windowed_ctx.overlay_manager.lock().unwrap();
                                mgr.take_dirty() || mgr.has_visible_overlays()
                            };

                            if needs_animation_redraw || needs_cursor_redraw || needs_motion_redraw || scroll_animating || needs_overlay_redraw || theme_animating {
                                // Request another frame to render updated animation values
                                // For cursor blink, also re-request continuous redraw for next frame
                                if needs_cursor_redraw {
                                    // Keep requesting redraws as long as a text input is focused
                                    if blinc_layout::widgets::has_focused_text_input() {
                                        blinc_layout::widgets::text_input::request_continuous_redraw_pub();
                                    }
                                }
                                window.request_redraw();
                            }
                        }
                    }

                    _ => {}
                }

                ControlFlow::Continue
            })
            .map_err(|e| BlincError::Platform(e.to_string()))?;

        Ok(())
    }

    /// Placeholder for non-windowed builds
    #[cfg(not(feature = "windowed"))]
    pub fn run<F, E>(_config: WindowConfig, _ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder,
    {
        Err(BlincError::Platform(
            "Windowed feature not enabled. Add 'windowed' feature to blinc_app".to_string(),
        ))
    }
}

/// Convert platform mouse button to layout mouse button
#[cfg(all(feature = "windowed", not(target_os = "android")))]
fn convert_mouse_button(button: blinc_platform::MouseButton) -> MouseButton {
    match button {
        blinc_platform::MouseButton::Left => MouseButton::Left,
        blinc_platform::MouseButton::Right => MouseButton::Right,
        blinc_platform::MouseButton::Middle => MouseButton::Middle,
        blinc_platform::MouseButton::Back => MouseButton::Back,
        blinc_platform::MouseButton::Forward => MouseButton::Forward,
        blinc_platform::MouseButton::Other(n) => MouseButton::Other(n),
    }
}

/// Convert layout cursor style to platform cursor
#[cfg(all(feature = "windowed", not(target_os = "android")))]
fn convert_cursor_style(cursor: CursorStyle) -> blinc_platform::Cursor {
    match cursor {
        CursorStyle::Default => blinc_platform::Cursor::Default,
        CursorStyle::Pointer => blinc_platform::Cursor::Pointer,
        CursorStyle::Text => blinc_platform::Cursor::Text,
        CursorStyle::Crosshair => blinc_platform::Cursor::Crosshair,
        CursorStyle::Move => blinc_platform::Cursor::Move,
        CursorStyle::NotAllowed => blinc_platform::Cursor::NotAllowed,
        CursorStyle::ResizeNS => blinc_platform::Cursor::ResizeNS,
        CursorStyle::ResizeEW => blinc_platform::Cursor::ResizeEW,
        CursorStyle::ResizeNESW => blinc_platform::Cursor::ResizeNESW,
        CursorStyle::ResizeNWSE => blinc_platform::Cursor::ResizeNWSE,
        CursorStyle::Grab => blinc_platform::Cursor::Grab,
        CursorStyle::Grabbing => blinc_platform::Cursor::Grabbing,
        CursorStyle::Wait => blinc_platform::Cursor::Wait,
        CursorStyle::Progress => blinc_platform::Cursor::Progress,
        CursorStyle::None => blinc_platform::Cursor::None,
    }
}

/// Convenience function to run a windowed app with default configuration
#[cfg(feature = "windowed")]
pub fn run_windowed<F, E>(ui_builder: F) -> Result<()>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder,
{
    WindowedApp::run(WindowConfig::default(), ui_builder)
}

/// Convenience function to run a windowed app with a title
#[cfg(feature = "windowed")]
pub fn run_windowed_with_title<F, E>(title: &str, ui_builder: F) -> Result<()>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder,
{
    let config = WindowConfig {
        title: title.to_string(),
        ..Default::default()
    };
    WindowedApp::run(config, ui_builder)
}
