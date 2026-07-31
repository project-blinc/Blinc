//! `let` in a widget body is dropped, so the binding reads as zero.
//!
//! `collect_children_into` partitions a widget body into its `children`
//! array. `Expression` statements become children; `If` / `While` are
//! carried through wrapped in a `__cf_children__` marker. Everything
//! else hits `_ => continue` and is discarded, including `Let`.
//!
//! The comment there justifies it with "bindings are hoisted by earlier
//! passes". Nothing does. The only hoisting pass is `consts.rs`, which
//! lifts `const` declarations out of `__blinc_const_group__` markers and
//! never touches `let`. So a `let` written inside a widget body reaches
//! the JIT as nothing at all, and later reads of that name resolve to an
//! undefined slot, which reads zero.
//!
//! This was originally mis-filed as four Zyntax codegen bugs -- missing
//! loop-header phis, a bad `let` initialiser, array indexing. Zyntax
//! reproduced all of them in plain ZynML and found none: `while` and
//! `let` are fine there. Every symptom came from this one pass, and the
//! shapes below are the same ones that pointed the wrong way, kept so
//! they cannot again.
//!
//! Node counts include the view root the DSL mounts under, so a body
//! with one Text is 3: root + Div + Text.
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;

fn node_count(dsl: &BlincDsl) -> usize {
    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let root = widget.build(&mut tree);
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        count += 1;
        stack.extend(tree.children(id));
    }
    count
}

fn count_of(src: &str, name: &str) -> usize {
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, name).expect("compile");
    node_count(&dsl)
}

/// One Text child, no statements. Anchors every count below.
#[test]
fn baseline_one_text_child() {
    assert_eq!(
        count_of(
            r#"component C { view { Div(class="a") { Text("x") } } }
               view { C() }"#,
            "gap_base.blinc",
        ),
        3,
        "root + Div + Text"
    );
}

/// A `let` does not disturb the widgets around it -- it is dropped, and
/// dropping it is invisible until something reads the binding.
#[test]
fn a_let_does_not_swallow_sibling_widgets() {
    assert_eq!(
        count_of(
            r#"component C { view { Div(class="a") { let i = 0 Text("x") } } }
               view { C() }"#,
            "gap_let.blinc",
        ),
        3,
        "the Text still renders alongside the let"
    );
}

/// A signal-driven `while` emits one child per iteration. Signals are
/// process-global rather than bindings in the body, so the dropped-`let`
/// bug cannot reach them -- which is exactly why every working loop in
/// the corpus is written this way.
#[test]
fn a_signal_driven_while_emits_one_child_per_iteration() {
    assert_eq!(
        count_of(
            r#"signal gw: i32 = 0
               component C { view { Div(class="a") { while gw.get() < 3 { Text("r") gw.set(gw.get() + 1) } } } }
               view { C() }"#,
            "gap_while.blinc",
        ),
        5,
        "root + Div + 3 Texts"
    );
}

/// Passes today, but for the wrong reason, and it is the trap that sent
/// this to the wrong repo.
///
/// The `let` is dropped here too. `i` therefore starts at the undefined
/// slot's zero, which is the value the `let` was going to give it, and
/// the surviving assignment takes it to 3. Reading this as "stores work,
/// initialisers do not" is what produced a bogus SSA diagnosis.
#[test]
fn a_local_appears_to_work_when_its_initialiser_is_zero() {
    assert_eq!(
        count_of(
            r#"component C { view { Div(class="a") { let i = 0 i = i + 3 if i == 3 { Text("x") } } } }
               view { C() }"#,
            "gap_store.blinc",
        ),
        3,
        "i == 3 holds, because 0 happened to be the intended initial value"
    );
}

/// THE BUG. Identical to the test above except the initialiser is 3
/// rather than 0, so the dropped `let` is observable: `i` reads zero and
/// the guard is false.
#[test]
#[ignore = "known bug: collect_children_into drops `let` from a widget body"]
fn a_let_initialiser_survives_into_the_body() {
    assert_eq!(
        count_of(
            r#"component C { view { Div(class="a") { let i = 3 if i == 3 { Text("x") } } } }
               view { C() }"#,
            "gap_init.blinc",
        ),
        3,
        "i was initialised to 3, so the Text must render"
    );
}

/// The same bug reached through a loop, which is what a desugared
/// `for` would need: the counter's `let` is dropped, so the binding the
/// loop body increments is not the one the guard reads.
#[test]
#[ignore = "known bug: collect_children_into drops `let` from a widget body"]
fn a_local_counter_survives_a_loop() {
    assert_eq!(
        count_of(
            r#"signal gc: i32 = 0
               component C {
                 view {
                   Div(class="a") {
                     let i = 0
                     while gc.get() < 3 { i = i + 1 gc.set(gc.get() + 1) }
                     if i == 3 { Text("advanced") }
                   }
                 }
               }
               view { C() }"#,
            "gap_carry.blinc",
        ),
        3,
        "three iterations, so i must be 3"
    );
}

/// Unrelated to the dropped `let`, and still open: indexing an array
/// faults. Ignored rather than asserted because a SIGSEGV takes the
/// whole test binary with it.
#[test]
#[ignore = "separate open bug: array indexing SIGSEGVs; run alone"]
fn an_array_can_be_indexed() {
    assert_eq!(
        count_of(
            r#"component C {
                 view {
                   Div(class="a") {
                     let xs = ["a", "b", "c"]
                     Text(xs[0])
                   }
                 }
               }
               view { C() }"#,
            "gap_index.blinc",
        ),
        3,
    );
}
