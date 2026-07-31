//! `items.map(|it| { row(it) })` — the intended list-rendering idiom.
//!
//! Declarative rather than an imperative loop in a widget body: the list
//! is declared outside the widget body, and `map` produces one child per
//! element. It composes with `: View` functions, which give the closure
//! body a scope that can hold bindings.
//!
//! State today: the surface PARSES, and Blinc lowers none of it, so the
//! call contributes no children. What is missing is recorded per test.
//!
//! Zyntax has the protocol this should build on -- ZynML's prelude
//! declares an `Iterator` trait, a `ListIterator<T>`, and
//! `impl<T> IntoIterator for List<T>`, and an array literal already
//! lowers to that `List<T>`. The open question is reach, not existence:
//! Blinc does not load the ZynML prelude and has no trait/impl surface
//! of its own, so whether `map` resolves through those impls or needs a
//! host-side extern that walks the list is the thing to settle before
//! writing the lowering.
//!
//! (The `for`-loop stubs in the compiler's `cfg.rs` / `typed_cfg.rs` are
//! a different, unrelated path and not what the prelude uses.)
use blinc_dsl_core::BlincDsl;
use blinc_layout::tree::LayoutTree;

fn parses(src: &str, name: &str) -> Result<(), String> {
    let dsl = BlincDsl::new().map_err(|e| format!("{e:?}"))?;
    dsl.parse_to_typed_ast(src, name)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

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

/// The idiom parses: `suffix_method` takes `.map(...)` and the lambda
/// argument needs no new grammar. Only lowering is missing.
#[test]
fn the_map_idiom_parses() {
    parses(
        r#"fn row(l: string): View { Div(class="r") { Text(l) } }
           view { let items = ["a", "b"] Div(class="a") { items.map(|it| { row(it) }) } }"#,
        "map_parse.blinc",
    )
    .expect("the map surface must parse");
}

/// A list binding belongs outside the widget body. The view function
/// body holds it; a widget body would drop it (see
/// `let_in_widget_body.rs`).
#[test]
fn a_list_binds_in_the_view_body() {
    parses(
        r#"view { let items = ["a", "b"] Div(class="a") { Text("x") } }"#,
        "map_bind.blinc",
    )
    .expect("a list binds in the view body");
}

/// There is no module-scope list yet. `const_decl` requires `: type` and
/// takes only `const_literal` (float / integer / bool / string), and
/// there is no top-level `let`, so a list shared across components has
/// nowhere to live above the view.
#[test]
fn there_is_no_module_scope_list() {
    assert!(
        parses(
            r#"const items = ["a", "b"]
               view { Div(class="a") { Text("x") } }"#,
            "map_const.blinc",
        )
        .is_err(),
        "if this starts passing, const grew array support and the map \
         idiom should move its list up there"
    );
}

/// NOT IMPLEMENTED. `map` parses but nothing lowers it, so the call
/// produces no children and the row never renders.
#[test]
#[ignore = "not implemented: map has no lowering, so it emits no children"]
fn map_emits_one_child_per_element() {
    assert_eq!(
        count_of(
            r#"fn row(l: string): View { Div(class="r") { Text(l) } }
               view { let items = ["a", "b"] Div(class="a") { items.map(|it| { row(it) }) } }"#,
            "map_render.blinc",
        ),
        6,
        "root + Div.a + two Div.r each with a Text"
    );
}
