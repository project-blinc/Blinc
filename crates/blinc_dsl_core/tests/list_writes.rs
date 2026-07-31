//! Writing a list signal from the DSL.
//!
//! `set([...])` expands at compile time into a clear plus one push per
//! element, so only the elements are ever marshalled — a list itself is
//! never handed across the extern boundary.
//!
//! `set` is only claimed when the argument is an array literal. Every
//! scalar signal has a `set` too, and those belong to
//! `resolve_signal_calls`; `push` and `clear` have no scalar meaning.
//!
//! Signal names are distinct per test: the registry is process-global.
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

/// `init { }` seeds the list, and the view renders what it set — the
/// whole loop inside the DSL, with no host involvement.
#[test]
fn init_can_seed_a_list_the_view_maps_over() {
    let dsl = compile(
        &format!(
            r#"signal w_seed: List
               {ROW}
               component C {{
                 init {{ w_seed.set(["one", "two"]) }}
                 view {{ Div(class="p") {{ w_seed.map(|t| Row(t)) }} }}
               }}
               view {{ C() }}"#
        ),
        "w_seed.blinc",
    );
    // `init` runs on mount, which the first build performs.
    let _ = node_count(&dsl);
    assert_eq!(
        blinc_runtime::signal::get_string_list("w_seed"),
        Some(vec!["one".to_string(), "two".to_string()]),
        "init wrote the list"
    );
    assert_eq!(node_count(&dsl), 6, "root + Div.p + 2x(Div.r + Text)");
}

/// `set` replaces rather than appends, so seeding twice does not
/// accumulate.
#[test]
fn set_replaces_the_previous_contents() {
    blinc_runtime::signal::set_string_list("w_replace", vec!["stale".into()]);
    let dsl = compile(
        &format!(
            r#"signal w_replace: List
               {ROW}
               component C {{
                 init {{ w_replace.set(["fresh"]) }}
                 view {{ Div(class="p") {{ w_replace.map(|t| Row(t)) }} }}
               }}
               view {{ C() }}"#
        ),
        "w_replace.blinc",
    );
    let _ = node_count(&dsl);
    assert_eq!(
        blinc_runtime::signal::get_string_list("w_replace"),
        Some(vec!["fresh".to_string()]),
        "the stale element is gone"
    );
}

/// `push` appends one element, which is the shape an FSM action uses.
#[test]
fn push_appends_one_element() {
    blinc_runtime::signal::set_string_list("w_push", vec!["first".into()]);
    let dsl = compile(
        &format!(
            r#"signal w_push: List
               {ROW}
               component C {{
                 init {{ w_push.push("second") }}
                 view {{ Div(class="p") {{ w_push.map(|t| Row(t)) }} }}
               }}
               view {{ C() }}"#
        ),
        "w_push.blinc",
    );
    let _ = node_count(&dsl);
    assert_eq!(
        blinc_runtime::signal::get_string_list("w_push"),
        Some(vec!["first".to_string(), "second".to_string()])
    );
}

/// `clear` empties the list.
#[test]
fn clear_empties_the_list() {
    blinc_runtime::signal::set_string_list("w_clear", vec!["a".into(), "b".into()]);
    let dsl = compile(
        &format!(
            r#"signal w_clear: List
               {ROW}
               component C {{
                 init {{ w_clear.clear() }}
                 view {{ Div(class="p") {{ w_clear.map(|t| Row(t)) }} }}
               }}
               view {{ C() }}"#
        ),
        "w_clear.blinc",
    );
    let _ = node_count(&dsl);
    assert_eq!(
        blinc_runtime::signal::get_string_list("w_clear"),
        Some(Vec::new()),
        "emptied"
    );
}

/// A scalar `set` is untouched: it has a single non-array argument and
/// belongs to `resolve_signal_calls`. If the list pass claimed it, the
/// counter below would never reach 7.
#[test]
fn a_scalar_set_is_left_alone() {
    let dsl = compile(
        r#"signal w_scalar: i32 = 0
           component C {
             init { w_scalar.set(7) }
             view { Div(class="p") { Text("x") } }
           }
           view { C() }"#,
        "w_scalar.blinc",
    );
    let _ = node_count(&dsl);
    assert_eq!(
        blinc_runtime::signal::get_i32("w_scalar"),
        Some(7),
        "the scalar set still went through its own path"
    );
}

/// Clause order inside a component is free. `init` before `style`,
/// after `view`, and between them all mean the same thing — nothing
/// about the order survives into the emitted impl, where `init` and
/// `view` are both just methods.
#[test]
fn component_clause_order_is_free() {
    let orders = [
        // init, then style, then view
        r#"component C { init { w_order.push("x") } style { .p { gap: 4px } } view { Div(class="p") { Text("t") } } }"#,
        // style, then init, then view
        r#"component C { style { .p { gap: 4px } } init { w_order.push("x") } view { Div(class="p") { Text("t") } } }"#,
        // view, then init
        r#"component C { view { Div(class="p") { Text("t") } } init { w_order.push("x") } }"#,
        // view, then style, then init
        r#"component C { view { Div(class="p") { Text("t") } } style { .p { gap: 4px } } init { w_order.push("x") } }"#,
    ];
    for (i, component) in orders.iter().enumerate() {
        blinc_runtime::signal::set_string_list("w_order", Vec::new());
        let dsl = compile(
            &format!("signal w_order: List\n{component}\nview {{ C() }}"),
            &format!("w_order_{i}.blinc"),
        );
        let _ = node_count(&dsl);
        assert_eq!(
            blinc_runtime::signal::get_string_list("w_order"),
            Some(vec!["x".to_string()]),
            "ordering {i} must behave identically"
        );
    }
}

/// `signal feed = [...]` — the annotation is optional when an
/// initializer says what it is, and the elements are the starting
/// contents.
#[test]
fn a_list_signal_can_be_declared_by_its_initializer() {
    let dsl = compile(
        &format!(
            r#"signal w_inferred = ["a", "b"]
               {ROW}
               view {{ Div(class="p") {{ w_inferred.map(|t| Row(t)) }} }}"#
        ),
        "w_inferred.blinc",
    );
    assert_eq!(node_count(&dsl), 6, "root + Div.p + 2x(Div.r + Text)");
    assert_eq!(
        blinc_runtime::signal::get_string_list("w_inferred"),
        Some(vec!["a".to_string(), "b".to_string()])
    );
}

/// `signal feed = []` — an empty list still declares the signal, so a
/// view can map over it before anything has been written.
#[test]
fn an_empty_initializer_declares_the_signal() {
    let dsl = compile(
        &format!(
            r#"signal w_empty_init = []
               {ROW}
               component C {{
                 init {{ w_empty_init.push("later") }}
                 view {{ Div(class="p") {{ w_empty_init.map(|t| Row(t)) }} }}
               }}
               view {{ C() }}"#
        ),
        "w_empty_init.blinc",
    );
    let _ = node_count(&dsl);
    assert_eq!(
        blinc_runtime::signal::get_string_list("w_empty_init"),
        Some(vec!["later".to_string()]),
        "declared empty, then written by init"
    );
}
