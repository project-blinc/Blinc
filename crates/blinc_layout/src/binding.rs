//! Signal-bound property binding registry — Phase 2 of the unified
//! property channel ([[project-reactive-architecture-v2]]).
//!
//! This is the substrate for `.bg(&state)`-style call sites: when a
//! signal value changes, the registry walks subscribers and queues
//! partial property updates via the same channel CSS animations,
//! transitions, and stateful refreshes already use ([`PropertyId`] +
//! [`SideEffects`] + the closure-form `queue_prop_update_partial`).
//!
//! # Architecture
//!
//! The registry lives in `blinc_layout` because it needs `LayoutNodeId`
//! and the queue API; `blinc_core` exposes a [global notifier hook][1]
//! that fires on every `State<T>::set`. The registry installs itself as
//! that notifier on first access — no init order coupling for platform
//! runners.
//!
//! [1]: blinc_core::reactive::set_property_binding_notifier
//!
//! # Lifecycle
//!
//! - **Register**: `Div::build` calls [`PropertyBindingRegistry::register`]
//!   for every reactive modifier on the element. The registration carries
//!   the target `LayoutNodeId`, the `PropertyId`, the `State<T>` (cloned)
//!   for current-value reads, and a closure that knows how to write the
//!   value into the right `RenderProps` field.
//! - **Fire**: `State<T>::set` triggers the global notifier → registry
//!   walks subscribers for that signal id → each subscriber reads the
//!   latest value and pushes a [`PartialPropertyUpdate`][crate::stateful::PartialPropertyUpdate]
//!   onto the unified queue. Platform runners drain the queue next frame.
//! - **Cleanup**: `remove_subtree_nodes` calls
//!   [`PropertyBindingRegistry::unregister_node`] for every removed
//!   layout id. Bindings for nodes that no longer exist are evicted, so
//!   stale subscribers can't fire on the next signal change.
//!
//! # API (this is the foundation — builder integration lands in P2.2)
//!
//! Phase 2.1 ships the registry + `IntoReactive` trait. Phase 2.2 wires
//! the trait into `Div` builder methods so `.bg(&state)` actually
//! registers. Phase 2.3+ rolls it across visual / layout properties.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use blinc_core::reactive::{Computed, DerivedId, Signal, SignalId, State};

use crate::element::RenderProps;
use crate::property::{PropertyId, SideEffects};
use crate::stateful::queue_prop_update_partial;
use crate::tree::LayoutNodeId;

/// Reads the current value out of a signal's reactive graph and packages
/// it in a [`BoundValue`]. Re-invoked on every fire (not cached).
pub type ReadFn = Arc<dyn Fn() -> Option<BoundValue> + Send + Sync>;

/// Writes a [`BoundValue`] into the right `RenderProps` field. Built
/// once at `Bound` construction; reused on every fire.
pub type WriteFn = Arc<dyn Fn(&mut RenderProps, &BoundValue) + Send + Sync>;

/// Writes a [`BoundValue`] into the right taffy `Style` field. Mirror
/// of [`WriteFn`] for layout-affecting bindings.
pub type LayoutWriteFn = Arc<dyn Fn(&mut taffy::Style, &BoundValue) + Send + Sync>;

/// A reactive value source — what an [`IntoReactive<T>`] impl resolves to.
///
/// Builder methods examine the variant: `Const` becomes an immediate
/// `RenderProps` write at build time; `Bound` keeps a cheap `State<T>`
/// clone that the builder uses to (a) read the initial value and (b)
/// register a subscription against the minted `LayoutNodeId`.
pub enum Reactive<T> {
    /// Eager value — no subscription, written directly into the element
    /// state at build time.
    Const(T),
    /// Signal-bound — register a subscription against the State's
    /// signal id at build time. The builder method supplies the
    /// `PropertyId` + writer; this just carries the data source.
    Bound(State<T>),
    /// Computed (derived) value — register a subscription against the
    /// Computed's [`DerivedId`] at build time. Fires when ANY of the
    /// derived's dependencies set, via the derived-binding notifier.
    /// Bridge from [`blinc_core::reactive::Computed`] to the builder
    /// surface. Multi-source reactivity (e.g. multi-axis transforms)
    /// composes naturally here — the compute closure inside the
    /// Computed combines as many source signals as needed.
    Computed(Computed<T>),
}

/// Type-erased value carrier for reactive bindings.
///
/// `Reactive::Bound` stores the read + write closures as type-erased
/// (`Arc<dyn Fn>`); they shuttle values through this `Any`-backed
/// wrapper rather than the concrete `T`. The downcast happens inside
/// the `write` closure where the concrete type is known at the original
/// `IntoReactive` construction site.
pub struct BoundValue(pub Box<dyn std::any::Any + Send + Sync>);

impl BoundValue {
    pub fn new<T: Send + Sync + 'static>(v: T) -> Self {
        Self(Box::new(v))
    }

    /// Downcast to a reference of the inner type. Returns `None` on
    /// type mismatch (programmer error — would mean a binding's
    /// write closure doesn't match its read closure's type).
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.0.downcast_ref::<T>()
    }
}

/// Trait that lets `Div` builder methods accept both eager `T` values
/// and signal-bound `&State<T>` references at the same call site.
///
/// Implemented in this crate for `T` (eager) and for `&State<T>`
/// (signal-bound) — the two impls coexist via blanket-less specialisation
/// (each impl targets a distinct type, no overlap).
///
/// # Example (post-P2.2 surface)
///
/// ```ignore
/// // Eager — compiles to direct RenderProps write at build time
/// div().bg(Color::RED)
///
/// // Signal-bound — registers a subscription; fires on state.set()
/// let bg = State::new(...);
/// div().bg(&bg)
/// ```
pub trait IntoReactive<T> {
    fn into_reactive(self) -> Reactive<T>;
}

// Eager: any value of T is a Const reactive.
impl<T> IntoReactive<T> for T {
    fn into_reactive(self) -> Reactive<T> {
        Reactive::Const(self)
    }
}

// Signal-bound: a `&State<T>` produces a Bound reactive. `State<T>::clone`
// is cheap (Arc internals) so capturing it here doesn't allocate the
// underlying graph again.
impl<T: Clone + Send + 'static> IntoReactive<T> for &State<T> {
    fn into_reactive(self) -> Reactive<T> {
        Reactive::Bound(self.clone())
    }
}

// Owned `State<T>` also works — convenient for `.bg(state.clone())`
// patterns where the caller doesn't want to type a reference.
impl<T: Clone + Send + 'static> IntoReactive<T> for State<T> {
    fn into_reactive(self) -> Reactive<T> {
        Reactive::Bound(self)
    }
}

// Derived (computed) values — `&Computed<T>` produces a Computed
// reactive. The Computed<T> wrapper bundles the DerivedId with the
// reactive graph, so the binding has everything it needs to read
// the current value on every fire.
impl<T: Clone + Send + 'static> IntoReactive<T> for &Computed<T> {
    fn into_reactive(self) -> Reactive<T> {
        Reactive::Computed(self.clone())
    }
}

impl<T: Clone + Send + 'static> IntoReactive<T> for Computed<T> {
    fn into_reactive(self) -> Reactive<T> {
        Reactive::Computed(self)
    }
}

// Standalone signal: bridge a `Signal<T>` into the same `Reactive::Bound`
// channel `&State<T>` uses, by wrapping it with the process-global
// reactive graph + dirty flag. This lets `.bg(signal)` / `.bg(&signal)`
// share the property-binding registry path with `.bg(&state)`.
//
// `Signal<T>` is `Copy`, so both impl arms are trivial — the by-value
// form doesn't even need an explicit clone.
impl<T: Clone + Send + 'static> IntoReactive<T> for Signal<T> {
    fn into_reactive(self) -> Reactive<T> {
        Reactive::Bound(State::new(
            self,
            blinc_core::reactive::global_graph(),
            blinc_core::reactive::global_dirty_flag(),
        ))
    }
}

impl<T: Clone + Send + 'static> IntoReactive<T> for &Signal<T> {
    fn into_reactive(self) -> Reactive<T> {
        Reactive::Bound(State::new(
            *self,
            blinc_core::reactive::global_graph(),
            blinc_core::reactive::global_dirty_flag(),
        ))
    }
}

// =========================================================================
// PendingBinding — what Div / other element builders hold between method
// chaining and `build()`. Each represents a deferred registration: at
// build time, after the `LayoutNodeId` is minted, every pending binding
// gets `register(node_id)` called on it.
// =========================================================================

/// Type-erased pending binding stored on `Div` (and other builders).
/// Each element collects these as `.bg(&signal)` / `.opacity(&signal)` /
/// etc. are called, then drains them in `build()` once the node id
/// exists.
pub trait PendingBinding: Send + Sync {
    /// Register this binding against `node_id` in the global registry.
    /// Called once during the element's `build()` after the layout node
    /// is minted.
    fn register(&self, node_id: LayoutNodeId);
}

/// Concrete typed-binding writer — `Arc<dyn Fn(&mut RenderProps, T)>`,
/// generic over the value type. Lifted to a type alias so clippy's
/// `type_complexity` lint doesn't fire on the `TypedPendingBinding`
/// struct field.
pub type TypedWriteFn<T> = Arc<dyn Fn(&mut RenderProps, T) + Send + Sync>;

/// Source of a typed pending binding — either a single-signal
/// `State<T>` (subscribed under a `SignalId`) or a derived
/// `Computed<T>` (subscribed under a `DerivedId`). The PendingBinding
/// impl dispatches on this enum at register-time so a single
/// [`TypedPendingBinding`] type handles both surfaces.
enum BindingSource<T: Clone + Send + Sync + 'static> {
    Signal(State<T>),
    Derived(Computed<T>),
}

/// Concrete `PendingBinding` for a typed `(BindingSource<T>, write:
/// fn(&mut RenderProps, T))` triple. Held on the element builder via
/// `Box<dyn PendingBinding>`. Originally only carried `State<T>`;
/// extended in the Phase 8 follow-up (Derived ↔ IntoReactive bridge)
/// to also carry `Computed<T>` so `Reactive::Computed` arms reuse the
/// same struct.
pub struct TypedPendingBinding<T: Clone + Send + Sync + 'static> {
    source: BindingSource<T>,
    property: PropertyId,
    write: TypedWriteFn<T>,
}

impl<T: Clone + Send + Sync + 'static> TypedPendingBinding<T> {
    /// Build a pending binding that, when registered against a node,
    /// will subscribe to `state` and write its values into `RenderProps`
    /// via `write` whenever the signal fires.
    pub fn new(
        state: State<T>,
        property: PropertyId,
        write: impl Fn(&mut RenderProps, T) + Send + Sync + 'static,
    ) -> Self {
        Self {
            source: BindingSource::Signal(state),
            property,
            write: Arc::new(write),
        }
    }

    /// Build a pending binding backed by a `Computed<T>` — fires when
    /// any of the computed's dependencies (recursive) change.
    pub fn from_computed(
        computed: Computed<T>,
        property: PropertyId,
        write: impl Fn(&mut RenderProps, T) + Send + Sync + 'static,
    ) -> Self {
        Self {
            source: BindingSource::Derived(computed),
            property,
            write: Arc::new(write),
        }
    }
}

impl<T: Clone + Send + Sync + 'static> PendingBinding for TypedPendingBinding<T> {
    fn register(&self, node_id: LayoutNodeId) {
        let write = Arc::clone(&self.write);
        match &self.source {
            BindingSource::Signal(state) => {
                register_typed(
                    state.signal_id(),
                    node_id,
                    self.property,
                    state.clone(),
                    move |p, v| write(p, v),
                );
            }
            BindingSource::Derived(computed) => {
                register_typed_computed(
                    computed.derived_id(),
                    node_id,
                    self.property,
                    computed.clone(),
                    move |p, v| write(p, v),
                );
            }
        }
    }
}

/// Closure type for taffy-style writes — `Arc<dyn Fn(&mut taffy::Style,
/// T)>`. Mirror of [`TypedWriteFn`] for the layout-binding path.
pub type TypedLayoutWriteFn<T> = Arc<dyn Fn(&mut taffy::Style, T) + Send + Sync>;

/// Layout-binding variant of [`TypedPendingBinding`]. Fires
/// `queue_layout_update_partial` on every signal change, which patches
/// the live taffy `Style` and triggers relayout next frame.
///
/// Used by Phase 2.4 layout-affecting builder methods (`.w(&signal)`,
/// `.h(&signal)`, `.padding(&signal)`, etc.).
pub struct LayoutPendingBinding<T: Clone + Send + Sync + 'static> {
    source: BindingSource<T>,
    property: PropertyId,
    write: TypedLayoutWriteFn<T>,
}

impl<T: Clone + Send + Sync + 'static> LayoutPendingBinding<T> {
    /// Build a layout-targeting pending binding. When registered against
    /// a node, every `state.set(...)` fires a partial update whose
    /// `layout_write` closure mutates the taffy `Style` via `write`.
    pub fn new(
        state: State<T>,
        property: PropertyId,
        write: impl Fn(&mut taffy::Style, T) + Send + Sync + 'static,
    ) -> Self {
        Self {
            source: BindingSource::Signal(state),
            property,
            write: Arc::new(write),
        }
    }

    /// Build a layout-targeting pending binding backed by a
    /// `Computed<T>` — fires when any of the computed's dependencies
    /// set. Symmetric with [`TypedPendingBinding::from_computed`].
    pub fn from_computed(
        computed: Computed<T>,
        property: PropertyId,
        write: impl Fn(&mut taffy::Style, T) + Send + Sync + 'static,
    ) -> Self {
        Self {
            source: BindingSource::Derived(computed),
            property,
            write: Arc::new(write),
        }
    }
}

impl<T: Clone + Send + Sync + 'static> PendingBinding for LayoutPendingBinding<T> {
    fn register(&self, node_id: LayoutNodeId) {
        let write = Arc::clone(&self.write);
        let property = self.property;
        match &self.source {
            BindingSource::Signal(state) => {
                register_typed_layout(
                    state.signal_id(),
                    node_id,
                    property,
                    state.clone(),
                    move |style, v| write(style, v),
                );
            }
            BindingSource::Derived(computed) => {
                register_typed_layout_computed(
                    computed.derived_id(),
                    node_id,
                    property,
                    computed.clone(),
                    move |style, v| write(style, v),
                );
            }
        }
    }
}

// =========================================================================
// Registry
// =========================================================================

/// What target a subscriber writes into when its signal fires.
/// Visual props write `RenderProps`; layout props write the taffy
/// `Style` (and trigger `compute_layout` next frame via the side-effect
/// metadata).
enum SubscriberWrite {
    Render(WriteFn),
    Layout(LayoutWriteFn),
}

struct Subscriber {
    node_id: LayoutNodeId,
    property: PropertyId,
    read: ReadFn,
    write: SubscriberWrite,
}

/// Process-global registry of signal-bound + derived-bound property
/// subscribers.
///
/// Two parallel index tables — one keyed by `SignalId` (driven by
/// `State<T>::set` → `notify_property_bindings`) and one keyed by
/// `DerivedId` (driven by `ReactiveGraph::set`'s dirty propagation →
/// `notify_property_bindings_for_derived`). Both go through the same
/// `Subscriber` struct + `fire` dispatch logic; only the key namespace
/// differs.
pub struct PropertyBindingRegistry {
    /// `signal_id → list of subscribers waiting on that signal`.
    bindings: HashMap<SignalId, Vec<Subscriber>>,
    /// `derived_id → list of subscribers waiting on that derived`.
    /// Parallels `bindings` for `Computed<T>`-backed subscriptions.
    derived_bindings: HashMap<DerivedId, Vec<Subscriber>>,
    /// `node_id → list of signal_ids that node subscribed to`.
    /// Used by `unregister_node` to evict in O(per-binding-on-that-node)
    /// time instead of scanning every signal's subscriber list.
    by_node: HashMap<LayoutNodeId, Vec<SignalId>>,
    /// Parallel reverse index for derived subscriptions — same role
    /// as `by_node` but for the `derived_bindings` map.
    derived_by_node: HashMap<LayoutNodeId, Vec<DerivedId>>,
}

impl PropertyBindingRegistry {
    fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            derived_bindings: HashMap::new(),
            by_node: HashMap::new(),
            derived_by_node: HashMap::new(),
        }
    }

    /// Register a signal-bound subscription. Called by `Div::build` (or
    /// the equivalent collection pass) for every reactive modifier on
    /// the element, after the `LayoutNodeId` has been minted.
    ///
    /// `read` reads the current value from the signal's reactive graph;
    /// `write` writes that value into the right `RenderProps` field.
    /// Both are stored as `Arc<dyn Fn>` so the registry can hold
    /// heterogeneous subscribers in one table.
    pub fn register(
        &mut self,
        signal_id: SignalId,
        node_id: LayoutNodeId,
        property: PropertyId,
        read: ReadFn,
        write: WriteFn,
    ) {
        self.bindings
            .entry(signal_id)
            .or_default()
            .push(Subscriber {
                node_id,
                property,
                read,
                write: SubscriberWrite::Render(write),
            });
        self.by_node.entry(node_id).or_default().push(signal_id);
    }

    /// Register a derived-bound subscription. The `read` closure
    /// reads the current value of a `Derived<T>` via the supplied
    /// graph; fired by `notify_property_bindings_for_derived` when
    /// any of the derived's dependencies set. Otherwise identical to
    /// [`Self::register`].
    pub fn register_derived(
        &mut self,
        derived_id: DerivedId,
        node_id: LayoutNodeId,
        property: PropertyId,
        read: ReadFn,
        write: WriteFn,
    ) {
        self.derived_bindings
            .entry(derived_id)
            .or_default()
            .push(Subscriber {
                node_id,
                property,
                read,
                write: SubscriberWrite::Render(write),
            });
        self.derived_by_node
            .entry(node_id)
            .or_default()
            .push(derived_id);
    }

    /// Register a derived-bound *layout-targeting* subscription —
    /// `register_derived` is for `RenderProps` writes; this is the
    /// taffy-Style sibling. Fires the same way (via the derived
    /// notifier) but the write closure mutates `taffy::Style`.
    pub fn register_derived_layout(
        &mut self,
        derived_id: DerivedId,
        node_id: LayoutNodeId,
        property: PropertyId,
        read: ReadFn,
        write: LayoutWriteFn,
    ) {
        self.derived_bindings
            .entry(derived_id)
            .or_default()
            .push(Subscriber {
                node_id,
                property,
                read,
                write: SubscriberWrite::Layout(write),
            });
        self.derived_by_node
            .entry(node_id)
            .or_default()
            .push(derived_id);
    }

    /// Register a layout-targeting subscription. The `write` closure
    /// mutates the live taffy `Style` (instead of `RenderProps`) on
    /// every signal fire; the side-effect metadata on `property` tells
    /// the drain step to schedule `compute_layout` next frame.
    pub fn register_layout(
        &mut self,
        signal_id: SignalId,
        node_id: LayoutNodeId,
        property: PropertyId,
        read: ReadFn,
        write: LayoutWriteFn,
    ) {
        self.bindings
            .entry(signal_id)
            .or_default()
            .push(Subscriber {
                node_id,
                property,
                read,
                write: SubscriberWrite::Layout(write),
            });
        self.by_node.entry(node_id).or_default().push(signal_id);
    }

    /// Evict every subscription belonging to the given node. Called by
    /// `remove_subtree_nodes` so stale subscribers can't fire on the
    /// next signal change after a structural rebuild dropped the node.
    pub fn unregister_node(&mut self, node_id: LayoutNodeId) {
        if let Some(signal_ids) = self.by_node.remove(&node_id) {
            for sig_id in signal_ids {
                if let Some(subs) = self.bindings.get_mut(&sig_id) {
                    subs.retain(|s| s.node_id != node_id);
                    if subs.is_empty() {
                        self.bindings.remove(&sig_id);
                    }
                }
            }
        }
        if let Some(derived_ids) = self.derived_by_node.remove(&node_id) {
            for d_id in derived_ids {
                if let Some(subs) = self.derived_bindings.get_mut(&d_id) {
                    subs.retain(|s| s.node_id != node_id);
                    if subs.is_empty() {
                        self.derived_bindings.remove(&d_id);
                    }
                }
            }
        }
    }

    /// Walk every subscriber waiting on `signal_id` and queue a partial
    /// property update for each. Called by the global notifier from
    /// `blinc_core::reactive::set_property_binding_notifier` whenever a
    /// `State<T>::set` (or `update`) fires.
    pub fn fire(&self, signal_id: SignalId) {
        let Some(subs) = self.bindings.get(&signal_id) else {
            return;
        };
        Self::dispatch_subscribers(subs);
    }

    /// Walk every subscriber waiting on `derived_id` and queue a
    /// partial property update for each. Called by the parallel
    /// derived-binding notifier when `ReactiveGraph::set` propagates
    /// dirty markers through derived chains.
    pub fn fire_derived(&self, derived_id: DerivedId) {
        let Some(subs) = self.derived_bindings.get(&derived_id) else {
            return;
        };
        Self::dispatch_subscribers(subs);
    }

    fn dispatch_subscribers(subs: &[Subscriber]) {
        for sub in subs {
            let Some(value) = (sub.read)() else {
                continue;
            };
            match &sub.write {
                SubscriberWrite::Render(write) => {
                    let write = Arc::clone(write);
                    queue_prop_update_partial(
                        sub.node_id,
                        sub.property,
                        sub.property.side_effects(),
                        move |props| write(props, &value),
                    );
                }
                SubscriberWrite::Layout(write) => {
                    let write = Arc::clone(write);
                    crate::stateful::queue_layout_update_partial(
                        sub.node_id,
                        sub.property,
                        sub.property.side_effects(),
                        move |style| write(style, &value),
                    );
                }
            }
        }
    }

    /// Number of unique signals currently subscribed to. For diagnostics
    /// / tests.
    pub fn signal_count(&self) -> usize {
        self.bindings.len()
    }

    /// Number of unique derived values currently subscribed to.
    /// For diagnostics / tests.
    pub fn derived_count(&self) -> usize {
        self.derived_bindings.len()
    }

    /// Number of subscribers for `signal_id`. For diagnostics / tests.
    pub fn subscriber_count(&self, signal_id: SignalId) -> usize {
        self.bindings.get(&signal_id).map(|v| v.len()).unwrap_or(0)
    }

    /// Number of subscribers for `derived_id`. For diagnostics /
    /// tests.
    pub fn derived_subscriber_count(&self, derived_id: DerivedId) -> usize {
        self.derived_bindings
            .get(&derived_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Drop every binding. Test-only: each test creates a fresh
    /// `ReactiveGraph` whose signal-id counter restarts at 0, so test
    /// runs share signal ids and would observe each other's subscribers
    /// without an explicit reset.
    #[cfg(test)]
    pub fn clear_for_tests(&mut self) {
        self.bindings.clear();
        self.derived_bindings.clear();
        self.by_node.clear();
        self.derived_by_node.clear();
    }
}

/// Process-global registry instance. The first call to `global()`
/// installs the notifier hook in `blinc_core::reactive` — subsequent
/// `State<T>::set` calls feed into `fire` automatically.
#[allow(clippy::incompatible_msrv)]
static REGISTRY: LazyLock<Mutex<PropertyBindingRegistry>> = LazyLock::new(|| {
    blinc_core::reactive::set_property_binding_notifier(|signal_id| {
        // Lock the registry on each fire; brief — we drain to the
        // partial-update queue and release. Drain happens on a different
        // thread (platform runner main thread) so contention is minimal.
        if let Ok(reg) = REGISTRY.lock() {
            reg.fire(signal_id);
        }
    });
    // Pair: when a Computed<T>'s underlying derived flips dirty
    // inside ReactiveGraph::set's dirty walk, the core fires this
    // notifier. We dispatch to the parallel derived_bindings map.
    blinc_core::reactive::set_derived_binding_notifier(|derived_id| {
        if let Ok(reg) = REGISTRY.lock() {
            reg.fire_derived(derived_id);
        }
    });
    Mutex::new(PropertyBindingRegistry::new())
});

/// Access the global registry. Lazy-initialises + installs the core
/// notifier hook on first call.
pub fn with_registry<R>(f: impl FnOnce(&mut PropertyBindingRegistry) -> R) -> R {
    let mut reg = REGISTRY.lock().unwrap();
    f(&mut reg)
}

/// Convenience: register a signal-bound subscription using a typed
/// `State<T>` + a typed writer. Wraps the boilerplate of constructing
/// the type-erased `read` + `write` closures.
///
/// This is the API `Div` builder methods will call internally in P2.2+.
pub fn register_typed<T>(
    signal_id: SignalId,
    node_id: LayoutNodeId,
    property: PropertyId,
    state: blinc_core::reactive::State<T>,
    write: impl Fn(&mut RenderProps, T) + Send + Sync + 'static,
) where
    T: Clone + Send + Sync + 'static,
{
    let write = Arc::new(write);
    let state_for_read = state.clone();
    let read: ReadFn = Arc::new(move || state_for_read.try_get().map(BoundValue::new));
    let write_dyn: WriteFn = {
        let write = Arc::clone(&write);
        Arc::new(move |props: &mut RenderProps, val: &BoundValue| {
            if let Some(v) = val.downcast_ref::<T>() {
                write(props, v.clone());
            }
        })
    };
    with_registry(|reg| reg.register(signal_id, node_id, property, read, write_dyn));
}

/// Convenience: register a derived-bound subscription using a typed
/// `Computed<T>` + a typed writer. Mirror of [`register_typed`] for
/// the derived path — sits behind `TypedPendingBinding::from_computed`
/// and the `Reactive::Computed` arm of every reactive builder.
pub fn register_typed_computed<T>(
    derived_id: DerivedId,
    node_id: LayoutNodeId,
    property: PropertyId,
    computed: Computed<T>,
    write: impl Fn(&mut RenderProps, T) + Send + Sync + 'static,
) where
    T: Clone + Send + Sync + 'static,
{
    let write = Arc::new(write);
    let computed_for_read = computed.clone();
    let read: ReadFn = Arc::new(move || computed_for_read.try_get().map(BoundValue::new));
    let write_dyn: WriteFn = {
        let write = Arc::clone(&write);
        Arc::new(move |props: &mut RenderProps, val: &BoundValue| {
            if let Some(v) = val.downcast_ref::<T>() {
                write(props, v.clone());
            }
        })
    };
    with_registry(|reg| reg.register_derived(derived_id, node_id, property, read, write_dyn));
}

/// Convenience: register a derived-bound layout-targeting
/// subscription. Mirror of [`register_typed_layout`] for the derived
/// path — sits behind `LayoutPendingBinding::from_computed`.
pub fn register_typed_layout_computed<T>(
    derived_id: DerivedId,
    node_id: LayoutNodeId,
    property: PropertyId,
    computed: Computed<T>,
    write: impl Fn(&mut taffy::Style, T) + Send + Sync + 'static,
) where
    T: Clone + Send + Sync + 'static,
{
    let write = Arc::new(write);
    let computed_for_read = computed.clone();
    let read: ReadFn = Arc::new(move || computed_for_read.try_get().map(BoundValue::new));
    let write_dyn: LayoutWriteFn = {
        let write = Arc::clone(&write);
        Arc::new(move |style: &mut taffy::Style, val: &BoundValue| {
            if let Some(v) = val.downcast_ref::<T>() {
                write(style, v.clone());
            }
        })
    };
    with_registry(|reg| {
        reg.register_derived_layout(derived_id, node_id, property, read, write_dyn)
    });
}

/// Convenience: register a layout-targeting signal-bound subscription.
/// Counterpart to [`register_typed`] for layout-affecting properties.
/// The supplied `write` closure mutates the live taffy `Style` (instead
/// of `RenderProps`) on every signal fire.
pub fn register_typed_layout<T>(
    signal_id: SignalId,
    node_id: LayoutNodeId,
    property: PropertyId,
    state: blinc_core::reactive::State<T>,
    write: impl Fn(&mut taffy::Style, T) + Send + Sync + 'static,
) where
    T: Clone + Send + Sync + 'static,
{
    let write = Arc::new(write);
    let state_for_read = state.clone();
    let read: ReadFn = Arc::new(move || state_for_read.try_get().map(BoundValue::new));
    let write_dyn: LayoutWriteFn = {
        let write = Arc::clone(&write);
        Arc::new(move |style: &mut taffy::Style, val: &BoundValue| {
            if let Some(v) = val.downcast_ref::<T>() {
                write(style, v.clone());
            }
        })
    };
    with_registry(|reg| reg.register_layout(signal_id, node_id, property, read, write_dyn));
}

/// Convenience: unregister all bindings for a node. Called by
/// `remove_subtree_nodes` in the renderer cleanup path.
pub fn unregister_node(node_id: LayoutNodeId) {
    with_registry(|reg| reg.unregister_node(node_id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::reactive::{ReactiveGraph, State};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// Serialise the binding tests — they share the process-global
    /// registry, and each `fresh_state` call rebuilds a `ReactiveGraph`
    /// whose signal-id counter restarts at 0. Without serialisation,
    /// two parallel tests both register subscribers for `SignalId(0)`
    /// and one test's `set()` fires the other's binding.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Acquire the lock + reset the registry to a clean state. Returns
    /// a guard so the lock is held for the test body's duration.
    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        // Poisoning on a previous test's panic shouldn't kill subsequent
        // tests — recover from the poison so the suite keeps running.
        let guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        with_registry(|r| r.clear_for_tests());
        let _ = crate::stateful::take_pending_partial_prop_updates();
        guard
    }

    /// Build a fresh `State<T>` over a private ReactiveGraph. Avoids
    /// the BlincContextState global init (which is one-shot and can't
    /// be reused across tests). Also touches the registry so the
    /// LazyLock fires + installs the core notifier before the test
    /// interacts with `State::set`.
    fn fresh_state<T: Clone + Send + 'static>(initial: T) -> State<T> {
        with_registry(|_| {});
        let graph: Arc<Mutex<ReactiveGraph>> = Arc::new(Mutex::new(ReactiveGraph::new()));
        let signal = graph.lock().unwrap().create_signal(initial);
        let dirty: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        State::new(signal, graph, dirty)
    }

    fn mint_node(tree: &mut crate::tree::LayoutTree) -> LayoutNodeId {
        tree.create_node(taffy::Style::default())
    }

    #[test]
    fn register_then_fire_queues_partial_update() {
        let _guard = lock_and_reset();
        let state = fresh_state::<i32>(0);

        let mut tree = crate::tree::LayoutTree::new();
        let node_id = mint_node(&mut tree);

        let fire_count = Arc::new(AtomicUsize::new(0));
        let fire_count_for_write = Arc::clone(&fire_count);
        register_typed(
            state.signal_id(),
            node_id,
            PropertyId::Background,
            state.clone(),
            move |_props, _v: i32| {
                fire_count_for_write.fetch_add(1, Ordering::SeqCst);
            },
        );

        // Set the signal — should fire one binding → one queued update.
        state.set(42);

        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1, "exactly one partial update queued");
        assert_eq!(updates[0].node_id, node_id);
        assert_eq!(updates[0].property, PropertyId::Background);

        // Apply it to a RenderProps to invoke the writer.
        let mut props = RenderProps::default();
        (updates.into_iter().next().unwrap().render_write.unwrap())(&mut props);
        assert_eq!(fire_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unregister_node_stops_firing() {
        let _guard = lock_and_reset();
        let state = fresh_state::<i32>(0);

        let mut tree = crate::tree::LayoutTree::new();
        let node_id = mint_node(&mut tree);

        register_typed(
            state.signal_id(),
            node_id,
            PropertyId::Opacity,
            state.clone(),
            |_p, _v: i32| {},
        );

        // Sanity: registered.
        assert_eq!(with_registry(|r| r.subscriber_count(state.signal_id())), 1);

        unregister_node(node_id);

        // After unregister: no subscribers, fire is a no-op.
        assert_eq!(with_registry(|r| r.subscriber_count(state.signal_id())), 0);

        state.set(99);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert!(
            updates.is_empty(),
            "unregistered binding must not fire (got {} updates)",
            updates.len()
        );
    }

    #[test]
    fn multiple_subscribers_one_signal() {
        let _guard = lock_and_reset();
        let state = fresh_state::<i32>(0);

        let mut tree = crate::tree::LayoutTree::new();
        let n1 = mint_node(&mut tree);
        let n2 = mint_node(&mut tree);
        let n3 = mint_node(&mut tree);

        for nid in [n1, n2, n3] {
            register_typed(
                state.signal_id(),
                nid,
                PropertyId::Background,
                state.clone(),
                |_p, _v: i32| {},
            );
        }

        state.set(7);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 3, "all three subscribers fired");
        assert!(updates.iter().any(|u| u.node_id == n1));
        assert!(updates.iter().any(|u| u.node_id == n2));
        assert!(updates.iter().any(|u| u.node_id == n3));
    }

    #[test]
    fn unregister_removes_only_targeted_node() {
        let _guard = lock_and_reset();
        let state = fresh_state::<i32>(0);

        let mut tree = crate::tree::LayoutTree::new();
        let keep = mint_node(&mut tree);
        let drop_node = mint_node(&mut tree);

        register_typed(
            state.signal_id(),
            keep,
            PropertyId::Background,
            state.clone(),
            |_p, _v: i32| {},
        );
        register_typed(
            state.signal_id(),
            drop_node,
            PropertyId::Background,
            state.clone(),
            |_p, _v: i32| {},
        );

        unregister_node(drop_node);

        state.set(1);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1, "only the surviving subscriber fired");
        assert_eq!(updates[0].node_id, keep);
    }

    #[test]
    fn into_reactive_const_path() {
        // No registry interaction; doesn't need the lock.
        let r = 42_i32.into_reactive();
        match r {
            Reactive::Const(v) => assert_eq!(v, 42),
            Reactive::Bound(_) => panic!("expected Const"),
            Reactive::Computed(_) => panic!("expected Const"),
        }
    }

    #[test]
    fn into_reactive_bound_path_from_state_ref() {
        let _guard = lock_and_reset();
        let state = fresh_state::<i32>(7);
        let r = (&state).into_reactive();
        match r {
            Reactive::Const(_) => panic!("expected Bound"),
            Reactive::Bound(s) => assert_eq!(s.try_get(), Some(7)),
            Reactive::Computed(_) => panic!("&State must produce Bound, not Computed"),
        }
    }

    #[test]
    fn pending_binding_registers_and_fires() {
        let _guard = lock_and_reset();
        let state = fresh_state::<i32>(0);

        let mut tree = crate::tree::LayoutTree::new();
        let node_id = mint_node(&mut tree);

        // Pretend this is what a builder method does at chain time.
        let pending =
            TypedPendingBinding::new(state.clone(), PropertyId::Opacity, |_p, _v: i32| {});

        // And this is what `build()` does after minting node_id.
        pending.register(node_id);

        // Fire the signal — the binding should queue an update.
        state.set(123);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].node_id, node_id);
        assert_eq!(updates[0].property, PropertyId::Opacity);
    }

    #[test]
    fn div_bg_eager_path_unchanged() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};
        use blinc_core::Color;

        // Eager `.bg(Color)` — exercises the Const branch. No binding
        // should be registered.
        let mut tree = crate::tree::LayoutTree::new();
        let element = div().bg(Color::from_hex(0xff0000));
        let _node_id = element.build(&mut tree);

        // No signal-bound bindings → registry stays empty.
        assert_eq!(with_registry(|r| r.signal_count()), 0);
    }

    #[test]
    fn div_bg_bound_path_fires_on_state_set() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};
        use blinc_core::Color;

        let bg_state = fresh_state::<Color>(Color::from_hex(0xff0000));
        let element = div().bg(&bg_state);

        // Initial value seeded into the builder before build.
        let props = element.render_props();
        assert!(props.background.is_some());

        // Build the element — registration fires inside build().
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        assert_eq!(
            with_registry(|r| r.subscriber_count(bg_state.signal_id())),
            1,
            "exactly one binding registered for this state"
        );

        // Drain any updates queued during build (none expected) so the
        // fire check below measures cleanly.
        let _ = crate::stateful::take_pending_partial_prop_updates();

        // Set the state — the binding fires → queue gets a partial update.
        bg_state.set(Color::from_hex(0x00ff00));
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].node_id, node_id);
        assert_eq!(updates[0].property, PropertyId::Background);

        // Apply the queued update — RenderProps.background should reflect
        // the new colour.
        let mut props = RenderProps::default();
        (updates.into_iter().next().unwrap().render_write.unwrap())(&mut props);
        match props.background {
            Some(blinc_core::Brush::Solid(c)) => {
                assert_eq!(c, Color::from_hex(0x00ff00));
            }
            other => panic!("expected Solid green, got {other:?}"),
        }
    }

    #[test]
    fn div_opacity_bound_path_fires_on_state_set() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};

        let opacity_state = fresh_state::<f32>(1.0);
        let element = div().opacity(&opacity_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        opacity_state.set(0.5);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].node_id, node_id);
        assert_eq!(updates[0].property, PropertyId::Opacity);

        let mut props = RenderProps::default();
        (updates.into_iter().next().unwrap().render_write.unwrap())(&mut props);
        assert!((props.opacity - 0.5).abs() < 1e-6);
    }

    #[test]
    fn div_rounded_bound_path_fires_on_state_set() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};

        let radius_state = fresh_state::<f32>(4.0);
        let element = div().rounded(&radius_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        radius_state.set(16.0);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].node_id, node_id);
        assert_eq!(updates[0].property, PropertyId::CornerRadius);

        let mut props = RenderProps::default();
        (updates.into_iter().next().unwrap().render_write.unwrap())(&mut props);
        assert!((props.border_radius.top_left - 16.0).abs() < 1e-6);
        assert!((props.border_radius.bottom_right - 16.0).abs() < 1e-6);
        assert!(props.border_radius_explicit);
    }

    #[test]
    fn div_border_color_bound_path_fires_on_state_set() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};
        use blinc_core::Color;

        let bc_state = fresh_state::<Color>(Color::from_hex(0x000000));
        let element = div().border_color(&bc_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        bc_state.set(Color::from_hex(0xff8800));
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].node_id, node_id);
        assert_eq!(updates[0].property, PropertyId::BorderColor);

        let mut props = RenderProps::default();
        (updates.into_iter().next().unwrap().render_write.unwrap())(&mut props);
        assert_eq!(props.border_color, Some(Color::from_hex(0xff8800)));
    }

    #[test]
    fn div_shadow_bound_path_fires_on_state_set() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};
        use blinc_core::{Color, Shadow};

        let initial = Shadow::new(0.0, 2.0, 4.0, Color::from_hex(0x000000));
        let next = Shadow::new(0.0, 8.0, 16.0, Color::from_hex(0x222222));
        let shadow_state = fresh_state::<Shadow>(initial);
        let element = div().shadow(&shadow_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        shadow_state.set(next);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].node_id, node_id);
        assert_eq!(updates[0].property, PropertyId::Shadow);

        let mut props = RenderProps::default();
        (updates.into_iter().next().unwrap().render_write.unwrap())(&mut props);
        assert_eq!(props.shadow.len(), 1);
        assert_eq!(props.shadow[0], next);
    }

    #[test]
    fn div_transform_bound_path_fires_on_state_set() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};
        use blinc_core::Transform;

        let t_state = fresh_state::<Transform>(Transform::translate(0.0, 0.0));
        let element = div().transform(&t_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        t_state.set(Transform::translate(10.0, 20.0));
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].node_id, node_id);
        assert_eq!(updates[0].property, PropertyId::Transform);

        let mut props = RenderProps::default();
        (updates.into_iter().next().unwrap().render_write.unwrap())(&mut props);
        assert!(props.transform.is_some());
    }

    /// Layout-binding path — `.w(&state)` should emit a `layout_write`
    /// (not `render_write`) with `needs_layout = true`.
    #[test]
    fn div_w_bound_path_emits_layout_write() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};

        let w_state = fresh_state::<f32>(100.0);
        let element = div().w(&w_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        // Sanity: initial value seeded into taffy style.
        let style = tree.get_style(node_id).expect("style");
        assert!(
            matches!(style.size.width, taffy::Dimension::Length(v) if (v - 100.0).abs() < 1e-6)
        );

        let _ = crate::stateful::take_pending_partial_prop_updates();
        w_state.set(250.0);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        let upd = updates.into_iter().next().unwrap();
        assert_eq!(upd.node_id, node_id);
        assert_eq!(upd.property, PropertyId::Width);
        assert!(
            upd.effects.needs_layout,
            "Width changes must trigger layout"
        );
        assert!(
            upd.render_write.is_none(),
            "Width is layout-only, no RenderProps write"
        );
        assert!(upd.layout_write.is_some(), "Width must have a layout_write");

        // Apply the layout write — the taffy Style should pick up the new width.
        let mut style = tree.get_style(node_id).unwrap();
        (upd.layout_write.unwrap())(&mut style);
        assert!(
            matches!(style.size.width, taffy::Dimension::Length(v) if (v - 250.0).abs() < 1e-6)
        );
    }

    #[test]
    fn div_h_bound_path_emits_layout_write() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};

        let h_state = fresh_state::<f32>(50.0);
        let element = div().h(&h_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        h_state.set(75.0);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        let upd = updates.into_iter().next().unwrap();
        assert_eq!(upd.property, PropertyId::Height);
        assert!(upd.effects.needs_layout);
        assert!(upd.layout_write.is_some());
    }

    #[test]
    fn div_gap_bound_path_emits_layout_write() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};

        let gap_state = fresh_state::<f32>(2.0); // 8px
        let element = div().gap(&gap_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        gap_state.set(4.0); // 16px
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        let upd = updates.into_iter().next().unwrap();
        assert_eq!(upd.property, PropertyId::Gap);
        assert!(upd.effects.needs_layout);

        let mut style = tree.get_style(node_id).unwrap();
        (upd.layout_write.unwrap())(&mut style);
        // gap units are 4px each — gap(4.0) → 16.0
        match style.gap.width {
            taffy::LengthPercentage::Length(v) => assert!((v - 16.0).abs() < 1e-6),
            other => panic!("expected Length, got {other:?}"),
        }
    }

    #[test]
    fn div_padding_bound_path_emits_layout_write() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};

        let p_state = fresh_state::<f32>(2.0);
        let element = div().p(&p_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        p_state.set(6.0); // 24px
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        let upd = updates.into_iter().next().unwrap();
        assert_eq!(upd.property, PropertyId::Padding);
        assert!(upd.effects.needs_layout);

        let mut style = tree.get_style(node_id).unwrap();
        (upd.layout_write.unwrap())(&mut style);
        match style.padding.left {
            taffy::LengthPercentage::Length(v) => assert!((v - 24.0).abs() < 1e-6),
            other => panic!("expected Length, got {other:?}"),
        }
    }

    // ============================================================
    // Phase 8.1 — transform_origin + transform_width +
    // bind_transform_from. Layout-property animation helpers that
    // route per-frame visual changes through a GPU scale transform
    // instead of taffy::Style mutation, skipping compute_layout.
    // ============================================================

    #[test]
    fn div_transform_origin_propagates_to_render_props() {
        use crate::div::{ElementBuilder, div};
        let element = div().transform_origin(0.0, 50.0);
        let props = element.render_props();
        assert_eq!(props.transform_origin, Some([0.0, 50.0]));
    }

    /// Pull `(sx, sy)` from a `Transform::scale(sx, sy)` value. Panics
    /// if the supplied transform isn't a 2D affine produced by
    /// `Transform::scale` (the only shape `transform_width`
    /// and the cn::progress mapper emit).
    fn affine_scale(t: &blinc_core::Transform) -> (f32, f32) {
        match t {
            blinc_core::Transform::Affine2D(a) => (a.elements[0], a.elements[3]),
            blinc_core::Transform::Mat4(_) => {
                panic!("expected 2D affine scale, got Mat4")
            }
        }
    }

    #[test]
    fn div_transform_width_const_sets_scale_and_left_origin() {
        use crate::div::{ElementBuilder, div};
        let element = div().transform_width(0.75_f32);
        let props = element.render_props();
        assert_eq!(
            props.transform_origin,
            Some([0.0, 50.0]),
            "transform_width must pivot at the left edge"
        );
        let t = props.transform.expect("transform set");
        let (sx, sy) = affine_scale(&t);
        assert!((sx - 0.75).abs() < 1e-5, "scale_x must be 0.75, got {sx}");
        assert!((sy - 1.0).abs() < 1e-5, "scale_y must be 1.0, got {sy}");
    }

    #[test]
    fn div_transform_width_bound_fires_partial_update_on_set() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};

        let frac_state = fresh_state::<f32>(0.0);
        let element = div().transform_width(&frac_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        frac_state.set(0.5);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        let upd = updates.into_iter().next().unwrap();
        assert_eq!(upd.node_id, node_id);
        assert_eq!(upd.property, PropertyId::Transform);
        assert!(upd.render_write.is_some(), "must write to RenderProps");
        assert!(upd.layout_write.is_none(), "must NOT mutate taffy::Style");
        assert!(
            !upd.effects.needs_layout,
            "transform_width is GPU-only — must not request relayout"
        );

        // Apply the render write — render_props.transform should now be scale(0.5, 1.0).
        let mut props = RenderProps::default();
        (upd.render_write.unwrap())(&mut props);
        let t = props.transform.expect("transform written");
        let (sx, sy) = affine_scale(&t);
        assert!((sx - 0.5).abs() < 1e-5);
        assert!((sy - 1.0).abs() < 1e-5);
    }

    #[test]
    fn div_bind_transform_from_maps_arbitrary_signal_to_transform() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};
        use blinc_core::Transform;

        // State carries a 0..=100 progress value. Mapper produces a
        // scale-x transform matching what cn::progress's reactive
        // path does internally — exercises the same primitive.
        let value_state = fresh_state::<f32>(25.0);
        let element = div().bind_transform_from(value_state.clone(), |v: f32| {
            let f = (v / 100.0).clamp(0.0, 1.0);
            Transform::scale(f, 1.0)
        });
        // Seeded transform reflects the initial value (25/100 = 0.25 scale).
        let initial_props = element.render_props();
        let initial_t = initial_props.transform.expect("seeded transform");
        let (initial_sx, _) = affine_scale(&initial_t);
        assert!((initial_sx - 0.25).abs() < 1e-5);

        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        value_state.set(80.0);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        let upd = updates.into_iter().next().unwrap();
        assert_eq!(upd.node_id, node_id);
        assert_eq!(upd.property, PropertyId::Transform);

        let mut props = RenderProps::default();
        (upd.render_write.unwrap())(&mut props);
        let t = props.transform.expect("mapper applied");
        // 80 / 100 = 0.8
        let (sx, _) = affine_scale(&t);
        assert!((sx - 0.8).abs() < 1e-5, "got {sx}");
    }

    // ============================================================
    // Single-input transform sugars made reactive (P8.1 follow-up).
    // `.scale` / `.rotate` / `.rotate_deg` each take impl IntoReactive<f32>;
    // eager values stay zero-cost via the blanket `IntoReactive for T`,
    // bound values subscribe + patch props.transform per signal-set.
    // ============================================================

    #[test]
    fn div_scale_const_path_unchanged() {
        use crate::div::{ElementBuilder, div};
        let element = div().scale(0.5_f32);
        let t = element.render_props().transform.expect("transform set");
        let (sx, sy) = affine_scale(&t);
        assert!((sx - 0.5).abs() < 1e-5 && (sy - 0.5).abs() < 1e-5);
    }

    #[test]
    fn div_scale_bound_path_fires_partial_update_on_set() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};

        let s = fresh_state::<f32>(1.0);
        let element = div().scale(&s);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        s.set(2.0);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        let upd = updates.into_iter().next().unwrap();
        assert_eq!(upd.node_id, node_id);
        assert_eq!(upd.property, PropertyId::Transform);
        assert!(upd.layout_write.is_none(), "GPU-only path, no taffy write");

        let mut props = RenderProps::default();
        (upd.render_write.unwrap())(&mut props);
        let (sx, sy) = affine_scale(&props.transform.unwrap());
        // Uniform scale on both axes from a single f32 — matches the
        // legacy eager .scale(f) shape.
        assert!((sx - 2.0).abs() < 1e-5 && (sy - 2.0).abs() < 1e-5);
    }

    #[test]
    fn div_rotate_const_path_unchanged() {
        use crate::div::{ElementBuilder, div};
        // PI/4 (45°) — affine elements [cos, sin, -sin, cos, 0, 0].
        let element = div().rotate(std::f32::consts::FRAC_PI_4);
        let t = element.render_props().transform.expect("transform set");
        if let blinc_core::Transform::Affine2D(a) = t {
            let expected = std::f32::consts::FRAC_1_SQRT_2;
            assert!((a.elements[0] - expected).abs() < 1e-5);
            assert!((a.elements[1] - expected).abs() < 1e-5);
        } else {
            panic!("expected affine 2d");
        }
    }

    #[test]
    fn div_rotate_bound_path_fires_partial_update_on_set() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};

        let angle_state = fresh_state::<f32>(0.0);
        let element = div().rotate(&angle_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        // PI/2 (90°): cos=0, sin=1.
        angle_state.set(std::f32::consts::FRAC_PI_2);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1);
        let upd = updates.into_iter().next().unwrap();
        assert_eq!(upd.node_id, node_id);
        assert_eq!(upd.property, PropertyId::Transform);

        let mut props = RenderProps::default();
        (upd.render_write.unwrap())(&mut props);
        if let blinc_core::Transform::Affine2D(a) = props.transform.unwrap() {
            assert!(a.elements[0].abs() < 1e-5, "cos(90°) ~ 0");
            assert!((a.elements[1] - 1.0).abs() < 1e-5, "sin(90°) ~ 1");
        } else {
            panic!("expected affine 2d");
        }
    }

    #[test]
    fn div_rotate_deg_bound_path_converts_degrees_to_radians() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};

        let deg_state = fresh_state::<f32>(0.0);
        let element = div().rotate_deg(&deg_state);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        let _ = crate::stateful::take_pending_partial_prop_updates();
        deg_state.set(90.0);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        let upd = updates.into_iter().next().unwrap();
        assert_eq!(upd.node_id, node_id);

        let mut props = RenderProps::default();
        (upd.render_write.unwrap())(&mut props);
        if let blinc_core::Transform::Affine2D(a) = props.transform.unwrap() {
            // 90° should produce the same affine as π/2 rad.
            assert!(a.elements[0].abs() < 1e-5, "cos(90°) ~ 0");
            assert!((a.elements[1] - 1.0).abs() < 1e-5, "sin(90°) ~ 1");
        } else {
            panic!("expected affine 2d");
        }
    }

    // =====================================================================
    // Derived ↔ IntoReactive bridge (Phase 8 follow-up). The substrate:
    //   blinc_core::reactive::ReactiveGraph::set fires
    //   `notify_property_bindings_for_derived` for every derived that
    //   flipped dirty during the dirty-propagation walk; the binding
    //   registry's parallel `derived_bindings` map dispatches that into
    //   subscriber fires.
    // The end-to-end test: build a Div with a Computed-bound transform,
    // set a source signal, observe the partial update.
    // =====================================================================

    /// Shared-graph fresh State<T> — produces a State backed by a
    /// caller-supplied SharedReactiveGraph so multiple states + a
    /// derived can all coexist in the same graph.
    fn state_in_graph<T: Clone + Send + 'static>(
        graph: &Arc<Mutex<ReactiveGraph>>,
        initial: T,
    ) -> State<T> {
        let signal = graph.lock().unwrap().create_signal(initial);
        let dirty: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        State::new(signal, Arc::clone(graph), dirty)
    }

    #[test]
    fn computed_into_reactive_produces_computed_variant() {
        let _guard = lock_and_reset();
        with_registry(|_| {});
        let graph: Arc<Mutex<ReactiveGraph>> = Arc::new(Mutex::new(ReactiveGraph::new()));
        let x_signal = graph.lock().unwrap().create_signal(0.0f32);
        let derived = graph
            .lock()
            .unwrap()
            .create_derived(move |g| g.get(x_signal).unwrap_or(0.0));
        let computed = blinc_core::Computed::new(derived, Arc::clone(&graph));

        // The IntoReactive impl should resolve to Reactive::Computed.
        let r: Reactive<f32> = (&computed).into_reactive();
        assert!(matches!(r, Reactive::Computed(_)));
    }

    #[test]
    fn computed_bound_transform_fires_on_dep_signal_set() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};
        use blinc_core::Transform;

        with_registry(|_| {});
        let graph: Arc<Mutex<ReactiveGraph>> = Arc::new(Mutex::new(ReactiveGraph::new()));

        let x_state = state_in_graph::<f32>(&graph, 10.0);
        let y_state = state_in_graph::<f32>(&graph, 20.0);
        let x_signal = x_state.signal();
        let y_signal = y_state.signal();

        // Build a derived that depends on BOTH x and y. The first
        // `get_derived` call (inside .transform()'s initial-value
        // seed) records both signals as dependencies via the
        // auto-tracking system; future sets on either fire the
        // derived's dirty bit and the parallel binding notifier.
        let derived = graph
            .lock()
            .unwrap()
            .create_derived(move |g: &ReactiveGraph| {
                let x = g.get(x_signal).unwrap_or(0.0);
                let y = g.get(y_signal).unwrap_or(0.0);
                Transform::translate(x, y)
            });
        let computed = blinc_core::Computed::new(derived, Arc::clone(&graph));

        let element = div().transform(&computed);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);

        // Force initial dep-tracking by reading the computed's value.
        // Without this, the auto-tracking deps list is empty and the
        // first `x.set` won't propagate dirty to the derived.
        let _ = computed.try_get();

        let _ = crate::stateful::take_pending_partial_prop_updates();
        // Set x → derived flips dirty → notifier fires → binding
        // queues a partial update with the recomputed transform.
        x_state.set(100.0);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(
            updates.len(),
            1,
            "x.set must fire one partial update via the derived binding"
        );
        let upd = updates.into_iter().next().unwrap();
        assert_eq!(upd.node_id, node_id);
        assert_eq!(upd.property, PropertyId::Transform);
        assert!(upd.render_write.is_some());
        assert!(upd.layout_write.is_none());

        let mut props = RenderProps::default();
        (upd.render_write.unwrap())(&mut props);
        // Computed should have recomputed: x=100, y=20 → translate(100, 20)
        if let blinc_core::Transform::Affine2D(a) = props.transform.unwrap() {
            assert!(
                (a.elements[4] - 100.0).abs() < 1e-5,
                "tx must be 100, got {}",
                a.elements[4]
            );
            assert!(
                (a.elements[5] - 20.0).abs() < 1e-5,
                "ty must be 20, got {}",
                a.elements[5]
            );
        } else {
            panic!("expected 2d affine");
        }

        // Set y → derived flips dirty AGAIN → another partial update.
        let _ = crate::stateful::take_pending_partial_prop_updates();
        y_state.set(200.0);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert_eq!(updates.len(), 1, "y.set must also fire one partial update");
        let upd = updates.into_iter().next().unwrap();
        let mut props = RenderProps::default();
        (upd.render_write.unwrap())(&mut props);
        if let blinc_core::Transform::Affine2D(a) = props.transform.unwrap() {
            assert!((a.elements[4] - 100.0).abs() < 1e-5);
            assert!((a.elements[5] - 200.0).abs() < 1e-5);
        } else {
            panic!("expected 2d affine");
        }
    }

    #[test]
    fn computed_unregister_node_drops_derived_subscriber() {
        let _guard = lock_and_reset();
        use crate::div::{ElementBuilder, div};
        use blinc_core::Transform;

        with_registry(|_| {});
        let graph: Arc<Mutex<ReactiveGraph>> = Arc::new(Mutex::new(ReactiveGraph::new()));
        let x_state = state_in_graph::<f32>(&graph, 0.0);
        let x_signal = x_state.signal();
        let derived = graph
            .lock()
            .unwrap()
            .create_derived(move |g: &ReactiveGraph| {
                Transform::translate(g.get(x_signal).unwrap_or(0.0), 0.0)
            });
        let computed = blinc_core::Computed::new(derived, Arc::clone(&graph));
        let derived_id = computed.derived_id();

        let element = div().transform(&computed);
        let mut tree = crate::tree::LayoutTree::new();
        let node_id = element.build(&mut tree);
        let _ = computed.try_get(); // force dep tracking

        // One subscriber on the derived after build.
        with_registry(|reg| {
            assert_eq!(reg.derived_subscriber_count(derived_id), 1);
        });

        // Drop subscribers for the node.
        unregister_node(node_id);
        with_registry(|reg| {
            assert_eq!(
                reg.derived_subscriber_count(derived_id),
                0,
                "unregister_node must drop derived subscribers too"
            );
        });

        // Set x — derived dirty fires the notifier, but the registry
        // has no subscribers for this derived → no partial update
        // queued.
        let _ = crate::stateful::take_pending_partial_prop_updates();
        x_state.set(123.0);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        assert!(
            updates.is_empty(),
            "no subscribers → no partial updates after node unregister"
        );
    }
}
