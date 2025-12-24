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

use std::any::TypeId;
use std::hash::{Hash, Hasher};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use blinc_animation::{AnimationScheduler, SchedulerHandle};
use blinc_core::reactive::{Derived, ReactiveGraph, Signal};
use blinc_layout::prelude::*;
use blinc_platform::{
    ControlFlow, Event, EventLoop, InputEvent, KeyState, LifecycleEvent, MouseEvent, Platform,
    TouchEvent, Window, WindowConfig, WindowEvent,
};

use crate::app::BlincApp;
use crate::error::{BlincError, Result};

/// Shared animation scheduler for the application (thread-safe)
pub type SharedAnimationScheduler = Arc<Mutex<AnimationScheduler>>;

#[cfg(all(feature = "windowed", not(target_os = "android")))]
use blinc_platform_desktop::DesktopPlatform;

/// Shared dirty flag type for element refs
pub type RefDirtyFlag = Arc<AtomicBool>;

/// Shared reactive graph for the application (thread-safe)
pub type SharedReactiveGraph = Arc<Mutex<ReactiveGraph>>;

use std::collections::HashMap;

/// Key for identifying a signal in the keyed state system
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StateKey {
    /// Hash of the user-provided key
    key_hash: u64,
    /// Type ID of the signal value
    type_id: TypeId,
}

impl StateKey {
    fn new<T: 'static, K: Hash>(key: &K) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        Self {
            key_hash: hasher.finish(),
            type_id: TypeId::of::<T>(),
        }
    }

    fn from_string<T: 'static>(key: &str) -> Self {
        Self::new::<T, _>(&key)
    }
}

/// Stores keyed state across rebuilds
///
/// This enables component-level state management where each signal
/// is identified by a unique string key rather than call order.
pub struct HookState {
    /// Keyed signals: key -> raw signal ID
    signals: HashMap<StateKey, u64>,
}

impl HookState {
    fn new() -> Self {
        Self {
            signals: HashMap::new(),
        }
    }

    /// Get an existing signal by key
    fn get(&self, key: &StateKey) -> Option<u64> {
        self.signals.get(key).copied()
    }

    /// Store a signal with the given key
    fn insert(&mut self, key: StateKey, signal_id: u64) {
        self.signals.insert(key, signal_id);
    }
}

/// Shared hook state for the application
pub type SharedHookState = Arc<Mutex<HookState>>;

/// A bound state value with direct get/set methods
///
/// This is returned by `use_state` and provides a convenient API for
/// reading and writing state without needing to access the reactive graph directly.
#[derive(Clone)]
pub struct State<T> {
    signal: Signal<T>,
    reactive: SharedReactiveGraph,
    dirty_flag: RefDirtyFlag,
}

impl<T: Clone + Send + 'static> State<T> {
    /// Get the current value
    pub fn get(&self) -> T
    where
        T: Default,
    {
        self.reactive.lock().unwrap().get(self.signal).unwrap_or_default()
    }

    /// Get the current value, returning None if not found
    pub fn try_get(&self) -> Option<T> {
        self.reactive.lock().unwrap().get(self.signal)
    }

    /// Set a new value
    pub fn set(&self, value: T) {
        self.reactive.lock().unwrap().set(self.signal, value);
        self.dirty_flag.store(true, Ordering::SeqCst);
    }

    /// Update the value using a function
    pub fn update(&self, f: impl FnOnce(T) -> T) {
        self.reactive.lock().unwrap().update(self.signal, f);
        self.dirty_flag.store(true, Ordering::SeqCst);
    }

    /// Get the underlying signal (for advanced use cases)
    pub fn signal(&self) -> Signal<T> {
        self.signal
    }
}

/// Context passed to the UI builder function
pub struct WindowedContext {
    /// Current window width in physical pixels (matches surface size)
    pub width: f32,
    /// Current window height in physical pixels (matches surface size)
    pub height: f32,
    /// Current scale factor (physical / logical)
    pub scale_factor: f64,
    /// Whether the window is focused
    pub focused: bool,
    /// Event router for input event handling
    pub event_router: EventRouter,
    /// Animation scheduler for spring/keyframe animations
    pub animations: SharedAnimationScheduler,
    /// Shared dirty flag for element refs - when set, triggers UI rebuild
    ref_dirty_flag: RefDirtyFlag,
    /// Reactive graph for signal-based state management
    reactive: SharedReactiveGraph,
    /// Hook state for call-order based signal persistence
    hooks: SharedHookState,
}

impl WindowedContext {
    fn from_window<W: Window>(
        window: &W,
        event_router: EventRouter,
        animations: SharedAnimationScheduler,
        ref_dirty_flag: RefDirtyFlag,
        reactive: SharedReactiveGraph,
        hooks: SharedHookState,
    ) -> Self {
        // Use physical size for rendering - the surface is in physical pixels
        // UI layout and rendering must use physical dimensions to match the surface
        let (width, height) = window.size();
        Self {
            width: width as f32,
            height: height as f32,
            scale_factor: window.scale_factor(),
            focused: window.is_focused(),
            event_router,
            animations,
            ref_dirty_flag,
            reactive,
            hooks,
        }
    }


    /// Update context from window (preserving event router, dirty flag, and reactive graph)
    fn update_from_window<W: Window>(&mut self, window: &W) {
        let (width, height) = window.size();
        self.width = width as f32;
        self.height = height as f32;
        self.scale_factor = window.scale_factor();
        self.focused = window.is_focused();
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

        State {
            signal,
            reactive: Arc::clone(&self.reactive),
            dirty_flag: Arc::clone(&self.ref_dirty_flag),
        }
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
        let key = format!("{}:{}:{}", location.file(), location.line(), location.column());
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
            let signal = self.reactive.lock().unwrap().create_signal(shared_state.clone());
            let raw_id = signal.id().to_raw();
            hooks.insert(state_key, raw_id);
            shared_state
        }
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

        let platform = DesktopPlatform::new().map_err(|e| BlincError::Platform(e.to_string()))?;
        let event_loop = platform
            .create_event_loop_with_config(config)
            .map_err(|e| BlincError::Platform(e.to_string()))?;

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
        // Shared dirty flag for element refs
        let ref_dirty_flag: RefDirtyFlag = Arc::new(AtomicBool::new(false));
        // Shared reactive graph for signal-based state management
        let reactive: SharedReactiveGraph = Arc::new(Mutex::new(ReactiveGraph::new()));
        // Shared hook state for use_state persistence
        let hooks: SharedHookState = Arc::new(Mutex::new(HookState::new()));
        // Shared animation scheduler for spring/keyframe animations
        let animations: SharedAnimationScheduler = Arc::new(Mutex::new(AnimationScheduler::new()));

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

                                    surface = Some(surf);
                                    surface_config = Some(config);
                                    app = Some(blinc_app);

                                    // Initialize context with event router, animations, dirty flag, reactive graph, and hooks
                                    ctx = Some(WindowedContext::from_window(
                                        window,
                                        EventRouter::new(),
                                        Arc::clone(&animations),
                                        Arc::clone(&ref_dirty_flag),
                                        Arc::clone(&reactive),
                                        Arc::clone(&hooks),
                                    ));

                                    tracing::info!("Blinc windowed app initialized");
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

                                // Dispatch RESIZE event to elements
                                if let (Some(ref mut windowed_ctx), Some(ref tree)) =
                                    (&mut ctx, &render_tree)
                                {
                                    windowed_ctx
                                        .event_router
                                        .on_window_resize(tree, width as f32, height as f32);
                                }
                            }
                        }
                    }

                    Event::Window(WindowEvent::Focused(focused)) => {
                        // Update context focus state
                        if let Some(ref mut windowed_ctx) = ctx {
                            windowed_ctx.focused = focused;

                            // Dispatch WINDOW_FOCUS or WINDOW_BLUR to the focused element
                            windowed_ctx.event_router.on_window_focus(focused);
                        }
                    }

                    Event::Window(WindowEvent::CloseRequested) => {
                        return ControlFlow::Exit;
                    }

                    // Handle input events
                    Event::Input(input_event) => {
                        // First phase: collect events using immutable borrow
                        let pending_events = if let (Some(ref mut windowed_ctx), Some(ref tree)) =
                            (&mut ctx, &render_tree)
                        {
                            let router = &mut windowed_ctx.event_router;

                            // Collect events from router
                            // Tuple: (node_id, event_type, mouse_x, mouse_y, scroll_delta_x, scroll_delta_y)
                            let mut pending_events: Vec<(LayoutNodeId, u32, f32, f32, f32, f32)> = Vec::new();

                            // Set up callback to collect events
                            router.set_event_callback({
                                let events = &mut pending_events as *mut Vec<(LayoutNodeId, u32, f32, f32, f32, f32)>;
                                move |node, event_type| {
                                    // SAFETY: This callback is only used within this scope
                                    unsafe {
                                        (*events).push((node, event_type, 0.0, 0.0, 0.0, 0.0));
                                    }
                                }
                            });

                            match input_event {
                                InputEvent::Mouse(mouse_event) => match mouse_event {
                                    MouseEvent::Moved { x, y } => {
                                        router.on_mouse_move(tree, x, y);
                                        for event in pending_events.iter_mut() {
                                            event.2 = x;
                                            event.3 = y;
                                        }
                                    }
                                    MouseEvent::ButtonPressed { button, x, y } => {
                                        let btn = convert_mouse_button(button);
                                        router.on_mouse_down(tree, x, y, btn);
                                        for event in pending_events.iter_mut() {
                                            event.2 = x;
                                            event.3 = y;
                                        }
                                    }
                                    MouseEvent::ButtonReleased { button, x, y } => {
                                        let btn = convert_mouse_button(button);
                                        router.on_mouse_up(tree, x, y, btn);
                                        for event in pending_events.iter_mut() {
                                            event.2 = x;
                                            event.3 = y;
                                        }
                                    }
                                    MouseEvent::Left => {
                                        router.on_mouse_leave();
                                    }
                                    MouseEvent::Entered => {
                                        let (mx, my) = router.mouse_position();
                                        router.on_mouse_move(tree, mx, my);
                                        for event in pending_events.iter_mut() {
                                            event.2 = mx;
                                            event.3 = my;
                                        }
                                    }
                                },
                                InputEvent::Keyboard(kb_event) => match kb_event.state {
                                    KeyState::Pressed => {
                                        router.on_key_down(0);
                                    }
                                    KeyState::Released => {
                                        router.on_key_up(0);
                                    }
                                },
                                InputEvent::Touch(touch_event) => match touch_event {
                                    TouchEvent::Started { x, y, .. } => {
                                        router.on_mouse_down(tree, x, y, MouseButton::Left);
                                        for event in pending_events.iter_mut() {
                                            event.2 = x;
                                            event.3 = y;
                                        }
                                    }
                                    TouchEvent::Moved { x, y, .. } => {
                                        router.on_mouse_move(tree, x, y);
                                        for event in pending_events.iter_mut() {
                                            event.2 = x;
                                            event.3 = y;
                                        }
                                    }
                                    TouchEvent::Ended { x, y, .. } => {
                                        router.on_mouse_up(tree, x, y, MouseButton::Left);
                                        for event in pending_events.iter_mut() {
                                            event.2 = x;
                                            event.3 = y;
                                        }
                                    }
                                    TouchEvent::Cancelled { .. } => {
                                        router.on_mouse_leave();
                                    }
                                },
                                InputEvent::Scroll { delta_x, delta_y } => {
                                    let (mx, my) = router.mouse_position();
                                    router.on_scroll(tree, delta_x, delta_y);
                                    for event in pending_events.iter_mut() {
                                        event.2 = mx;
                                        event.3 = my;
                                        // Set scroll delta for scroll events
                                        event.4 = delta_x;
                                        event.5 = delta_y;
                                    }
                                }
                            }

                            router.clear_event_callback();
                            pending_events
                        } else {
                            Vec::new()
                        };

                        // Second phase: dispatch events with mutable borrow
                        // This automatically marks the tree dirty when handlers fire
                        if let Some(ref mut tree) = render_tree {
                            for (node, event_type, mouse_x, mouse_y, scroll_dx, scroll_dy) in pending_events {
                                // Use scroll-specific dispatch for scroll events to pass delta
                                if event_type == blinc_core::events::event_types::SCROLL {
                                    tree.dispatch_scroll_event(node, mouse_x, mouse_y, scroll_dx, scroll_dy);
                                } else {
                                    tree.dispatch_event(node, event_type, mouse_x, mouse_y);
                                }
                            }
                        }
                    }

                    Event::Frame => {
                        if let (
                            Some(ref mut blinc_app),
                            Some(ref surf),
                            Some(ref config),
                            Some(ref mut windowed_ctx),
                        ) = (&mut app, &surface, &surface_config, &mut ctx)
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

                            // Check if event handlers marked anything dirty (auto-rebuild)
                            if let Some(ref tree) = render_tree {
                                if tree.needs_rebuild() {
                                    needs_rebuild = true;
                                }
                            }

                            // Check if element refs were modified (triggers rebuild)
                            if ref_dirty_flag.swap(false, Ordering::SeqCst) {
                                needs_rebuild = true;
                            }

                            // Tick animations and trigger rebuild if any are still active
                            if windowed_ctx.animations.lock().unwrap().tick() {
                                needs_rebuild = true;
                            }

                            // Build/rebuild render tree only when needed
                            // The tree persists across frames for stable node IDs and event handling
                            if needs_rebuild || render_tree.is_none() {
                                // Build UI and create render tree
                                let ui = ui_builder(windowed_ctx);
                                let mut tree = RenderTree::from_element(&ui);
                                tree.compute_layout(windowed_ctx.width, windowed_ctx.height);

                                // Transfer node states and scroll offsets from old tree to preserve state across rebuilds
                                if let Some(ref old_tree) = render_tree {
                                    tree.transfer_states_from(old_tree);
                                    tree.transfer_scroll_offsets_from(old_tree);
                                }

                                render_tree = Some(tree);
                                needs_rebuild = false;
                            }

                            // Render from the cached tree
                            if let Some(ref tree) = render_tree {
                                if let Err(e) =
                                    blinc_app.render_tree(tree, &view, windowed_ctx.width as u32, windowed_ctx.height as u32)
                                {
                                    tracing::error!("Render error: {}", e);
                                }
                            }

                            frame.present();
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
