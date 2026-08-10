//! Backend-agnostic guard dispatch.
//!
//! Event-driven transitions resolve in pure Rust by reading the
//! [`super::registry::FsmRegistry`]. Tick-driven transitions can't
//! — they call user-authored guard expressions (lifted to
//! stand-alone functions by the DSL post-parse pass) that need
//! whichever compile backend produced them.
//!
//! This module is the boundary. A backend implements
//! [`GuardDispatcher::call_guard`] and registers an instance via
//! [`set_guard_dispatcher`]; the [`super::instance::FsmStateId`]
//! widget shim calls into the trait object without knowing
//! whether the actual call goes through Cranelift's JIT
//! (`runtime.call_function`) or a direct extern-C function
//! pointer (AOT-linked binary).
//!
//! The trait is intentionally small: each guard is a zero-arg
//! function returning `i32` (1 = fire, 0 = don't). This matches
//! how `blinc_dsl_core::populate_fsm_registry_pass` lifts
//! guard expressions today, and is trivially representable in
//! the AOT path's static symbol table.

use std::sync::{Arc, OnceLock};

/// Strategy for invoking a lifted-guard symbol.
///
/// Implementors:
/// - `JitGuardDispatcher` (in `blinc_dsl_core`) — wraps a
///   `BlincDsl` handle, dispatches via Zyntax's
///   `runtime.call_function` so Cranelift-compiled guards run
///   in-process.
/// - `AotGuardDispatcher` (future, in a per-app generated crate
///   or in `blinc_runtime::aot`) — wraps a static table of
///   `extern "C" fn() -> i32` pointers produced at LLVM link
///   time.
///
/// `Send + Sync` because the dispatcher lives behind an `Arc` in
/// a process-wide slot and may be called from any thread that
/// dispatches widget events.
pub trait GuardDispatcher: Send + Sync + 'static {
    /// Call the lifted guard function identified by `symbol`.
    ///
    /// Returns:
    /// - `Some(true)` — guard fired.
    /// - `Some(false)` — guard ran but didn't fire (typical
    ///   case for a tick where no condition is currently met).
    /// - `None` — the symbol couldn't be resolved or the call
    ///   itself errored. Treated as "guard didn't fire" by the
    ///   `FsmStateId::on_tick` walker; the registered tick loop
    ///   continues with the next guard.
    fn call_guard(&self, symbol: &str) -> Option<bool>;

    /// Call the lifted transition-action function identified
    /// by `symbol`. Used for [`super::registry::TransitionAction::Symbol`]
    /// — DSL `on X.Y -> Z { ctx.count += 1 }` bodies get
    /// lifted to `__fsm_action_<Fsm>_<idx>__` and dispatched
    /// here.
    ///
    /// The lifted action is a zero-arg `extern "C" fn()`; side
    /// effects (signal writes, externs) happen during the call.
    ///
    /// Returns:
    /// - `Some(())` — action ran to completion.
    /// - `None` — symbol couldn't be resolved or the call
    ///   errored. The dispatch loop logs once and continues
    ///   with subsequent actions / subscribers; a missing
    ///   action symbol shouldn't abort the rest of the
    ///   transition.
    ///
    /// Default impl returns `None` so existing dispatchers that
    /// haven't been recompiled against this trait extension
    /// degrade gracefully (no action support, rather than
    /// failing to build).
    fn call_action(&self, _symbol: &str) -> Option<()> {
        None
    }
}

/// Process-wide guard-dispatcher slot. Set once at app startup
/// by whichever backend is active; subsequent attempts replace
/// the previous dispatcher (last write wins) so hot-reload-style
/// re-bootstrap flows don't deadlock.
static GLOBAL_DISPATCHER: OnceLock<std::sync::RwLock<Option<Arc<dyn GuardDispatcher>>>> =
    OnceLock::new();

fn slot() -> &'static std::sync::RwLock<Option<Arc<dyn GuardDispatcher>>> {
    GLOBAL_DISPATCHER.get_or_init(|| std::sync::RwLock::new(None))
}

/// Install the process-wide guard dispatcher. Replaces any
/// previously-installed dispatcher.
///
/// Typically called once at app startup — the JIT path's
/// `BlincDsl::new()` does this, and the AOT path's generated
/// init function does it too. Apps that ship without any FSM
/// support can leave the slot empty; `FsmStateId::on_tick`
/// silently returns `None` in that case (no transitions fire).
pub fn set_guard_dispatcher(dispatcher: Arc<dyn GuardDispatcher>) {
    let mut guard = slot()
        .write()
        .expect("blinc_runtime::fsm::dispatch slot poisoned");
    *guard = Some(dispatcher);
}

/// Clear the currently-installed dispatcher. Used by tests
/// that want to verify the no-dispatcher fallback path.
pub fn clear_guard_dispatcher() {
    let mut guard = slot()
        .write()
        .expect("blinc_runtime::fsm::dispatch slot poisoned");
    *guard = None;
}

/// Call a guard via the currently-installed dispatcher. Returns
/// `None` if no dispatcher is installed (e.g. app has no FSM
/// support compiled in) or the dispatcher itself returned `None`.
pub(crate) fn call_guard(symbol: &str) -> Option<bool> {
    let guard = slot()
        .read()
        .expect("blinc_runtime::fsm::dispatch slot poisoned");
    let dispatcher = guard.as_ref()?;
    dispatcher.call_guard(symbol)
}

/// Call a transition-action via the currently-installed
/// dispatcher. Returns `None` if no dispatcher is installed or
/// the symbol couldn't be resolved.
pub(crate) fn call_action(symbol: &str) -> Option<()> {
    let guard = slot()
        .read()
        .expect("blinc_runtime::fsm::dispatch slot poisoned");
    let Some(dispatcher) = guard.as_ref() else {
        tracing::warn!(
            symbol = symbol,
            "no GuardDispatcher installed — fsm action skipped"
        );
        return None;
    };
    dispatcher.call_action(symbol)
}

/// Test-only mutex alias — folded into the crate-wide
/// [`crate::GLOBAL_REGISTRY_TEST_LOCK`] so that dispatcher tests
/// serialize against `reload::tests::*` and `signal::tests::*`,
/// not just against each other. Previously a separate mutex, but
/// `clear_all_destructive` (in `reload::tests`) mutates the
/// dispatcher slot too, so the locks must be the same one to
/// prevent the race. Kept under this name so existing call sites
/// (`fsm/instance.rs`, `fsm/dispatch.rs`) don't need to be touched.
#[cfg(test)]
pub(crate) use crate::GLOBAL_REGISTRY_TEST_LOCK as DISPATCHER_TEST_LOCK;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Simple test dispatcher that records every symbol called
    /// and returns a programmed result.
    struct ScriptedDispatcher {
        calls: Mutex<Vec<String>>,
        results: Mutex<std::collections::HashMap<String, bool>>,
    }

    impl ScriptedDispatcher {
        fn new(results: &[(&str, bool)]) -> Self {
            let mut map = std::collections::HashMap::new();
            for (sym, fire) in results {
                map.insert((*sym).to_string(), *fire);
            }
            Self {
                calls: Mutex::new(Vec::new()),
                results: Mutex::new(map),
            }
        }
    }

    impl GuardDispatcher for ScriptedDispatcher {
        fn call_guard(&self, symbol: &str) -> Option<bool> {
            self.calls.lock().unwrap().push(symbol.to_string());
            self.results.lock().unwrap().get(symbol).copied()
        }
    }

    /// With no dispatcher installed, `call_guard` returns `None`.
    /// This is the "FSM not compiled in" fallback.
    #[test]
    fn no_dispatcher_returns_none() {
        let _guard = DISPATCHER_TEST_LOCK.lock().unwrap();
        clear_guard_dispatcher();
        assert_eq!(call_guard("__fsm_tick_guard_Loader_0__"), None);
    }

    /// Installed dispatcher routes through to `call_guard` and
    /// records the symbol. Pins the trait wiring against
    /// accidental Box-vs-Arc confusion or wrong-arg-type churn.
    #[test]
    fn installed_dispatcher_is_invoked() {
        let _guard = DISPATCHER_TEST_LOCK.lock().unwrap();
        let scripted = Arc::new(ScriptedDispatcher::new(&[
            ("__fsm_tick_guard_Loader_0__", true),
            ("__fsm_tick_guard_Loader_1__", false),
        ]));
        set_guard_dispatcher(scripted.clone());

        assert_eq!(call_guard("__fsm_tick_guard_Loader_0__"), Some(true));
        assert_eq!(call_guard("__fsm_tick_guard_Loader_1__"), Some(false));
        assert_eq!(call_guard("__missing__"), None);

        let calls = scripted.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);

        // Restore the no-dispatcher state so other tests get a
        // clean slot regardless of execution order. (Cargo runs
        // tests in arbitrary order within the same binary.)
        clear_guard_dispatcher();
    }
}

// =====================================================================
// Machine driving — the fiber-backed path
// =====================================================================

/// Drives an FSM that was lowered to a fiber, for hosts that compile one.
///
/// The registry path keys a machine's state cell by the FSM's NAME, so
/// one FSM has one instance for the process. A fiber has no name to
/// collide over: a mount constructs a machine, holds the token, and two
/// mounts hold two machines with separate state and separate context.
///
/// Installed by whichever layer owns the compiled program, the same way
/// [`GuardDispatcher`] is. Absent, the registry path runs unchanged.
pub trait MachineDriver: Send + Sync + 'static {
    /// Construct a machine for `fsm`. `None` when the program has none,
    /// which is every program compiled before the fiber lowering and
    /// any FSM whose states could not be derived.
    fn machine_for(&self, fsm: &str) -> Option<u64>;

    /// Deliver `event_code` and run the machine to its next suspension,
    /// answering with the state code it settled in. `None` means the
    /// machine finished or its code is gone — a caller's cue to drop
    /// and remount rather than to retry.
    fn step(&self, fsm: &str, machine: u64, event_code: u32) -> Option<u32>;

    /// The signal id behind `field` of `machine`'s context.
    ///
    /// Per instance, so a binding asks for it rather than resolving a
    /// name — there is no name to resolve.
    fn context_signal_id(&self, fsm: &str, machine: u64, field: &str) -> Option<u64>;

    /// Release a machine. What a component does when it unmounts.
    fn drop_machine(&self, machine: u64);
}

static GLOBAL_MACHINE_DRIVER: OnceLock<std::sync::RwLock<Option<Arc<dyn MachineDriver>>>> =
    OnceLock::new();

fn machine_slot() -> &'static std::sync::RwLock<Option<Arc<dyn MachineDriver>>> {
    GLOBAL_MACHINE_DRIVER.get_or_init(|| std::sync::RwLock::new(None))
}

/// Install the driver. Replaces any previous one, so a hot reload that
/// rebuilds the program installs the new one over the old.
pub fn set_machine_driver(driver: Arc<dyn MachineDriver>) {
    if let Ok(mut slot) = machine_slot().write() {
        *slot = Some(driver);
    }
}

/// Remove the driver, returning the registry path to being the only one.
pub fn clear_machine_driver() {
    if let Ok(mut slot) = machine_slot().write() {
        *slot = None;
    }
}

/// The installed driver, if a host compiled machines.
pub fn machine_driver() -> Option<Arc<dyn MachineDriver>> {
    machine_slot().read().ok().and_then(|s| s.clone())
}
