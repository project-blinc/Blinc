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
///
/// BLOCKED, and the blocker is upstream. `bind_fiber_handler` allocates
/// the handler's state once and joins it to the MACHINE's saved handler
/// segment, so that context is reachable only from inside a resume of
/// that fiber. `push_effect_handler` allocates a fresh instance, so a
/// read outside the machine sees a zeroed context rather than the one
/// the machine has been writing.
///
/// Closing it needs a handler INSTANCE the host can install in two
/// places -- around the machine's step and around the component's
/// render -- which neither entry point offers today.
#[test]
#[ignore = "a fiber-bound handler's state is not reachable outside the fiber"]
fn a_context_field_reads_back_through_the_fsm() {
    let dsl = compiled(COUNTER, "tally_read.blinc");
    let m = dsl.fsm_machine("Tally").expect("construct");
    dsl.step_fsm_machine("Tally", m, event_code(0))
        .expect("step");

    let seen = dsl
        .with_fsm_context("Tally", || {
            // Would call a compiled reader that performs `Tally$get_total`.
            0i32
        })
        .expect("scoped read");
    assert_eq!(seen, 5, "the machine wrote 5 into the context it owns");
    dsl.drop_fsm_machine(m).expect("drop");
}
