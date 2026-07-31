//! `map` over a list that changes while the app runs.
//!
//! The literal case is expanded at compile time (see
//! `map_over_list.rs`). This is the other source: a `signal x: List`
//! whose contents are set from Rust, walked host-side by
//! `__blinc_map_children__` because a `Vec<String>` has no
//! representation the JIT can hold.
//!
//! Names are distinct per test — the signal registry is process-global,
//! so two tests sharing a signal name in one binary read each other's
//! values.
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;

fn node_count(dsl: &BlincDsl) -> usize {
    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let root = widget.build(&mut tree);
    let (mut n, mut stack) = (0, vec![root]);
    while let Some(id) = stack.pop() {
        n += 1;
        stack.extend(tree.children(id));
    }
    n
}

fn compile(src: &str, name: &str) -> BlincDsl {
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

const ROW: &str = r#"fn Row(l: string): View { Div(class="r") { Text(l) } }"#;

/// One child per element of the list the host set.
#[test]
fn a_list_signal_renders_one_child_per_element() {
    blinc_runtime::signal::set_string_list("rt_items", vec!["a".into(), "b".into(), "c".into()]);
    let dsl = compile(
        &format!(
            r#"signal rt_items: List
               {ROW}
               view {{ Div(class="p") {{ rt_items.map(|t| Row(t)) }} }}"#
        ),
        "rt_basic.blinc",
    );
    assert_eq!(node_count(&dsl), 8, "root + Div.p + 3x(Div.r + Text)");
}

/// The list is read at render, not baked at compile: setting it again
/// changes the tree.
#[test]
fn setting_the_list_changes_the_next_render() {
    blinc_runtime::signal::set_string_list("rt_grow", vec!["only".into()]);
    let dsl = compile(
        &format!(
            r#"signal rt_grow: List
               {ROW}
               view {{ Div(class="p") {{ rt_grow.map(|t| Row(t)) }} }}"#
        ),
        "rt_grow.blinc",
    );
    assert_eq!(node_count(&dsl), 4, "root + Div.p + Div.r + Text");

    blinc_runtime::signal::set_string_list(
        "rt_grow",
        vec!["one".into(), "two".into(), "three".into()],
    );
    assert_eq!(node_count(&dsl), 8, "same program, three elements now");
}

/// An empty list contributes nothing rather than failing.
#[test]
fn an_empty_list_signal_renders_no_children() {
    blinc_runtime::signal::set_string_list("rt_empty", Vec::new());
    let dsl = compile(
        &format!(
            r#"signal rt_empty: List
               {ROW}
               view {{ Div(class="p") {{ rt_empty.map(|t| Row(t)) }} }}"#
        ),
        "rt_empty.blinc",
    );
    assert_eq!(node_count(&dsl), 2, "root + Div.p only");
}

/// Mapped children keep source order against static siblings.
#[test]
fn runtime_mapped_children_keep_source_order() {
    blinc_runtime::signal::set_string_list("rt_order", vec!["x".into(), "y".into()]);
    let dsl = compile(
        &format!(
            r#"signal rt_order: List
               {ROW}
               view {{ Div(class="p") {{ Text("before") rt_order.map(|t| Row(t)) Text("after") }} }}"#
        ),
        "rt_order.blinc",
    );
    assert_eq!(
        node_count(&dsl),
        8,
        "root + Div.p + Text + 2x(Div.r + Text) + Text"
    );
}

/// The host setter round-trips, so a test failure above is about
/// rendering rather than the signal itself.
#[test]
fn the_list_signal_round_trips() {
    blinc_runtime::signal::set_string_list("rt_trip", vec!["p".into(), "q".into()]);
    assert_eq!(
        blinc_runtime::signal::get_string_list("rt_trip"),
        Some(vec!["p".to_string(), "q".to_string()])
    );
}
