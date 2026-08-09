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

    rt.drop_fiber(machine)
        .expect("a component unmounting lets go");
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

/// A stateful handler and the machine that reads through it.
///
/// Separate from [`OBSERVER`] so the two state stories stay legible:
/// there the state lives in the machine, here it lives in the handler.
/// A signal is the second kind.
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

/// Several handlers share one effect, stateful and stateless mixed, in
/// either declaration order.
///
/// This faulted the process with SIGBUS when the stateful handler was
/// declared second, and swapping the two made the identical program
/// run — fixed upstream by giving each effect its own op ABI. Kept
/// because the lowering depends on it: a signal's cell IS its handler's
/// state, so one effect per signal TYPE with one handler per signal is
/// exactly this shape, and it only works if handler order does not
/// decide whether the program runs.
#[test]
fn handlers_sharing_an_effect_do_not_depend_on_declaration_order() {
    const STATELESS_FIRST: &str = r#"
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
    const STATEFUL_FIRST: &str = r#"
effect C { def next(): i64 }
handler Seq for C {
    var n: i64 = 0
    def next(): i64 { self.n = self.n + 1  return self.n }
}
handler Flat for C { def next(): i64 { return 3 } }
@effect(C)
fiber def m(): i64 {
    let mut s: i64 = 0
    while s < 100 { s = s + next()  yield s }
    return s
}
"#;

    // Each handler is checked in both programs, so a crossed op ABI
    // shows up as one answering with the other's behaviour rather than
    // only as a fault.
    //
    // `resume_fiber_within` opens a FRESH scope per step, so the
    // stateful handler restarts and answers 1 every time: the machine's
    // own total climbs 1, 2, 3 while the handler's counter does not.
    // The bound case, where the counter does climb, is covered
    // separately.
    for (label, src) in [
        ("stateless declared first", STATELESS_FIRST),
        ("stateful declared first", STATEFUL_FIRST),
    ] {
        let mut rt = runtime_with(src);

        let seq = rt.get_fiber("m").expect("construct");
        for expect in [1, 2, 3] {
            assert_eq!(
                rt.resume_fiber_within(seq, &["Seq"]).expect("step"),
                HostFiberStep::Yielded(ZyntaxValue::Int(expect)),
                "{label}: the stateful handler answers 1 into a fresh scope",
            );
        }
        rt.drop_fiber(seq).expect("drop");

        let flat = rt.get_fiber("m").expect("construct");
        for expect in [3, 6, 9] {
            assert_eq!(
                rt.resume_fiber_within(flat, &["Flat"]).expect("step"),
                HostFiberStep::Yielded(ZyntaxValue::Int(expect)),
                "{label}: the stateless handler answers the same each time",
            );
        }
        rt.drop_fiber(flat).expect("drop");
    }
}
