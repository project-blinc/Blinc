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
    // `with_stateful_callback`, not `new`: `State::set` notifies ONLY
    // through this per-instance callback -- unlike `Signal::set`, it
    // never reaches the global stateful-deps notifier. A widget that
    // owns its State (switch, checkbox) writes through `State::set`, so
    // without this a toggle updated the value and told nobody: no
    // stateful refreshed, nothing queued a rebuild, and the frame only
    // repainted when some unrelated event forced one.
    State::with_stateful_callback(
        sig,
        global_graph(),
        global_dirty_flag(),
        std::sync::Arc::new(|ids: &[blinc_core::reactive::SignalId]| {
            blinc_layout::check_stateful_deps(ids);
        }),
    )
}

/// Per-key `SharedTextInputData`, so a DSL text field keeps what the
/// user typed across rebuilds.
///
/// `cn::input` takes external state that "persists across rebuilds" --
/// but a DSL wrapper is reconstructed every render, so calling
/// `text_input_data()` inline would hand the widget a fresh, empty
/// buffer each time and typing would vanish. Extern widgets carry no
/// call-site identity (unlike cn's `#[track_caller]` `InstanceKey`), so
/// the key has to come from the DSL author.
///
/// An empty key returns a detached buffer: the field still works within
/// one render, but its contents do not survive a rebuild.
pub fn text_input_data_keyed(key: &str) -> blinc_layout::widgets::text_input::SharedTextInputData {
    use blinc_layout::widgets::text_input::{SharedTextInputData, text_input_data};
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    if key.is_empty() {
        return text_input_data();
    }
    static STORE: OnceLock<Mutex<HashMap<String, SharedTextInputData>>> = OnceLock::new();
    let store = STORE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = store.lock().expect("text input store poisoned");
    guard
        .entry(key.to_string())
        .or_insert_with(text_input_data)
        .clone()
}

/// Per-key `SharedTextAreaState`. Same rationale as
/// [`text_input_data_keyed`]: the textarea keeps its contents in state
/// that must outlive a rebuild, and a DSL wrapper is reconstructed
/// every render.
pub fn text_area_state_keyed(key: &str) -> blinc_layout::widgets::text_area::SharedTextAreaState {
    use blinc_layout::widgets::text_area::{SharedTextAreaState, text_area_state};
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    if key.is_empty() {
        return text_area_state();
    }
    static STORE: OnceLock<Mutex<HashMap<String, SharedTextAreaState>>> = OnceLock::new();
    let store = STORE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = store.lock().expect("textarea store poisoned");
    guard
        .entry(key.to_string())
        .or_insert_with(text_area_state)
        .clone()
}

/// The DSL call-site id, captured when the widget struct is built.
///
/// `current_call_id()` only reads correctly *inside* the
/// `__push_call_id__` / `__pop_call_id__` bracket the lowering emits
/// around each call — i.e. while the FFI constructs the struct. A
/// widget's `to_cn_widget()` runs lazily at first build, long after the
/// bracket popped, so reading it there yields 0 and every instance
/// collides. Capturing through `Default` pins it at the right moment.
#[derive(Clone, Copy)]
pub struct CallSiteId(pub u64);

impl Default for CallSiteId {
    fn default() -> Self {
        Self(blinc_dsl_core::current_call_id())
    }
}

/// `SharedTextInputData` for a text field, keyed by whichever identity
/// the field actually has.
///
/// A bound signal is the real identity: the field belongs to that
/// signal, two fields bound to it share a buffer, and the DSL can read
/// what was typed. Unbound, the call site is the identity — distinct
/// `cn.Input(...)` calls get distinct buffers without the author naming
/// anything.
pub fn text_input_data_for(
    value: &Reactive<String>,
    call_site: CallSiteId,
) -> blinc_layout::widgets::text_input::SharedTextInputData {
    let key = match value {
        Reactive::Signal(s) => format!("sig:{:?}", s.id()),
        _ => format!("call:{}", call_site.0),
    };
    let data = text_input_data_keyed(&key);
    // Seed once from the bound value so the field opens showing it.
    if let Reactive::Signal(s) = value
        && let Some(current) = s.try_get()
        && data.lock().map(|d| d.value.is_empty()).unwrap_or(false)
        && !current.is_empty()
        && let Ok(mut d) = data.lock()
    {
        d.value = current;
    }
    data
}
