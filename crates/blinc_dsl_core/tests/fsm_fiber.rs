//! `fsm` declarations, driven as fibers.
//!
//! The pass emits an `@effect(<Fsm>$Events) fiber def <Fsm>$machine()`
//! beside the registry lowering. These drive it the way a mounted
//! component will: one machine per instance, an event armed and
//! delivered per step, and a drop on unmount.
use blinc_dsl_core::BlincDsl;

/// Two states and two events, so a wrong from-state guard or a wrong
/// event code shows up as the machine sitting still.
const TOGGLE: &str = r#"
fsm Toggle {
    state Off
    state On
    initial Off
    on Off.Flip -> On
    on On.Flip  -> Off
    on On.Reset -> Off
}
"#;

/// The event codes the pass assigns: first-appearance order, carrying
/// the runtime's offset so a code means one event on both paths.
fn event_code(nth: u32) -> u32 {
    nth + blinc_runtime::fsm::FSM_EVENT_CODE_OFFSET
}

fn compiled(src: &str, name: &str) -> BlincDsl {
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

/// The pass emits a machine that can be constructed at all.
#[test]
fn an_fsm_becomes_a_constructible_machine() {
    let dsl = compiled(TOGGLE, "toggle.blinc");
    let m = dsl
        .fsm_machine("Toggle")
        .expect("the pass emits a fiber def named after the fsm");
    dsl.drop_fsm_machine(m).expect("drop");
}

/// The machine starts in the declared initial state and advances on the
/// event the transition table names.
#[test]
fn a_machine_starts_at_initial_and_advances_on_its_event() {
    let dsl = compiled(TOGGLE, "toggle_feed.blinc");
    let m = dsl.fsm_machine("Toggle").expect("construct");

    // Off(0) --Flip--> On(1) --Flip--> Off(0)
    for expect in [1, 0, 1] {
        assert_eq!(
            dsl.step_fsm_machine("Toggle", m, event_code(0))
                .expect("step"),
            Some(expect),
        );
    }
    dsl.drop_fsm_machine(m).expect("drop");
}

/// An event with no rule for the current state leaves the state alone.
///
/// Passing for a weaker reason than it reads: no transition fires at
/// all right now, so this cannot distinguish "no rule matched" from
/// "nothing matches". It becomes load-bearing once the two ignored
/// tests above pass.
#[test]
fn an_event_with_no_rule_for_this_state_does_nothing() {
    let dsl = compiled(TOGGLE, "toggle_reset.blinc");
    let m = dsl.fsm_machine("Toggle").expect("construct");

    // Starts Off, and `Reset` has no rule from Off.
    for _ in 0..3 {
        assert_eq!(
            dsl.step_fsm_machine("Toggle", m, event_code(1))
                .expect("step"),
            Some(0),
            "Reset is declared on On only, so Off stays Off",
        );
    }
    dsl.drop_fsm_machine(m).expect("drop");
}

/// The reason for the whole change: two machines from one `fsm` hold
/// separate state. The registry path keys the state cell by the FSM's
/// name, so this is the case it cannot express.
#[test]
fn two_instances_of_one_fsm_do_not_share_state() {
    let dsl = compiled(TOGGLE, "toggle_two.blinc");
    let a = dsl.fsm_machine("Toggle").expect("construct a");
    let b = dsl.fsm_machine("Toggle").expect("construct b");

    // `a` flips once: Off -> On.
    assert_eq!(
        dsl.step_fsm_machine("Toggle", a, event_code(0))
            .expect("a step"),
        Some(1),
    );
    // `b` has not been stepped, so its first flip is also Off -> On.
    assert_eq!(
        dsl.step_fsm_machine("Toggle", b, event_code(0))
            .expect("b step"),
        Some(1),
        "b starts from the initial state, not from a's",
    );
    // And `a` continues from where IT was.
    assert_eq!(
        dsl.step_fsm_machine("Toggle", a, event_code(0))
            .expect("a step"),
        Some(0),
        "a flips back to Off, unaffected by b",
    );

    dsl.drop_fsm_machine(a).expect("drop a");
    dsl.drop_fsm_machine(b).expect("drop b");
}

/// Do the synthesized declarations survive into the compiled module?
/// `get_effect_handler` reads the module's handler table, so a token
/// here means the handler is present and resolvable.
#[test]
fn the_synthesized_handler_is_in_the_compiled_module() {
    let dsl = compiled(TOGGLE, "toggle_present.blinc");
    assert!(
        dsl.effect_handler_present("Toggle$HostEvents"),
        "the pass emits the handler but it is not in the module",
    );
}

/// A transition body runs when the machine takes that transition.
///
/// The body is a lifted zero-arg fn writing the FSM's context signal,
/// so the signal is what proves it ran. Context lives in module-level
/// signals, not in the machine, so this is behaviour parity with the
/// registry path rather than per-instance context.
const COUNTER: &str = r#"
fsm Tally {
    context {
        total: i32 = 0
    }
    state Idle
    initial Idle
    on Idle.Bump -> Idle { ctx.total += 5 }
}
"#;

#[test]
fn a_transition_body_no_longer_writes_a_module_level_signal() {
    let dsl = compiled(COUNTER, "tally.blinc");
    let m = dsl.fsm_machine("Tally").expect("construct");
    let signal = format!("tally.__fsm_ctx_{}_{}", "Tally", "total");

    for _ in 0..3 {
        dsl.step_fsm_machine("Tally", m, event_code(0))
            .expect("step");
    }
    assert_eq!(
        blinc_runtime::signal::get_i32(&signal),
        Some(0),
        "the context moved onto the handler, so the old global stays at \
         its seeded default",
    );
    dsl.drop_fsm_machine(m).expect("drop");
}

/// The end of the delegation path: step a machine, then read the field
/// back through the FSM that owns it.
#[test]
fn a_context_field_reads_back_through_the_fsm() {
    let dsl = compiled(COUNTER, "tally_read.blinc");
    let m = dsl.fsm_machine("Tally").expect("construct");

    assert_eq!(
        dsl.read_fsm_context::<i32>("Tally", m, "total")
            .expect("read"),
        0,
        "the declared default is what a machine starts with",
    );
    for expect in [5, 10, 15] {
        dsl.step_fsm_machine("Tally", m, event_code(0))
            .expect("step");
        assert_eq!(
            dsl.read_fsm_context::<i32>("Tally", m, "total")
                .expect("read"),
            expect,
            "the read resolves to the context this machine has been writing",
        );
    }
    dsl.drop_fsm_machine(m).expect("drop");
}

/// Two machines from one fsm hold separate CONTEXT, not just separate
/// state. This is what a module-level signal per field cannot express,
/// however the name is qualified.
#[test]
fn two_instances_of_one_fsm_do_not_share_context() {
    let dsl = compiled(COUNTER, "tally_two.blinc");
    let a = dsl.fsm_machine("Tally").expect("construct a");
    let b = dsl.fsm_machine("Tally").expect("construct b");

    for _ in 0..3 {
        dsl.step_fsm_machine("Tally", a, event_code(0))
            .expect("a step");
    }
    dsl.step_fsm_machine("Tally", b, event_code(0))
        .expect("b step");

    assert_eq!(
        dsl.read_fsm_context::<i32>("Tally", a, "total")
            .expect("read a"),
        15,
        "a stepped three times",
    );
    assert_eq!(
        dsl.read_fsm_context::<i32>("Tally", b, "total")
            .expect("read b"),
        5,
        "b stepped once, and a's writes did not reach it",
    );

    dsl.drop_fsm_machine(a).expect("drop a");
    dsl.drop_fsm_machine(b).expect("drop b");
}

/// A tick guard reads the context the machine owns and fires when it
/// crosses its threshold. The host sends a tick like any other event.
const GATED: &str = r#"
fsm Gate {
    context {
        level: i32 = 0
    }
    state Low
    state High
    initial Low
    on Low.Fill -> Low { ctx.level += 4 }
    tick Low -> High when ctx.level > 10
}
"#;

#[test]
fn a_tick_guard_fires_once_its_context_crosses() {
    let dsl = compiled(GATED, "gate.blinc");
    let m = dsl.fsm_machine("Gate").expect("construct");
    let tick = blinc_dsl_core::FSM_TICK_EVENT_CODE;

    // Below the threshold: a tick changes nothing.
    dsl.step_fsm_machine("Gate", m, event_code(0))
        .expect("fill");
    assert_eq!(
        dsl.step_fsm_machine("Gate", m, tick).expect("tick"),
        Some(0),
        "level is 4, so the guard does not fire",
    );

    // Cross it, then tick.
    dsl.step_fsm_machine("Gate", m, event_code(0))
        .expect("fill");
    dsl.step_fsm_machine("Gate", m, event_code(0))
        .expect("fill");
    assert_eq!(
        dsl.read_fsm_context::<i32>("Gate", m, "level")
            .expect("read"),
        12,
    );
    assert_eq!(
        dsl.step_fsm_machine("Gate", m, tick).expect("tick"),
        Some(1),
        "level is 12, so Low -> High",
    );
    dsl.drop_fsm_machine(m).expect("drop");
}

/// A tick guard belongs to its own machine's context, so one machine
/// crossing the threshold does not advance another.
#[test]
fn a_tick_guard_reads_its_own_machines_context() {
    let dsl = compiled(GATED, "gate_two.blinc");
    let a = dsl.fsm_machine("Gate").expect("a");
    let b = dsl.fsm_machine("Gate").expect("b");
    let tick = blinc_dsl_core::FSM_TICK_EVENT_CODE;

    for _ in 0..3 {
        dsl.step_fsm_machine("Gate", a, event_code(0))
            .expect("fill a");
    }
    assert_eq!(
        dsl.step_fsm_machine("Gate", a, tick).expect("tick a"),
        Some(1)
    );
    assert_eq!(
        dsl.step_fsm_machine("Gate", b, tick).expect("tick b"),
        Some(0),
        "b never filled, so its own guard does not fire",
    );

    dsl.drop_fsm_machine(a).expect("drop a");
    dsl.drop_fsm_machine(b).expect("drop b");
}

/// The reader is generic, so a context of mixed types needs no accessor
/// per type. Asserting on more than i32 keeps that honest.
const MIXED: &str = r#"
fsm Mixed {
    context {
        count: i32 = 1
        ratio: f64 = 0.5
        armed: bool = false
    }
    state Only
    initial Only
    on Only.Go -> Only {
        ctx.count += 2
        ctx.ratio += 0.25
        ctx.armed = true
    }
}
"#;

#[test]
fn context_fields_read_back_at_their_declared_types() {
    let dsl = compiled(MIXED, "mixed.blinc");
    let m = dsl.fsm_machine("Mixed").expect("construct");

    assert_eq!(dsl.read_fsm_context::<i32>("Mixed", m, "count").unwrap(), 1);
    assert_eq!(
        dsl.read_fsm_context::<f64>("Mixed", m, "ratio").unwrap(),
        0.5
    );
    assert!(!dsl.read_fsm_context::<bool>("Mixed", m, "armed").unwrap());

    dsl.step_fsm_machine("Mixed", m, event_code(0))
        .expect("step");

    assert_eq!(dsl.read_fsm_context::<i32>("Mixed", m, "count").unwrap(), 3);
    assert_eq!(
        dsl.read_fsm_context::<f64>("Mixed", m, "ratio").unwrap(),
        0.75
    );
    assert!(dsl.read_fsm_context::<bool>("Mixed", m, "armed").unwrap());

    dsl.drop_fsm_machine(m).expect("drop");
}

/// A context write notifies, because the storage is a signal.
///
/// The reason handler state holds the field's ID rather than its value:
/// per-instance identity comes from the handler, while the value and
/// its subscribers stay in the reactive graph, so a write goes down the
/// same path every other signal write does. Holding the value directly
/// gave correct per-instance context that nothing could observe.
#[test]
fn a_context_write_reaches_the_deps_notifier() {
    use std::sync::Mutex;
    static SEEN: Mutex<Vec<u64>> = Mutex::new(Vec::new());

    blinc_core::reactive::set_stateful_deps_notifier(|ids| {
        let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        seen.extend(ids.iter().map(|id| id.to_raw()));
    });

    let dsl = compiled(COUNTER, "tally_notify.blinc");
    let m = dsl.fsm_machine("Tally").expect("construct");
    let id = dsl
        .fsm_context_signal_id("Tally", m, "total")
        .expect("per-instance signal id");

    SEEN.lock().unwrap_or_else(|e| e.into_inner()).clear();
    dsl.step_fsm_machine("Tally", m, event_code(0))
        .expect("step");

    let seen = SEEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        seen.contains(&id),
        "the transition body wrote the context and nothing heard it; \
         notified {seen:?}, wanted {id}",
    );
    dsl.drop_fsm_machine(m).expect("drop");
}

/// Two machines bind to two different signals, which is what makes a
/// per-instance binding possible at all.
#[test]
fn two_machines_expose_different_signal_ids() {
    let dsl = compiled(COUNTER, "tally_ids.blinc");
    let a = dsl.fsm_machine("Tally").expect("a");
    let b = dsl.fsm_machine("Tally").expect("b");

    let ia = dsl
        .fsm_context_signal_id("Tally", a, "total")
        .expect("a id");
    let ib = dsl
        .fsm_context_signal_id("Tally", b, "total")
        .expect("b id");
    assert_ne!(ia, ib, "one signal per instance, not one per field name");

    dsl.drop_fsm_machine(a).expect("drop a");
    dsl.drop_fsm_machine(b).expect("drop b");
}

/// The seam the widget layer will use.
///
/// `blinc_runtime` owns the widget-side FSM substrate and cannot depend
/// on the DSL, so machines reach it through an installed driver — the
/// same shape as `GuardDispatcher`. These drive a machine entirely
/// through that trait, with no `BlincDsl` in hand.
#[test]
fn a_machine_drives_through_the_runtime_seam() {
    let dsl = compiled(COUNTER, "tally_seam.blinc");
    dsl.install_runtime_bridge();

    let driver = blinc_runtime::fsm::machine_driver().expect("a driver is installed");
    // Two component instances, so two scopes.
    let a = driver.machine_for("Tally", 0xA).expect("construct a");
    let b = driver.machine_for("Tally", 0xB).expect("construct b");
    assert_ne!(a, b, "two mounts, two machines");
    assert_eq!(
        driver.machine_for("Tally", 0xA),
        Some(a),
        "asking again for the same scope reuses the machine — a view body \
         asks on every rebuild, and constructing there would reset the context"
    );

    for _ in 0..2 {
        driver.step("Tally", a, event_code(0)).expect("step a");
    }
    driver.step("Tally", b, event_code(0)).expect("step b");

    let ia = driver
        .context_signal_id("Tally", a, "total")
        .expect("a's signal");
    let ib = driver
        .context_signal_id("Tally", b, "total")
        .expect("b's signal");
    assert_ne!(ia, ib, "each machine binds to its own signal");

    // Read through the graph, which is where a binding would read.
    assert_eq!(blinc_runtime::signal::get_i32_by_id(ia), Some(10));
    assert_eq!(blinc_runtime::signal::get_i32_by_id(ib), Some(5));

    driver.drop_machine(a);
    driver.drop_machine(b);
}
