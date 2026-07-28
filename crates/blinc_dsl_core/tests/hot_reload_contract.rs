//! What a `.blinc` hot reload needs from the DSL, pinned as tests.
//!
//! Reloading a source file means recompiling and re-rendering while the
//! app keeps running. Three properties make that possible, and each is
//! easy to break by accident.
use blinc_dsl_core::BlincDsl;

/// A live instance can compile again. The runtime accepts the
/// redefinition rather than erroring on an already-defined symbol.
#[test]
fn recompiling_the_same_instance() {
    let dsl = BlincDsl::new().unwrap();
    let a = r#"signal p_a: i32 = 1
        view { Div { } }"#;
    let b = r#"signal p_a: i32 = 1
        signal p_b: i32 = 2
        view { Div { } }"#;
    dsl.compile_source(a, "probe.blinc").expect("first compile");
    dsl.compile_source(b, "probe.blinc")
        .expect("a second compile on a live instance must succeed");
}

/// Signal VALUES outlive the instance that declared them: the registry
/// is process-global and keyed by name, and a declared default applies
/// only when the signal is first minted. So a reload keeps whatever the
/// user had typed or clicked.
#[test]
fn a_fresh_instance_sees_existing_signals() {
    let one = BlincDsl::new().unwrap();
    one.compile_source(
        r#"signal q_v: i32 = 5
           view { Div { } }"#,
        "q.blinc",
    )
    .unwrap();
    one.set_signal_i32("q_v", 42);

    let two = BlincDsl::new().unwrap();
    two.compile_source(
        r#"signal q_v: i32 = 5
           view { Div { } }"#,
        "q.blinc",
    )
    .unwrap();
    assert_eq!(
        two.get_signal_i32("q_v"),
        Some(42),
        "a reload must not reset a signal to its declared default"
    );
}

/// Does a recompile actually swap what renders?
#[test]
fn recompile_swaps_the_view() {
    use blinc_layout::div::ElementBuilder;
    fn count(dsl: &BlincDsl) -> usize {
        let w = dsl.view_widget();
        fn walk(e: &dyn ElementBuilder) -> usize {
            1 + e
                .children_builders()
                .iter()
                .map(|c| walk(c.as_ref()))
                .sum::<usize>()
        }
        walk(w.as_ref())
    }
    let dsl = BlincDsl::new().unwrap();
    dsl.compile_source(r#"view { Div { } }"#, "swap.blinc")
        .unwrap();
    let before = count(&dsl);
    dsl.compile_source(r#"view { Div { Div { } Div { } Div { } } }"#, "swap.blinc")
        .unwrap();
    let after = count(&dsl);
    println!("view nodes before={before} after={after}");
    assert!(after > before, "a recompile must swap the rendered view");
}
