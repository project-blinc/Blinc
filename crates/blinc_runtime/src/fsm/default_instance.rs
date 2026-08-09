//! Per-thread default `FsmInstance` substrate, backed by
//! `blinc_layout::stateful::SharedState<FsmStateId>`.
//!
//! Storage is the exact widget-state cell `Stateful<S>` already
//! consumes — `Arc<Mutex<StatefulInner<FsmStateId>>>`. Widgets
//! that want to follow the FSM call
//! `Stateful::<FsmStateId>::with_shared_state(default_state("Foo"))`
//! and read state through normal `Stateful` callbacks. The DSL
//! `Div(on_click = "Foo.Event")` extern dispatches against the
//! same cell. No parallel substrate layer.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use blinc_layout::stateful::{SharedState, request_redraw, use_fsm_keyed};

use super::instance::FsmStateId;
use super::registry::with_fsm_registry;

pub use super::registry::TransitionAction;

fn state_key(fsm_name: &str) -> String {
    // Keyed by the scope in effect as well as the name, so two mounts of
    // one component follow two state cells. `NO_SCOPE` keeps a single
    // cell per name, which is what code outside a scoped view sees.
    match super::scope::current_scope() {
        super::scope::NO_SCOPE => format!("__fsm:state:{fsm_name}"),
        scope => format!("__fsm:state:{scope:x}:{fsm_name}"),
    }
}

/// `SharedState<FsmStateId>` for `fsm_name`'s default instance.
/// `None` if `BlincContextState` isn't initialised or the FSM
/// isn't registered.
///
/// Pass the returned handle to
/// `Stateful::<FsmStateId>::with_shared_state(...)` to bind a
/// widget to this FSM's current state — the widget's
/// `on_state` callback re-runs whenever the state advances.
pub fn default_state(fsm_name: &str) -> Option<SharedState<FsmStateId>> {
    if !blinc_core::context_state::BlincContextState::is_initialized() {
        return None;
    }
    let initial = FsmStateId::from_fsm_name(fsm_name)?;
    Some(use_fsm_keyed::<_, FsmStateId>(
        &state_key(fsm_name),
        initial,
    ))
}

/// Current state name. Reads through the `SharedState` when
/// present, otherwise resolves through the fallback code table
/// + the registry's name lookup.
pub fn current_state_name(fsm_name: &str) -> Option<Arc<str>> {
    if let Some(shared) = default_state(fsm_name) {
        return shared
            .lock()
            .ok()
            .and_then(|inner| inner.state.state_name());
    }
    let code = current_state_code(fsm_name)?;
    with_fsm_registry(|r| {
        let id = r.id_of(fsm_name)?;
        r.get(id)?.state_name(code).cloned()
    })
}

/// Current state code. Same fallback shape as
/// [`current_state_name`].
pub fn current_state_code(fsm_name: &str) -> Option<u32> {
    if let Some(shared) = default_state(fsm_name) {
        return shared.lock().ok().map(|inner| inner.state.variant);
    }
    FALLBACK_CODES.with(|m| {
        if let Some(&c) = m.borrow().get(fsm_name) {
            return Some(c);
        }
        let initial = with_fsm_registry(|r| {
            r.id_of(fsm_name)
                .and_then(|id| r.get(id))
                .map(|d| d.initial_code)
        })?;
        m.borrow_mut().insert(Arc::from(fsm_name), initial);
        Some(initial)
    })
}

/// Reset to the registered initial state. No-op if the FSM
/// isn't registered. Transition actions / effects do NOT fire
/// — reset is a host operation.
pub fn reset_default(fsm_name: &str) -> Option<Arc<str>> {
    let initial = FsmStateId::from_fsm_name(fsm_name)?;
    let name = initial.state_name()?;
    if let Some(shared) = default_state(fsm_name) {
        if let Ok(mut inner) = shared.lock() {
            inner.state = initial;
        }
        request_redraw();
    } else {
        FALLBACK_CODES.with(|m| {
            m.borrow_mut().insert(Arc::from(fsm_name), initial.variant);
        });
    }
    Some(name)
}

/// Dispatch `event_name` against `fsm_name`'s default instance.
///
/// Returns `Some((from_name, to_name))` when a registered
/// transition fires. On success the `SharedState` advances
/// (and any bound `Stateful<FsmStateId>` widget gets refreshed
/// on the next frame via `needs_visual_update` +
/// `request_redraw`), every transition action runs (currently
/// `i32` signal writes), and every callback registered via
/// [`register_transition_effect`] fires in order.
pub fn dispatch_default(fsm_name: &str, event_name: &str) -> Option<(Arc<str>, Arc<str>)> {
    // One transition, one notification. A transition's action block
    // usually writes several context fields, and a `Stateful` refresh
    // builds its element eagerly -- so notifying on the first write
    // bakes the values written so far and drops the rebuilds the later
    // writes queue. See `batch_stateful_deps`.
    blinc_core::reactive::batch_stateful_deps(|| dispatch_default_inner(fsm_name, event_name))
}

fn dispatch_default_inner(fsm_name: &str, event_name: &str) -> Option<(Arc<str>, Arc<str>)> {
    tracing::debug!(fsm = fsm_name, event = event_name, "fsm dispatch_default");
    let event_code = with_fsm_registry(|r| {
        let id = r.id_of(fsm_name)?;
        r.get(id)?.event_code(event_name)
    })?;

    if let Some(outcome) = step_machine(fsm_name, event_code) {
        let (from_name, to_name) = outcome;
        // No `actions` to run: the machine executed the transition body
        // itself, as performs against its own handler. Running the
        // registry's lifted symbols too would apply every context write
        // a second time.
        finish_transition(fsm_name, event_name, &from_name, &to_name, &[]);
        return Some((from_name, to_name));
    }

    let (from_name, to_name, actions) = if let Some(shared) = default_state(fsm_name) {
        let (from_state, to_state, actions) = {
            let mut inner = shared.lock().ok()?;
            let from = inner.state;
            let actions = with_fsm_registry(|r| {
                let def = r.get(from.fsm_id)?;
                def.transitions
                    .iter()
                    .find(|t| t.from_code == from.variant && t.event_code == event_code)
                    .map(|t| t.actions.clone())
            })
            .unwrap_or_default();
            if !inner.dispatch(event_code) {
                tracing::debug!(
                    fsm = fsm_name,
                    event = event_name,
                    "fsm dispatch_default: no matching transition for current state"
                );
                return None;
            }
            (from, inner.state, actions)
        };
        request_redraw();
        let from_name = from_state.state_name()?;
        let to_name = to_state.state_name()?;
        tracing::debug!(
            fsm = fsm_name,
            event = event_name,
            from = %from_name,
            to = %to_name,
            actions = actions.len(),
            "fsm transition fired (SharedState path) + request_redraw"
        );
        (from_name, to_name, actions)
    } else {
        let current_code = current_state_code(fsm_name)?;
        let (from_name, to_name, next_code, actions) = with_fsm_registry(|r| {
            let id = r.id_of(fsm_name)?;
            let def = r.get(id)?;
            let transition = def
                .transitions
                .iter()
                .find(|t| t.from_code == current_code && t.event_code == event_code)?;
            let from = def.state_name(transition.from_code).cloned()?;
            let to = def.state_name(transition.to_code).cloned()?;
            Some((from, to, transition.to_code, transition.actions.clone()))
        })?;
        FALLBACK_CODES.with(|m| {
            m.borrow_mut().insert(Arc::from(fsm_name), next_code);
        });
        tracing::debug!(
            fsm = fsm_name,
            event = event_name,
            from = %from_name,
            to = %to_name,
            actions = actions.len(),
            "fsm transition fired (FALLBACK path — no SharedState; no redraw will run)"
        );
        (from_name, to_name, actions)
    };

    finish_transition(fsm_name, event_name, &from_name, &to_name, &actions);
    Some((from_name, to_name))
}

/// Run a fired transition's actions, then its effects and subscribers,
/// in the order a caller can rely on. Shared by the machine path (which
/// passes no actions, having already run the body) and the registry
/// path.
fn finish_transition(
    fsm_name: &str,
    event_name: &str,
    from_name: &Arc<str>,
    to_name: &Arc<str>,
    actions: &[TransitionAction],
) {
    for action in actions {
        execute_action(action);
    }
    EFFECTS.with(|e| {
        if let Some(list) = e.borrow().get(fsm_name) {
            for cb in list {
                cb(from_name, event_name, to_name);
            }
        }
    });
    let triggered_path = format!("{from_name}.{event_name}");
    let matched: Vec<Arc<TransitionSubscriber>> = SUBSCRIBERS.with(|s| {
        s.borrow()
            .get(fsm_name)
            .map(|v| {
                v.iter()
                    .filter(|(p, _)| p.as_str() == triggered_path.as_str())
                    .map(|(_, cb)| cb.clone())
                    .collect()
            })
            .unwrap_or_default()
    });
    for cb in matched {
        cb();
    }

    // All-paths subscribers: fire AFTER path-filtered subscribers so
    // the user-visible firing order is "specific filter first, then
    // generic". Each callback receives the formatted
    // `"From.Event"` path so the DSL `subscribe(|state| match state
    // { ... })` form can route on it.
    let all_matched: Vec<Arc<TransitionSubscriberPathed>> = SUBSCRIBERS_ALL.with(|s| {
        s.borrow()
            .get(fsm_name)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    });
    for cb in all_matched {
        cb(&triggered_path);
    }
}

/// Advance the machine backing this scope, if one does.
///
/// The registry still decides WHETHER a transition fires and what the
/// state names are: it is a pure table lookup on `(from_code,
/// event_code)`, and keeping it as the decision point means a machine
/// answering with an unchanged state code is not mistaken for a
/// self-transition. The machine's job is to EXECUTE — its own state
/// advance, and the context writes in the transition body, which are
/// performs against the handler that owns this instance's context.
///
/// `None` means no machine backs this scope, and the caller should walk
/// the registry itself as it did before.
fn step_machine(fsm_name: &str, event_code: u32) -> Option<(Arc<str>, Arc<str>)> {
    let driver = super::dispatch::machine_driver()?;
    let machine = driver.machine_for(fsm_name, super::scope::current_scope())?;

    let shared = default_state(fsm_name)?;
    let from = shared.lock().ok()?.state;
    let (from_name, to_name, to_code) = with_fsm_registry(|r| {
        let def = r.get(from.fsm_id)?;
        let t = def
            .transitions
            .iter()
            .find(|t| t.from_code == from.variant && t.event_code == event_code)?;
        Some((
            def.state_name(t.from_code).cloned()?,
            def.state_name(t.to_code).cloned()?,
            t.to_code,
        ))
    })?;

    // The machine reaches the same state by running the rule itself; the
    // yielded code is what it actually settled in, so a machine and a
    // table that disagree resolve in the machine's favour rather than
    // drifting silently.
    let settled = driver.step(fsm_name, machine, event_code);
    if let Some(code) = settled
        && code != to_code
    {
        tracing::warn!(
            fsm = fsm_name,
            expected = to_code,
            settled = code,
            "machine settled in a different state than the registry rule predicted"
        );
    }
    let landed = settled.unwrap_or(to_code);

    if let Ok(mut inner) = shared.lock() {
        inner.state.variant = landed;
    }
    request_redraw();
    tracing::debug!(
        fsm = fsm_name,
        from = %from_name,
        to = %to_name,
        "fsm transition fired (machine path)"
    );
    Some((from_name, to_name))
}

fn execute_action(action: &TransitionAction) {
    match action {
        TransitionAction::SetI32 { signal, value } => {
            tracing::debug!(signal = %signal, value = *value, "fsm action: SetI32");
            crate::signal::set_i32(signal, *value);
        }
        TransitionAction::AddI32 { signal, delta } => {
            let current = crate::signal::get_i32_or_default(signal);
            tracing::debug!(
                signal = %signal,
                delta = *delta,
                from = current,
                to = current + *delta,
                "fsm action: AddI32"
            );
            crate::signal::set_i32(signal, current + delta);
        }
        TransitionAction::Symbol(name) => {
            // JIT-resolved action: arbitrary DSL `{ ctx.count = … }`
            // bodies lifted to a top-level `extern "C" fn()`. A
            // missing dispatcher / unresolved symbol just no-ops
            // — same fallback policy guards use.
            tracing::debug!(symbol = %name, "fsm action: Symbol dispatch");
            if super::dispatch::call_action(name).is_none() {
                tracing::warn!(
                    symbol = %name,
                    "fsm action symbol did not run — no dispatcher installed or symbol unresolved"
                );
            }
        }
    }
}

thread_local! {
    /// Per-thread fallback state code, used when
    /// `BlincContextState` isn't initialised.
    static FALLBACK_CODES: RefCell<HashMap<Arc<str>, u32>> = RefCell::new(HashMap::new());
    static EFFECTS: RefCell<HashMap<String, Vec<TransitionEffect>>> = RefCell::new(HashMap::new());
    /// Per-FSM list of `(path, callback)` subscribers registered
    /// from DSL `init { … }` blocks via
    /// `<Fsm>.subscribe("From.Event", || { … })`. Each callback
    /// fires after a successful default-instance transition whose
    /// `"From.Event"` path equals the registered filter. The
    /// closure is wrapped in `Arc` so [`dispatch_default`] can snapshot
    /// the matching subset without holding the borrow across user
    /// code.
    static SUBSCRIBERS: RefCell<SubscriberMap> = RefCell::new(HashMap::new());
    /// Per-FSM list of "all-paths" subscribers — fired on every
    /// successful transition, receiving the matched
    /// `"From.Event"` path string. Registered from DSL
    /// `<Fsm>.subscribe(|state| …)` (single-arg form, distinct
    /// from the path-filter / zero-arg shape). Same `Arc` snapshot
    /// pattern as `SUBSCRIBERS` so dispatch doesn't hold the
    /// borrow across user code.
    static SUBSCRIBERS_ALL: RefCell<SubscriberAllMap> = RefCell::new(HashMap::new());
}

/// Callback signature for host-side effects registered via
/// [`register_transition_effect`]. Args are
/// `(from_state, event, to_state)`.
pub type TransitionEffect = Box<dyn Fn(&str, &str, &str) + 'static>;

/// Callback signature for DSL-registered FSM subscribers. The
/// closure takes no args — `register_subscriber` already filters
/// on the registered `"From.Event"` path, so by the time the
/// callback fires the path is known to match. Registered from a
/// DSL `init { ... }` block via
/// `<Fsm>.subscribe("From.Event", || { … })` — see
/// [`register_subscriber`].
pub type TransitionSubscriber = dyn Fn() + 'static;

/// Path-receiving subscriber. Same firing site as
/// [`TransitionSubscriber`] but the callback receives the matched
/// `"From.Event"` path so the closure body can dispatch on it
/// (e.g. `match state { "Counting.Increment" -> … }`). Registered
/// from DSL `<Fsm>.subscribe(|state| { … })` — the single-arg
/// form distinct from the path-filter / zero-arg shape.
pub type TransitionSubscriberPathed = dyn Fn(&str) + 'static;

/// Per-thread subscriber table: FSM name → list of
/// `(path filter, callback)` pairs. Aliased so the `thread_local!`
/// + lookup sites don't repeat the deeply-nested generics.
type SubscriberMap = HashMap<String, Vec<(String, Arc<TransitionSubscriber>)>>;

/// All-paths-subscriber table: FSM name → list of callbacks that
/// fire on every transition. Separate from `SubscriberMap` so the
/// dispatch loop can iterate without a path-filter check.
type SubscriberAllMap = HashMap<String, Vec<Arc<TransitionSubscriberPathed>>>;

/// Register a DSL-side subscriber that fires after each successful
/// default-instance transition whose triggered `"From.Event"` path
/// equals `path`. Distinct from [`register_transition_effect`]:
/// subscribers are path-filtered and zero-arg, intended to be
/// driven by `__fsm_subscribe__` host-extern calls emitted from
/// DSL `init { ... }` blocks. Effects are intended for host-side
/// integrations and receive the full `(from, event, to)` triple.
pub fn register_subscriber(fsm_name: &str, path: &str, cb: impl Fn() + 'static) {
    SUBSCRIBERS.with(|s| {
        s.borrow_mut()
            .entry(fsm_name.to_string())
            .or_default()
            .push((path.to_string(), Arc::new(cb)));
    });
}

/// All-paths subscriber registered from DSL
/// `<Fsm>.subscribe(|state| { … })`. Fires on every successful
/// default-instance transition, receiving the formatted
/// `"From.Event"` path as the callback's single argument so the
/// closure body can dispatch on it (typically via `match state`).
pub fn register_subscriber_all(fsm_name: &str, cb: impl Fn(&str) + 'static) {
    SUBSCRIBERS_ALL.with(|s| {
        s.borrow_mut()
            .entry(fsm_name.to_string())
            .or_default()
            .push(Arc::new(cb));
    });
}

/// Register a callback to run after each successful default-
/// instance transition for `fsm_name`. Reserved for side
/// effects that can't be expressed in DSL (logging, network,
/// etc.) — signal writes belong in transition actions.
pub fn register_transition_effect(fsm_name: &str, cb: impl Fn(&str, &str, &str) + 'static) {
    EFFECTS.with(|e| {
        e.borrow_mut()
            .entry(fsm_name.to_string())
            .or_default()
            .push(Box::new(cb));
    });
}

/// Test helper.
pub fn clear_all() {
    FALLBACK_CODES.with(|m| m.borrow_mut().clear());
    EFFECTS.with(|e| e.borrow_mut().clear());
    SUBSCRIBERS.with(|s| s.borrow_mut().clear());
    SUBSCRIBERS_ALL.with(|s| s.borrow_mut().clear());
}
