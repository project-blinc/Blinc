//! The host fiber-and-effect API, reachable from Blinc.
//!
//! An FSM is going to become an `@effect(E) fiber def`: the machine's
//! state is the fiber's suspension point, an event resumes it, and the
//! host holds a token per mounted component instead of one registry
//! entry per FSM NAME. Before any of that, the API has to be callable
//! from this crate at all — the pinned Zyntax revision predated every
//! symbol below, so this test is the proof the bump bought something.
//!
//! It is deliberately written against the raw runtime rather than any
//! Blinc wrapper. There is no wrapper yet, and the point is to pin what
//! the substrate does before building on it.
use zyntax_embed::{EffectHandlerToken, FiberToken, HostFiberStep, TieredRuntime, ZyntaxValue};

/// A machine that folds an effect-supplied event into its own state and
/// suspends after each one. Shaped like an FSM: `state` is what a
/// component would re-render from, `yield` is the transition.
const OBSERVER: &str = r#"
effect Event {
    def next_event(): i64
}

handler Feed for Event {
    def next_event(): i64 { return 3 }
}

@effect(Event)
fiber def machine(): i64 {
    let mut state: i64 = 0
    while state < 100 {
        let e = next_event()
        state = state + e
        yield state
    }
    return state
}
"#;

/// Parsed with ZynML's grammar, not Blinc's. Blinc's has no `effect` /
/// `handler` / `fiber def` surface and is not getting one: the lowering
/// pass synthesizes `TypedDeclaration::Effect`, `::EffectHandler` and a
/// `TypedFunction { is_fiber: true }` directly. Writing the shapes as
/// source keeps these tests about what the substrate DOES.
fn runtime_with(src: &str) -> TieredRuntime {
    let mut config = zyntax_embed::TieredConfig::development();
    config.enable_osr = true;
    config.enable_hot_reload = true;
    let mut rt = TieredRuntime::new(config).expect("runtime should start");
    let grammar = zynml::Grammar2::from_source(zynml::ZYNML_GRAMMAR).expect("grammar");
    let program = grammar
        .parse_with_filename(src, "fiber_reach.zyn")
        .expect("parse");
    rt.compile_typed_program(program).expect("compile");
    rt
}

/// The whole contract a mounted FSM needs: construct a machine, step it
/// with an event source installed, and let go.
#[test]
fn a_machine_steps_under_a_host_installed_event_source() {
    let mut rt = runtime_with(OBSERVER);
    let machine: FiberToken = rt.get_fiber("machine").expect("construct");

    assert_eq!(
        rt.resume_fiber_within(machine, &["Feed"]).expect("step"),
        HostFiberStep::Yielded(ZyntaxValue::Int(3)),
        "the handler supplies the event; the machine folds and suspends",
    );
    assert_eq!(
        rt.resume_fiber_within(machine, &["Feed"]).expect("step"),
        HostFiberStep::Yielded(ZyntaxValue::Int(6)),
    );

    rt.drop_fiber(machine).expect("a component unmounting lets go");
}

/// Identity is the token, and the name is recoverable from it rather
/// than load-bearing. This is what replaces the name-keyed registry:
/// resolution happens once, and an unresolvable name is an error rather
/// than a silent adoption.
#[test]
fn a_handler_token_pins_its_resolution_and_names_it_back() {
    let mut rt = runtime_with(OBSERVER);
    let feed: EffectHandlerToken = rt.get_effect_handler("Feed").expect("resolve once");
    assert_eq!(rt.effect_handler_name(feed), Some("Feed"));

    let machine = rt.get_fiber("machine").expect("construct");
    assert_eq!(
        rt.resume_fiber_handled(machine, &[feed]).expect("step"),
        HostFiberStep::Yielded(ZyntaxValue::Int(3)),
        "driving through a token needs no name at the step",
    );

    assert!(
        rt.get_effect_handler("NoSuchHandler").is_err(),
        "an unresolvable handler is an error, not a pick",
    );
    rt.drop_fiber(machine).expect("drop");
}

/// Two machines from one definition do not share state — the thing a
/// name-keyed FSM registry cannot express, since two instances of one
/// component have the same name by construction.
#[test]
fn two_machines_from_one_definition_hold_separate_state() {
    let mut rt = runtime_with(OBSERVER);
    let a = rt.get_fiber("machine").expect("construct a");
    let b = rt.get_fiber("machine").expect("construct b");

    // `a` alone advances twice.
    rt.resume_fiber_within(a, &["Feed"]).expect("a step 1");
    rt.resume_fiber_within(a, &["Feed"]).expect("a step 2");

    assert_eq!(
        rt.resume_fiber_within(b, &["Feed"]).expect("b step 1"),
        HostFiberStep::Yielded(ZyntaxValue::Int(3)),
        "b starts where it started, untouched by a's steps",
    );
    assert_eq!(
        rt.resume_fiber_within(a, &["Feed"]).expect("a step 3"),
        HostFiberStep::Yielded(ZyntaxValue::Int(9)),
        "and a kept its own",
    );

    rt.drop_fiber(a).expect("drop a");
    rt.drop_fiber(b).expect("drop b");
}

/// A stateful handler, in its own program.
///
/// Deliberately NOT added to [`OBSERVER`]: declaring a stateful handler
/// alongside a stateless one for the same effect faults the process
/// with SIGBUS rather than erroring, which is worth knowing before a
/// lowering pass starts emitting handler sets per effect.
const COUNTER: &str = r#"
effect Counter {
    def next(): i64
}

handler Seq for Counter {
    var n: i64 = 0
    def next(): i64 {
        self.n = self.n + 1
        return self.n
    }
}

@effect(Counter)
fiber def watcher(): i64 {
    let mut state: i64 = 0
    while state < 100 {
        state = state + next()
        yield state
    }
    return state
}
"#;

/// Handler state is per-scope, allocated by the language. A bound
/// handler carries it for the machine's lifetime; a per-step scope
/// rebuilds it each time. This is the choice that decides whether a
/// component's signals are stable across renders or churn every frame.
#[test]
fn a_bound_handler_carries_state_where_a_per_step_scope_rebuilds_it() {
    let mut rt = runtime_with(COUNTER);

    // Per-step: `Seq` is stateful, and a fresh scope means a fresh
    // counter, so the machine sees 1 every time.
    let per_step = rt.get_fiber("watcher").expect("construct");
    for expect in [1, 2, 3] {
        assert_eq!(
            rt.resume_fiber_within(per_step, &["Seq"]).expect("step"),
            HostFiberStep::Yielded(ZyntaxValue::Int(expect)),
            "state accumulates in the MACHINE, but the handler restarts",
        );
    }
    rt.drop_fiber(per_step).expect("drop");

    // Bound: one handler state for the machine's lifetime, so the
    // counter climbs and the folded totals are 1, 3, 6.
    let bound = rt.get_fiber("watcher").expect("construct");
    let seq = rt.get_effect_handler("Seq").expect("resolve once");
    rt.bind_fiber_handler(bound, seq).expect("bind");
    for expect in [1, 3, 6] {
        assert_eq!(
            rt.resume_fiber(bound).expect("step"),
            HostFiberStep::Yielded(ZyntaxValue::Int(expect)),
        );
    }
    rt.drop_fiber(bound).expect("drop");
}

/// UPSTREAM BUG, minimal repro. Ignored because it faults the process
/// rather than failing, which takes the rest of the binary with it.
///
/// A stateful handler declared AFTER a stateless one for the same
/// effect faults with SIGBUS when installed. Declare the stateful one
/// first and the identical program works, so it is declaration order
/// and not the pairing. Nothing about the machine matters: the same
/// fiber, the same step, the same handler name.
///
/// This constrains the lowering pass directly. A signal's handler is
/// stateful — its cell is the handler state — so a set of signals
/// sharing one effect per TYPE is exactly this shape, and any
/// stateless handler emitted alongside them decides whether the
/// program runs by where it lands in the declaration list. Until it is
/// fixed upstream, a signal wants its own effect rather than a shared
/// one.
#[test]
#[ignore = "faults the process: stateful handler declared after a stateless one on the same effect"]
fn upstream_a_stateful_handler_declared_second_faults_on_install() {
    const ORDERED: &str = r#"
effect C { def next(): i64 }
handler Flat for C { def next(): i64 { return 3 } }
handler Seq for C {
    var n: i64 = 0
    def next(): i64 { self.n = self.n + 1  return self.n }
}
@effect(C)
fiber def m(): i64 {
    let mut s: i64 = 0
    while s < 100 { s = s + next()  yield s }
    return s
}
"#;
    let mut rt = runtime_with(ORDERED);
    let f = rt.get_fiber("m").expect("construct");
    assert_eq!(
        rt.resume_fiber_within(f, &["Seq"]).expect("step"),
        HostFiberStep::Yielded(ZyntaxValue::Int(1)),
        "swapping the two handler declarations makes this pass",
    );
    rt.drop_fiber(f).expect("drop");
}
