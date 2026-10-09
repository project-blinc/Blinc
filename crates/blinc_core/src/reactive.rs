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

    /// How many host effect scopes are open on this thread. See
    /// [`ReactiveGraph::begin_effect`].
    static HOST_EFFECT_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Returns `true` if a `flush_effects` callback is currently running
/// on this thread — i.e. re-entrant signal access would deadlock if
/// it took the mutex.
fn is_in_flush() -> bool {
    IN_FLIGHT_GRAPH.with(|c| !c.get().is_null())
}

/// Whether a write made now has to wait: inside an effect or derived that
/// holds the graph, or inside a host effect scope. A host scope holds no
/// lock and sets no in-flight pointer, so [`is_in_flush`] is false there and
/// reads take the ordinary locked path; only the write waits.
fn defers_writes() -> bool {
    is_in_flush() || HOST_EFFECT_DEPTH.with(|d| d.get() > 0)
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
        if defers_writes() {
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
        if defers_writes() {
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
        if defers_writes() {
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
        if defers_writes() {
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
type ComputeFn = Box<dyn Fn(&ReactiveGraph) -> Box<dyn Any + Send> + Send>;

struct DerivedNode {
    /// Cached value (if computed)
    value: Option<Box<dyn Any + Send>>,
    /// Version of cached value
    cached_version: u64,
    /// The compute function.
    ///
    /// `Option` so an evaluation can TAKE it, releasing the borrow on the
    /// derived map for the duration of the call. A nested `computed()`
    /// then has a free map to insert into. Previously the evaluation held
    /// a raw pointer into the map across the closure, so an insert could
    /// have reallocated under it.
    ///
    /// `None` only while this derived is mid-evaluation, which also makes
    /// a self-referential read return `None` instead of recursing.
    compute: Option<ComputeFn>,
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
    /// The effect function. `None` for a host-run effect, whose body lives in
    /// another language and is driven through
    /// [`ReactiveGraph::begin_effect`] and [`ReactiveGraph::end_effect`].
    run: Option<Box<dyn FnMut(&ReactiveGraph) + Send>>,
    /// Dependencies (signals this effect reads from)
    dependencies: SmallVec<[SignalId; 4]>,
    /// Whether the effect needs to run
    dirty: Cell<bool>,
    /// Depth in the dependency graph
    depth: u32,
}

/// The reactive graph that manages all signals, derived values, and effects
pub struct ReactiveGraph {
    /// Interior-mutable so a signal can be CREATED while a derived or
    /// effect closure is in flight: that path only has `&ReactiveGraph`,
    /// because the lock guard is held further up the stack. Every other
    /// field touched during an evaluation is already `RefCell`/`Cell` for
    /// the same reason.
    ///
    /// No raw pointer into this map is held across a call into user code,
    /// which is what makes insertion during an evaluation sound here.
    /// `derived` and `effects` are NOT like this — `get_derived` and
    /// `run_effect` point into them across the closure, so inserting
    /// there mid-evaluation could reallocate under a live pointer.
    signals: RefCell<SlotMap<SignalId, SignalNode>>,
    /// Interior-mutable for the same reason as `signals`: a `computed()`
    /// created inside an evaluation needs to insert through
    /// `&ReactiveGraph`. Safe to insert into mid-evaluation only because
    /// `with_compute` takes the closure out rather than pointing into the
    /// map across the call.
    derived: RefCell<SlotMap<DerivedId, DerivedNode>>,
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
    /// Host effects that are due, in the order they became due. Handed out
    /// by [`Self::take_due_host_effects`] rather than run.
    due_host_effects: Vec<EffectId>,
    /// Open host effect scopes, innermost last.
    effect_scopes: Vec<EffectScope>,
    /// Signals written while any host scope was open, so an effect can see
    /// at `end_effect` that something it read changed before it was
    /// subscribed. Emptied when the outermost scope closes.
    scope_writes: Vec<SignalId>,
}

/// An open host effect scope.
struct EffectScope {
    effect: EffectId,
    /// The tracking buffer that was active when the scope began, put back
    /// when it ends.
    outer_tracking: Option<Vec<SignalId>>,
    /// How many entries `scope_writes` held at the start.
    writes_from: usize,
    /// The thread that began it. The tracking buffer and the write-deferral
    /// flag are per thread, so a scope ended elsewhere would corrupt both.
    thread: std::thread::ThreadId,
}

impl ReactiveGraph {
    /// Create a new reactive graph
    pub fn new() -> Self {
        Self {
            signals: RefCell::new(SlotMap::with_key()),
            derived: RefCell::new(SlotMap::with_key()),
            effects: SlotMap::with_key(),
            pending_effects: RefCell::new(VecDeque::new()),
            batch_depth: Cell::new(0),
            tracking: RefCell::new(None),
            global_version: Cell::new(0),
            derived_dirty_buffer: RefCell::new(SmallVec::new()),
            due_host_effects: Vec::new(),
            effect_scopes: Vec::new(),
            scope_writes: Vec::new(),
        }
    }

    // =========================================================================
    // SIGNALS
    // =========================================================================

    /// Create a new signal with an initial value
    /// Create a signal.
    ///
    /// Takes `&self` so this works through the in-flight graph, which is
    /// all a derived or effect closure has: a nested `signal()` used to
    /// deadlock on the graph mutex the evaluation already held.
    pub fn create_signal<T: Send + 'static>(&self, initial: T) -> Signal<T> {
        let id = self.signals.borrow_mut().insert(SignalNode {
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
            .borrow()
            .get(signal.id)
            .and_then(|node| node.value.downcast_ref::<T>().cloned())
    }

    /// Get the current value without tracking as a dependency
    pub fn get_untracked<T: Clone + 'static>(&self, signal: Signal<T>) -> Option<T> {
        self.signals
            .borrow()
            .get(signal.id)
            .and_then(|node| node.value.downcast_ref::<T>().cloned())
    }

    /// Set the value of a signal, triggering reactive updates
    pub fn set<T: Send + 'static>(&mut self, signal: Signal<T>, value: T) {
        // Mutate under a borrow that ends before anything calls out.
        // `mark_dirty` and `flush_effects` run user closures, and one of
        // those may now CREATE a signal, which borrows this map mutably.
        // Holding the borrow across them would panic.
        let subscribers: SmallVec<[SubscriberId; 4]> = {
            let mut signals = self.signals.borrow_mut();
            let Some(node) = signals.get_mut(signal.id) else {
                return;
            };
            node.value = Box::new(value);
            node.version += 1;
            node.subscribers.clone()
        };
        if !self.effect_scopes.is_empty() {
            self.scope_writes.push(signal.id);
        }
        {
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
        self.signals.borrow().get(id).map(|n| n.version)
    }

    // =========================================================================
    // DERIVED VALUES
    // =========================================================================

    /// Create a derived (computed) value
    /// Create a derived.
    ///
    /// Takes `&self` so this works through the in-flight graph, which is
    /// all a closure has. Sound because `with_compute` takes a derived's
    /// closure out of the map for the duration of its evaluation, so an
    /// insert here cannot reallocate under a live reference.
    pub fn create_derived<T, F>(&self, compute: F) -> Derived<T>
    where
        T: Clone + Send + 'static,
        F: Fn(&ReactiveGraph) -> T + Send + 'static,
    {
        // Wrap the compute function to return boxed Any
        let compute_boxed =
            move |graph: &ReactiveGraph| -> Box<dyn Any + Send> { Box::new(compute(graph)) };

        let id = self.derived.borrow_mut().insert(DerivedNode {
            value: None,
            cached_version: 0,
            compute: Some(Box::new(compute_boxed)),
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
    /// A derived read by another computation or effect, which holds this
    /// graph and so cannot cache: a clean derived gives its cached value, a
    /// dirty or never-computed one runs its compute closure without caching.
    /// Either way the reader comes to depend on the derived's signals, so it
    /// re-runs when they change; the derived itself stays as it was.
    pub fn read_derived_in_flight<T: Clone + 'static>(&self, derived: Derived<T>) -> Option<T> {
        // Clean: answer from the cache and hand the reader our deps,
        // under a borrow that ends before we return.
        {
            let map = self.derived.borrow();
            let node = map.get(derived.id)?;
            if !node.dirty.get() {
                if let Some(value) = node.value.as_ref().and_then(|v| v.downcast_ref::<T>()) {
                    if let Some(tracking) = self.tracking.borrow_mut().as_mut() {
                        tracking.extend(node.dependencies.iter().copied());
                    }
                    return Some(value.clone());
                }
            }
        }
        // Dirty: run it with the map free, so the closure may create.
        // Its reads go through this graph's `get`, which records them in
        // the reader's tracking.
        self.with_compute(derived.id, |compute| compute(self))?
            .downcast::<T>()
            .ok()
            .map(|value| *value)
    }

    pub fn peek_derived<T: Clone + 'static>(&self, derived: Derived<T>) -> Option<T> {
        let map = self.derived.borrow();
        let node = map.get(derived.id)?;
        node.value.as_ref()?.downcast_ref::<T>().cloned()
    }

    /// Run a derived's compute with the derived map NOT borrowed.
    ///
    /// Takes the closure out, calls it, puts it back. That is what lets a
    /// nested `computed()` or `signal()` insert while this evaluation is
    /// in flight. A guard restores the closure even if it panics, so one
    /// bad compute does not leave the derived permanently inert.
    fn with_compute<R>(&self, id: DerivedId, f: impl FnOnce(&ComputeFn) -> R) -> Option<R> {
        let compute = self.derived.borrow_mut().get_mut(id)?.compute.take()?;

        struct Restore<'a> {
            map: &'a RefCell<SlotMap<DerivedId, DerivedNode>>,
            id: DerivedId,
            compute: Option<ComputeFn>,
        }
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                if let Some(c) = self.compute.take()
                    && let Some(node) = self.map.borrow_mut().get_mut(self.id)
                {
                    node.compute = Some(c);
                }
            }
        }
        let mut guard = Restore {
            map: &self.derived,
            id,
            compute: Some(compute),
        };
        let out = f(guard.compute.as_ref().expect("just set"));
        drop(guard);
        Some(out)
    }

    pub fn get_derived<T: Clone + 'static>(&mut self, derived: Derived<T>) -> Option<T> {
        // Note: For now, we don't track derived -> derived dependencies
        // This would require converting DerivedId to SignalId somehow
        // Future: support full derived -> derived dep tracking

        // Cached answer, under a borrow that ends here.
        {
            let map = self.derived.borrow();
            let node = map.get(derived.id)?;
            if !node.dirty.get() {
                if let Some(ref cached) = node.value {
                    // The reader comes to depend on the derived's signals,
                    // as it does when the derived has to be recomputed.
                    if let Some(tracking) = self.tracking.borrow_mut().as_mut() {
                        tracking.extend(node.dependencies.iter().copied());
                    }
                    return cached.downcast_ref::<T>().cloned();
                }
            }
            node.dirty.set(false);
        }

        // Need to recompute - track dependencies. The reader's own tracking
        // (an effect or host scope in progress) is kept and put back below.
        let outer_tracking = self.tracking.replace(Some(Vec::new()));

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

        // Taken out of the map for the call, so the closure is free to
        // create a signal or another computed. Previously this was a raw
        // pointer into the map, which an insert could have reallocated
        // under.
        let value = self.with_compute(derived.id, |compute| compute(self));
        let Some(value) = value else {
            drop(_in_flight_guard);
            IN_FLIGHT_GRAPH.with(|c| c.set(prev_in_flight));
            self.tracking.replace(outer_tracking);
            return None;
        };

        drop(_in_flight_guard);
        // Restore an enclosing in-flight scope (nested evaluation inside
        // an effect) rather than leaving it cleared.
        IN_FLIGHT_GRAPH.with(|c| c.set(prev_in_flight));

        // Get tracked dependencies, give the reader its tracking back, and
        // make the reader depend on them too, as a read of a clean derived
        // does.
        let deps = self.tracking.replace(outer_tracking).unwrap_or_default();
        if let Some(outer) = self.tracking.borrow_mut().as_mut() {
            outer.extend(deps.iter().copied());
        }

        // Update the node
        // Subscription bookkeeping. Collected first, then applied, so no
        // borrow spans the signals map's own borrow.
        let mut derived_map = self.derived.borrow_mut();
        if let Some(node) = derived_map.get_mut(derived.id) {
            // Unsubscribe from old dependencies
            for &dep_id in &node.dependencies {
                if let Some(sig) = self.signals.borrow_mut().get_mut(dep_id) {
                    sig.subscribers
                        .retain(|s| *s != SubscriberId::Derived(derived.id));
                }
            }

            // Subscribe to new dependencies
            for &dep_id in &deps {
                if let Some(sig) = self.signals.borrow_mut().get_mut(dep_id) {
                    let sub = SubscriberId::Derived(derived.id);
                    if !sig.subscribers.contains(&sub) {
                        sig.subscribers.push(sub);
                    }
                }
            }

            // Update depth based on dependencies
            let max_dep_depth = {
                let signals = self.signals.borrow();
                deps.iter()
                    .filter_map(|&id| signals.get(id))
                    .map(|_| 0u32) // Signals have depth 0
                    .max()
                    .unwrap_or(0)
            };

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
            run: Some(Box::new(run)),
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
        self.due_host_effects.retain(|id| *id != effect.id);
        if let Some(node) = self.effects.remove(effect.id) {
            // Unsubscribe from all dependencies
            for &dep_id in &node.dependencies {
                if let Some(sig) = self.signals.borrow_mut().get_mut(dep_id) {
                    sig.subscribers
                        .retain(|s| *s != SubscriberId::Effect(effect.id));
                }
            }
        }
    }

    // ---------------------------------------------------------------------
    // Host-run effects
    //
    // An effect whose body is code in another language. The graph holds no
    // closure for it: it knows only the effect's id, its dependencies and
    // whether it is due. The host asks which are due, runs each between
    // `begin_effect` and `end_effect`, and the graph tracks what it read.
    // ---------------------------------------------------------------------

    /// Create an effect with no Rust body. It starts due, like any effect.
    /// [`Self::flush_effects`] never runs it: it is handed out by
    /// [`Self::take_due_host_effects`] instead.
    pub fn create_host_effect(&mut self) -> Effect {
        let id = self.effects.insert(EffectNode {
            run: None,
            dependencies: SmallVec::new(),
            dirty: Cell::new(true),
            depth: 0,
        });
        self.pending_effects.borrow_mut().push_back(id);
        if self.batch_depth.get() == 0 {
            self.flush_effects();
        }
        Effect { id }
    }

    /// The host effects that are due, in the order they became due (the
    /// order `run_effect` would have used), clearing the list. Closure
    /// effects are not returned: they run as they always did.
    ///
    /// Every id returned should be run between [`Self::begin_effect`] and
    /// [`Self::end_effect`]. Until it is, the effect stays dirty and is not
    /// handed out again.
    ///
    /// Not called from inside the graph's own flush: flush runs under the
    /// graph lock and the host must not be called there.
    pub fn take_due_host_effects(&mut self) -> Vec<EffectId> {
        if self.batch_depth.get() == 0 {
            self.flush_effects();
        }
        let mut due = std::mem::take(&mut self.due_host_effects);
        // One that was disposed, or already run, is no longer due.
        due.retain(|id| self.effects.get(*id).is_some_and(|n| n.dirty.get()));
        due
    }

    /// Open a tracking scope for a host effect.
    ///
    /// Until the matching [`Self::end_effect`], every signal read through the
    /// graph is recorded as a dependency of `effect`, and writes made through
    /// the `Signal` API wait, as inside a closure effect. Unlike a closure
    /// effect the scope holds no lock and sets no in-flight pointer, so the
    /// host is free to call back into the graph, which takes the ordinary
    /// locked path.
    ///
    /// Scopes nest: the tracking that was active is saved and put back by
    /// `end_effect`, so a host effect can run while another scope is open.
    /// False if `effect` does not exist or is not a host effect.
    ///
    /// The scope belongs to the thread that began it; begin and end on the
    /// same one.
    pub fn begin_effect(&mut self, effect: EffectId) -> bool {
        let Some(node) = self.effects.get(effect) else {
            return false;
        };
        if node.run.is_some() {
            return false;
        }
        // Clean from here, as in `run_effect`: a change made while it runs
        // must queue it again.
        node.dirty.set(false);
        let outer_tracking = self.tracking.replace(Some(Vec::new()));
        self.effect_scopes.push(EffectScope {
            effect,
            outer_tracking,
            writes_from: self.scope_writes.len(),
            thread: std::thread::current().id(),
        });
        HOST_EFFECT_DEPTH.with(|d| d.set(d.get() + 1));
        true
    }

    /// Close the scope `begin_effect` opened for `effect`: record what it
    /// read as its dependencies and restore the tracking that was active
    /// before.
    ///
    /// Safe to call when the host went wrong between begin and end. A scope
    /// above `effect` that was never ended is abandoned (its reads are
    /// discarded and the tracking beneath it restored), and calling it for
    /// an effect with no open scope, or twice, does nothing.
    ///
    /// Writes made through the `Signal` API during the scope are still
    /// waiting; the caller drains them once the graph is unlocked, which
    /// [`end_host_effect`] does for the global graph.
    pub fn end_effect(&mut self, effect: EffectId) {
        let Some(at) = self.effect_scopes.iter().rposition(|s| s.effect == effect) else {
            return;
        };
        let abandoned = self.effect_scopes.len() - at - 1;

        // Scopes above this one were never ended: drop them, putting each
        // one's outer tracking back as we go.
        for _ in 0..abandoned {
            if let Some(scope) = self.effect_scopes.pop() {
                self.tracking.replace(scope.outer_tracking);
            }
        }
        let scope = self.effect_scopes.pop().expect("found above");
        debug_assert_eq!(
            scope.thread,
            std::thread::current().id(),
            "end_effect on a different thread from begin_effect"
        );
        let deps = self
            .tracking
            .replace(scope.outer_tracking)
            .unwrap_or_default();
        HOST_EFFECT_DEPTH.with(|d| d.set(d.get().saturating_sub(abandoned as u32 + 1)));

        // A signal this effect read was written while it ran, before it was
        // subscribed to it: the change would otherwise be lost.
        let changed = self.scope_writes[scope.writes_from.min(self.scope_writes.len())..]
            .iter()
            .any(|written| deps.contains(written));
        if self.effect_scopes.is_empty() {
            self.scope_writes.clear();
        }

        self.resubscribe_effect(effect, deps);
        if changed {
            self.mark_dirty(SubscriberId::Effect(effect));
            if self.batch_depth.get() == 0 {
                self.flush_effects();
            }
        }
    }

    /// Replace an effect's subscriptions with `deps`.
    fn resubscribe_effect(&mut self, effect_id: EffectId, deps: Vec<SignalId>) {
        let Some(node) = self.effects.get_mut(effect_id) else {
            return;
        };
        // Unsubscribe from old dependencies
        for &dep_id in &node.dependencies {
            if let Some(sig) = self.signals.borrow_mut().get_mut(dep_id) {
                sig.subscribers
                    .retain(|s| *s != SubscriberId::Effect(effect_id));
            }
        }

        // Subscribe to new dependencies
        for &dep_id in &deps {
            if let Some(sig) = self.signals.borrow_mut().get_mut(dep_id) {
                let sub = SubscriberId::Effect(effect_id);
                if !sig.subscribers.contains(&sub) {
                    sig.subscribers.push(sub);
                }
            }
        }

        node.dependencies = deps.into_iter().collect();
    }

    /// Remove a signal. Deriveds and effects that read it stop depending
    /// on it, and every handle to it reads `None` from now on: slot keys
    /// are versioned, so a later signal in the same slot is not aliased.
    /// False if it was already gone.
    pub fn dispose_signal(&mut self, id: SignalId) -> bool {
        let Some(node) = self.signals.borrow_mut().remove(id) else {
            return false;
        };
        for sub in node.subscribers {
            match sub {
                SubscriberId::Derived(d) => {
                    if let Some(derived) = self.derived.borrow_mut().get_mut(d) {
                        derived.dependencies.retain(|s| *s != id);
                    }
                }
                SubscriberId::Effect(e) => {
                    if let Some(effect) = self.effects.get_mut(e) {
                        effect.dependencies.retain(|s| *s != id);
                    }
                }
            }
        }
        if let Some(tracking) = self.tracking.borrow_mut().as_mut() {
            tracking.retain(|s| *s != id);
        }
        true
    }

    /// Remove a derived, unsubscribing it from the signals it read. Its
    /// compute closure is dropped with it. False if it was already gone.
    pub fn dispose_derived(&mut self, id: DerivedId) -> bool {
        let Some(node) = self.derived.borrow_mut().remove(id) else {
            return false;
        };
        for &dep_id in &node.dependencies {
            if let Some(sig) = self.signals.borrow_mut().get_mut(dep_id) {
                sig.subscribers.retain(|s| *s != SubscriberId::Derived(id));
            }
        }
        self.derived_dirty_buffer.borrow_mut().retain(|d| *d != id);
        true
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
                // Flip and collect under a borrow that ends before the
                // recursion, which borrows this map again.
                let subscribers: SmallVec<[SubscriberId; 4]> = {
                    let map = self.derived.borrow();
                    let Some(node) = map.get(id) else {
                        return;
                    };
                    if node.dirty.get() {
                        return;
                    }
                    node.dirty.set(true);
                    node.subscribers.clone()
                };
                // Record for the per-set property-binding fire (drained
                // at the end of `set`). Each derived can only flip once
                // per set, since we returned above if already dirty, so
                // no dedup is needed.
                self.derived_dirty_buffer.borrow_mut().push(id);
                for sub in subscribers {
                    self.mark_dirty(sub);
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
            // A host effect has no body to run: it becomes due.
            if self.effects.get(effect_id).is_some_and(|n| n.run.is_none()) {
                if self.effects.get(effect_id).is_some_and(|n| n.dirty.get())
                    && !self.due_host_effects.contains(&effect_id)
                {
                    self.due_host_effects.push(effect_id);
                }
                continue;
            }
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

        // Get the run function - we need to be careful with mutability
        // For now, we'll use a simple approach that requires unsafe
        let run_ptr: *mut Box<dyn FnMut(&ReactiveGraph) + Send> = {
            match self.effects.get_mut(effect_id) {
                Some(node) => match node.run.as_mut() {
                    Some(run) => {
                        node.dirty.set(false);
                        run as *mut _
                    }
                    // A host effect: the host runs it.
                    None => return,
                },
                None => return,
            }
        };

        // Start tracking dependencies. Whatever was being tracked (a host
        // effect scope this was called from) is put back afterwards.
        let outer_tracking = self.tracking.replace(Some(Vec::new()));

        // Set the thread-local in-flight graph pointer so `Signal<T>::get`
        // / `set` / `update` calls inside the effect closure take the
        // re-entrant fast path (read directly from this graph, queue
        // writes for post-flush draining) instead of re-acquiring the
        // global mutex. Cleared in a guard's Drop so a panic inside the
        // closure can't leak the pointer to subsequent code.
        struct InFlightGuard(*const ReactiveGraph);
        impl Drop for InFlightGuard {
            fn drop(&mut self) {
                IN_FLIGHT_GRAPH.with(|c| c.set(self.0));
            }
        }
        let prev_in_flight = IN_FLIGHT_GRAPH.with(|c| c.get());
        IN_FLIGHT_GRAPH.with(|c| c.set(self as *const _));
        let _guard = InFlightGuard(prev_in_flight);

        // SAFETY: We're not modifying the effect while running it
        // (though the effect can modify signals, which is fine)
        unsafe {
            (*run_ptr)(self);
        }
        drop(_guard);

        // Get tracked dependencies
        let deps = self.tracking.replace(outer_tracking).unwrap_or_default();

        self.resubscribe_effect(effect_id, deps);
    }

    /// Get statistics about the reactive graph
    pub fn stats(&self) -> ReactiveStats {
        ReactiveStats {
            signal_count: self.signals.borrow().len(),
            derived_count: self.derived.borrow().len(),
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

thread_local! {
    /// Nesting depth of [`batch_stateful_deps`], and the ids collected
    /// while inside it.
    static DEPS_BATCH: std::cell::RefCell<(usize, Vec<SignalId>)> =
        const { std::cell::RefCell::new((0, Vec::new())) };
}

/// Coalesce every stateful-deps notification `f` produces into one,
/// fired after `f` returns.
///
/// A `Stateful` refresh does not queue a marker — it re-runs the
/// callback and queues the element it BUILT. So a notification part-way
/// through a multi-write action bakes the values written so far and
/// misses the rest. An FSM transition setting `busy` then `caption`
/// rebuilt the view on the `busy` write, capturing the old `caption`;
/// the `caption` write then queued its own rebuild, which was dropped
/// because the first rebuild had already replaced those nodes. The
/// frame painted new `busy` with stale `caption`, and only the next
/// interaction squared it up.
///
/// Re-entrant: only the outermost call flushes.
pub fn batch_stateful_deps<R>(f: impl FnOnce() -> R) -> R {
    DEPS_BATCH.with(|b| b.borrow_mut().0 += 1);
    let result = f();
    let flush = DEPS_BATCH.with(|b| {
        let mut guard = b.borrow_mut();
        guard.0 -= 1;
        if guard.0 == 0 {
            Some(std::mem::take(&mut guard.1))
        } else {
            None
        }
    });
    if let Some(mut ids) = flush
        && !ids.is_empty()
    {
        ids.dedup();
        if let Some(notifier) = STATEFUL_DEPS_NOTIFIER.get() {
            notifier(&ids);
        }
    }
    result
}

/// Fire the stateful-deps notifier. No-op if none installed.
///
/// Inside [`batch_stateful_deps`] the ids are collected instead, so a
/// multi-write action notifies once with all of them.
pub(crate) fn notify_stateful_deps(ids: &[SignalId]) {
    let batched = DEPS_BATCH.with(|b| {
        let mut guard = b.borrow_mut();
        if guard.0 > 0 {
            guard.1.extend_from_slice(ids);
            true
        } else {
            false
        }
    });
    if batched {
        return;
    }
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

/// Open a tracking scope for a host effect on the process-global graph.
///
/// The global-graph form of [`ReactiveGraph::begin_effect`]. The graph lock
/// is held only for this call, so the host can run its effect body, reading
/// and writing signals, before calling [`end_host_effect`].
pub fn begin_host_effect(effect: EffectId) -> bool {
    let graph = global_graph();
    let mut g = graph.lock().expect("reactive graph poisoned");
    g.begin_effect(effect)
}

/// Close a host effect's scope on the process-global graph, then run the
/// writes that waited while it was open.
///
/// The global-graph form of [`ReactiveGraph::end_effect`], with the one
/// thing that cannot be done under the lock: the writes the host made during
/// the scope were deferred, and applying them takes the graph lock again, so
/// they are drained after it is released. Only the outermost scope drains.
/// Call this on every path out of the effect body, including after the host
/// throws.
pub fn end_host_effect(effect: EffectId) {
    {
        let graph = global_graph();
        let mut g = graph.lock().expect("reactive graph poisoned");
        g.end_effect(effect);
    }
    if !defers_writes() {
        drain_deferred_writes();
    }
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
    /// `read`, via the private `read_adapter` field.
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
    /// through `read`, via the private `read_adapter` field.
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
        // locking again would deadlock against itself.
        if let Some(value) = with_in_flight_graph(|g| g.read_derived_in_flight(self.derived)) {
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
    // Inside a derived or effect closure the graph mutex is already held
    // by this thread, so go through the in-flight graph the way reads do.
    // The signal lands in the live graph, so a read later in the same
    // evaluation sees it and records it as a dependency.
    //
    // Checked before moving `initial`, since the closure would consume it.
    if is_in_flush() {
        if let Some(s) = with_in_flight_graph(|g| g.create_signal(initial)) {
            return s;
        }
        unreachable!("is_in_flush() was true, so the pointer is non-null");
    }
    let graph = global_graph();
    let g = graph.lock().unwrap();
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
    // Inside a derived or effect closure the mutex is already held by this
    // thread, so go through the in-flight graph as reads do. Checked
    // before the move, since the closure would consume `compute`.
    if is_in_flush() {
        if let Some(derived) = with_in_flight_graph(|g| g.create_derived(compute)) {
            return Computed::new(derived, graph);
        }
        unreachable!("is_in_flush() was true, so the pointer is non-null");
    }
    let derived = {
        let g = graph.lock().unwrap();
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

/// A node removed from the global graph, as told to the dispose notifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposed {
    Signal(SignalId),
    Derived(DerivedId),
}

/// Global notifier for disposals, so `blinc_layout` can drop the property
/// bindings of a node that no longer exists. Same lifecycle as
/// [`PROPERTY_BINDING_NOTIFIER`]: installed once, with the binding registry.
static DISPOSE_NOTIFIER: std::sync::OnceLock<Box<dyn Fn(Disposed) + Send + Sync + 'static>> =
    std::sync::OnceLock::new();

/// Install the global dispose notifier. Idempotent: only the first call wins.
pub fn set_dispose_notifier(notifier: impl Fn(Disposed) + Send + Sync + 'static) {
    let _ = DISPOSE_NOTIFIER.set(Box::new(notifier));
}

/// Remove a signal from the global graph, together with its property
/// bindings.
///
/// Inside an effect or a computation the graph is already locked and the
/// running closure may belong to what is being removed, so the removal is
/// deferred like a write made there: it runs after the next write on this
/// thread completes.
pub fn dispose_signal(id: SignalId) {
    dispose(Disposed::Signal(id));
}

/// Remove a derived from the global graph, together with its property
/// bindings. Deferred inside an effect or a computation, as
/// [`dispose_signal`] is.
pub fn dispose_derived(id: DerivedId) {
    dispose(Disposed::Derived(id));
}

fn dispose(node: Disposed) {
    if is_in_flush() {
        DEFERRED_WRITES.with(|q| q.borrow_mut().push(Box::new(move || dispose(node))));
        return;
    }
    let removed = {
        let graph = global_graph();
        let mut g = graph.lock().unwrap();
        match node {
            Disposed::Signal(id) => g.dispose_signal(id),
            Disposed::Derived(id) => g.dispose_derived(id),
        }
    };
    // Told after the graph lock is released: the binding registry is
    // locked before the graph on the notify path.
    if removed {
        if let Some(notifier) = DISPOSE_NOTIFIER.get() {
            notifier(node);
        }
    }
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
    fn a_computed_reading_a_computed_follows_its_signals() {
        let base = signal(1i32);
        let doubled = computed::<i32, _>(move |_| base.try_get().unwrap_or(0) * 2);
        let inner = doubled.clone();
        let quadrupled = computed::<i32, _>(move |_| inner.try_get().unwrap_or(0) * 2);
        assert_eq!(quadrupled.try_get(), Some(4));

        base.set(3);
        assert_eq!(quadrupled.try_get(), Some(12));
        assert_eq!(
            doubled.try_get(),
            Some(6),
            "the inner one still recomputes on its own read"
        );
    }

    #[test]
    fn an_effect_reading_a_computed_reruns_when_its_signals_change() {
        let base = signal(1i32);
        let doubled = computed::<i32, _>(move |_| base.try_get().unwrap_or(0) * 2);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        let inner = doubled.clone();
        let _watch = effect(move |_| log.lock().unwrap().push(inner.try_get().unwrap_or(-1)));

        base.set(5);
        assert_eq!(*seen.lock().unwrap(), vec![2, 10]);
    }

    #[test]
    fn disposed_signal_reads_none_and_leaves_its_readers() {
        let mut graph = ReactiveGraph::new();
        let a = graph.create_signal(2i32);
        let b = graph.create_signal(3i32);
        let sum = graph.create_derived(move |g| g.get(a).unwrap_or(0) + g.get(b).unwrap_or(0));
        assert_eq!(graph.get_derived(sum), Some(5));

        assert!(graph.dispose_signal(a.id()));
        assert!(!graph.dispose_signal(a.id()), "already gone");
        assert_eq!(graph.get(a), None);
        assert_eq!(
            graph.derived.borrow()[sum.id].dependencies.as_slice(),
            &[b.id()],
            "the derived no longer depends on the disposed signal"
        );

        // A new signal may reuse the slot; the old handle must not see it.
        let c = graph.create_signal(9i32);
        assert_eq!(graph.get(a), None);
        assert_eq!(graph.get(c), Some(9));
    }

    #[test]
    fn disposed_derived_stops_being_dirtied() {
        let mut graph = ReactiveGraph::new();
        let a = graph.create_signal(1i32);
        let doubled = graph.create_derived(move |g| g.get(a).unwrap_or(0) * 2);
        assert_eq!(graph.get_derived(doubled), Some(2));

        assert!(graph.dispose_derived(doubled.id));
        assert!(!graph.dispose_derived(doubled.id), "already gone");
        assert!(graph.signals.borrow()[a.id()].subscribers.is_empty());

        graph.set(a, 5);
        assert!(graph.take_dirty_derived().is_empty());
        assert_eq!(graph.get_derived(doubled), None);
    }

    #[test]
    fn disposing_inside_a_computation_waits_for_the_next_write() {
        let trigger = signal(0i32);
        let victim = signal(7i32);
        let victim_id = victim.id();
        let disposer = computed::<i32, _>(move |_| {
            if trigger.try_get().unwrap_or(0) > 0 {
                dispose_signal(victim_id);
            }
            0
        });
        assert_eq!(disposer.try_get(), Some(0));

        trigger.set(1);
        // Runs the compute closure, which defers the dispose instead of
        // re-locking the graph it runs under.
        assert_eq!(disposer.try_get(), Some(0));
        assert_eq!(victim.try_get(), Some(7), "not yet: deferred");

        trigger.set(2);
        assert_eq!(victim.try_get(), None, "applied after the next write");
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

#[cfg(test)]
mod in_flight_creation_tests {
    use super::*;

    /// Creating a signal inside a computed used to deadlock: `signal()`
    /// took the graph mutex that the evaluation already held.
    ///
    /// Every test here would HANG rather than fail before the fix, so run
    /// them with a timeout if you are bisecting.
    #[test]
    fn a_signal_can_be_created_inside_a_computed() {
        let trigger = signal(1_i32);
        let c = computed(move |g| {
            let made = signal(41_i32);
            g.get(trigger).unwrap_or(0) + g.get_untracked(made).unwrap_or(0)
        });
        assert_eq!(c.try_get(), Some(42));
    }

    /// The new signal must be readable in the SAME evaluation, not just
    /// reserved for later. Reserving a slot and inserting on return would
    /// pass the test above and fail this one.
    #[test]
    fn the_new_signal_is_readable_immediately() {
        let c = computed(move |g| {
            let made = signal(7_i32);
            // Read it back through the same in-flight graph.
            g.get(made).unwrap_or(-1)
        });
        assert_eq!(c.try_get(), Some(7));
    }

    /// Writing to a freshly created signal inside the evaluation works
    /// too: the write path already defers while in flight.
    #[test]
    fn a_fresh_signal_accepts_a_write_in_flight() {
        let c = computed(move |g| {
            let made = signal(1_i32);
            let first = g.get_untracked(made).unwrap_or(0);
            made.set(2);
            first
        });
        assert_eq!(c.try_get(), Some(1));
    }

    /// Nested evaluation: a computed read inside another computed, with a
    /// signal created at the inner level. Exercises the in-flight pointer
    /// save/restore as well as creation.
    #[test]
    fn creation_survives_a_nested_evaluation() {
        let base = signal(10_i32);
        let inner = computed(move |g| {
            let extra = signal(5_i32);
            g.get(base).unwrap_or(0) + g.get_untracked(extra).unwrap_or(0)
        });
        let outer = computed(move |_g| inner.try_get().unwrap_or(0) * 2);
        assert_eq!(outer.try_get(), Some(30));
    }

    /// A computed can be created inside another computed's evaluation.
    ///
    /// This is the half that needed `with_compute`: the evaluation used to
    /// hold a raw pointer into the derived map across the closure, so an
    /// insert could have reallocated under it. A deadlock would have been
    /// the lucky outcome.
    #[test]
    fn a_computed_can_be_created_inside_a_computed() {
        let base = signal(6_i32);
        let outer = computed(move |g| {
            let inner = computed(move |g2| g2.get(base).unwrap_or(0) * 7);
            inner.try_get().unwrap_or(0) + g.get(base).unwrap_or(0)
        });
        assert_eq!(outer.try_get(), Some(48));
    }

    /// A derived mid-evaluation has its closure taken out, so a nested
    /// read of the SAME derived answers `None` instead of recursing.
    ///
    /// A consequence of `with_compute`, and the reason it returns
    /// `Option`: worth pinning so it is not mistaken for a bug later.
    #[test]
    fn a_derived_being_evaluated_reports_no_compute() {
        let g = ReactiveGraph::new();
        let a = g.create_signal(2_i32);
        let d = g.create_derived(move |gg| gg.get(a).unwrap_or(0));
        // Outer take succeeds; the inner one sees it already taken.
        let inner = g.with_compute(d.id, |_| g.with_compute(d.id, |_| ()));
        assert_eq!(
            inner,
            Some(None),
            "a nested evaluation of the same derived should find no closure"
        );
        // And it is put back afterwards.
        assert!(
            g.with_compute(d.id, |_| ()).is_some(),
            "the closure was not restored"
        );
    }

    /// Creation must not disturb dependency tracking: the computed still
    /// re-fires when its real dependency changes.
    #[test]
    fn creating_a_signal_does_not_break_tracking() {
        let dep = signal(1_i32);
        let c = computed(move |g| {
            let _scratch = signal(0_i32);
            g.get(dep).unwrap_or(0) * 10
        });
        assert_eq!(c.try_get(), Some(10));
        dep.set(3);
        assert_eq!(c.try_get(), Some(30), "the computed did not re-fire");
    }

    /// Host-run effects: effects whose body is code in another language.
    mod host_effects {
        use super::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        fn due(g: &mut ReactiveGraph) -> Vec<EffectId> {
            g.take_due_host_effects()
        }

        /// Run an effect body that reads `reads`, between begin and end.
        fn run(g: &mut ReactiveGraph, effect: EffectId, reads: &[Signal<i32>]) {
            assert!(g.begin_effect(effect));
            for sig in reads {
                let _ = g.get(*sig);
            }
            g.end_effect(effect);
        }

        #[test]
        fn a_new_host_effect_is_due_once() {
            let mut g = ReactiveGraph::new();
            let e = g.create_host_effect();
            assert_eq!(due(&mut g), vec![e.id()]);
            assert!(due(&mut g).is_empty(), "it was handed out twice");
        }

        #[test]
        fn it_is_due_again_when_something_it_read_changes() {
            let mut g = ReactiveGraph::new();
            let a = g.create_signal(1);
            let e = g.create_host_effect();
            due(&mut g);
            run(&mut g, e.id(), &[a]);
            assert!(due(&mut g).is_empty(), "due with nothing changed");

            g.set(a, 2);
            assert_eq!(due(&mut g), vec![e.id()]);
        }

        #[test]
        fn a_write_to_something_it_did_not_read_leaves_it_alone() {
            let mut g = ReactiveGraph::new();
            let (a, b) = (g.create_signal(1), g.create_signal(1));
            let e = g.create_host_effect();
            due(&mut g);
            run(&mut g, e.id(), &[a]);

            g.set(b, 2);
            assert!(due(&mut g).is_empty());
        }

        #[test]
        fn what_it_reads_is_replaced_on_every_run() {
            let mut g = ReactiveGraph::new();
            let (a, b) = (g.create_signal(1), g.create_signal(1));
            let e = g.create_host_effect();
            due(&mut g);
            run(&mut g, e.id(), &[a]);
            run(&mut g, e.id(), &[b]);

            g.set(a, 2);
            assert!(
                due(&mut g).is_empty(),
                "still subscribed to what it stopped reading"
            );
            g.set(b, 2);
            assert_eq!(due(&mut g), vec![e.id()]);
        }

        #[test]
        fn closure_effects_run_in_the_same_flush_and_host_ones_are_handed_out() {
            let mut g = ReactiveGraph::new();
            let a = g.create_signal(1);
            let hits = Arc::new(AtomicUsize::new(0));
            let counted = Arc::clone(&hits);
            g.create_effect(move |g| {
                let _ = g.get(a);
                counted.fetch_add(1, Ordering::SeqCst);
            });
            let e = g.create_host_effect();
            due(&mut g);
            run(&mut g, e.id(), &[a]);
            let before = hits.load(Ordering::SeqCst);

            g.set(a, 2);

            assert_eq!(
                hits.load(Ordering::SeqCst),
                before + 1,
                "the closure effect did not run"
            );
            assert_eq!(due(&mut g), vec![e.id()]);
        }

        #[test]
        fn host_effects_are_handed_out_in_the_order_they_became_due() {
            let mut g = ReactiveGraph::new();
            let a = g.create_signal(1);
            let first = g.create_host_effect();
            let second = g.create_host_effect();
            let third = g.create_host_effect();
            due(&mut g);
            // Subscribe in a different order from creation.
            run(&mut g, third.id(), &[a]);
            run(&mut g, first.id(), &[a]);
            run(&mut g, second.id(), &[a]);

            g.set(a, 2);
            assert_eq!(due(&mut g).len(), 3);
        }

        #[test]
        fn a_write_made_while_it_runs_is_not_lost() {
            let mut g = ReactiveGraph::new();
            let a = g.create_signal(1);
            let e = g.create_host_effect();
            due(&mut g);

            assert!(g.begin_effect(e.id()));
            let _ = g.get(a);
            // Written before the effect is subscribed to it.
            g.set(a, 2);
            g.end_effect(e.id());

            assert_eq!(
                due(&mut g),
                vec![e.id()],
                "the change made during the run was lost"
            );
        }

        #[test]
        fn scopes_nest_and_each_keeps_its_own_reads() {
            let mut g = ReactiveGraph::new();
            let (x, y, z) = (g.create_signal(0), g.create_signal(0), g.create_signal(0));
            let (outer, inner) = (g.create_host_effect(), g.create_host_effect());
            due(&mut g);

            assert!(g.begin_effect(outer.id()));
            let _ = g.get(x);
            assert!(g.begin_effect(inner.id()));
            let _ = g.get(y);
            g.end_effect(inner.id());
            let _ = g.get(z);
            g.end_effect(outer.id());

            g.set(y, 1);
            assert_eq!(
                due(&mut g),
                vec![inner.id()],
                "the inner read leaked into the outer"
            );
            run(&mut g, inner.id(), &[y]);
            g.set(x, 1);
            g.set(z, 1);
            let mut now = due(&mut g);
            now.dedup();
            assert_eq!(now, vec![outer.id()]);
        }

        #[test]
        fn an_inner_scope_that_never_ended_does_not_wedge_the_outer() {
            let mut g = ReactiveGraph::new();
            let (x, p) = (g.create_signal(0), g.create_signal(0));
            let (outer, inner) = (g.create_host_effect(), g.create_host_effect());
            due(&mut g);

            assert!(g.begin_effect(outer.id()));
            let _ = g.get(x);
            assert!(g.begin_effect(inner.id()));
            let _ = g.get(p);
            // The host threw inside the inner body and only the outer ends.
            g.end_effect(outer.id());

            assert!(!defers_writes(), "a scope was left open");
            assert!(g.tracking.borrow().is_none(), "tracking was left on");
            g.set(x, 1);
            assert_eq!(due(&mut g), vec![outer.id()]);
            g.set(p, 1);
            assert!(
                due(&mut g).is_empty(),
                "the abandoned scope's reads were recorded"
            );
            // And the graph is still usable.
            run(&mut g, inner.id(), &[p]);
        }

        #[test]
        fn ending_twice_or_a_scope_that_is_not_open_does_nothing() {
            let mut g = ReactiveGraph::new();
            let a = g.create_signal(0);
            let e = g.create_host_effect();
            due(&mut g);
            run(&mut g, e.id(), &[a]);

            g.end_effect(e.id());
            g.end_effect(e.id());

            assert!(!defers_writes());
            g.set(a, 1);
            assert_eq!(
                due(&mut g),
                vec![e.id()],
                "a stray end disturbed the subscriptions"
            );
        }

        #[test]
        fn a_closure_effect_run_inside_a_scope_does_not_take_the_hosts_reads() {
            let mut g = ReactiveGraph::new();
            let (x, w, y) = (g.create_signal(0), g.create_signal(0), g.create_signal(0));
            let e = g.create_host_effect();
            due(&mut g);

            assert!(g.begin_effect(e.id()));
            let _ = g.get(x);
            // Created inside the scope, so it runs at once.
            g.create_effect(move |g| {
                let _ = g.get(w);
            });
            let _ = g.get(y);
            g.end_effect(e.id());

            g.set(w, 1);
            assert!(
                due(&mut g).is_empty(),
                "the closure effect's read became the host's"
            );
            g.set(x, 1);
            assert_eq!(
                due(&mut g),
                vec![e.id()],
                "a read before the closure effect was lost"
            );
            run(&mut g, e.id(), &[x, y]);
            g.set(y, 1);
            assert_eq!(
                due(&mut g),
                vec![e.id()],
                "a read after the closure effect was lost"
            );
        }

        #[test]
        fn a_derived_recomputed_inside_a_scope_keeps_the_hosts_reads() {
            let mut g = ReactiveGraph::new();
            let (x, s) = (g.create_signal(1), g.create_signal(1));
            let d = g.create_derived(move |g| g.get(s).unwrap_or(0) * 2);
            let e = g.create_host_effect();
            due(&mut g);

            assert!(g.begin_effect(e.id()));
            let _ = g.get(x);
            assert_eq!(g.get_derived(d), Some(2));
            g.end_effect(e.id());

            g.set(x, 2);
            assert_eq!(
                due(&mut g),
                vec![e.id()],
                "the read before the derived was lost"
            );
            run(&mut g, e.id(), &[x]);
            let _ = {
                assert!(g.begin_effect(e.id()));
                let v = g.get_derived(d);
                g.end_effect(e.id());
                v
            };
            g.set(s, 5);
            assert_eq!(
                due(&mut g),
                vec![e.id()],
                "the host did not come to depend on the derived's signals"
            );
        }

        #[test]
        fn disposing_an_effect_while_it_runs_is_safe() {
            let mut g = ReactiveGraph::new();
            let a = g.create_signal(0);
            let e = g.create_host_effect();
            due(&mut g);

            assert!(g.begin_effect(e.id()));
            let _ = g.get(a);
            g.dispose_effect(e);
            g.end_effect(e.id());

            assert!(!defers_writes());
            assert!(g.tracking.borrow().is_none());
            assert!(due(&mut g).is_empty());
        }

        #[test]
        fn a_disposed_effect_that_was_due_is_not_handed_out() {
            let mut g = ReactiveGraph::new();
            let e = g.create_host_effect();
            g.dispose_effect(e);
            assert!(due(&mut g).is_empty());
        }

        #[test]
        fn a_closure_effect_cannot_be_begun_as_a_host_effect() {
            let mut g = ReactiveGraph::new();
            let closure = g.create_effect(|_| {});
            assert!(!g.begin_effect(closure.id()));
            assert!(!defers_writes());
        }

        /// A scope ended on another thread than it began on would corrupt
        /// the per-thread tracking, so debug builds say so.
        #[cfg(debug_assertions)]
        #[test]
        fn ending_a_scope_on_another_thread_is_caught_in_debug_builds() {
            let mut g = ReactiveGraph::new();
            let e = g.create_host_effect();
            g.take_due_host_effects();
            assert!(g.begin_effect(e.id()));

            let ended = std::thread::spawn(move || g.end_effect(e.id())).join();
            assert!(ended.is_err(), "a cross-thread end went unnoticed");
        }

        /// The global-graph forms: `Signal` writes made inside the scope
        /// wait, and apply once it ends.
        #[test]
        fn signal_writes_wait_inside_a_scope_and_apply_after_it() {
            let sig = signal(0_i32);
            let effect = global_graph().lock().unwrap().create_host_effect();
            global_graph().lock().unwrap().take_due_host_effects();

            assert!(begin_host_effect(effect.id()));
            let _ = sig.get();
            sig.set(5);
            assert_eq!(sig.get(), 0, "the write was not deferred");
            end_host_effect(effect.id());

            assert_eq!(sig.get(), 5, "the write never applied");
            assert!(!defers_writes());
            // And the write made after the read woke the effect.
            assert_eq!(
                global_graph().lock().unwrap().take_due_host_effects(),
                vec![effect.id()]
            );
            global_graph().lock().unwrap().dispose_effect(effect);
        }
    }
}
