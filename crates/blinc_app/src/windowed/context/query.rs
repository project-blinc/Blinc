//! `WindowedContext` element queries and its trait implementations.

use crate::windowed::{SharedElementRegistry, WindowedContext};
use blinc_animation::{
    AnimatedTimeline, AnimatedValue, AnimationContext, SchedulerHandle, SharedAnimatedTimeline,
    SharedAnimatedValue, SpringConfig,
};
use blinc_core::context_state::StateKey;
use blinc_core::reactive::{Derived, ReactiveGraph, Signal, SignalId, State};
use std::hash::Hash;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

impl WindowedContext {
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

    /// Create or retrieve a persistent fine-grained reactive cell,
    /// auto-keyed by the caller's source location via
    /// `#[track_caller]`. Returns a [`blinc_core::State`] — a
    /// signal-backed slot with `.get()` / `.set()` / `.update()`
    /// methods. Survives UI rebuilds.
    ///
    /// `State<T>` is the fine-grained reactive primitive: each
    /// `.set()` notifies only the things that registered a
    /// dependency on this signal. Use it for values like counters,
    /// flags, form fields — anything you'd want to read in one
    /// place and mutate from another.
    ///
    /// For Stateful FSM handles (hover / press / drag / custom
    /// state machines), reach for [`Self::use_state_for`] /
    /// `use_state_for(initial)` instead — those return a
    /// `SharedState<S>` (an `Arc<Mutex<StatefulInner<S>>>`), which
    /// is a different abstraction.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn counter(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let count = ctx.use_state(0i32);
    ///     stateful::<NoState>()
    ///         .deps([count.signal_id()])
    ///         .on_state(move |_| {
    ///             div()
    ///                 .child(text(&format!("Clicks: {}", count.get())))
    ///                 .on_click({
    ///                     let count = count.clone();
    ///                     move |_| count.update(|n| n + 1)
    ///                 })
    ///         })
    /// }
    /// ```
    #[track_caller]
    pub fn use_state<T>(&self, initial: T) -> blinc_core::State<T>
    where
        T: Clone + Send + 'static,
    {
        // Forward to the global context's `use_state`. Both this
        // method and `BlincContextState::use_state` are
        // `#[track_caller]` so `Location::caller()` lands on the
        // ORIGINAL caller of `ctx.use_state(...)`, not on this
        // wrapper line.
        blinc_core::context_state::BlincContextState::get().use_state(initial)
    }

    /// Create or retrieve a persistent FSM handle (`SharedState<S>`)
    /// with an explicit hash key. Use this for reusable components
    /// called multiple times from the same source line (loops,
    /// list items, factory functions).
    ///
    /// The key can be any type that implements `Hash` (strings,
    /// numbers, tuples, `InstanceKey`, etc).
    ///
    /// For the common "one widget per source line" case, prefer
    /// the bare [`Self::use_fsm`] / `use_fsm(initial)` — it derives
    /// the key from `#[track_caller]` automatically.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Reusable component — string key per instance
    /// fn feature_card(ctx: &WindowedContext, id: &str) -> impl ElementBuilder {
    ///     let handle = ctx.use_fsm_keyed(id, ButtonState::Idle);
    ///     stateful_from_handle(handle).on_state(|state, div| { /* … */ })
    /// }
    ///
    /// // Or with a numeric key in a loop
    /// for i in 0..3 {
    ///     let handle = ctx.use_fsm_keyed(i, ButtonState::Idle);
    ///     // …
    /// }
    /// ```
    pub fn use_fsm_keyed<K, S>(&self, key: K, initial: S) -> blinc_layout::SharedState<S>
    where
        K: Hash,
        S: blinc_layout::StateTransitions + Clone + Send + 'static,
    {
        // Forward to the standalone keyed version so both API
        // surfaces share one implementation and can't drift.
        blinc_layout::stateful::use_fsm_keyed(key, initial)
    }

    /// **Deprecated.** Renamed to [`Self::use_fsm_keyed`].
    ///
    /// The new name says what the method returns (a `SharedState<S>`
    /// FSM handle, not a `State<T>` reactive cell). Existing call
    /// sites still compile; they just emit a deprecation warning.
    #[deprecated(
        since = "0.5.2",
        note = "use `use_fsm_keyed(key, initial)` instead — returns a SharedState<S> (FSM handle)"
    )]
    #[track_caller]
    pub fn use_state_for<K, S>(&self, key: K, initial: S) -> blinc_layout::SharedState<S>
    where
        K: Hash,
        S: blinc_layout::StateTransitions + Clone + Send + 'static,
    {
        self.use_fsm_keyed(key, initial)
    }

    /// Create or retrieve a persistent FSM handle (`SharedState<S>`),
    /// auto-keyed by the caller's source location via
    /// `#[track_caller]`. The common case for "one Stateful per call
    /// site" — no manual key plumbing required.
    ///
    /// For loops or reusable factories called multiple times from
    /// the same source line, use [`Self::use_fsm_keyed`] with an
    /// explicit per-instance key (collisions on the auto-derived
    /// `(file, line, column)` key would otherwise alias every
    /// iteration onto the same slot).
    ///
    /// `Sync` is not required on `S` — the value lives behind
    /// `Arc<Mutex<…>>`.
    #[track_caller]
    pub fn use_fsm<S>(&self, initial: S) -> blinc_layout::SharedState<S>
    where
        S: blinc_layout::StateTransitions + Clone + Send + 'static,
    {
        blinc_layout::stateful::use_fsm(initial)
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

    #[track_caller]
    fn use_signal<T: Clone + Send + 'static>(&self, initial: T) -> Signal<T> {
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
