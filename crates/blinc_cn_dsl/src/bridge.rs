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
