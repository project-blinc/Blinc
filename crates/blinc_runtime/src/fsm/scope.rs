//! The FSM instance scope: which component instance an unqualified
//! `<Fsm>.<field>` / `<Fsm>.trigger(...)` refers to.
//!
//! DSL code names an FSM but never an instance of one. `Play.pct` inside
//! a component body means "this component's Play", and two mounts of that
//! component mean two machines. Nothing in the emitted code carries the
//! distinction: an `on_click` lowers to a bare `extern "C" fn()` and a
//! transition body lifts to a no-arg top-level fn, so neither can be
//! handed an instance as an argument.
//!
//! So the instance travels dynamically. A component view enters its scope
//! on the way in and leaves it on the way out, and every deferred callback
//! captures the scope live at the moment it is registered and reinstates it
//! when it fires. A read or a trigger then asks for the current scope
//! rather than being told.
//!
//! Scope [`NO_SCOPE`] means no component claimed one. It resolves to the
//! process-wide instance keyed by FSM name, which is what every FSM did
//! before machines existed — so code outside a scoped view keeps its
//! previous behaviour instead of silently addressing a different machine.

use std::cell::Cell;

/// The scope of code that no component view claimed. Resolves to the
/// one-per-name instance rather than to a per-mount machine.
pub const NO_SCOPE: u64 = 0;

thread_local! {
    /// The instance whose FSMs unqualified names currently refer to.
    ///
    /// Thread-local because a scope is only meaningful on the thread
    /// running a build or dispatching an event, and both are the UI
    /// thread. A worker thread reads [`NO_SCOPE`] and addresses the
    /// named instance, which is the behaviour it had before.
    static CURRENT: Cell<u64> = const { Cell::new(NO_SCOPE) };
}

/// The scope in effect on this thread.
pub fn current_scope() -> u64 {
    CURRENT.with(|c| c.get())
}

/// Enter `scope`, answering with the one it displaced. Pair with
/// [`exit_scope`], or prefer [`with_scope`], which cannot leak a frame.
pub fn enter_scope(scope: u64) -> u64 {
    CURRENT.with(|c| c.replace(scope))
}

/// Restore the scope `enter_scope` displaced.
pub fn exit_scope(previous: u64) {
    CURRENT.with(|c| c.set(previous));
}

/// Run `f` in `scope`, restoring the previous scope even if `f` panics.
///
/// The restore is what makes a captured scope safe to reinstate from a
/// callback: an event handler that unwinds cannot leave the next build
/// addressing the wrong instance.
pub fn with_scope<T>(scope: u64, f: impl FnOnce() -> T) -> T {
    struct Restore(u64);
    impl Drop for Restore {
        fn drop(&mut self) {
            exit_scope(self.0);
        }
    }
    let _restore = Restore(enter_scope(scope));
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_no_scope() {
        assert_eq!(current_scope(), NO_SCOPE);
    }

    #[test]
    fn with_scope_nests_and_restores() {
        with_scope(7, || {
            assert_eq!(current_scope(), 7);
            with_scope(9, || assert_eq!(current_scope(), 9));
            assert_eq!(current_scope(), 7);
        });
        assert_eq!(current_scope(), NO_SCOPE);
    }

    /// A handler that unwinds must not leave its scope installed — the
    /// next build would address that component's machine instead of its
    /// own, and nothing about the resulting mix-up would look like a
    /// panic.
    #[test]
    fn scope_restores_through_a_panic() {
        let unwound = std::panic::catch_unwind(|| {
            with_scope(11, || panic!("handler blew up"));
        });
        assert!(unwound.is_err());
        assert_eq!(current_scope(), NO_SCOPE);
    }
}
