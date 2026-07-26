#![allow(clippy::type_complexity)]
//! Fine-grained reactive signal system
//!
//! Inspired by Leptos/SolidJS signals with automatic dependency tracking.
//! This implements a push-pull hybrid reactive system:
//! - Signals push invalidation notifications to subscribers
//! - Derived values pull (lazily compute) their values when accessed
//! - Effects are scheduled and batched for efficiency
//!
//! # State
//!
//! The [`State<T>`] type provides a convenient wrapper around a signal with
//! thread-safe access to the reactive graph. It's the primary API for component
//! state management.
//!
//! ```ignore
//! use blinc_core::reactive::State;
//!
//! // State is typically obtained from a context
//! let counter: State<i32> = ctx.use_state_keyed("counter", || 0);
//!
//! // Read the current value
//! let value = counter.get();
//!
//! // Update the value (triggers reactive updates)
//! counter.set(value + 1);
//!
//! // Update the value and rebuild UI tree
//! counter.set_rebuild(value + 1);
//! ```

use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

new_key_type! {
    /// Unique identifier for a signal
    pub struct SignalId;
    /// Unique identifier for a derived/computed value
    pub struct DerivedId;
    /// Unique identifier for an effect
    pub struct EffectId;
}

/// Subscriber types that can react to signal changes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscriberId {
    Derived(DerivedId),
    Effect(EffectId),
}

/// A reactive signal handle (cheap to copy)
#[derive(Debug)]
pub struct Signal<T> {
    id: SignalId,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Signal<T> {}

impl<T> Signal<T> {
    /// Get the signal's internal ID
    pub fn id(&self) -> SignalId {
        self.id
    }

    /// Alias for [`Self::id`] — matches `State<T>::signal_id` so
    /// `signal.signal_id()` and `state.signal_id()` both work in
    /// `.deps([…])` declarations.
    pub fn signal_id(&self) -> SignalId {
        self.id
    }

    /// Reconstruct a Signal from a raw SignalId
    ///
    /// # Safety
    /// The caller must ensure the SignalId refers to a signal of type T.
    /// This is primarily for internal use by the hook system.
    pub fn from_id(id: SignalId) -> Self {
        Signal {
            id,
            _marker: std::marker::PhantomData,
        }
    }
}

// =========================================================================
// Re-entrancy infrastructure — TLS in-flight graph + deferred writes.
//
// The process-global graph is protected by a `Mutex`. Inside
// `flush_effects` (called while the mutex is held), user effect
// closures may want to read or write signals. Re-acquiring the
// global mutex from inside such a closure deadlocks — same thread,
// non-reentrant mutex.
//
// Two-piece fix that keeps the public `Signal<T>::get / set / update`
// API unchanged:
//
// 1. **In-flight graph pointer** — `run_effect` stashes
//    `self as *const ReactiveGraph` in TLS for the duration of the
//    closure call. Reads (`Signal::try_get`) check the TLS first; if
//    set, they borrow the graph directly without locking.
//
// 2. **Deferred-write queue** — writes attempted while the in-flight
//    pointer is set get pushed onto a TLS queue of boxed closures.
//    After the outer write returns (notifications fired, lock long
//    released), [`drain_deferred_writes`] re-invokes each one. This
//    matches the "write semantics defer to next tick" pattern
//    SolidJS / Leptos use to keep effect bodies' read values stable
//    within a single fire.
// =========================================================================

thread_local! {
    /// Pointer to the `ReactiveGraph` currently executing inside
    /// `run_effect`. `null` when no effect is in flight. Reads
    /// inside effect closures use this to bypass the global mutex.
    static IN_FLIGHT_GRAPH: std::cell::Cell<*const ReactiveGraph> =
        const { std::cell::Cell::new(std::ptr::null()) };

    /// Writes deferred while [`IN_FLIGHT_GRAPH`] was non-null.
    /// Drained by [`drain_deferred_writes`] from the outer
    /// `Signal<T>::set` after notifications complete.
    static DEFERRED_WRITES: std::cell::RefCell<Vec<Box<dyn FnOnce() + Send>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Returns `true` if a `flush_effects` callback is currently running
/// on this thread — i.e. re-entrant signal access would deadlock if
/// it took the mutex.
fn is_in_flush() -> bool {
    IN_FLIGHT_GRAPH.with(|c| !c.get().is_null())
}

/// Run `f` against the in-flight graph if one is set; otherwise
/// returns `None` and the caller takes the global-mutex path.
fn with_in_flight_graph<R>(f: impl FnOnce(&ReactiveGraph) -> R) -> Option<R> {
    let p = IN_FLIGHT_GRAPH.with(|c| c.get());
    if p.is_null() {
        return None;
    }
    // SAFETY: pointer is set only for the duration of `run_effect`'s
    // closure invocation, which holds `&mut self` to the graph.
    // We're a borrow within that window — single-threaded, no
    // aliasing.
    Some(f(unsafe { &*p }))
}

/// Drain queued deferred writes. Called from the outer write's
/// continuation after notifications fire. Each closure is a fresh
/// `Signal<T>::set` call that takes the normal path (in-flight
/// pointer is clear by the time this runs).
fn drain_deferred_writes() {
    // Re-entrant safety: a deferred write may itself queue more.
    // Loop until the queue is empty. Don't hold the borrow across
    // the call.
    loop {
        let next = DEFERRED_WRITES.with(|q| q.borrow_mut().pop());
        match next {
            Some(f) => f(),
            None => break,
        }
    }
}

// =========================================================================
// Signal<T> rich API — operates against the process-global graph.
//
// These methods make `Signal<T>` a first-class reactive primitive:
// callers can `signal(0).set(...)` / `.get()` / `.update(...)` without
// holding a `State<T>` wrapper or routing through `BlincContextState`.
// Each call grabs the global graph Arc (cheap), takes its mutex briefly,
// then fires the same property-binding + derived + stateful-deps
// notifiers that `State<T>::set` does. `Signal<T>` stays `Copy` — the
// graph reference is never stored on the handle.
// =========================================================================

impl<T: Clone + Send + 'static> Signal<T> {
    /// Read the current value. Returns `None` if the signal is no
    /// longer in the graph (e.g. graph reset between tests).
    ///
    /// Re-entrancy: when called from inside an effect closure, takes
    /// the fast path against the in-flight graph reference (no
    /// mutex acquisition) so DSL effect bodies that call
    /// `<signal>.get()` don't deadlock against the lock the outer
    /// `flush_effects` holds.
    pub fn try_get(&self) -> Option<T> {
        if let Some(value) = with_in_flight_graph(|g| g.get(*self)) {
            return value;
        }
        let graph = global_graph();
        let g = graph.lock().unwrap();
        g.get(*self)
    }

    /// Read the current value, falling back to `T::default()` if the
    /// signal isn't resolvable. Matches `State<T>::get` ergonomics.
    pub fn get(&self) -> T
    where
        T: Default,
    {
        self.try_get().unwrap_or_default()
    }

    /// Set a new value. Fires every subscriber: property bindings
    /// (`.bg(&signal)` etc.), derived chains, and `Stateful` elements
    /// declaring this signal in `.deps([...])`.
    ///
    /// Visual-only — does not flip the dirty flag. Use
    /// [`Self::set_rebuild`] for structural changes.
    ///
    /// Re-entrancy: if called while an effect closure is in flight
    /// (the global graph mutex is held by an outer `flush_effects`),
    /// the write is queued in the per-thread deferred-writes table
    /// and drained after the outer `Signal::set` completes its
    /// notifications. Writes during an effect are visible to
    /// subsequent reads but NOT to the in-progress effect's own
    /// in-this-fire reads — matching the SolidJS / Leptos semantics.
    pub fn set(&self, value: T) {
        let id = *self;
        if is_in_flush() {
            DEFERRED_WRITES.with(|q| {
                q.borrow_mut()
                    .push(Box::new(move || Signal::<T>::from_id(id.id).set(value)));
            });
            return;
        }
        let dirty_derived = {
            let graph = global_graph();
            let mut g = graph.lock().unwrap();
            g.set(*self, value);
            g.take_dirty_derived()
        };
        notify_stateful_deps(&[self.id]);
        notify_property_bindings(self.id);
        notify_portals(self.id);
        for d_id in dirty_derived {
            notify_property_bindings_for_derived(d_id);
        }
        drain_deferred_writes();
    }

    /// Set a new value AND flip the global dirty flag, requesting a
    /// full tree rebuild. Use for structural changes (adding/removing
    /// children, swapping branches); prefer [`Self::set`] otherwise.
    pub fn set_rebuild(&self, value: T) {
        let id = *self;
        if is_in_flush() {
            DEFERRED_WRITES.with(|q| {
                q.borrow_mut().push(Box::new(move || {
                    Signal::<T>::from_id(id.id).set_rebuild(value)
                }));
            });
            return;
        }
        let dirty_derived = {
            let graph = global_graph();
            let mut g = graph.lock().unwrap();
            g.set(*self, value);
            g.take_dirty_derived()
        };
        GLOBAL_DIRTY.store(true, Ordering::SeqCst);
        notify_stateful_deps(&[self.id]);
        notify_property_bindings(self.id);
        notify_portals(self.id);
        for d_id in dirty_derived {
            notify_property_bindings_for_derived(d_id);
        }
        drain_deferred_writes();
    }

    /// Update the value via a closure. Fires the same subscribers as
    /// [`Self::set`].
    pub fn update(&self, f: impl FnOnce(T) -> T) {
        // Deferring an `update` mid-flush means: read the value NOW
        // (off the in-flight graph), apply `f`, and queue a `set` of
        // the result. Without this, deferring `update` would lose the
        // closure's snapshot semantics.
        if is_in_flush() {
            // Read current value via the in-flight graph fast path,
            // apply f, queue the resulting set.
            let current = self.try_get();
            let Some(current) = current else { return };
            let next = f(current);
            self.set(next);
            return;
        }
        let dirty_derived = {
            let graph = global_graph();
            let mut g = graph.lock().unwrap();
            g.update(*self, f);
            g.take_dirty_derived()
        };
        notify_stateful_deps(&[self.id]);
        notify_property_bindings(self.id);
        notify_portals(self.id);
        for d_id in dirty_derived {
            notify_property_bindings_for_derived(d_id);
        }
        drain_deferred_writes();
    }

    /// Update the value AND flip the global dirty flag.
    pub fn update_rebuild(&self, f: impl FnOnce(T) -> T) {
        if is_in_flush() {
            let current = self.try_get();
            let Some(current) = current else { return };
            let next = f(current);
            self.set_rebuild(next);
            return;
        }
        let dirty_derived = {
            let graph = global_graph();
            let mut g = graph.lock().unwrap();
            g.update(*self, f);
            g.take_dirty_derived()
        };
        GLOBAL_DIRTY.store(true, Ordering::SeqCst);
        notify_stateful_deps(&[self.id]);
        notify_property_bindings(self.id);
        notify_portals(self.id);
        for d_id in dirty_derived {
            notify_property_bindings_for_derived(d_id);
        }
        drain_deferred_writes();
    }
}

impl SignalId {
    /// Convert to raw u64 for storage
    pub fn to_raw(&self) -> u64 {
        use slotmap::Key;
        // SlotMap key data contains version + index
        self.data().as_ffi()
    }

    /// Reconstruct from raw u64
    pub fn from_raw(raw: u64) -> Self {
        slotmap::KeyData::from_ffi(raw).into()
    }
}

impl DerivedId {
    /// Convert to raw u64 for cross-FFI storage. Used by the DSL
    /// `computed { … } : T` lowering to bake a `Computed<T>` handle
    /// into JIT code as an i64 literal.
    pub fn to_raw(&self) -> u64 {
        use slotmap::Key;
        self.data().as_ffi()
    }

    /// Reconstruct from raw u64.
    pub fn from_raw(raw: u64) -> Self {
        slotmap::KeyData::from_ffi(raw).into()
    }
}

/// A derived/computed value handle
#[derive(Debug)]
pub struct Derived<T> {
    id: DerivedId,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Clone for Derived<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Derived<T> {}

impl<T> Derived<T> {
    pub fn id(&self) -> DerivedId {
        self.id
    }

    /// Reconstruct a `Derived<T>` from a raw `DerivedId`.
    ///
    /// # Safety
    /// The caller must ensure the `DerivedId` refers to a derived
    /// computed of type `T`. Used by the FFI boundary to rehydrate
    /// a [`Computed<T>`] from a raw `u64` handle baked into JIT
    /// code by `computed { … } : T` lowering.
    pub fn from_id(id: DerivedId) -> Self {
        Self {
            id,
            _marker: std::marker::PhantomData,
        }
    }
}

/// An effect handle
#[derive(Debug, Clone, Copy)]
pub struct Effect {
    id: EffectId,
}

impl Effect {
    pub fn id(&self) -> EffectId {
        self.id
    }
}

/// Internal signal node storage
struct SignalNode {
    /// The signal value (type-erased)
    value: Box<dyn Any + Send>,
    /// Version counter for change detection
    version: u64,
    /// Subscribers to notify on change
    subscribers: SmallVec<[SubscriberId; 4]>,
}

/// Internal derived node storage
struct DerivedNode {
    /// Cached value (if computed)
    value: Option<Box<dyn Any + Send>>,
    /// Version of cached value
    cached_version: u64,
    /// The compute function
    compute: Box<dyn Fn(&ReactiveGraph) -> Box<dyn Any + Send> + Send>,
    /// Dependencies (signals this derived reads from)
    dependencies: SmallVec<[SignalId; 4]>,
    /// Subscribers to notify when this derived changes
    subscribers: SmallVec<[SubscriberId; 4]>,
    /// Whether the cached value is stale
    dirty: Cell<bool>,
    /// Depth in the dependency graph (for topological ordering)
    depth: u32,
}

/// Internal effect node storage
struct EffectNode {
    /// The effect function
    run: Box<dyn FnMut(&ReactiveGraph) + Send>,
    /// Dependencies (signals this effect reads from)
    dependencies: SmallVec<[SignalId; 4]>,
    /// Whether the effect needs to run
    dirty: Cell<bool>,
    /// Depth in the dependency graph
    depth: u32,
}

/// The reactive graph that manages all signals, derived values, and effects
pub struct ReactiveGraph {
    signals: SlotMap<SignalId, SignalNode>,
    derived: SlotMap<DerivedId, DerivedNode>,
    effects: SlotMap<EffectId, EffectNode>,
    /// Pending effects to run
    pending_effects: RefCell<VecDeque<EffectId>>,
    /// Current batch depth (> 0 means we're in a batch)
    batch_depth: Cell<u32>,
    /// Currently tracking dependencies (for auto-tracking)
    tracking: RefCell<Option<Vec<SignalId>>>,
    /// Global version counter
    global_version: Cell<u64>,
    /// Per-set buffer of derived ids that just transitioned from
    /// clean to dirty. Drained at the end of every [`Self::set`] call
    /// to fire `notify_property_bindings_for_derived` once per
    /// affected derived (Phase 8 follow-up: Derived ↔ property-binding
    /// bridge, [[project-reactive-architecture-v2]]).
    derived_dirty_buffer: RefCell<SmallVec<[DerivedId; 4]>>,
}

impl ReactiveGraph {
    /// Create a new reactive graph
    pub fn new() -> Self {
        Self {
            signals: SlotMap::with_key(),
            derived: SlotMap::with_key(),
            effects: SlotMap::with_key(),
            pending_effects: RefCell::new(VecDeque::new()),
            batch_depth: Cell::new(0),
            tracking: RefCell::new(None),
            global_version: Cell::new(0),
            derived_dirty_buffer: RefCell::new(SmallVec::new()),
        }
    }

    // =========================================================================
    // SIGNALS
    // =========================================================================

    /// Create a new signal with an initial value
    pub fn create_signal<T: Send + 'static>(&mut self, initial: T) -> Signal<T> {
        let id = self.signals.insert(SignalNode {
            value: Box::new(initial),
            version: 0,
            subscribers: SmallVec::new(),
        });
        Signal {
            id,
            _marker: std::marker::PhantomData,
        }
    }

    /// Get the current value of a signal
    ///
    /// If called within a tracking context (effect or derived), this signal
    /// will be recorded as a dependency.
    pub fn get<T: Clone + 'static>(&self, signal: Signal<T>) -> Option<T> {
        // Record dependency if we're tracking
        if let Some(ref mut deps) = *self.tracking.borrow_mut() {
            if !deps.contains(&signal.id) {
                deps.push(signal.id);
            }
        }

        self.signals
            .get(signal.id)
            .and_then(|node| node.value.downcast_ref::<T>().cloned())
    }

    /// Get the current value without tracking as a dependency
    pub fn get_untracked<T: Clone + 'static>(&self, signal: Signal<T>) -> Option<T> {
        self.signals
            .get(signal.id)
            .and_then(|node| node.value.downcast_ref::<T>().cloned())
    }

    /// Set the value of a signal, triggering reactive updates
    pub fn set<T: Send + 'static>(&mut self, signal: Signal<T>, value: T) {
        if let Some(node) = self.signals.get_mut(signal.id) {
            node.value = Box::new(value);
            node.version += 1;
            self.global_version.set(self.global_version.get() + 1);

            // Mark all subscribers as dirty. mark_dirty recursively
            // walks derived -> derived chains, collecting every
            // derived that flipped from clean to dirty into
            // `derived_dirty_buffer`. The buffer is drained by
            // `State::set` AFTER it releases its lock on the graph
            // and fires `notify_property_bindings_for_derived` per
            // id — firing inline here would deadlock, because the
            // binding registry's read closures call
            // `Computed::try_get` which re-acquires this same
            // mutex.
            let subscribers: SmallVec<[SubscriberId; 4]> = node.subscribers.clone();
            for sub in subscribers {
                self.mark_dirty(sub);
            }

            // If not in a batch, flush effects immediately
            if self.batch_depth.get() == 0 {
                self.flush_effects();
            }
        }
    }

    /// Drain the per-set list of derived ids that flipped to dirty
    /// during the most recent `set` (or chain of effects following
    /// it). Returns ids in the order they were dirtied. Empty if
    /// nothing flipped.
    ///
    /// Called by `State::set` immediately AFTER dropping its lock on
    /// the graph, so the property-binding registry's read closures
    /// can re-enter the lock safely while firing.
    pub fn take_dirty_derived(&self) -> SmallVec<[DerivedId; 4]> {
        std::mem::take(&mut *self.derived_dirty_buffer.borrow_mut())
    }

    /// Update a signal using a function
    pub fn update<T: Clone + Send + 'static, F: FnOnce(T) -> T>(
        &mut self,
        signal: Signal<T>,
        f: F,
    ) {
        if let Some(current) = self.get_untracked(signal) {
            self.set(signal, f(current));
        }
    }

    /// Get the version of a signal (for change detection)
    pub fn signal_version(&self, id: SignalId) -> Option<u64> {
        self.signals.get(id).map(|n| n.version)
    }

    // =========================================================================
    // DERIVED VALUES
    // =========================================================================

    /// Create a derived (computed) value
    pub fn create_derived<T, F>(&mut self, compute: F) -> Derived<T>
    where
        T: Clone + Send + 'static,
        F: Fn(&ReactiveGraph) -> T + Send + 'static,
    {
        // Wrap the compute function to return boxed Any
        let compute_boxed =
            move |graph: &ReactiveGraph| -> Box<dyn Any + Send> { Box::new(compute(graph)) };

        let id = self.derived.insert(DerivedNode {
            value: None,
            cached_version: 0,
            compute: Box::new(compute_boxed),
            dependencies: SmallVec::new(),
            subscribers: SmallVec::new(),
            dirty: Cell::new(true), // Start dirty to force initial computation
            depth: 0,
        });

        Derived {
            id,
            _marker: std::marker::PhantomData,
        }
    }

    /// Get the value of a derived, computing if necessary
    /// Read a derived's cached value without recomputing.
    ///
    /// For re-entrant reads: [`get_derived`](Self::get_derived) needs
    /// `&mut self`, so a compute closure that reads another derived
    /// cannot call it, and going through `Computed::try_get` would
    /// re-lock the graph mutex on a thread that already holds it and
    /// deadlock. Returns `None` when the derived has never been
    /// computed; the value may be stale, since derived-to-derived
    /// dependencies are not tracked (see `get_derived`).
    pub fn peek_derived<T: Clone + 'static>(&self, derived: Derived<T>) -> Option<T> {
        let node = self.derived.get(derived.id)?;
        node.value.as_ref()?.downcast_ref::<T>().cloned()
    }

    pub fn get_derived<T: Clone + 'static>(&mut self, derived: Derived<T>) -> Option<T> {
        // Note: For now, we don't track derived -> derived dependencies
        // This would require converting DerivedId to SignalId somehow
        // Future: support full derived -> derived dep tracking

        let node = self.derived.get(derived.id)?;

        // If not dirty and we have a cached value, return it
        if !node.dirty.get() {
            if let Some(ref cached) = node.value {
                return cached.downcast_ref::<T>().cloned();
            }
        }

        // Need to recompute - track dependencies
        self.tracking.replace(Some(Vec::new()));

        // Get compute function (we need to be careful with borrowing)
        let compute: *const Box<dyn Fn(&ReactiveGraph) -> Box<dyn Any + Send> + Send> = {
            let node = self.derived.get(derived.id)?;
            // We can't call compute while borrowing node, so just mark dirty = false
            node.dirty.set(false);

            // Return a reference we can use - actually we need to restructure this
            // For now, let's use a simpler approach
            &node.compute as *const _
        };

        // Set the in-flight pointer around the compute call, mirroring
        // run_effect: a JIT'd DSL closure (or any nested code) reading a
        // signal via `Signal::try_get` must take the re-entrant fast path
        // — the caller already holds the global graph mutex here, so the
        // fallback lock path would deadlock the thread against itself.
        // The fast path also records the read into `self.tracking`, which
        // is exactly the dependency registration this recompute needs.
        // Guarded so a panic inside compute can't leak the pointer.
        struct InFlightGuard;
        impl Drop for InFlightGuard {
            fn drop(&mut self) {
                IN_FLIGHT_GRAPH.with(|c| c.set(std::ptr::null()));
            }
        }
        let prev_in_flight = IN_FLIGHT_GRAPH.with(|c| c.get());
        IN_FLIGHT_GRAPH.with(|c| c.set(self as *const _));
        let _in_flight_guard = InFlightGuard;

        // SAFETY: We're not modifying derived while calling compute
        let value = unsafe { (*compute)(self) };

        drop(_in_flight_guard);
        // Restore an enclosing in-flight scope (nested evaluation inside
        // an effect) rather than leaving it cleared.
        IN_FLIGHT_GRAPH.with(|c| c.set(prev_in_flight));

        // Get tracked dependencies
        let deps = self.tracking.take().unwrap_or_default();

        // Update the node
        if let Some(node) = self.derived.get_mut(derived.id) {
            // Unsubscribe from old dependencies
            for &dep_id in &node.dependencies {
                if let Some(sig) = self.signals.get_mut(dep_id) {
                    sig.subscribers
                        .retain(|s| *s != SubscriberId::Derived(derived.id));
                }
            }

            // Subscribe to new dependencies
            for &dep_id in &deps {
                if let Some(sig) = self.signals.get_mut(dep_id) {
                    let sub = SubscriberId::Derived(derived.id);
                    if !sig.subscribers.contains(&sub) {
                        sig.subscribers.push(sub);
                    }
                }
            }

            // Update depth based on dependencies
            let max_dep_depth = deps
                .iter()
                .filter_map(|&id| self.signals.get(id))
                .map(|_| 0u32) // Signals have depth 0
                .max()
                .unwrap_or(0);

            node.dependencies = deps.into_iter().collect();
            node.depth = max_dep_depth + 1;
            node.cached_version = self.global_version.get();

            let result = value.downcast_ref::<T>().cloned();
            node.value = Some(value);
            result
        } else {
            None
        }
    }

    // =========================================================================
    // EFFECTS
    // =========================================================================

    /// Create an effect that runs when its dependencies change
    pub fn create_effect<F>(&mut self, run: F) -> Effect
    where
        F: FnMut(&ReactiveGraph) + Send + 'static,
    {
        let id = self.effects.insert(EffectNode {
            run: Box::new(run),
            dependencies: SmallVec::new(),
            dirty: Cell::new(true), // Run immediately
            depth: 0,
        });

        // Schedule initial run
        self.pending_effects.borrow_mut().push_back(id);

        if self.batch_depth.get() == 0 {
            self.flush_effects();
        }

        Effect { id }
    }

    /// Dispose of an effect, removing it from the graph
    pub fn dispose_effect(&mut self, effect: Effect) {
        if let Some(node) = self.effects.remove(effect.id) {
            // Unsubscribe from all dependencies
            for &dep_id in &node.dependencies {
                if let Some(sig) = self.signals.get_mut(dep_id) {
                    sig.subscribers
                        .retain(|s| *s != SubscriberId::Effect(effect.id));
                }
            }
        }
    }

    // =========================================================================
    // BATCHING
    // =========================================================================

    /// Start a batch - effects won't run until the batch ends
    pub fn batch_start(&self) {
        self.batch_depth.set(self.batch_depth.get() + 1);
    }

    /// End a batch and flush pending effects
    pub fn batch_end(&mut self) {
        let depth = self.batch_depth.get();
        if depth > 0 {
            self.batch_depth.set(depth - 1);
            if depth == 1 {
                self.flush_effects();
            }
        }
    }

    /// Run a function in a batch context
    pub fn batch<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.batch_start();
        let result = f(self);
        self.batch_end();
        result
    }

    // =========================================================================
    // INTERNAL
    // =========================================================================

    /// Mark a subscriber as dirty
    fn mark_dirty(&mut self, sub: SubscriberId) {
        match sub {
            SubscriberId::Derived(id) => {
                if let Some(node) = self.derived.get(id)
                    && !node.dirty.get()
                {
                    node.dirty.set(true);
                    // Record for the per-set property-binding fire
                    // (drained at the end of `set`). Each derived can
                    // only flip once per set (we're inside the
                    // `!dirty.get()` arm), so no dedup is needed.
                    self.derived_dirty_buffer.borrow_mut().push(id);
                    // Propagate to derived's subscribers
                    let subscribers: SmallVec<[SubscriberId; 4]> = node.subscribers.clone();
                    for sub in subscribers {
                        self.mark_dirty(sub);
                    }
                }
            }
            SubscriberId::Effect(id) => {
                if let Some(node) = self.effects.get(id) {
                    if !node.dirty.get() {
                        node.dirty.set(true);
                        self.pending_effects.borrow_mut().push_back(id);
                    }
                }
            }
        }
    }

    /// Flush all pending effects
    fn flush_effects(&mut self) {
        // Sort by depth for proper execution order
        let mut effects: Vec<EffectId> = self.pending_effects.borrow_mut().drain(..).collect();
        effects.sort_by_key(|id| self.effects.get(*id).map(|n| n.depth).unwrap_or(0));

        for effect_id in effects {
            self.run_effect(effect_id);
        }
    }

    /// Run a single effect
    fn run_effect(&mut self, effect_id: EffectId) {
        // Check if still dirty (might have been run as dependency of another)
        let should_run = self
            .effects
            .get(effect_id)
            .map(|n| n.dirty.get())
            .unwrap_or(false);

        if !should_run {
            return;
        }

        // Start tracking dependencies
        self.tracking.replace(Some(Vec::new()));

        // Get the run function - we need to be careful with mutability
        // For now, we'll use a simple approach that requires unsafe
        let run_ptr: *mut Box<dyn FnMut(&ReactiveGraph) + Send> = {
            if let Some(node) = self.effects.get_mut(effect_id) {
                node.dirty.set(false);
                &mut node.run as *mut _
            } else {
                return;
            }
        };

        // Set the thread-local in-flight graph pointer so `Signal<T>::get`
        // / `set` / `update` calls inside the effect closure take the
        // re-entrant fast path (read directly from this graph, queue
        // writes for post-flush draining) instead of re-acquiring the
        // global mutex. Cleared in a guard's Drop so a panic inside the
        // closure can't leak the pointer to subsequent code.
        struct InFlightGuard;
        impl Drop for InFlightGuard {
            fn drop(&mut self) {
                IN_FLIGHT_GRAPH.with(|c| c.set(std::ptr::null()));
            }
        }
        IN_FLIGHT_GRAPH.with(|c| c.set(self as *const _));
        let _guard = InFlightGuard;

        // SAFETY: We're not modifying the effect while running it
        // (though the effect can modify signals, which is fine)
        unsafe {
            (*run_ptr)(self);
        }
        drop(_guard);

        // Get tracked dependencies
        let deps = self.tracking.take().unwrap_or_default();

        // Update subscriptions
        if let Some(node) = self.effects.get_mut(effect_id) {
            // Unsubscribe from old dependencies
            for &dep_id in &node.dependencies {
                if let Some(sig) = self.signals.get_mut(dep_id) {
                    sig.subscribers
                        .retain(|s| *s != SubscriberId::Effect(effect_id));
                }
            }

            // Subscribe to new dependencies
            for &dep_id in &deps {
                if let Some(sig) = self.signals.get_mut(dep_id) {
                    let sub = SubscriberId::Effect(effect_id);
                    if !sig.subscribers.contains(&sub) {
                        sig.subscribers.push(sub);
                    }
                }
            }

            node.dependencies = deps.into_iter().collect();
        }
    }

    /// Get statistics about the reactive graph
    pub fn stats(&self) -> ReactiveStats {
        ReactiveStats {
            signal_count: self.signals.len(),
            derived_count: self.derived.len(),
            effect_count: self.effects.len(),
            pending_effects: self.pending_effects.borrow().len(),
            global_version: self.global_version.get(),
        }
    }
}

impl Default for ReactiveGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the reactive graph
#[derive(Debug, Clone)]
pub struct ReactiveStats {
    pub signal_count: usize,
    pub derived_count: usize,
    pub effect_count: usize,
    pub pending_effects: usize,
    pub global_version: u64,
}

// =============================================================================
// STATE - High-level API for component state management
// =============================================================================

/// Shared reactive graph for thread-safe access
pub type SharedReactiveGraph = Arc<Mutex<ReactiveGraph>>;

/// Shared dirty flag for triggering UI rebuilds
pub type DirtyFlag = Arc<AtomicBool>;

/// Callback for notifying stateful elements of signal changes
pub type StatefulDepsCallback = Arc<dyn Fn(&[SignalId]) + Send + Sync>;

/// Global notifier for property-binding subscribers
/// ([[project-reactive-architecture-v2]] Phase 2). Registered once by
/// `blinc_layout::binding` on first use; fires on every `State<T>::set`
/// in addition to the per-State `stateful_deps_callback`.
///
/// blinc_core can't depend on blinc_layout (cyclic dep), so the binding
/// registry lives in blinc_layout and the core just exposes this hook.
/// `OnceLock` means a single notifier is installed for the process
/// lifetime; subsequent `set_property_binding_notifier` calls are
/// silently ignored — matches the singleton lifecycle of the binding
/// registry.
static PROPERTY_BINDING_NOTIFIER: std::sync::OnceLock<
    Box<dyn Fn(SignalId) + Send + Sync + 'static>,
> = std::sync::OnceLock::new();

/// Install the global property-binding notifier. Called by
/// `blinc_layout` on first access of its registry. Idempotent: only the
/// first call wins.
pub fn set_property_binding_notifier(notifier: impl Fn(SignalId) + Send + Sync + 'static) {
    let _ = PROPERTY_BINDING_NOTIFIER.set(Box::new(notifier));
}

/// Fire the property-binding notifier for a signal that just changed.
/// No-op if no notifier is installed (binding registry never accessed).
pub(crate) fn notify_property_bindings(id: SignalId) {
    if let Some(notifier) = PROPERTY_BINDING_NOTIFIER.get() {
        notifier(id);
    }
}

/// Global notifier for stateful-element dependency tracking.
/// Installed by [`crate::context_state::BlincContextState`] on first
/// init; fired by [`Signal<T>::set`] / [`Signal<T>::update`] so that
/// `Stateful` elements with `.deps([signal.id()])` refresh on the
/// same path as `State<T>::set` does today.
static STATEFUL_DEPS_NOTIFIER: std::sync::OnceLock<
    Box<dyn Fn(&[SignalId]) + Send + Sync + 'static>,
> = std::sync::OnceLock::new();

/// Install the global stateful-deps notifier. Idempotent.
pub fn set_stateful_deps_notifier(notifier: impl Fn(&[SignalId]) + Send + Sync + 'static) {
    let _ = STATEFUL_DEPS_NOTIFIER.set(Box::new(notifier));
}

/// Fire the stateful-deps notifier. No-op if none installed.
pub(crate) fn notify_stateful_deps(ids: &[SignalId]) {
    if let Some(notifier) = STATEFUL_DEPS_NOTIFIER.get() {
        notifier(ids);
    }
}

/// Global notifier for portal subscribers (`blinc_portal_ui`). Mirrors
/// [`set_property_binding_notifier`]: a single notifier installed on
/// first portal creation routes per-`SignalId` change events to the
/// portal-side subscription map, which in turn flips per-portal dirty
/// flags so the next composite re-paints any portal that read the
/// signal during its last frame. Fired alongside the retained-mode
/// notifiers from every `Signal::set` / `Signal::update` —
/// portal-bound and retained-bound visuals stay in sync on the same
/// notification edge.
static PORTAL_NOTIFIER: std::sync::OnceLock<Box<dyn Fn(SignalId) + Send + Sync + 'static>> =
    std::sync::OnceLock::new();

/// Install the global portal notifier. Idempotent: first installer wins.
pub fn set_portal_notifier(notifier: impl Fn(SignalId) + Send + Sync + 'static) {
    let _ = PORTAL_NOTIFIER.set(Box::new(notifier));
}

/// Fire the portal notifier. No-op if none installed.
pub(crate) fn notify_portals(id: SignalId) {
    if let Some(notifier) = PORTAL_NOTIFIER.get() {
        notifier(id);
    }
}

/// Run `f` with automatic dependency tracking enabled and return the
/// closure's result paired with every [`SignalId`] read inside it.
///
/// This is the public seam over the same `tracking` cell that powers
/// `create_effect` / `create_derived`: a portal calls this around its
/// per-frame render closure, gets back the list of signals every
/// `Signal::get()` (and `State::get()`) touched, and uses the diff
/// against the previous frame's read set to keep its subscription map
/// in sync. Nesting is safe — the prior tracking buffer (if any) is
/// saved on entry and restored on exit so a portal running INSIDE an
/// effect doesn't strand the outer tracker's deps.
///
/// The closure is free to read, write, or fire any other reactive
/// operation; only reads observed via the standard `Signal::get` /
/// `State::get` path are recorded.
pub fn with_read_tracking<R>(f: impl FnOnce() -> R) -> (R, Vec<SignalId>) {
    let graph = global_graph();
    // Stash whatever tracker was active (effect, derived, or none) and
    // install a fresh one for the closure.
    let prev = {
        let mut g = graph.lock().expect("reactive graph poisoned");
        g.tracking.replace(Some(Vec::new()))
    };
    let result = f();
    // Take what `f` accumulated, restore the prior tracker.
    let deps = {
        let mut g = graph.lock().expect("reactive graph poisoned");
        g.tracking.replace(prev).unwrap_or_default()
    };
    (result, deps)
}

// =============================================================================
// Process-global default reactive graph
//
// `Signal<T>` standalone (no `State<T>` wrapper, no `BlincContextState`
// required) operates against this graph. The same Arc is used by
// `BlincContextState` so that `use_state` / `use_state_keyed` produce
// `State<T>` values that share dependency tracking with bare
// `signal(...)` / `computed(...)` / `effect(...)` calls.
// =============================================================================

/// Process-wide default reactive graph. First touch initialises it;
/// every `signal(...)`, `computed(...)`, `effect(...)`, and every
/// `Signal<T>::get/set/update` operates against this Arc.
static GLOBAL_GRAPH: LazyLock<SharedReactiveGraph> =
    LazyLock::new(|| Arc::new(Mutex::new(ReactiveGraph::new())));

/// Process-wide default dirty flag, paired with [`GLOBAL_GRAPH`].
/// Platform runners read this every frame to decide whether to
/// re-render. `BlincContextState` shares the same Arc.
static GLOBAL_DIRTY: LazyLock<DirtyFlag> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));

/// Get a clone of the process-global reactive graph Arc. Cheap —
/// just an Arc bump. Platform runners should use this instead of
/// minting their own graph so standalone `signal(...)` shares the
/// reactive surface with `State<T>` / `Computed<T>` callers.
pub fn global_graph() -> SharedReactiveGraph {
    Arc::clone(&GLOBAL_GRAPH)
}

/// Get a clone of the process-global dirty flag Arc.
pub fn global_dirty_flag() -> DirtyFlag {
    Arc::clone(&GLOBAL_DIRTY)
}

/// A bound state value with direct get/set methods
///
/// This is the primary API for component state management. It wraps a signal
/// with thread-safe access to the reactive graph and provides convenient
/// methods for reading and writing state.
///
/// # Example
///
/// ```ignore
/// // State is typically obtained from a context
/// let counter: State<i32> = ctx.use_state_keyed("counter", || 0);
///
/// // Read the current value
/// let value = counter.get();
///
/// // Update the value (doesn't trigger tree rebuild)
/// counter.set(value + 1);
///
/// // Update the value AND trigger tree rebuild
/// counter.set_rebuild(value + 1);
/// ```
#[derive(Clone)]
pub struct State<T> {
    signal: Signal<T>,
    reactive: SharedReactiveGraph,
    dirty_flag: DirtyFlag,
    /// Optional callback for notifying stateful elements of signal changes
    stateful_deps_callback: Option<StatefulDepsCallback>,
    /// Optional read adapter, for a `State<T>` that subscribes to a
    /// signal of a *different* stored type.
    ///
    /// `signal` still carries the source signal's id, so binding
    /// registration and `deps()` subscribe to the right thing; only the
    /// read is redirected. Used to expose an `f64` signal as a
    /// `State<f32>` for the f32-backed layout properties, without
    /// wrapping it in a derived -- a derived source would register a
    /// derived binding, which does not refresh those properties.
    read_adapter: Option<Arc<dyn Fn() -> Option<T> + Send + Sync>>,
}

impl<T: Clone + Send + 'static> State<T> {
    /// Create a new State wrapper
    pub fn new(signal: Signal<T>, reactive: SharedReactiveGraph, dirty_flag: DirtyFlag) -> Self {
        Self {
            signal,
            reactive,
            dirty_flag,
            stateful_deps_callback: None,
            read_adapter: None,
        }
    }

    /// Build a `State<T>` that subscribes to `source` but reads through
    /// `read`. See [`State::read_adapter`].
    pub fn mapped(
        source: SignalId,
        read: Arc<dyn Fn() -> Option<T> + Send + Sync>,
        reactive: SharedReactiveGraph,
        dirty_flag: DirtyFlag,
    ) -> Self {
        Self {
            signal: Signal::from_id(source),
            reactive,
            dirty_flag,
            stateful_deps_callback: None,
            read_adapter: Some(read),
        }
    }

    /// Create a new State wrapper with a stateful deps callback
    pub fn with_stateful_callback(
        signal: Signal<T>,
        reactive: SharedReactiveGraph,
        dirty_flag: DirtyFlag,
        callback: StatefulDepsCallback,
    ) -> Self {
        Self {
            signal,
            reactive,
            dirty_flag,
            stateful_deps_callback: Some(callback),
            read_adapter: None,
        }
    }

    /// Get the current value
    pub fn get(&self) -> T
    where
        T: Default,
    {
        if let Some(read) = &self.read_adapter {
            return read().unwrap_or_default();
        }
        self.reactive
            .lock()
            .unwrap()
            .get(self.signal)
            .unwrap_or_default()
    }

    /// Get the current value, returning None if not found
    pub fn try_get(&self) -> Option<T> {
        if let Some(read) = &self.read_adapter {
            return read();
        }
        self.reactive.lock().unwrap().get(self.signal)
    }

    /// Set a new value
    ///
    /// This updates the value without triggering a tree rebuild.
    /// The renderer reads values at render time, so changes are
    /// reflected on the next frame automatically.
    ///
    /// Use `set_rebuild()` only when the change affects tree structure
    /// (adding/removing elements, changing text content, etc.)
    pub fn set(&self, value: T) {
        // Set + drain the dirty-derived list in one lock window so the
        // ids the *just-completed* set produced are the ones we fire
        // for. Drop the lock BEFORE invoking notifiers — the binding
        // registry's read closures re-enter this mutex.
        let dirty_derived = {
            let mut g = self.reactive.lock().unwrap();
            g.set(self.signal, value);
            g.take_dirty_derived()
        };
        // Notify stateful elements if callback is set
        if let Some(ref callback) = self.stateful_deps_callback {
            callback(&[self.signal.id()]);
        }
        // Fire signal-bound property-binding subscribers (P2).
        notify_property_bindings(self.signal.id());
        notify_portals(self.signal.id());
        // Fire derived-bound property-binding subscribers for every
        // derived that flipped to dirty during this set (Phase 8
        // follow-up: Derived ↔ IntoReactive bridge).
        for d_id in dirty_derived {
            notify_property_bindings_for_derived(d_id);
        }
    }

    /// Set a new value AND trigger a UI tree rebuild
    ///
    /// Only use this when the state change affects tree structure:
    /// - Adding or removing elements
    /// - Changing text content
    /// - Changing layout-affecting properties (size, padding, etc.)
    ///
    /// For visual-only changes (colors, opacity, animations), use `set()`.
    pub fn set_rebuild(&self, value: T) {
        let dirty_derived = {
            let mut g = self.reactive.lock().unwrap();
            g.set(self.signal, value);
            g.take_dirty_derived()
        };
        self.dirty_flag.store(true, Ordering::SeqCst);
        // Property bindings still fire even on the rebuild path — a
        // signal-bound `.bg(&state)` should patch alongside the rebuild.
        notify_property_bindings(self.signal.id());
        notify_portals(self.signal.id());
        for d_id in dirty_derived {
            notify_property_bindings_for_derived(d_id);
        }
    }

    /// Update the value using a function
    ///
    /// Does not trigger rebuild. Use `update_rebuild()` for structural changes.
    pub fn update(&self, f: impl FnOnce(T) -> T) {
        let dirty_derived = {
            let mut g = self.reactive.lock().unwrap();
            g.update(self.signal, f);
            g.take_dirty_derived()
        };
        // Notify stateful elements if callback is set
        if let Some(ref callback) = self.stateful_deps_callback {
            callback(&[self.signal.id()]);
        }
        notify_property_bindings(self.signal.id());
        notify_portals(self.signal.id());
        for d_id in dirty_derived {
            notify_property_bindings_for_derived(d_id);
        }
    }

    /// Update the value AND trigger a UI tree rebuild
    pub fn update_rebuild(&self, f: impl FnOnce(T) -> T) {
        let dirty_derived = {
            let mut g = self.reactive.lock().unwrap();
            g.update(self.signal, f);
            g.take_dirty_derived()
        };
        self.dirty_flag.store(true, Ordering::SeqCst);
        notify_property_bindings(self.signal.id());
        notify_portals(self.signal.id());
        for d_id in dirty_derived {
            notify_property_bindings_for_derived(d_id);
        }
    }

    /// Get the underlying signal (for advanced use cases)
    pub fn signal(&self) -> Signal<T> {
        self.signal
    }

    /// Get the signal ID (for dependency tracking)
    pub fn signal_id(&self) -> SignalId {
        self.signal.id()
    }
}

/// Global notifier for derived-driven property-binding subscribers.
/// Parallels [`PROPERTY_BINDING_NOTIFIER`] but keyed by `DerivedId`
/// instead of `SignalId`. Fires from inside [`ReactiveGraph::set`]
/// after the dirty propagation walk completes, for every derived
/// that was freshly dirtied this set.
///
/// blinc_core can't depend on blinc_layout (cyclic dep), so the
/// property-binding registry installs both notifiers as a pair on
/// first access. Same OnceLock idempotence story.
static DERIVED_BINDING_NOTIFIER: std::sync::OnceLock<
    Box<dyn Fn(DerivedId) + Send + Sync + 'static>,
> = std::sync::OnceLock::new();

/// Install the global derived-binding notifier. Paired with
/// [`set_property_binding_notifier`] — `blinc_layout::binding`
/// installs both on first registry access.
pub fn set_derived_binding_notifier(notifier: impl Fn(DerivedId) + Send + Sync + 'static) {
    let _ = DERIVED_BINDING_NOTIFIER.set(Box::new(notifier));
}

/// Fire the derived-binding notifier for a derived whose value
/// might have changed (i.e. its dirty bit was just flipped). No-op
/// if no notifier is installed.
pub(crate) fn notify_property_bindings_for_derived(id: DerivedId) {
    if let Some(notifier) = DERIVED_BINDING_NOTIFIER.get() {
        notifier(id);
    }
}

/// Ergonomic wrapper around [`Derived<T>`] that also carries the
/// reactive graph reference. Same shape as [`State<T>`] — both bundle
/// a handle (Signal / Derived) with a `SharedReactiveGraph` so
/// readers don't need to plumb the graph through every call site.
///
/// `Computed<T>` is the public binding-friendly form of the lazy
/// computed value. The underlying [`Derived<T>`] handle is exposed
/// via [`Self::derived`] for advanced uses that need raw access to
/// `ReactiveGraph::get_derived` etc.
///
/// # Reactivity
///
/// Reads call `get_derived` on the underlying graph, which:
/// 1. Recomputes the value if the cache is stale (dirty bit set).
/// 2. Auto-tracks dependencies — any signal touched inside the
///    compute closure subscribes this derived for future dirty
///    notifications.
///
/// When any tracked dependency fires via `State::set`, this
/// derived's dirty bit flips and the property-binding registry is
/// notified via `notify_property_bindings_for_derived` — bindings
/// that subscribed to this `Computed<T>` re-fire and read the
/// recomputed value.
///
/// # Example
///
/// ```ignore
/// let graph: SharedReactiveGraph = ...;
/// let x = State::new(...);
/// let y = State::new(...);
/// let x_sig = x.signal_id();
/// let y_sig = y.signal_id();
/// // create_derived auto-tracks signal reads
/// let pos = {
///     let mut g = graph.lock().unwrap();
///     let d = g.create_derived(move |g| {
///         let x = g.get::<f32>(Signal::from_id(x_sig)).unwrap_or(0.0);
///         let y = g.get::<f32>(Signal::from_id(y_sig)).unwrap_or(0.0);
///         (x, y)
///     });
///     Computed::new(d, graph.clone())
/// };
/// ```
pub struct Computed<T> {
    derived: Derived<T>,
    reactive: SharedReactiveGraph,
    /// Optional read adapter, for a `Computed<T>` that subscribes to a
    /// derived of a *different* stored type.
    ///
    /// `derived` still carries the source derived's id, so binding
    /// registration keys off the upstream and fires whenever it does;
    /// only the read is redirected. Mirrors [`State::mapped`] for the
    /// derived path -- wrapping the upstream in a second `Computed`
    /// instead would need derived-to-derived dependency tracking,
    /// which `get_derived` does not do, so the wrapper would never be
    /// marked dirty.
    read_adapter: Option<Arc<dyn Fn() -> Option<T> + Send + Sync>>,
}

impl<T> Clone for Computed<T> {
    fn clone(&self) -> Self {
        Self {
            derived: self.derived,
            reactive: Arc::clone(&self.reactive),
            read_adapter: self.read_adapter.clone(),
        }
    }
}

impl<T: Clone + Send + 'static> Computed<T> {
    /// Create a new `Computed<T>` bundling a `Derived<T>` handle with
    /// the reactive graph it lives in.
    pub fn new(derived: Derived<T>, reactive: SharedReactiveGraph) -> Self {
        Self {
            derived,
            reactive,
            read_adapter: None,
        }
    }

    /// Build a `Computed<T>` that subscribes to `source` but reads
    /// through `read`. See [`Computed::read_adapter`].
    pub fn mapped(
        source: DerivedId,
        read: Arc<dyn Fn() -> Option<T> + Send + Sync>,
        reactive: SharedReactiveGraph,
    ) -> Self {
        Self {
            derived: Derived::from_id(source),
            reactive,
            read_adapter: Some(read),
        }
    }

    /// Reconstruct a `Computed<T>` from a raw `DerivedId`, anchored
    /// to the process-global reactive graph.
    ///
    /// # Safety
    /// The caller must ensure the `DerivedId` refers to a derived
    /// of type `T` and lives in the global graph (i.e. was minted
    /// by the [`computed`] / [`derived`] free function or one of the
    /// `BlincContextState::use_*` helpers). Used by the FFI
    /// boundary to rehydrate a `Computed<T>` baked into JIT code by
    /// `computed { … } : T` lowering — the lowering hands an
    /// `i64` derived-id to the host extern; the extern thunk calls
    /// this to recover a typed handle.
    pub fn from_id(id: DerivedId) -> Self {
        Self {
            derived: Derived::from_id(id),
            reactive: global_graph(),
            read_adapter: None,
        }
    }

    /// Get the current value, recomputing if stale. Always returns
    /// `Some` unless the derived handle is invalid (i.e. the graph
    /// was rebuilt and the derived id no longer resolves).
    pub fn try_get(&self) -> Option<T> {
        if let Some(read) = &self.read_adapter {
            return read();
        }
        // Re-entrant path first, mirroring `Signal::try_get`: when this
        // thread is already inside a compute (it holds the graph mutex),
        // locking again would deadlock against itself. Reads the cached
        // value rather than recomputing, since recompute needs `&mut`.
        if let Some(value) = with_in_flight_graph(|g| g.peek_derived(self.derived)) {
            return value;
        }
        self.reactive.lock().unwrap().get_derived(self.derived)
    }

    /// Get the current value, panicking on failure. Matches
    /// [`State<T>::get`]'s ergonomic shape.
    pub fn get(&self) -> T {
        self.try_get()
            .expect("Computed::get: derived handle does not resolve in its graph")
    }

    /// The underlying `Derived<T>` handle, for advanced use.
    pub fn derived(&self) -> Derived<T> {
        self.derived
    }

    /// The derived's id — used by the property-binding registry to
    /// key subscriptions.
    pub fn derived_id(&self) -> DerivedId {
        self.derived.id
    }

    /// The shared reactive graph this computed lives in. Cloned to
    /// give callers an independent `Arc<Mutex<…>>` handle.
    pub fn graph(&self) -> SharedReactiveGraph {
        Arc::clone(&self.reactive)
    }
}

// =========================================================================
// SolidJS-style free functions over the process-global graph
//
// These match the familiar `signal()` / `computed()` / `derived()` /
// `effect()` surface from SolidJS / Leptos. Each operates against
// [`GLOBAL_GRAPH`] so the values they produce interoperate seamlessly
// with `State<T>`, `use_state*`, and the property-binding registry.
// =========================================================================

/// Create a fresh standalone reactive signal initialised to `initial`.
/// Lives in the process-global graph; cleaned up when its slotmap key
/// is reclaimed (currently: never — slotmap keys aren't reclaimed
/// until the graph itself drops, which matches the existing
/// `use_state_keyed` story).
///
/// Returned `Signal<T>` is `Copy` — capture by value in closures
/// without `.clone()` boilerplate. Use [`Signal::set`] / [`Signal::get`]
/// / [`Signal::update`] to interact.
///
/// # Example
/// ```ignore
/// use blinc_core::reactive::signal;
///
/// let count = signal(0_i32);
/// // count is Copy — re-capture freely.
/// button.on_click(move |_| count.update(|v| v + 1));
/// label.text(&count.get().to_string());
/// ```
pub fn signal<T: Send + 'static>(initial: T) -> Signal<T> {
    let graph = global_graph();
    let mut g = graph.lock().unwrap();
    g.create_signal(initial)
}

/// Create a derived (computed) value that auto-tracks every signal
/// touched inside `compute`. The closure runs lazily — first read,
/// then again after any tracked dependency changes.
///
/// Returns a [`Computed<T>`] which plugs into the same
/// `IntoReactive<T>` channel as `Signal<T>` / `State<T>`; pass
/// `&computed` to any reactive setter (`.bg`, `.opacity`, `.w`, …).
///
/// # Example
/// ```ignore
/// let a = signal(1);
/// let b = signal(2);
/// let sum = computed(move |g| g.get(a).unwrap_or(0) + g.get(b).unwrap_or(0));
/// // sum.get() === 3; sum re-fires whenever a or b sets.
/// ```
pub fn computed<T, F>(compute: F) -> Computed<T>
where
    T: Clone + Send + 'static,
    F: Fn(&ReactiveGraph) -> T + Send + 'static,
{
    let graph = global_graph();
    let derived = {
        let mut g = graph.lock().unwrap();
        g.create_derived(compute)
    };
    Computed::new(derived, graph)
}

/// SolidJS-flavoured alias for [`computed`] — same semantics, just
/// the name `derived` for callers more comfortable with that term.
pub fn derived<T, F>(compute: F) -> Computed<T>
where
    T: Clone + Send + 'static,
    F: Fn(&ReactiveGraph) -> T + Send + 'static,
{
    computed(compute)
}

/// Create an effect that runs every time any signal touched inside
/// `run` changes. Auto-tracks dependencies on first run.
///
/// Effects are side-effects — logging, IO, custom integrations.
/// For UI updates prefer property bindings (`.bg(&signal)`) or
/// `Stateful` + `.deps([...])`; effects don't have a render path.
///
/// # Example
/// ```ignore
/// let count = signal(0);
/// let _e = effect(move |g| {
///     println!("count = {}", g.get(count).unwrap_or(0));
/// });
/// count.set(5); // prints "count = 5" next batch flush
/// ```
pub fn effect<F>(run: F) -> Effect
where
    F: FnMut(&ReactiveGraph) + Send + 'static,
{
    let handle = {
        let graph = global_graph();
        let mut g = graph.lock().unwrap();
        g.create_effect(run)
    };
    // The effect's initial run happens inside `create_effect`'s
    // `flush_effects`. If that closure queued writes via the
    // re-entrancy fast path, drain them here — outside the lock,
    // outside the in-flight window — so the writes actually take
    // effect.
    drain_deferred_writes();
    handle
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A derived whose compute closure reads a signal through the
    /// GLOBAL path (`Signal::try_get`) instead of the passed `&graph`.
    /// This is exactly what a JIT'd DSL `computed {{ sig.get() }}` does —
    /// the generated code can't thread the graph param, so it goes
    /// through the process-global registry. Before `get_derived` set
    /// `IN_FLIGHT_GRAPH` around compute, that read re-locked the global
    /// mutex the caller already held: a self-deadlock (the DSL computed
    /// tests hung forever). Also asserts the in-flight read registers
    /// the dependency, so the derived re-fires on set.
    #[test]
    fn derived_compute_may_read_signals_via_global_path() {
        let sig = signal(0.25_f64);
        let c = computed::<f64, _>(move |_graph| sig.try_get().unwrap_or(-1.0));
        assert_eq!(c.try_get(), Some(0.25));

        sig.set(0.85);
        assert_eq!(
            c.try_get(),
            Some(0.85),
            "global-path read inside compute must register the dependency"
        );
    }

    #[test]
    fn test_signal_create_get_set() {
        let mut graph = ReactiveGraph::new();

        let count = graph.create_signal(0i32);
        assert_eq!(graph.get(count), Some(0));

        graph.set(count, 42);
        assert_eq!(graph.get(count), Some(42));
    }

    #[test]
    fn test_signal_update() {
        let mut graph = ReactiveGraph::new();

        let count = graph.create_signal(10i32);
        graph.update(count, |x| x + 5);
        assert_eq!(graph.get(count), Some(15));
    }

    #[test]
    fn test_derived_basic() {
        let mut graph = ReactiveGraph::new();

        let count = graph.create_signal(5i32);
        let doubled = graph.create_derived(move |g| g.get(count).unwrap_or(0) * 2);

        assert_eq!(graph.get_derived(doubled), Some(10));

        graph.set(count, 7);
        assert_eq!(graph.get_derived(doubled), Some(14));
    }

    #[test]
    fn test_derived_caching() {
        let mut graph = ReactiveGraph::new();
        let compute_count = Arc::new(Mutex::new(0));

        let count = graph.create_signal(5i32);
        let compute_count_clone = compute_count.clone();
        let doubled = graph.create_derived(move |g| {
            *compute_count_clone.lock().unwrap() += 1;
            g.get(count).unwrap_or(0) * 2
        });

        // First access computes
        assert_eq!(graph.get_derived(doubled), Some(10));
        assert_eq!(*compute_count.lock().unwrap(), 1);

        // Second access uses cache
        assert_eq!(graph.get_derived(doubled), Some(10));
        assert_eq!(*compute_count.lock().unwrap(), 1);

        // After signal change, recomputes
        graph.set(count, 7);
        assert_eq!(graph.get_derived(doubled), Some(14));
        assert_eq!(*compute_count.lock().unwrap(), 2);
    }

    #[test]
    fn test_effect_runs_on_change() {
        let mut graph = ReactiveGraph::new();
        let effect_runs = Arc::new(Mutex::new(Vec::new()));

        let count = graph.create_signal(0i32);
        let effect_runs_clone = effect_runs.clone();

        let _effect = graph.create_effect(move |g| {
            let val = g.get(count).unwrap_or(0);
            effect_runs_clone.lock().unwrap().push(val);
        });

        // Effect runs immediately
        assert_eq!(*effect_runs.lock().unwrap(), vec![0]);

        // Effect runs on signal change
        graph.set(count, 1);
        assert_eq!(*effect_runs.lock().unwrap(), vec![0, 1]);

        graph.set(count, 2);
        assert_eq!(*effect_runs.lock().unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn test_batching() {
        let mut graph = ReactiveGraph::new();
        let effect_runs = Arc::new(Mutex::new(0));

        let a = graph.create_signal(1i32);
        let b = graph.create_signal(2i32);
        let effect_runs_clone = effect_runs.clone();

        let _effect = graph.create_effect(move |g| {
            let _a = g.get(a);
            let _b = g.get(b);
            *effect_runs_clone.lock().unwrap() += 1;
        });

        // Initial run
        assert_eq!(*effect_runs.lock().unwrap(), 1);

        // Without batching, effect runs twice
        *effect_runs.lock().unwrap() = 0;
        graph.set(a, 10);
        graph.set(b, 20);
        assert_eq!(*effect_runs.lock().unwrap(), 2);

        // With batching, effect runs once
        *effect_runs.lock().unwrap() = 0;
        graph.batch(|g| {
            g.set(a, 100);
            g.set(b, 200);
        });
        assert_eq!(*effect_runs.lock().unwrap(), 1);
    }

    #[test]
    fn test_dispose_effect() {
        let mut graph = ReactiveGraph::new();
        let effect_runs = Arc::new(Mutex::new(0));

        let count = graph.create_signal(0i32);
        let effect_runs_clone = effect_runs.clone();

        let effect = graph.create_effect(move |g| {
            let _val = g.get(count);
            *effect_runs_clone.lock().unwrap() += 1;
        });

        assert_eq!(*effect_runs.lock().unwrap(), 1);

        graph.set(count, 1);
        assert_eq!(*effect_runs.lock().unwrap(), 2);

        // Dispose the effect
        graph.dispose_effect(effect);

        // Effect should no longer run
        graph.set(count, 2);
        assert_eq!(*effect_runs.lock().unwrap(), 2);
    }

    #[test]
    fn test_multiple_signals() {
        let mut graph = ReactiveGraph::new();

        let a = graph.create_signal(1i32);
        let b = graph.create_signal(2i32);
        let c = graph.create_signal(3i32);

        let sum = graph.create_derived(move |g| {
            g.get(a).unwrap_or(0) + g.get(b).unwrap_or(0) + g.get(c).unwrap_or(0)
        });

        assert_eq!(graph.get_derived(sum), Some(6));

        graph.set(b, 10);
        assert_eq!(graph.get_derived(sum), Some(14));
    }

    #[test]
    fn test_stats() {
        let mut graph = ReactiveGraph::new();

        let _s1 = graph.create_signal(1);
        let _s2 = graph.create_signal(2);
        let _d1 = graph.create_derived(|_| 0);

        let stats = graph.stats();
        assert_eq!(stats.signal_count, 2);
        assert_eq!(stats.derived_count, 1);
    }
}
