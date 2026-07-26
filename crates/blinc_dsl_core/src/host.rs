// Transitional legacy op stream. `$Blinc$text` / `$Blinc$text_int` push to a
// per-thread scene buffer drained by `render_view` / `render_component`. Goes
// away once all primitives are value-returning widget constructors.

use std::cell::RefCell;

/// One declarative draw op emitted by the DSL during `render_view`. Legacy path.
#[derive(Debug, Clone, PartialEq)]
pub enum DslOp {
    Text(String),
    IntText(i32),
}

thread_local! {
    static SCENE_BUFFER: RefCell<Vec<DslOp>> = const { RefCell::new(Vec::new()) };
}

fn push_op(op: DslOp) {
    SCENE_BUFFER.with(|b| b.borrow_mut().push(op));
}

/// Drain and return everything pushed onto the scene buffer since the last call.
pub fn take_scene_ops() -> Vec<DslOp> {
    SCENE_BUFFER.with(|b| std::mem::take(&mut *b.borrow_mut()))
}

// =====================================================================
// Builtins
// =====================================================================

/// `$Blinc$text` — pushes a string literal onto the scene buffer.
///
/// # Safety
///
/// Called by Zyntax's JIT via [`ZyntaxRuntime::register_function`]; `s_ptr`
/// points at a `ZyntaxString` (`[i32 len][utf8 bytes…]`).
pub(crate) extern "C" fn blinc_text(s_ptr: *const i32) {
    if s_ptr.is_null() {
        tracing::warn!("$Blinc$text called with null pointer");
        return;
    }

    // SAFETY: runtime guarantees length-prefixed UTF-8 layout for `Ptr` string args.
    let raw = unsafe {
        let len = std::ptr::read_unaligned(s_ptr) as usize;
        let body = (s_ptr as *const u8).add(std::mem::size_of::<i32>());
        let bytes = std::slice::from_raw_parts(body, len);
        std::str::from_utf8(bytes).unwrap_or("<invalid utf-8>")
    };

    // Grammar's `string_literal` preserves surrounding quotes; strip them.
    let stripped = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);

    push_op(DslOp::Text(stripped.to_string()));
}

/// Reconstruct a typed `Signal<T>` from a raw `SignalId.to_raw()`
/// integer (i64 over the wire — Cranelift doesn't carry u64
/// constants through its value-map; see commit 54dc831b's notes for
/// why the DSL bakes ids as i64 even though the underlying type is
/// u64). Used by every `__signal_*_by_id_*` extern.
fn reconstruct_signal<T>(id_raw: i64) -> blinc_core::reactive::Signal<T> {
    let id = blinc_core::reactive::SignalId::from_raw(id_raw as u64);
    blinc_core::reactive::Signal::<T>::from_id(id)
}

/// `__signal_get_by_id_i32(id_raw)` — read an i32 signal by its
/// process-global `SignalId.to_raw()`. The DSL lowering pass
/// (`resolve_signal_calls`) bakes the id into the JIT code at compile
/// time, so this extern is the canonical reactive-read path — no name
/// lookup, no parallel storage. Returns `0` if the id no longer
/// resolves in the graph (graph reset between tests, etc.).
pub(crate) extern "C" fn blinc_signal_get_by_id_i32(id_raw: i64) -> i32 {
    reconstruct_signal::<i32>(id_raw).try_get().unwrap_or(0)
}

/// `__signal_get_by_id_i64(id_raw)` — i64 mirror. Distinct from the
/// i32 form: `signal n: i64` creates a `Signal<i64>`, and
/// `reconstruct_signal::<i32>` on that id would read the wrong slot
/// type.
pub(crate) extern "C" fn blinc_signal_get_by_id_i64(id_raw: i64) -> i64 {
    reconstruct_signal::<i64>(id_raw).try_get().unwrap_or(0)
}

/// `__signal_get_by_id_f64(id_raw)` — f64 mirror.
pub(crate) extern "C" fn blinc_signal_get_by_id_f64(id_raw: i64) -> f64 {
    reconstruct_signal::<f64>(id_raw).try_get().unwrap_or(0.0)
}

/// `__signal_get_by_id_string(id_raw)` — string mirror. Returns a
/// Zyntax length-prefixed pointer; the buffer leaks via
/// `blinc_string_alloc`.
pub(crate) extern "C" fn blinc_signal_get_by_id_string(id_raw: i64) -> *const i32 {
    let value = reconstruct_signal::<String>(id_raw)
        .try_get()
        .unwrap_or_default();
    blinc_string_alloc(&value)
}

/// `__signal_get_by_id_bool(id_raw) -> i32` — bool mirror. Returns
/// `1` for true / `0` for false / `0` when the id no longer
/// resolves. Wire type is `i32` because Zyntax's Cranelift backend
/// doesn't have a dedicated bool calling convention — the lowering
/// pass treats DSL `bool` as `i32` everywhere a value flows across
/// the FFI seam.
pub(crate) extern "C" fn blinc_signal_get_by_id_bool(id_raw: i64) -> i32 {
    if reconstruct_signal::<bool>(id_raw)
        .try_get()
        .unwrap_or(false)
    {
        1
    } else {
        0
    }
}

/// Decode a length-prefixed Zyntax string pointer to a `&str`.
fn decode_signal_name<'a>(name_ptr: *const i32) -> Option<&'a str> {
    if name_ptr.is_null() {
        return None;
    }
    // SAFETY: length-prefixed UTF-8 layout per Zyntax String param ABI.
    let raw = unsafe {
        let len = std::ptr::read_unaligned(name_ptr) as usize;
        let body = (name_ptr as *const u8).add(std::mem::size_of::<i32>());
        let bytes = std::slice::from_raw_parts(body, len);
        std::str::from_utf8(bytes).ok()?
    };
    Some(
        raw.strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(raw),
    )
}

/// `__signal_set_by_id_i32(id_raw, value)` — i32 signal write side.
/// Calls `Signal::<i32>::set(value)` directly on the reactive primitive
/// — that fires the property-binding registry the same way native
/// Rust `.set()` does, so any `.bg(&signal)` binding repaints.
pub(crate) extern "C" fn blinc_signal_set_by_id_i32(id_raw: i64, value: i32) {
    reconstruct_signal::<i32>(id_raw).set(value);
}

/// `__signal_set_by_id_i64(id_raw, value)` — i64 write side.
pub(crate) extern "C" fn blinc_signal_set_by_id_i64(id_raw: i64, value: i64) {
    reconstruct_signal::<i64>(id_raw).set(value);
}

/// `__signal_set_by_id_f64(id_raw, value)` — f64 mirror.
pub(crate) extern "C" fn blinc_signal_set_by_id_f64(id_raw: i64, value: f64) {
    reconstruct_signal::<f64>(id_raw).set(value);
}

/// `__signal_set_by_id_string(id_raw, value_ptr)` — string mirror.
pub(crate) extern "C" fn blinc_signal_set_by_id_string(id_raw: i64, value_ptr: *const i32) {
    let value = decode_signal_name(value_ptr).unwrap_or("");
    reconstruct_signal::<String>(id_raw).set(value.to_string());
}

/// `__signal_set_by_id_bool(id_raw, value: i32)` — bool mirror. The
/// JIT passes the bool as `i32` (0/1) per the same Zyntax calling-
/// convention quirk noted on the getter.
pub(crate) extern "C" fn blinc_signal_set_by_id_bool(id_raw: i64, value: i32) {
    reconstruct_signal::<bool>(id_raw).set(value != 0);
}

/// `__fsm_runtime_trigger__("<FsmName>", "<path>")` — dispatch a transition
/// on the default instance. Two shapes for the second arg:
///
///   * `"<State>.<Event>"` — guard form. Only fires when the FSM's
///     current state matches `<State>`. Useful for state-conditional
///     logic where the caller wants the dispatch to be a no-op unless
///     the FSM is in a specific state.
///   * `"<Event>"` (no `.`) — bare event form. Dispatches the event
///     against whatever the FSM's current state is; the registry's
///     own transition table picks the matching rule. Use this when
///     the same event can fire from multiple source states (e.g.
///     `Increment` valid in both `Idle` and `Counting`) and you
///     don't want to spell every prefix.
///
/// Bare-event was added because the state-prefix form double-fires
/// when you list every prefix to cover multiple sources: `trigger("Idle.X")`
/// advances state to Counting first, then `trigger("Counting.X")` matches
/// the new state and runs again on the same click. With bare-event the
/// runtime handles the dispatch table walk in one call.
pub(crate) extern "C" fn blinc_fsm_runtime_trigger(fsm_ptr: *const i32, path_ptr: *const i32) {
    let Some(fsm) = decode_signal_name(fsm_ptr) else {
        tracing::warn!("__fsm_runtime_trigger__ called with null fsm pointer");
        return;
    };
    let Some(path) = decode_signal_name(path_ptr) else {
        tracing::warn!("__fsm_runtime_trigger__ called with null path pointer");
        return;
    };
    if let Some((state, event)) = path.split_once('.') {
        let state = state.trim();
        let event = event.trim();
        let current = blinc_runtime::fsm::current_state_name(fsm);
        let matches_precondition = current.as_deref().map(|c| c == state).unwrap_or(false);
        if !matches_precondition {
            return;
        }
        blinc_runtime::fsm::dispatch_default(fsm, event);
    } else {
        // Bare event — dispatch unconditionally; the registry's
        // `step_event` looks up by (current_state, event) and the
        // first matching rule wins.
        blinc_runtime::fsm::dispatch_default(fsm, path.trim());
    }
}

/// `__blinc_computed_i32__(closure_ptr) -> i64` — create a value-
/// returning reactive derived against the process-global graph. The
/// DSL grammar's `computed { expr } : i32` rule wraps the body in a
/// zero-arg lambda; Zyntax lowers that to an
/// `extern "C" fn() -> i32` ptr. We transmute, hand the closure to
/// `blinc_core::reactive::computed(|_g| f())`, and return
/// `Computed::derived_id().to_raw() as i64`. Callers bind the
/// returned id to a DSL local; later passes consume the id as a
/// reactive prop-binding source.
///
/// # Safety
///
/// `closure_ptr` must remain valid for the lifetime of the
/// `ZyntaxRuntime`.
pub(crate) extern "C" fn blinc_dsl_computed_i32(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        tracing::warn!("__blinc_computed_i32__ called with null closure pointer");
        return 0;
    }
    type ComputedFn = extern "C" fn() -> i32;
    let func: ComputedFn = unsafe { std::mem::transmute(closure_ptr) };
    let computed = blinc_core::reactive::computed::<i32, _>(move |_graph| func());
    computed.derived_id().to_raw() as i64
}

/// bool mirror. The DSL `computed { … } : bool` wraps the body in a
/// zero-arg lambda whose return type Zyntax lowers as `i32` (same
/// 0/1 wire convention the signal getters use). We reconstruct a
/// `Computed<bool>` so downstream `Reactive::<bool>::from_computed_id`
/// rehydrates a typed handle.
pub(crate) extern "C" fn blinc_dsl_computed_bool(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        tracing::warn!("__blinc_computed_bool__ called with null closure pointer");
        return 0;
    }
    type ComputedFn = extern "C" fn() -> i32;
    let func: ComputedFn = unsafe { std::mem::transmute(closure_ptr) };
    let computed = blinc_core::reactive::computed::<bool, _>(move |_graph| func() != 0);
    computed.derived_id().to_raw() as i64
}

/// f64 mirror.
pub(crate) extern "C" fn blinc_dsl_computed_f64(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        tracing::warn!("__blinc_computed_f64__ called with null closure pointer");
        return 0;
    }
    type ComputedFn = extern "C" fn() -> f64;
    let func: ComputedFn = unsafe { std::mem::transmute(closure_ptr) };
    let computed = blinc_core::reactive::computed::<f64, _>(move |_graph| func());
    computed.derived_id().to_raw() as i64
}

/// String mirror. The closure body returns a Zyntax length-prefixed
/// string pointer; we decode it inside the wrapper closure so each
/// reactive re-evaluation produces a fresh owned `String` that the
/// derived caches.
pub(crate) extern "C" fn blinc_dsl_computed_string(closure_ptr: i64) -> i64 {
    if closure_ptr == 0 {
        tracing::warn!("__blinc_computed_string__ called with null closure pointer");
        return 0;
    }
    type ComputedFn = extern "C" fn() -> *const i32;
    let func: ComputedFn = unsafe { std::mem::transmute(closure_ptr) };
    let computed = blinc_core::reactive::computed::<String, _>(move |_graph| {
        let ptr = func();
        // SAFETY: closure body produces a length-prefixed string via
        // `blinc_string_alloc` (the Zyntax string-return ABI).
        unsafe { blinc_string_decode(ptr).to_string() }
    });
    computed.derived_id().to_raw() as i64
}

/// `__blinc_effect__(closure_ptr)` — register a reactive side-effect
/// against the process-global graph.
///
/// The DSL grammar's `effect_stmt` rule wraps the user's body in a
/// zero-arg lambda; Zyntax's compiler lowers the lambda to an
/// `extern "C" fn()` pointer and passes it as `closure_ptr`. We
/// transmute the pointer back to a callable fn type and hand it to
/// `blinc_core::reactive::effect(...)` — which auto-tracks every
/// signal read inside the closure body the same way native Rust
/// effects do.
///
/// # Safety
///
/// `closure_ptr` must remain valid for the lifetime of the
/// `ZyntaxRuntime` (same contract as `__fsm_subscribe__`).
pub(crate) extern "C" fn blinc_dsl_effect(closure_ptr: i64) {
    if closure_ptr == 0 {
        tracing::warn!("__blinc_effect__ called with null closure pointer");
        return;
    }
    type EffectFn = extern "C" fn();
    let func: EffectFn = unsafe { std::mem::transmute(closure_ptr) };
    blinc_core::reactive::effect(move |_graph| func());
}

/// `__fsm_subscribe__("<FsmName>", "<From.Event>", closure_ptr)` — registers a
/// path-filtered subscriber closure for the FSM's default-instance transitions.
///
/// # Safety
///
/// `closure_ptr` must remain valid for the lifetime of the `ZyntaxRuntime`.
pub(crate) extern "C" fn blinc_fsm_subscribe(
    fsm_ptr: *const i32,
    path_ptr: *const i32,
    closure_ptr: i64,
) {
    let Some(fsm) = decode_signal_name(fsm_ptr) else {
        tracing::warn!("__fsm_subscribe__ called with null fsm pointer");
        return;
    };
    let Some(path) = decode_signal_name(path_ptr) else {
        tracing::warn!("__fsm_subscribe__ called with null path pointer");
        return;
    };
    if closure_ptr == 0 {
        tracing::warn!("__fsm_subscribe__ called with null closure pointer");
        return;
    }
    blinc_runtime::fsm::register_subscriber(fsm, path, move || {
        // SAFETY: SSA lowering produces an `extern "C" fn()` lambda body.
        type SubscriberFn = extern "C" fn();
        let func: SubscriberFn = unsafe { std::mem::transmute(closure_ptr) };
        func();
    });
}

/// `__fsm_subscribe_all__("<FsmName>", closure_ptr)` — registers an
/// all-paths subscriber. The closure is the one-arg form
/// (`|state| { … }`) whose body receives the matched
/// `"From.Event"` path as a ZRTL-string pointer. Lowered from a
/// single-arg `<Fsm>.subscribe(closure)` at the DSL level.
///
/// # Safety
///
/// `closure_ptr` must remain valid for the lifetime of the
/// `ZyntaxRuntime`. The closure ABI is
/// `extern "C" fn(*const i32)` — string-ptr in, no return — matching
/// Zyntax's one-arg `CreateClosure` shape for closures whose only
/// param is a `Ptr<I8>`-typed string.
pub(crate) extern "C" fn blinc_fsm_subscribe_all(fsm_ptr: *const i32, closure_ptr: i64) {
    let Some(fsm) = decode_signal_name(fsm_ptr) else {
        tracing::warn!("__fsm_subscribe_all__ called with null fsm pointer");
        return;
    };
    if closure_ptr == 0 {
        tracing::warn!("__fsm_subscribe_all__ called with null closure pointer");
        return;
    }
    blinc_runtime::fsm::register_subscriber_all(fsm, move |path: &str| {
        // SAFETY: SSA lowering produces an `extern "C" fn(*const i32)`
        // lambda body for one-arg string-typed lambdas. Build a ZRTL
        // string (`[i32 length][utf8_bytes...]`) for the path and
        // hand it to the closure.
        type SubscriberFn = extern "C" fn(*const i32);
        let func: SubscriberFn = unsafe { std::mem::transmute(closure_ptr) };

        let bytes = path.as_bytes();
        let len = bytes.len() as i32;
        let total = 4 + bytes.len();
        let layout =
            std::alloc::Layout::from_size_align(total, 4).expect("ZRTL string layout for fsm path");
        // SAFETY: layout is non-zero (4 + len ≥ 4) and 4-aligned.
        let raw = unsafe { std::alloc::alloc(layout) };
        if raw.is_null() {
            tracing::warn!("__fsm_subscribe_all__ failed to allocate path buffer");
            return;
        }
        // SAFETY: `raw` is freshly allocated with at least `total` bytes;
        // the i32 length header lives in the first 4 bytes, the utf8
        // payload starts at offset 4. Writes are within bounds.
        unsafe {
            std::ptr::copy_nonoverlapping(len.to_le_bytes().as_ptr(), raw, 4);
            if !bytes.is_empty() {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), raw.add(4), bytes.len());
            }
        }
        func(raw as *const i32);
        // SAFETY: same layout used for the allocation; freeing after
        // the closure returns. The closure must not retain the
        // pointer past its own scope — match arms compare against
        // string literals, which Zyntax's `zrtl_string_equals`
        // resolves by reading the bytes, not by holding the ptr.
        unsafe { std::alloc::dealloc(raw, layout) };
    });
}

/// `$Blinc$text_int` — integer arm of `text(...)`. Pushes an int onto the scene buffer.
pub(crate) extern "C" fn blinc_text_int(n: i32) {
    push_op(DslOp::IntText(n));
}

// =====================================================================
// F-string desugaring builtins
// =====================================================================
//
// `f"hi {n}"` lowers to `string_concat("hi ", __fstring_format__(n))` via the
// normalization pass. Both names must resolve to host externs at JIT time.
// Strings produced here LEAK — acceptable for the prototype; fix path is a
// per-render arena bump allocator.

/// Encode a Rust `&str` as a Zyntax length-prefixed string (leaked).
pub(crate) fn blinc_string_alloc(s: &str) -> *const i32 {
    let len = s.len() as u32;
    let total = 4 + s.len();
    let mut buf: Vec<u8> = Vec::with_capacity(total);
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
    let ptr = buf.as_ptr() as *const i32;
    // Leak — see module comment above.
    std::mem::forget(buf);
    ptr
}

/// Decode a Zyntax length-prefixed string back to a `&str`.
///
/// # Safety
///
/// `ptr` must come from `blinc_string_alloc` (or any producer of the same layout).
pub(crate) unsafe fn blinc_string_decode<'a>(ptr: *const i32) -> &'a str {
    unsafe {
        if ptr.is_null() {
            return "";
        }
        let len = std::ptr::read_unaligned(ptr) as usize;
        let body = (ptr as *const u8).add(4);
        let bytes = std::slice::from_raw_parts(body, len);
        std::str::from_utf8(bytes).unwrap_or("<invalid utf-8>")
    }
}

/// `__fstring_format__` for i32 — decimal string of an integer.
pub(crate) extern "C" fn blinc_format_int(n: i32) -> *const i32 {
    let s = n.to_string();
    blinc_string_alloc(&s)
}

/// `string_concat` — joins two Zyntax-formatted strings into a fresh leaked one.
pub(crate) extern "C" fn blinc_string_concat(a: *const i32, b: *const i32) -> *const i32 {
    // SAFETY: length-prefixed string layout for String params.
    let a_str = unsafe { blinc_string_decode(a) };
    let b_str = unsafe { blinc_string_decode(b) };
    let mut out = String::with_capacity(a_str.len() + b_str.len());
    out.push_str(a_str);
    out.push_str(b_str);
    blinc_string_alloc(&out)
}
