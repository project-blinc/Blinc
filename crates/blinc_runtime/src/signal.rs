//! Process-global `name → SignalId` map plus thin typed accessors.
//!
//! Each declared signal name maps to a single
//! `blinc_core::reactive::Signal<T>` minted lazily on first lookup
//! against the process-global reactive graph. There is NO parallel
//! storage cell — the underlying `Signal<T>` lives in the graph, and
//! both the DSL compile-time pipeline (`blinc_dsl_core::signal_registry`)
//! and the FSM transition runtime (`fsm::default_instance::execute_action`)
//! share THIS map so they target the same id for the same name.
//!
//! ## Why blinc_runtime owns this
//!
//! `blinc_dsl_core` depends on `blinc_runtime`, not the other way
//! around. The FSM transition runtime (which fires `set_i32` /
//! `add_i32` actions) lives in `blinc_runtime`, so the map must live
//! at least that low. Keeping it here means a name maps to ONE
//! `SignalId` no matter which layer minted it first.
//!
//! ## Pre-Phase-1A history
//!
//! This module used to host a thread-local `HashMap<String, ZyntaxValue>`
//! plus typed accessors that stored values directly. That facade was
//! retired when the DSL reactive integration landed — the underlying
//! `blinc_core::reactive::Signal<T>` is now the storage,
//! and this map only carries the name→id mapping for callers that
//! reach in by name.

use blinc_core::reactive::{Signal, SignalId};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// Tag stored alongside each signal id so re-lookups for the same
/// name detect type mismatches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalType {
    I32,
    I64,
    F64,
    String,
    Bool,
    /// A list of strings, for list rendering.
    ///
    /// The scalar variants each have a `__signal_get_by_id_<T>` extern
    /// so the JIT can read them inline. This one deliberately does not:
    /// a `Vec<String>` has no representation the JIT can hold, so it is
    /// read only host-side, by the extern that walks it to produce
    /// children.
    StringList,
}

#[derive(Clone, Copy)]
struct Entry {
    id_raw: u64,
    ty: SignalType,
}

static REGISTRY: OnceLock<RwLock<HashMap<String, Entry>>> = OnceLock::new();

fn registry() -> &'static RwLock<HashMap<String, Entry>> {
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Look up an existing signal by name. Returns the raw `SignalId` plus
/// declared type, or `None` if `name` hasn't been minted yet.
pub fn lookup(name: &str) -> Option<(u64, SignalType)> {
    let map = registry().read().ok()?;
    map.get(name).map(|e| (e.id_raw, e.ty))
}

/// Mint a new `Signal<T>` against the process-global reactive graph
/// for `name` (or return the existing id if `name` was already
/// registered). Idempotent across multiple DSL compiles or repeated
/// host-side `set_*` calls.
///
/// Type-mismatch handling: if `name` was previously registered with a
/// different `SignalType`, we keep the original entry and log a
/// warning. Re-binding would invalidate any subscriber tracking the
/// old id.
pub fn mint_or_get(name: &str, ty: SignalType) -> u64 {
    if let Some((id, existing_ty)) = lookup(name) {
        if existing_ty != ty {
            tracing::warn!(
                name = name,
                existing = ?existing_ty,
                requested = ?ty,
                "signal type changed across declarations — keeping the original entry"
            );
        }
        return id;
    }

    let id_raw = match ty {
        SignalType::I32 => blinc_core::reactive::signal::<i32>(0).id().to_raw(),
        SignalType::I64 => blinc_core::reactive::signal::<i64>(0).id().to_raw(),
        SignalType::F64 => blinc_core::reactive::signal::<f64>(0.0).id().to_raw(),
        SignalType::String => blinc_core::reactive::signal::<String>(String::new())
            .id()
            .to_raw(),
        SignalType::Bool => blinc_core::reactive::signal::<bool>(false).id().to_raw(),
        SignalType::StringList => blinc_core::reactive::signal::<Vec<String>>(Vec::new())
            .id()
            .to_raw(),
    };
    registry()
        .write()
        .expect("signal registry RwLock poisoned")
        .entry(name.to_string())
        .or_insert(Entry { id_raw, ty });
    id_raw
}

// =====================================================================
// Typed name-keyed accessors — thin wrappers over `Signal<T>::from_id`.
//
// These exist so callers that reach in by name (FSM transition actions,
// hot-reload restore, host-side `BlincDsl::set_signal_*`) don't have to
// thread `SignalId`s themselves. Each call auto-mints the underlying
// signal on first use.
// =====================================================================

/// Set the current value of an i32-typed signal. Auto-mints if absent.
/// Calls `Signal::<i32>::set(value)` directly — fires the property
/// binding registry the same way native Rust `.set()` does.
pub fn set_i32(name: &str, value: i32) {
    let id_raw = mint_or_get(name, SignalType::I32);
    Signal::<i32>::from_id(SignalId::from_raw(id_raw)).set(value);
}

/// Read the current value of an i32-typed signal. `None` if undeclared
/// or the wrong type was minted.
pub fn get_i32(name: &str) -> Option<i32> {
    let (id_raw, SignalType::I32) = lookup(name)? else {
        return None;
    };
    Signal::<i32>::from_id(SignalId::from_raw(id_raw)).try_get()
}

/// Read with a default of `0` when absent.
pub fn get_i32_or_default(name: &str) -> i32 {
    get_i32(name).unwrap_or(0)
}

/// f64 mirror of [`set_i32`].
pub fn set_f64(name: &str, value: f64) {
    let id_raw = mint_or_get(name, SignalType::F64);
    Signal::<f64>::from_id(SignalId::from_raw(id_raw)).set(value);
}

/// f64 mirror of [`get_i32`].
pub fn get_f64(name: &str) -> Option<f64> {
    let (id_raw, SignalType::F64) = lookup(name)? else {
        return None;
    };
    Signal::<f64>::from_id(SignalId::from_raw(id_raw)).try_get()
}

/// f64 mirror of [`get_i32_or_default`].
pub fn get_f64_or_default(name: &str) -> f64 {
    get_f64(name).unwrap_or(0.0)
}

/// String mirror of [`set_i32`].
pub fn set_str(name: &str, value: impl Into<String>) {
    let id_raw = mint_or_get(name, SignalType::String);
    Signal::<String>::from_id(SignalId::from_raw(id_raw)).set(value.into());
}

/// String mirror of [`get_i32`].
pub fn get_str(name: &str) -> Option<String> {
    let (id_raw, SignalType::String) = lookup(name)? else {
        return None;
    };
    Signal::<String>::from_id(SignalId::from_raw(id_raw)).try_get()
}

/// String mirror of [`get_i32_or_default`].
pub fn get_str_or_default(name: &str) -> String {
    get_str(name).unwrap_or_default()
}

/// bool mirror of [`set_i32`].
pub fn set_bool(name: &str, value: bool) {
    let id_raw = mint_or_get(name, SignalType::Bool);
    Signal::<bool>::from_id(SignalId::from_raw(id_raw)).set(value);
}

/// bool mirror of [`get_i32`].
pub fn get_bool(name: &str) -> Option<bool> {
    let (id_raw, SignalType::Bool) = lookup(name)? else {
        return None;
    };
    Signal::<bool>::from_id(SignalId::from_raw(id_raw)).try_get()
}

/// bool mirror of [`get_i32_or_default`]. Defaults to `false`.
pub fn get_bool_or_default(name: &str) -> bool {
    get_bool(name).unwrap_or(false)
}

/// Drop every entry in the name → SignalId map. Used by hot-reload to
/// reset state between sessions and by tests for clean slates. Does
/// NOT remove the underlying `Signal<T>` storage from the global
/// reactive graph — those slots leak until the graph drops, but the
/// name handles are released.
pub fn clear_all() {
    if let Ok(mut map) = registry().write() {
        map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each (name, type) maps to one Signal<T>; second set updates the same
    /// storage.
    #[test]
    fn i32_round_trip_through_signal_primitive() {
        let _guard = crate::GLOBAL_REGISTRY_TEST_LOCK.lock().unwrap();
        clear_all();
        assert_eq!(get_i32("count_test"), None);
        set_i32("count_test", 42);
        assert_eq!(get_i32("count_test"), Some(42));
        set_i32("count_test", -7);
        assert_eq!(get_i32("count_test"), Some(-7));
    }

    #[test]
    fn typed_mismatch_returns_none() {
        let _guard = crate::GLOBAL_REGISTRY_TEST_LOCK.lock().unwrap();
        clear_all();
        set_i32("conflict", 100);
        // Reading as f64 misses — different SignalType in the map.
        assert_eq!(get_f64("conflict"), None);
        assert_eq!(get_str("conflict"), None);
        assert_eq!(get_i32("conflict"), Some(100));
    }

    #[test]
    fn f64_and_str_round_trip() {
        let _guard = crate::GLOBAL_REGISTRY_TEST_LOCK.lock().unwrap();
        clear_all();
        assert_eq!(get_f64_or_default("progress_t"), 0.0);
        set_f64("progress_t", 0.75);
        assert_eq!(get_f64("progress_t"), Some(0.75));

        assert_eq!(get_str_or_default("title_t"), "");
        set_str("title_t", "hello");
        assert_eq!(get_str("title_t").as_deref(), Some("hello"));
    }
}

// =====================================================================
// String-list accessors.
//
// The source for `items.map(|x| Row(x))` over a list that changes at
// runtime. Set from Rust; the DSL reads it only through the
// child-producing extern, never as a value.
// =====================================================================

/// Replace a string-list signal's contents. Auto-mints if absent.
///
/// Fires the same subscriber path as any other `set`, so a region that
/// rendered from this list re-renders.
pub fn set_string_list(name: &str, value: Vec<String>) {
    let id_raw = mint_or_get(name, SignalType::StringList);
    Signal::<Vec<String>>::from_id(SignalId::from_raw(id_raw)).set(value);
}

/// Read a string-list signal. `None` if undeclared or minted as another
/// type.
pub fn get_string_list(name: &str) -> Option<Vec<String>> {
    let (id_raw, SignalType::StringList) = lookup(name)? else {
        return None;
    };
    Signal::<Vec<String>>::from_id(SignalId::from_raw(id_raw)).try_get()
}

/// Read by raw signal id, for the extern that only has the baked id.
pub fn get_string_list_by_id(id_raw: u64) -> Option<Vec<String>> {
    Signal::<Vec<String>>::from_id(SignalId::from_raw(id_raw)).try_get()
}

/// Append one element to a string-list signal. Auto-mints if absent.
///
/// The DSL writes a list this way rather than handing one across the
/// FFI: only the element crosses, and a string already has a
/// representation on both sides.
pub fn push_string_list(name: &str, value: String) {
    let id_raw = mint_or_get(name, SignalType::StringList);
    let sig = Signal::<Vec<String>>::from_id(SignalId::from_raw(id_raw));
    let mut items = sig.try_get().unwrap_or_default();
    items.push(value);
    sig.set(items);
}

/// Empty a string-list signal. Auto-mints if absent, so clearing an
/// undeclared list is a no-op rather than an error.
pub fn clear_string_list(name: &str) {
    let id_raw = mint_or_get(name, SignalType::StringList);
    Signal::<Vec<String>>::from_id(SignalId::from_raw(id_raw)).set(Vec::new());
}

// =====================================================================
// Exports — the names a host may reach
// =====================================================================

/// Names the running program declared with `export { … }`.
///
/// Empty means no program has declared any, which today is every
/// program: the host reaches every signal by name because nothing is
/// private yet. Once signals are scoped, this is what keeps a host's
/// grip on the ones it is meant to have.
static EXPORTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Replace the export list. Called by the DSL after each compile, so a
/// reload that removes an `export` takes it away too.
pub fn set_exported(names: &[String]) {
    if let Ok(mut e) = EXPORTED.lock() {
        e.clear();
        e.extend_from_slice(names);
    }
}

/// The current export list.
pub fn exported() -> Vec<String> {
    EXPORTED.lock().map(|e| e.clone()).unwrap_or_default()
}

/// Whether a host may reach `name`.
///
/// Permissive while no program has exported anything: taking the host's
/// access away before scoping exists would break every caller for no
/// gain. A program that declares ANY export is stating its surface, and
/// is held to it.
pub fn is_reachable(name: &str) -> bool {
    match EXPORTED.lock() {
        Ok(e) if e.is_empty() => true,
        Ok(e) => e.iter().any(|n| n == name),
        Err(_) => true,
    }
}
