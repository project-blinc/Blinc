//! Scopes that own what is created inside them.
//!
//! A signal, a computed or an effect made while an [`Owner`] is current (inside
//! [`Owner::run`]) belongs to it. Disposing the owner disposes them, then its
//! child owners, and runs the cleanups registered with [`Owner::on_cleanup`].
//! This is how a row of a list or a branch that is shown only for a while gives
//! back what building it created.
//!
//! Keyed state (`use_state_keyed`) is meant to outlive a rebuild and is never
//! owned.
//!
//! Scopes are per thread: building happens on one thread, and only what that
//! thread creates inside `run` is recorded.

use std::cell::RefCell;
use std::rc::Rc;

use crate::reactive::{
    DerivedId, EffectId, SignalId, dispose_derived, dispose_host_effect, dispose_signal,
};

thread_local! {
    static CURRENT: RefCell<Vec<Rc<RefCell<Owned>>>> = const { RefCell::new(Vec::new()) };
}

#[derive(Default)]
struct Owned {
    signals: Vec<SignalId>,
    deriveds: Vec<DerivedId>,
    effects: Vec<EffectId>,
    children: Vec<Owner>,
    cleanups: Vec<Box<dyn FnOnce()>>,
    disposed: bool,
}

/// A scope that owns what is created inside [`Owner::run`].
#[derive(Clone, Default)]
pub struct Owner(Rc<RefCell<Owned>>);

/// Pops the scope `run` pushed, even if the closure panics.
struct Pop;

impl Drop for Pop {
    fn drop(&mut self) {
        CURRENT.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

impl Owner {
    /// A new scope. If one is current, it is a child of that one and goes
    /// when that one does.
    pub fn new() -> Self {
        let owner = Owner::default();
        if let Some(parent) = Self::current() {
            parent.0.borrow_mut().children.push(owner.clone());
        }
        owner
    }

    /// The scope that is current on this thread.
    pub fn current() -> Option<Owner> {
        CURRENT.with(|stack| stack.borrow().last().cloned().map(Owner))
    }

    /// Run `f` with this scope current: what `f` creates belongs to it.
    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        CURRENT.with(|stack| stack.borrow_mut().push(Rc::clone(&self.0)));
        let _pop = Pop;
        f()
    }

    /// Run `cleanup` when the current scope is disposed. Without one, it
    /// never runs.
    pub fn on_cleanup(cleanup: impl FnOnce() + 'static) {
        if let Some(owner) = Self::current() {
            owner.0.borrow_mut().cleanups.push(Box::new(cleanup));
        }
    }

    /// Dispose everything this scope owns, its child scopes first. Does
    /// nothing the second time.
    pub fn dispose(&self) {
        let owned = {
            let mut owned = self.0.borrow_mut();
            if owned.disposed {
                return;
            }
            owned.disposed = true;
            std::mem::take(&mut *owned)
        };
        for child in &owned.children {
            child.dispose();
        }
        for cleanup in owned.cleanups {
            cleanup();
        }
        for effect in owned.effects {
            dispose_host_effect(effect);
        }
        for derived in owned.deriveds {
            dispose_derived(derived);
        }
        for signal in owned.signals {
            dispose_signal(signal);
        }
        self.0.borrow_mut().disposed = true;
    }

    /// Whether [`Self::dispose`] has run.
    pub fn is_disposed(&self) -> bool {
        self.0.borrow().disposed
    }
}

fn with_current(f: impl FnOnce(&mut Owned)) {
    CURRENT.with(|stack| {
        if let Some(top) = stack.borrow().last() {
            f(&mut top.borrow_mut());
        }
    });
}

pub(crate) fn record_signal(id: SignalId) {
    with_current(|o| o.signals.push(id));
}

pub(crate) fn record_derived(id: DerivedId) {
    with_current(|o| o.deriveds.push(id));
}

pub(crate) fn record_effect(id: EffectId) {
    with_current(|o| o.effects.push(id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::{computed, effect, signal};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// They share the process-wide graph and each looks at what it made.
    static LOCK: Mutex<()> = Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn what_is_made_inside_goes_with_the_owner_and_nothing_else() {
        let _g = serial();
        let outside = signal(1_i32);
        let owner = Owner::new();
        let (inside, derived) = owner.run(|| {
            let s = signal(2_i32);
            let d = computed(move |_| s.try_get().unwrap_or(0) + 1);
            assert_eq!(d.try_get(), Some(3));
            (s, d)
        });

        owner.dispose();

        assert!(
            inside.try_get().is_none(),
            "an owned signal outlived its owner"
        );
        assert!(
            derived.try_get().is_none(),
            "an owned computed outlived its owner"
        );
        assert_eq!(
            outside.try_get(),
            Some(1),
            "a signal made outside was disposed"
        );
    }

    #[test]
    fn an_owned_effect_stops_running() {
        let _g = serial();
        let source = signal(0_i32);
        let runs = Arc::new(AtomicUsize::new(0));
        let owner = Owner::new();
        owner.run(|| {
            let runs = Arc::clone(&runs);
            let _ = effect(move |g| {
                let _ = g.get(source);
                runs.fetch_add(1, Ordering::SeqCst);
            });
        });
        let before = runs.load(Ordering::SeqCst);
        source.set(1);
        assert!(
            runs.load(Ordering::SeqCst) > before,
            "the effect did not run"
        );

        owner.dispose();
        let after = runs.load(Ordering::SeqCst);
        source.set(2);
        assert_eq!(runs.load(Ordering::SeqCst), after, "a disposed effect ran");
    }

    #[test]
    fn a_child_owner_goes_with_its_parent_and_not_the_other_way() {
        let _g = serial();
        let parent = Owner::new();
        let (child, in_parent, in_child) = parent.run(|| {
            let in_parent = signal(1_i32);
            let child = Owner::new();
            let in_child = child.run(|| signal(2_i32));
            (child, in_parent, in_child)
        });

        child.dispose();
        assert!(in_child.try_get().is_none());
        assert_eq!(
            in_parent.try_get(),
            Some(1),
            "disposing a child took the parent's"
        );

        parent.dispose();
        assert!(in_parent.try_get().is_none());
        assert!(parent.is_disposed() && child.is_disposed());
    }

    #[test]
    fn disposing_the_parent_disposes_a_child_that_was_not_yet() {
        let _g = serial();
        let parent = Owner::new();
        let (child, in_child) = parent.run(|| {
            let child = Owner::new();
            let in_child = child.run(|| signal(2_i32));
            (child, in_child)
        });

        parent.dispose();

        assert!(child.is_disposed());
        assert!(in_child.try_get().is_none());
    }

    #[test]
    fn cleanups_run_once_when_the_owner_is_disposed() {
        let _g = serial();
        let ran = Rc::new(RefCell::new(Vec::new()));
        let owner = Owner::new();
        owner.run(|| {
            let ran = Rc::clone(&ran);
            Owner::on_cleanup(move || ran.borrow_mut().push("first"));
        });
        owner.run(|| {
            let ran = Rc::clone(&ran);
            Owner::on_cleanup(move || ran.borrow_mut().push("second"));
        });
        assert!(ran.borrow().is_empty());

        owner.dispose();
        owner.dispose();

        assert_eq!(*ran.borrow(), vec!["first", "second"]);
    }

    #[test]
    fn a_scope_ends_when_its_closure_does_even_if_it_panics() {
        let _g = serial();
        let owner = Owner::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            owner.run(|| panic!("boom"));
        }));
        assert!(result.is_err());
        assert!(Owner::current().is_none(), "the scope was left current");
    }

    #[test]
    fn nothing_is_recorded_without_an_owner() {
        let _g = serial();
        assert!(Owner::current().is_none());
        let s = signal(5_i32);
        assert_eq!(s.try_get(), Some(5));
    }
}
