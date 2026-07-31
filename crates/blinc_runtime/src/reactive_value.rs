//! `Reactive<T>` — the first typed DSL value at the FFI boundary.
//!
//! The DSL has three reactive shapes that any consumer (cn widgets,
//! built-in Div / Text, user components, third-party widget packs)
//! can accept on a single prop:
//!
//! * **Literal** — `cn.Foo(prop = 0.5)` or `Div(w = 120.0)`. Static
//!   value baked into the call site.
//! * **Signal-bound** — `cn.Foo(prop = my_signal)`. Live-bound to a
//!   `signal foo: T` declaration; updates patch the property channel
//!   on `signal.set(...)`.
//! * **Computed** — `cn.Foo(prop = computed { … } : T)`. Live-bound
//!   to a derived value that auto-tracks the signals it reads inside
//!   the closure.
//!
//! `Reactive<T>` is the union the host extern thunk sees after
//! decoding the two-slot FFI shape (`tag: i32`, `payload: i64`)
//! synthesised by `lower_reactive_args` at the DSL call site. The
//! wrapper consumes the typed enum and routes it to the cn-side
//! `IntoReactive<T>` channel, or any other binding-capable surface.
//!
//! ## Layering
//!
//! Lives in `blinc_runtime` because it's shared between JIT
//! (`blinc_dsl_core`) and the AOT path (future `blinc_dsl_aot`).
//! Anything `blinc_core::reactive`-aware can use it; widget packs
//! don't need a parallel definition.
//!
//! ## What's NOT here (yet)
//!
//! * Multi-typed dispatch via a single Reactive over `Any` — out of
//!   scope. The macro generates per-`T` decoders.
//! * Effect-style props — `effect { … }` stays a side-effect-only
//!   surface (no value channel); the existing
//!   `__blinc_effect__` host extern handles it.
//!
//! ## String shape note
//!
//! `Reactive<String>` doesn't use the standard two-slot
//! `(tag, payload: i64)` wire format because a string literal can't
//! fit in an `i64`. The macro generates three slots instead:
//! `(tag, id_payload: i64, literal_ptr: *const i32)`. The
//! `from_signal_id` / `from_computed_id` / `from_literal`
//! constructors below let the macro assemble the typed variant
//! without `Reactive<String>` needing to call into the ZRTL
//! string-decoder itself (that helper lives in `blinc_dsl_core` and
//! `blinc_runtime` doesn't depend on it).

use blinc_core::reactive::{Computed, Signal, SignalId};

// Tag values used at the wire-format FFI boundary. The lowering
// pass writes one of these into the `tag` slot per Reactive prop;
// the host extern thunk pattern-matches on the value to pick the
// payload interpretation.
//
// Sentinel choices are deliberate:
// * `0 = Literal` — matches the macro's `Default::default()` for
//   any `i32` field, so an un-supplied Reactive prop falls through
//   as a literal default without special handling.
// * `1` and `2` follow naturally; new tags appended as needed.
pub const REACTIVE_TAG_LITERAL: i32 = 0;
pub const REACTIVE_TAG_SIGNAL: i32 = 1;
pub const REACTIVE_TAG_COMPUTED: i32 = 2;

/// Typed reactive value the host extern thunk decodes for any DSL
/// arg whose prop slot was declared `Reactive<T>` on the Rust side.
///
/// Pattern-match on this in widget wrappers; the cn-side
/// `IntoReactive<T>` channel accepts every variant via the matching
/// adapter (the wrapper's `to_cn_builder` glue).
///
/// `Clone` is derived; `Debug` is skipped because `Computed<T>`
/// holds an `Arc<Mutex<ReactiveGraph>>` that doesn't impl Debug.
/// Callers debug-print the literal value or the underlying ids
/// directly via the per-variant accessors.
#[derive(Clone)]
pub enum Reactive<T: Clone + Send + 'static> {
    Literal(T),
    Signal(Signal<T>),
    Computed(Computed<T>),
}

impl<T: Clone + Send + 'static> Reactive<T> {
    /// Lookup the current value if the binding resolves. Falls back
    /// to the supplied default for unresolvable signal / derived
    /// handles (the graph has been reset, the slotmap key was
    /// reclaimed, etc.). Useful for widgets that want a synchronous
    /// snapshot at build time rather than the cn-side
    /// `IntoReactive<T>` channel.
    pub fn get_or_else(&self, fallback: T) -> T {
        match self {
            Self::Literal(v) => v.clone(),
            Self::Signal(s) => s.try_get().unwrap_or(fallback),
            Self::Computed(c) => c.try_get().unwrap_or(fallback),
        }
    }
}

// =====================================================================
// FFI decoders — one per concrete `T`.
// =====================================================================
//
// Wire format: a `Reactive<T>` prop lands at the FFI thunk as two
// scalar slots, `tag: i32` and `payload: i64`. The tag picks the
// payload interpretation; the per-`T` decoder reconstructs the
// typed enum.
//
// For literals:
// * `i32`  — payload's low 32 bits.
// * `f64`  — `f64::from_bits(payload as u64)`.
// * `bool` — `payload != 0`.

impl Reactive<i32> {
    pub fn decode_ffi(tag: i32, payload: i64) -> Self {
        match tag {
            REACTIVE_TAG_SIGNAL => {
                Self::Signal(Signal::from_id(SignalId::from_raw(payload as u64)))
            }
            REACTIVE_TAG_COMPUTED => Self::Computed(Computed::from_id(
                blinc_core::reactive::DerivedId::from_raw(payload as u64),
            )),
            _ => Self::Literal(payload as i32),
        }
    }
}

impl Reactive<f64> {
    pub fn decode_ffi(tag: i32, payload: i64) -> Self {
        match tag {
            REACTIVE_TAG_SIGNAL => {
                Self::Signal(Signal::from_id(SignalId::from_raw(payload as u64)))
            }
            REACTIVE_TAG_COMPUTED => Self::Computed(Computed::from_id(
                blinc_core::reactive::DerivedId::from_raw(payload as u64),
            )),
            _ => Self::Literal(f64::from_bits(payload as u64)),
        }
    }
}

impl Reactive<bool> {
    pub fn decode_ffi(tag: i32, payload: i64) -> Self {
        match tag {
            REACTIVE_TAG_SIGNAL => {
                Self::Signal(Signal::from_id(SignalId::from_raw(payload as u64)))
            }
            REACTIVE_TAG_COMPUTED => Self::Computed(Computed::from_id(
                blinc_core::reactive::DerivedId::from_raw(payload as u64),
            )),
            _ => Self::Literal(payload != 0),
        }
    }
}

impl Reactive<String> {
    /// Build a signal-bound variant from the raw `SignalId.to_raw()`
    /// payload the lowering pass writes into the `id_payload` slot.
    /// The macro calls this when `tag == REACTIVE_TAG_SIGNAL`.
    pub fn from_signal_id(id_raw: u64) -> Self {
        Self::Signal(Signal::from_id(SignalId::from_raw(id_raw)))
    }

    /// Computed mirror of `from_signal_id`.
    pub fn from_computed_id(id_raw: u64) -> Self {
        Self::Computed(Computed::from_id(
            blinc_core::reactive::DerivedId::from_raw(id_raw),
        ))
    }

    /// Wrap a pre-decoded `String` literal as `Reactive::Literal`.
    /// The macro calls this for `tag == REACTIVE_TAG_LITERAL` after
    /// running the ZRTL string-decoder on the literal-payload pointer.
    pub fn from_literal(value: String) -> Self {
        Self::Literal(value)
    }
}

// =====================================================================
// Default impl — required by the `#[extern_widget]` macro's
// `#[skip]` field handling: any field excluded from the FFI gets
// `Default::default()` in the generated constructor. Reactive<T>
// for primitive T defaults to `Literal(T::default())` so the macro
// can synthesize an unsupplied reactive prop slot without panic.
// =====================================================================

impl<T: Clone + Send + Default + 'static> Default for Reactive<T> {
    fn default() -> Self {
        Self::Literal(T::default())
    }
}

// Bridge the DSL's `Reactive<T>` onto the cn/layout builder channel.
//
// The two enums carry the same three cases with different payloads:
// this one holds a raw `Signal<T>`, the layout one holds a `State<T>`
// (a signal plus the graph and dirty flag it belongs to). `Signal<T>`
// and `Computed<T>` already have `IntoReactive` impls that supply the
// process-global graph, so each arm just delegates.
//
// With this in place a `#[reactive] Reactive<T>` prop can be handed
// straight to a cn builder that takes `impl IntoReactive<T>`, and the
// binding stays live instead of snapshotting at build time.
impl<T: Clone + Send + 'static> blinc_layout::binding::IntoReactive<T> for Reactive<T> {
    fn into_reactive(self) -> blinc_layout::binding::Reactive<T> {
        match self {
            Self::Literal(v) => blinc_layout::binding::Reactive::Const(v),
            Self::Signal(s) => s.into_reactive(),
            Self::Computed(c) => c.into_reactive(),
        }
    }
}

// `Reactive<f64>` -> an f32-backed layout property.
//
// The DSL types every number as f64, its only float, while layout
// stores f32. This mirrors the f64 source impls in
// `blinc_layout::binding`: literals cast once, and signal / computed
// shapes narrow per read so the binding stays live instead of freezing
// at its build-time value.
//
// Without this a DSL wrapper would have to snapshot the value to hand
// it to a cn builder, which silently drops reactivity.
impl blinc_layout::binding::IntoReactive<f32> for Reactive<f64> {
    fn into_reactive(self) -> blinc_layout::binding::Reactive<f32> {
        match self {
            Self::Literal(v) => blinc_layout::binding::Reactive::Const(v as f32),
            Self::Signal(s) => s.into_reactive(),
            Self::Computed(c) => c.into_reactive(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_i32_roundtrip() {
        let r = Reactive::<i32>::decode_ffi(REACTIVE_TAG_LITERAL, 42);
        assert!(matches!(r, Reactive::Literal(42)));
    }

    #[test]
    fn literal_f64_roundtrip() {
        let payload = f64::to_bits(2.5) as i64;
        let r = Reactive::<f64>::decode_ffi(REACTIVE_TAG_LITERAL, payload);
        let Reactive::Literal(v) = r else {
            panic!("expected Literal");
        };
        assert!((v - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn literal_bool_roundtrip() {
        assert!(matches!(
            Reactive::<bool>::decode_ffi(REACTIVE_TAG_LITERAL, 0),
            Reactive::Literal(false)
        ));
        assert!(matches!(
            Reactive::<bool>::decode_ffi(REACTIVE_TAG_LITERAL, 1),
            Reactive::Literal(true)
        ));
    }

    #[test]
    fn unknown_tag_falls_back_to_literal() {
        // Defensive: a future tag value reaching an older binary
        // shouldn't crash — it lands in the Literal branch with
        // whatever the payload happened to be.
        let r = Reactive::<i32>::decode_ffi(99, 7);
        assert!(matches!(r, Reactive::Literal(7)));
    }

    #[test]
    fn string_literal_constructor() {
        let r = Reactive::<String>::from_literal("hi".to_string());
        let Reactive::Literal(s) = r else {
            panic!("expected Literal");
        };
        assert_eq!(s, "hi");
    }

    #[test]
    fn string_signal_constructor_roundtrips_id() {
        // String signals follow the same global-graph + raw-id
        // round-trip the scalar tests cover; build a fresh signal,
        // pass its id through `from_signal_id`, confirm the rehydrated
        // handle reads the seeded value.
        let s = blinc_core::reactive::signal::<String>("hello".to_string());
        let raw = s.id().to_raw();
        let r = Reactive::<String>::from_signal_id(raw);
        let Reactive::Signal(sig) = r else {
            panic!("expected Signal");
        };
        assert_eq!(sig.id(), s.id());
        assert_eq!(sig.try_get().as_deref(), Some("hello"));
    }

    #[test]
    fn signal_tag_constructs_handle() {
        // Build a fresh signal, get its id, decode it back through
        // the wire format, confirm the round-trip preserves the id.
        let s = blinc_core::reactive::signal(123_i32);
        let raw = s.id().to_raw() as i64;
        let r = Reactive::<i32>::decode_ffi(REACTIVE_TAG_SIGNAL, raw);
        let Reactive::Signal(sig) = r else {
            panic!("expected Signal");
        };
        assert_eq!(sig.id(), s.id());
        // Sanity: the reconstructed handle can read the current value.
        assert_eq!(sig.try_get(), Some(123));
    }
}
