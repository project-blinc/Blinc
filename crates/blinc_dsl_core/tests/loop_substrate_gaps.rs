//! What a `for` desugar can and cannot stand on today.
//!
//! `for x in xs` is the one unshipped P0 in the DSL feature table. The
//! grammar comment blames Zyntax's `process_for_loop`, which builds an
//! init/header/body/exit skeleton and emits no instructions at all
//! (both `pattern` and `iterator` are unused parameters), so the header
//! branches on nothing.
//!
//! Desugaring to the `while` that already works would sidestep that
//! entirely -- `while` in a view body does produce N children. These
//! tests pin why it is not that simple: the substrate a desugar needs
//! (a loop-carried counter, indexing) is itself broken, in ways that
//! have nothing to do with `for`.
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

/// A `let` in a view body does not disturb the children around it.
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

/// The shape a desugar would target: a `while` in a view body emits one
/// child per iteration. Counter is a signal, which is the only counter
/// that advances (see the two tests below).
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

/// GAP: a local read after a reassignment store is correct...
#[test]
fn a_local_reads_correctly_after_a_store() {
    assert_eq!(
        count_of(
            r#"component C { view { Div(class="a") { let i = 0 i = i + 3 if i == 3 { Text("x") } } } }
               view { C() }"#,
            "gap_store.blinc",
        ),
        3,
        "i == 3 holds, so the Text renders"
    );
}

/// GAP: ...but the same local read straight from its literal
/// initialiser does not. `let i = 3` then `if i == 3` finds the guard
/// false, so nothing renders. The initialiser appears not to be
/// materialised into the slot the comparison reads.
///
/// Together with the test above this is the reason a `for` cannot
/// simply desugar to `let i = 0` + `while`: the desugar's own counter
/// would be unreadable until something else wrote to it.
#[test]
#[ignore = "known gap: a let initialiser is not visible to a later read"]
fn a_local_read_from_its_initialiser_is_wrong() {
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

/// GAP: a local incremented inside a loop does not carry its value
/// across iterations. The loop is bounded by a signal so it always
/// terminates; the trailing `if` reports whether the local kept up.
///
/// Reads as missing phi nodes for loop-carried values at the header.
#[test]
#[ignore = "known gap: a local does not carry across loop iterations"]
fn a_local_carries_across_loop_iterations() {
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

/// GAP: indexing an array literal SIGSEGVs the JIT, so it cannot be the
/// element source for a desugared loop. Ignored rather than asserted --
/// a segfault takes the whole test binary with it, so this one is run
/// by hand.
#[test]
#[ignore = "known gap: array indexing SIGSEGVs the JIT"]
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

/// GAP: `.len()` on an array resolves to nothing. The diagnostic names
/// the ENCLOSING function ("Call to undefined function 'C$view'")
/// rather than the missing method, which is the standard Zyntax failure
/// mode for an unresolved call: the enclosing function is dropped.
#[test]
#[ignore = "known gap: no length operation on an array"]
fn an_array_has_a_length() {
    assert_eq!(
        count_of(
            r#"component C {
                 view {
                   Div(class="a") {
                     let xs = ["a", "b"]
                     if xs.len() == 2 { Text("x") }
                   }
                 }
               }
               view { C() }"#,
            "gap_len.blinc",
        ),
        3,
    );
}
