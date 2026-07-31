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

/// The annotation is optional. An unannotated function whose body ends
/// in a widget call is inferred to produce one.
///
/// The inference is Blinc's, and it has to be, though not for the
/// reason it first appears. Zyntax's `TypeChecker` DOES run on this
/// program: `LoweringContext::lower_program` calls `run_type_checking`
/// unless `SKIP_TYPE_CHECK` is set, and that runs `check_program` then
/// `apply_inferred_types`.
///
/// What it cannot do is infer a return type the grammar has already
/// decided. A missing annotation becomes `Type::Unit` at parse time, and
/// `apply_inferred_types` propagates `Type::Any` resolutions, not `Unit`
/// ones. Spelling it `Type::Any` in the grammar instead is not
/// available -- the action language rejects it as an unknown variant --
/// and `Type::Unresolved` is a pending NAME lookup rather than an
/// inference variable.
///
/// ZynML hits the same wall and answers it by convention: its own
/// unannotated `fn_def_generic` is `Type::Unit` too, and lowering emits
/// W0002 to nudge the author into annotating. This inference is the
/// Blinc-specific convenience on top of that.
#[test]
fn an_unannotated_widget_returning_fn_is_inferred() {
    assert_eq!(
        count_of(
            r#"fn myRow() { Div(class="r") { Text("x") } }
               view { Div(class="a") { myRow() } }"#,
            "vr_infer.blinc",
        ),
        4,
        "same as writing `: View`"
    );
}

/// Only the ABSENCE of an annotation is inferred, never a stated one. A
/// function that declares it returns something else keeps that, so a
/// helper which happens to end in a widget call is not silently
/// rewritten into a view.
#[test]
fn an_explicit_non_view_return_type_is_not_overridden() {
    assert_eq!(
        count_of(
            r#"fn myRow(): i32 { Div(class="r") { Text("x") } }
               view { Div(class="a") { myRow() } }"#,
            "vr_explicit.blinc",
        ),
        2,
        "root + Div.a only — the declared i32 return is respected"
    );
}

/// A function with no widget call at the end is left alone, so the
/// inference cannot capture ordinary helpers.
#[test]
fn a_non_widget_fn_is_not_inferred_as_a_view() {
    assert_eq!(
        count_of(
            r#"fn helper() { let x = 1 }
               view { Div(class="a") { Text("x") } }"#,
            "vr_helper.blinc",
        ),
        3,
        "root + Div.a + Text; helper is untouched"
    );
}
