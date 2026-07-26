//! Helpers for threading DSL reactive props into cn builders.
//!
//! Numeric narrowing is *not* here: the layout setters are generic
//! over [`blinc_layout::binding::IntoF32`], so a DSL `Reactive<f64>`
//! reaches an f32-backed property directly and the cast happens once,
//! inside the binding layer. Call sites need a `::<f64>` turbofish
//! only because `Reactive<T>` satisfies both `IntoReactive<T>` and the
//! blanket `IntoReactive<Reactive<T>>`, which leaves `N` ambiguous.

use blinc_dsl_core::Reactive;

/// Whether a reactive dimension should be applied at all.
///
/// Several cn wrappers treat `0.0` as "unset" so the widget's own
/// default survives. That sentinel only makes sense for a literal: a
/// bound value that happens to read `0.0` on the first frame is still
/// a live binding and must be wired up, or it would never update.
pub fn is_set(r: &Reactive<f64>) -> bool {
    match r {
        Reactive::Literal(v) => *v > 0.0,
        Reactive::Signal(_) | Reactive::Computed(_) => true,
    }
}

/// A `State<bool>` for cn widgets that own a toggle (`cn::switch`,
/// `cn::checkbox` take `&State<bool>`).
///
/// A DSL `signal`-bound prop maps onto the very same signal, so the
/// widget and the DSL share one source of truth: toggling the widget is
/// visible to the DSL and vice versa. A literal has no signal behind it,
/// so one is minted to hold the initial value -- the widget stays
/// interactive, the DSL just has no handle on it.
pub fn bool_state(r: &Reactive<bool>) -> blinc_core::reactive::State<bool> {
    use blinc_core::reactive::{Signal, State, global_dirty_flag, global_graph, signal};
    let sig: Signal<bool> = match r {
        Reactive::Signal(s) => *s,
        Reactive::Literal(v) => signal::<bool>(*v),
        // A computed is derived, so it has no signal to write back to.
        // Seed a fresh one from its current value; the widget owns it
        // from then on.
        Reactive::Computed(c) => signal::<bool>(c.try_get().unwrap_or(false)),
    };
    State::new(sig, global_graph(), global_dirty_flag())
}
