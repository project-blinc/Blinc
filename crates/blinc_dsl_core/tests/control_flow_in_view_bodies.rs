//! `if` / `while` inside a widget body must produce children.
//!
//! Before the `__cf_children__` carrier, `collect_children_into` kept only
//! bare `Expression` statements when partitioning a widget body into its
//! `children` array — every `if` / `while` was dropped with no diagnostic,
//! so the branch's widgets silently never rendered.
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

fn compile(src: &str, name: &str) -> BlincDsl {
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

/// Baseline: one static child Div wrapping a Text is root + Div + Text.
/// Anchors the node counts below — without it, "1 node" reads as a broken
/// probe rather than a dropped child.
#[test]
fn static_children_baseline() {
    let dsl = compile(
        r#"component C { view { Div(class="a") { Div(class="b") { Text("hi") } } } }
           view { C() }"#,
        "baseline.blinc",
    );
    assert_eq!(node_count(&dsl), 3);
}

#[test]
fn if_true_emits_children_and_if_false_gates_them() {
    let yes = compile(
        r#"component C { view { Div(class="a") { if true { Div(class="b") { Text("hi") } } } } }
           view { C() }"#,
        "if_true.blinc",
    );
    assert_eq!(
        node_count(&yes),
        3,
        "a taken branch must contribute its widgets as children"
    );

    let no = compile(
        r#"component C { view { Div(class="a") { if false { Div(class="b") { Text("hi") } } } } }
           view { C() }"#,
        "if_false.blinc",
    );
    assert_eq!(
        node_count(&no),
        1,
        "an untaken branch must contribute nothing"
    );
}

/// A branch mixing a side effect with a child: the child is pushed and the
/// side effect still runs. Regression for the child-vs-side-effect
/// discriminator — wrapping a void `__signal_set` call as a child used to
/// abort codegen with "arg not in value_map".
#[test]
fn side_effects_inside_a_branch_still_run() {
    let dsl = compile(
        r#"signal cf_probe_hit: i32
           component C { view { Div(class="a") { if true { cf_probe_hit.set(42) Text("x") } } } }
           view { C() }"#,
        "side_effect.blinc",
    );
    dsl.set_signal_i32("cf_probe_hit", 0);
    let nodes = node_count(&dsl);
    assert_eq!(dsl.get_signal_i32("cf_probe_hit"), Some(42));
    assert_eq!(nodes, 2, "root Div + the branch's Text");
}

/// A loop emits one child per iteration.
///
/// Every typed-AST pass that rewrites a widget call has to walk into loop
/// bodies. `resolve_extern_widget_named_args` did not, so the `Text` call
/// kept the short argument list its surface form had and reached Cranelift
/// with 2 args against a 4-parameter signature. The verifier rejected the
/// whole function, the backend skipped it, and the call site degraded to a
/// constant — the loop appeared to run zero times.
#[test]
fn loop_emits_one_child_per_iteration() {
    let dsl = compile(
        r#"signal cf_rows: i32
           component C { view { Div(class="a") { while cf_rows.get() < 3 { Text("r") cf_rows.set(cf_rows.get() + 1) } } } }
           view { C() }"#,
        "loop_children.blinc",
    );
    dsl.set_signal_i32("cf_rows", 0);
    let nodes = node_count(&dsl);
    assert_eq!(
        dsl.get_signal_i32("cf_rows"),
        Some(3),
        "loop must run 3 times"
    );
    assert_eq!(nodes, 4, "root Div + one Text per iteration");
}

/// Same, with a nested widget-with-children as the per-iteration child.
#[test]
fn loop_emits_nested_widgets_per_iteration() {
    let dsl = compile(
        r#"signal cf_nest: i32
           component C { view { Div(class="a") { while cf_nest.get() < 3 { Div(class="row") { Text("r") } cf_nest.set(cf_nest.get() + 1) } } } }
           view { C() }"#,
        "loop_nested.blinc",
    );
    dsl.set_signal_i32("cf_nest", 0);
    let nodes = node_count(&dsl);
    assert_eq!(dsl.get_signal_i32("cf_nest"), Some(3));
    assert_eq!(nodes, 7, "root + 3 * (Div + Text)");
}

/// A loop body with no child push runs to completion inside a widget body.
#[test]
fn loop_body_executes_inside_a_widget_body() {
    let dsl = compile(
        r#"signal cf_probe_iter: i32
           component C { view { Div(class="a") { while cf_probe_iter.get() < 3 { cf_probe_iter.set(cf_probe_iter.get() + 1) } } } }
           view { C() }"#,
        "loop_side_effect.blinc",
    );
    dsl.set_signal_i32("cf_probe_iter", 0);
    let _ = node_count(&dsl);
    assert_eq!(dsl.get_signal_i32("cf_probe_iter"), Some(3));
}
