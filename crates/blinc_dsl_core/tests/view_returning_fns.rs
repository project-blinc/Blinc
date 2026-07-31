//! `fn row(label: string): View { … }` — user functions that return a
//! widget.
//!
//! A widget body is a children list, so a `let` there is dropped by
//! `collect_children_into` (see `let_in_widget_body.rs`). A function
//! body is an ordinary statement scope, so it keeps its bindings, which
//! makes a `: View` function the place to put anything a widget body
//! cannot hold.
//!
//! Two halves make it work:
//!
//! * `lower_view_to_value_returning` promotes a `: View` function the
//!   same way it promotes a component's `view`, rewriting the trailing
//!   widget call into a `Return` and recording the symbol.
//! * `lower_children_arrays_to_blocks` consults those recorded symbols
//!   when deciding what counts as a child. A user function's name
//!   carries no `$view` suffix, so without that the call is silently
//!   dropped at the call site even though the function itself is fine.
//!
//! Node counts include the view root the DSL mounts under.
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;

fn count_of(src: &str, name: &str) -> usize {
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(src, name).expect("compile");
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

/// The shape written inline, for every count below to be measured
/// against.
#[test]
fn inline_baseline() {
    assert_eq!(
        count_of(
            r#"view { Div(class="a") { Div(class="r") { Text("x") } } }"#,
            "vr_base.blinc",
        ),
        4,
        "root + Div.a + Div.r + Text"
    );
}

/// The same tree with the inner widget moved into a function.
#[test]
fn a_view_fn_renders_as_a_child() {
    assert_eq!(
        count_of(
            r#"fn myRow(): View { Div(class="r") { Text("x") } }
               view { Div(class="a") { myRow() } }"#,
            "vr_child.blinc",
        ),
        4,
        "identical to writing it inline"
    );
}

/// The point of the feature: a binding in the function body, which the
/// widget body itself cannot hold.
#[test]
fn a_view_fn_body_keeps_its_let() {
    assert_eq!(
        count_of(
            r#"fn myRow(): View {
                 let label = "x"
                 Div(class="r") { Text(label) }
               }
               view { Div(class="a") { myRow() } }"#,
            "vr_let.blinc",
        ),
        4,
        "the let survives and the Text renders"
    );
}

/// Parameters, so the function is worth having more than once.
#[test]
fn a_view_fn_takes_parameters() {
    assert_eq!(
        count_of(
            r#"fn row(label: string): View { Div(class="r") { Text(label) } }
               view { Div(class="a") { row("one") row("two") } }"#,
            "vr_params.blinc",
        ),
        6,
        "root + Div.a + two Div.r each with a Text"
    );
}

/// Mixed with static children, in source order.
#[test]
fn a_view_fn_sits_beside_static_children() {
    assert_eq!(
        count_of(
            r#"fn myRow(): View { Div(class="r") { Text("x") } }
               view { Div(class="a") { Text("before") myRow() } }"#,
            "vr_mixed.blinc",
        ),
        5,
        "root + Div.a + Text + Div.r + Text"
    );
}

/// A view fn calling another one, declared caller-first so a
/// single-pass promotion would miss the callee. Pins the fixpoint loop.
#[test]
fn a_view_fn_calls_another_declared_later() {
    assert_eq!(
        count_of(
            r#"fn outer(): View { Div(class="o") { inner() } }
               fn inner(): View { Div(class="i") { Text("x") } }
               view { Div(class="a") { outer() } }"#,
            "vr_nested.blinc",
        ),
        5,
        "root + Div.a + Div.o + Div.i + Text; a single-pass promotion would \
         leave outer unpromoted and drop it entirely, giving 2"
    );
}

/// A view fn as the whole view body, not nested in a widget.
#[test]
fn a_view_fn_can_be_the_entire_view() {
    assert_eq!(
        count_of(
            r#"fn myRow(): View { Div(class="r") { Text("x") } }
               view { myRow() }"#,
            "vr_whole.blinc",
        ),
        3,
        "root + Div.r + Text"
    );
}

/// The return type is the opt-in. A function without it is left alone,
/// so helpers that build a widget for their own reasons keep whatever
/// they return.
#[test]
fn without_the_view_return_type_nothing_is_promoted() {
    assert_eq!(
        count_of(
            r#"fn myRow() { Div(class="r") { Text("x") } }
               view { Div(class="a") { myRow() } }"#,
            "vr_optin.blinc",
        ),
        2,
        "root + Div.a only — myRow is not a view fn"
    );
}

/// A view fn may be capitalised. A capital-leading call parses as a
/// component reference, so both the validator and the call-site
/// lowering consult the set of `: View` functions: the validator so it
/// is not rejected as undeclared, and the lowering so it is called by
/// its own name rather than a `<Name>$view` symbol that was never
/// defined.
///
/// Capitalised is the natural spelling for something that renders, so
/// the alternative was a naming rule users had to learn.
#[test]
fn a_capitalised_view_fn_works() {
    assert_eq!(
        count_of(
            r#"fn Row(): View { Div(class="r") { Text("x") } }
               view { Div(class="a") { Row() } }"#,
            "vr_caps.blinc",
        ),
        4,
        "identical to the lower-case spelling"
    );
}

/// Capitalised, with parameters, mapped over a list — the spelling the
/// playground uses.
#[test]
fn a_capitalised_view_fn_maps_over_a_list() {
    assert_eq!(
        count_of(
            r#"fn Tag(label: string): View { Div(class="t") { Text(label) } }
               view { let tags = ["a", "b"] Div(class="p") { tags.map(|t| { Tag(t) }) } }"#,
            "vr_caps_map.blinc",
        ),
        6,
        "root + Div.p + 2x(Div.t + Text)"
    );
}
