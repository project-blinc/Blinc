//! `WindowedContext` reactive signals, per-frame tick callbacks and the theme API.

use super::*;

impl WindowedContext {
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

    /// Bare auto-keyed [`ScrollRef`] — uses `#[track_caller]` to
    /// derive a unique key from the source location, so the handle
    /// survives UI rebuilds without a manual key. For loops or
    /// repeated calls from the same line, use
    /// [`Self::use_scroll_ref_keyed`].
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let scroll_ref = ctx.use_scroll_ref();
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
    #[track_caller]
    pub fn use_scroll_ref(&self) -> blinc_layout::selector::ScrollRef {
        blinc_layout::selector::use_scroll_ref()
    }

    /// Hash-keyed [`ScrollRef`] for loops / list items / reusable
    /// component factories called multiple times from one line. The
    /// key can be any `Hash` type (`&str`, `u32`, tuples,
    /// `InstanceKey`, ...).
    ///
    /// Prefer [`Self::use_scroll_ref`] for the common one-call-site
    /// case.
    pub fn use_scroll_ref_keyed<K: Hash>(&self, key: K) -> blinc_layout::selector::ScrollRef {
        blinc_layout::selector::use_scroll_ref_keyed(key)
    }

    /// Create or retrieve a persistent reactive signal, auto-keyed
    /// by the caller's source location via `#[track_caller]`. The
    /// signal survives UI rebuilds — first call from a given line
    /// mints it, subsequent calls return the same handle.
    ///
    /// For loops or factories called multiple times from one line,
    /// use [`Self::use_signal_keyed`].
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = ctx.use_signal(0i32);
    /// // In an event handler:
    /// ctx.set(count, ctx.get(count).unwrap_or(0) + 1);
    /// ```
    #[track_caller]
    pub fn use_signal<T: Clone + Send + 'static>(&self, initial: T) -> Signal<T> {
        blinc_core::context_state::BlincContextState::get().use_signal(initial)
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
    // Tick Callback API (for per-frame updates like ECS systems)
    // =========================================================================

    /// Register a callback that runs each frame in the animation scheduler
    ///
    /// This creates a persistent tick callback keyed by source location.
    /// The callback receives delta time in seconds and runs on the animation
    /// scheduler's background thread at 120fps.
    ///
    /// Use this for ECS systems, physics, or any per-frame updates.
    /// The callback is registered once and persists across UI rebuilds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Create ECS world (persisted via use_state)
    /// let world = ctx.use_state_keyed("world", || Arc::new(Mutex::new(World::new())));
    ///
    /// // Register tick callback to run ECS systems
    /// ctx.use_tick_callback({
    ///     let world = world.get();
    ///     move |dt| {
    ///         let mut w = world.lock().unwrap();
    ///         // Run ECS systems with delta time
    ///         w.run_systems(dt);
    ///     }
    /// });
    /// ```
    #[track_caller]
    pub fn use_tick_callback<F>(&self, callback: F) -> blinc_animation::TickCallbackId
    where
        F: FnMut(f32) + Send + Sync + 'static,
    {
        let location = std::panic::Location::caller();
        let key = format!(
            "tick_{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
        self.use_tick_callback_for(&key, callback)
    }

    /// Register a tick callback with an explicit key
    ///
    /// Use this when you need to create multiple tick callbacks at the same
    /// source location (e.g., in a loop) or in reusable components.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Multiple tick callbacks with unique keys
    /// for i in 0..3 {
    ///     ctx.use_tick_callback_for(format!("system_{}", i), move |dt| {
    ///         // Per-frame update
    ///     });
    /// }
    /// ```
    pub fn use_tick_callback_for<K: Hash, F>(
        &self,
        key: K,
        callback: F,
    ) -> blinc_animation::TickCallbackId
    where
        F: FnMut(f32) + Send + Sync + 'static,
    {
        // Marker type for TickCallbackId storage
        struct TickCallbackMarker;

        let state_key = StateKey::new::<TickCallbackMarker, _>(&key);
        let mut hooks = self.hooks.lock().unwrap();

        if let Some(raw_id) = hooks.get(&state_key) {
            // Already registered - return existing ID
            blinc_animation::TickCallbackId::from_raw(raw_id)
        } else {
            // First time - register the callback with the scheduler
            let id = self
                .animation_handle()
                .register_tick_callback(callback)
                .expect("Animation scheduler should be alive");
            hooks.insert(state_key, id.to_raw());
            id
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
