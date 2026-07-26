//! The `reactive_dsl` example must keep compiling — it is the live
//! signal-binding canary, and since the computed-body fix it also
//! covers `computed { } : T` over FSM context signals, including a
//! two-source derivation.
use blinc_dsl_core::BlincDsl;

#[test]
fn reactive_dsl_example_compiles() {
    let _ = tracing_subscriber::fmt::try_init();
    let dsl = BlincDsl::new().expect("dsl init");
    dsl.compile_source(
        include_str!("../examples/reactive_dsl.blinc"),
        "reactive_dsl.blinc",
    )
    .expect("reactive_dsl.blinc must compile");
}
