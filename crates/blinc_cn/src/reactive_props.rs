//! Helpers for threading [`Reactive<T>`] values through cn widget
//! builders.
//!
//! cn widgets fall into two shapes, and the shape decides the
//! mechanism:
//!
//! * **Single visual property** (a colour, a radius, an opacity) —
//!   forward the `Reactive<T>` straight into the matching
//!   [`blinc_layout::div::Div`] setter, which registers a property
//!   binding. The signal patches `RenderProps` in place; no rebuild,
//!   no `compute_layout`. `cn::progress` takes this path.
//! * **Structural property** (`disabled`, `variant`, `checked`) — the
//!   value gates branches that pick different colours, borders,
//!   shadows, or FSM start states, so it can't be a single property
//!   write. Read the current value at build time and register the
//!   source signal via the widget's `Stateful::deps`, which rebuilds
//!   the subtree on change.
//!
//! [`current`] and [`dep_signal`] serve the second path: one reads the
//! build-time value, the other yields the [`SignalId`] to subscribe to.

use blinc_core::reactive::SignalId;
use blinc_layout::binding::Reactive;

/// Read the value a [`Reactive<T>`] holds right now.
///
/// Used by builders that branch on the value at build time. For
/// `Bound`/`Computed` this is the current signal read, which pairs with
/// a [`dep_signal`] subscription so the branch re-evaluates on change.
pub fn current<T>(r: &Reactive<T>) -> T
where
    T: Clone + Send + Default + 'static,
{
    match r {
        Reactive::Const(v) => v.clone(),
        Reactive::Bound(state) => state.try_get().unwrap_or_default(),
        Reactive::Computed(computed) => computed.try_get().unwrap_or_default(),
    }
}

/// The [`SignalId`] a structural prop should subscribe to, if any.
///
/// `Const` never changes, so it yields `None`. `Computed` is keyed by a
/// `DerivedId` rather than a `SignalId` and so can't drive `deps()`
/// directly; bind the underlying state instead, or use a property
/// binding where the shape allows one.
pub fn dep_signal<T: Clone + Send + 'static>(r: &Reactive<T>) -> Option<SignalId> {
    match r {
        Reactive::Bound(state) => Some(state.signal_id()),
        Reactive::Const(_) | Reactive::Computed(_) => None,
    }
}

/// Clone a [`Reactive<T>`] — the enum itself isn't `Clone` because it
/// carries type-erased handles, but every variant's payload is.
pub fn clone_reactive<T: Clone>(r: &Reactive<T>) -> Reactive<T> {
    match r {
        Reactive::Const(v) => Reactive::Const(v.clone()),
        Reactive::Bound(state) => Reactive::Bound(state.clone()),
        Reactive::Computed(computed) => Reactive::Computed(computed.clone()),
    }
}
