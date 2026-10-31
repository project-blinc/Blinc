//! Easing a bound colour from its old value to its new one.
//!
//! A signal-bound colour normally lands in `RenderProps` the moment its
//! source changes. With a transition set on the node and property, the
//! binding dispatch hands the change here instead: the value runs from where
//! it was to the new one over the transition's duration, one queued property
//! write per tick, through the same channel an immediate write uses.
//!
//! Off until a runner calls [`enable`], so a tree that is not being ticked
//! (a test, a headless render) sees every change at once.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use blinc_animation::Easing;
use blinc_core::Color;

use crate::element::RenderProps;
use crate::property::PropertyId;
use crate::tree::LayoutNodeId;

/// How long a change takes and how it is paced.
#[derive(Clone, Copy, Debug)]
pub struct Transition {
    pub duration_ms: f32,
    pub easing: Easing,
}

static ENABLED: AtomicBool = AtomicBool::new(false);

thread_local! { static FORCED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

/// Let transitions run. A runner calls this once its scheduler is ticking.
pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed) || FORCED.with(|f| f.get())
}

/// Run `f` with transitions on for this thread only.
pub fn with_transitions_enabled<R>(f: impl FnOnce() -> R) -> R {
    FORCED.with(|forced| {
        let before = forced.replace(true);
        let out = f();
        forced.set(before);
        out
    })
}

type Write = Arc<dyn Fn(&mut RenderProps, Color) + Send + Sync>;

struct Tween {
    scope: u64,
    from: Color,
    to: Color,
    elapsed_ms: f32,
    transition: Transition,
    write: Write,
}

/// Node ids restart in every tree, so a tree's scope is part of the key.
type Key = (u64, LayoutNodeId, PropertyId);

#[derive(Default)]
struct Engine {
    tweens: HashMap<Key, Tween>,
    /// The colour last written for a node and property, which a new change
    /// starts from when nothing is in flight.
    shown: HashMap<Key, Color>,
    ticker: Option<blinc_animation::TickCallbackId>,
}

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

fn with_engine<R>(f: impl FnOnce(&mut Engine) -> R) -> R {
    let mut guard = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Engine::default))
}

/// Records the colour a node shows now, so a first change eases from it.
pub(crate) fn seed(node: LayoutNodeId, property: PropertyId, shown: Color) {
    with_engine(|e| {
        e.shown
            .entry((crate::tree::scope::current(), node, property))
            .or_insert(shown);
    });
}

/// Starts easing `node`'s `property` toward `to`. `false` if transitions are
/// off or there is no earlier value to ease from, in which case the caller
/// writes `to` at once (and this has recorded it as the value shown).
pub(crate) fn retarget(
    node: LayoutNodeId,
    property: PropertyId,
    to: Color,
    transition: Transition,
    write: Write,
) -> bool {
    let key = (crate::tree::scope::current(), node, property);
    if !enabled() {
        with_engine(|e| {
            e.shown.insert(key, to);
        });
        return false;
    }
    let started = with_engine(|e| {
        let from = e
            .tweens
            .get(&key)
            .map(current)
            .or_else(|| e.shown.get(&key).copied());
        let Some(from) = from else {
            e.shown.insert(key, to);
            return false;
        };
        if from == to {
            e.tweens.remove(&key);
            e.shown.insert(key, to);
            return false;
        }
        e.tweens.insert(
            key,
            Tween {
                scope: crate::tree::scope::current(),
                from,
                to,
                elapsed_ms: 0.0,
                transition,
                write,
            },
        );
        true
    });
    if started {
        ensure_ticker();
    }
    started
}

fn current(t: &Tween) -> Color {
    let p = (t.elapsed_ms / t.transition.duration_ms.max(1.0)).clamp(0.0, 1.0);
    Color::lerp(&t.from, &t.to, t.transition.easing.apply(p))
}

/// Advances every tween by `dt_ms` and queues the colours they reach.
pub fn tick(dt_ms: f32) {
    let writes: Vec<_> = with_engine(|e| {
        let Engine { tweens, shown, .. } = e;
        let mut out = Vec::new();
        tweens.retain(|&(_, node, property), t| {
            t.elapsed_ms += dt_ms;
            let finished = t.elapsed_ms >= t.transition.duration_ms;
            let colour = if finished { t.to } else { current(t) };
            shown.insert((t.scope, node, property), colour);
            out.push((t.scope, node, property, colour, Arc::clone(&t.write)));
            !finished
        });
        out
    });
    for (scope, node, property, colour, write) in writes {
        let _in_tree = crate::tree::scope::enter(scope);
        let (w, beneath) = (Arc::clone(&write), write);
        crate::stateful::queue_prop_update_partial_beneath(
            node,
            property,
            property.side_effects(),
            move |props| w(props, colour),
            move |props| beneath(props, colour),
        );
    }
    if with_engine(|e| e.tweens.is_empty()) {
        stop_ticker();
    }
}

/// Drops what is held for a node that is gone.
pub(crate) fn forget_node(node: LayoutNodeId) {
    with_engine(|e| {
        let scope = crate::tree::scope::current();
        e.tweens.retain(|(s, n, _), _| !(*s == scope && *n == node));
        e.shown.retain(|(s, n, _), _| !(*s == scope && *n == node));
    });
}

fn ensure_ticker() {
    let Some(handle) = blinc_animation::try_get_scheduler() else {
        return;
    };
    with_engine(|e| {
        if e.ticker.is_none() {
            e.ticker = handle.register_tick_callback(|dt_s| tick(dt_s * 1000.0));
        }
    });
}

fn stop_ticker() {
    let Some(handle) = blinc_animation::try_get_scheduler() else {
        return;
    };
    with_engine(|e| {
        if let Some(id) = e.ticker.take() {
            handle.remove_tick_callback(id);
        }
    });
}
