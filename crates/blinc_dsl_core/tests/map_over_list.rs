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

/// A list can live at module scope, above every view in the file.
#[test]
fn a_const_list_binds_at_module_scope() {
    assert_eq!(
        count_of(
            r#"const scoped_items = ["a", "b"]
               fn ScopedTag(l: string): View { Div(class="t") { Text(l) } }
               view { Div(class="p") { scoped_items.map(|t| ScopedTag(t)) } }"#,
            "map_const_scope.blinc",
        ),
        6,
        "root + Div.p + 2x(Div.t + Text)"
    );
}

/// The same list serves more than one view, which is the reason to
/// hoist it out of a view body in the first place.
#[test]
fn a_module_scope_list_is_shared_across_views() {
    assert_eq!(
        count_of(
            r#"const shared_items = ["a", "b"]
               fn SharedTag(l: string): View { Div(class="t") { Text(l) } }
               component SharedA { view { Div(class="a") { shared_items.map(|t| SharedTag(t)) } } }
               component SharedB { view { Div(class="b") { shared_items.map(|t| SharedTag(t)) } } }
               view { Div(class="p") { SharedA() SharedB() } }"#,
            "map_const_shared.blinc",
        ),
        12,
        "root + Div.p + each component's Div + 2x(Div.t + Text) per component"
    );
}

/// NOT IMPLEMENTED. `map` parses but nothing lowers it, so the call
/// produces no children and the row never renders.
#[test]
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

/// Mapped children keep source order against static siblings.
#[test]
fn mapped_children_keep_source_order() {
    assert_eq!(
        count_of(
            r#"fn row(l: string): View { Div(class="r") { Text(l) } }
               view {
                 let items = ["a", "b"]
                 Div(class="a") { Text("before") items.map(|it| { row(it) }) Text("after") }
               }"#,
            "map_order.blinc",
        ),
        8,
        "root + Div.a + Text + 2x(Div.r + Text) + Text"
    );
}

/// The closure body is a scope, so it can hold a binding the widget
/// body could not.
#[test]
fn the_closure_body_can_hold_a_binding() {
    assert_eq!(
        count_of(
            r#"fn row(l: string): View { Div(class="r") { Text(l) } }
               view {
                 let items = ["a"]
                 Div(class="a") { items.map(|it| { let label = it row(label) }) }
               }"#,
            "map_binding.blinc",
        ),
        4,
        "root + Div.a + Div.r + Text"
    );
}

/// An empty list contributes nothing rather than failing.
#[test]
fn an_empty_list_maps_to_no_children() {
    assert_eq!(
        count_of(
            r#"fn row(l: string): View { Div(class="r") { Text(l) } }
               view { let items = [] Div(class="a") { items.map(|it| { row(it) }) } }"#,
            "map_empty.blinc",
        ),
        2,
        "root + Div.a only"
    );
}

/// A map whose list this pass cannot see is left alone rather than
/// dropped, so it fails visibly rather than rendering a silent blank.
#[test]
fn an_unknown_receiver_is_left_untouched() {
    parses(
        r#"fn row(l: string): View { Div(class="r") { Text(l) } }
           view { Div(class="a") { unknown.map(|it| { row(it) }) } }"#,
        "map_unknown.blinc",
    )
    .expect("still parses; the call is simply not expanded");
}

/// The closure body needs no braces for a single expression. The
/// grammar has a bare single-statement lambda form, and the expansion
/// treats a one-expression body the same either way.
#[test]
fn the_closure_body_needs_no_braces() {
    assert_eq!(
        count_of(
            r#"fn Tag(l: string): View { Div(class="t") { Text(l) } }
               view { let tags = ["a", "b"] Div(class="p") { tags.map(|t| Tag(t)) } }"#,
            "map_bare.blinc",
        ),
        6,
        "identical to the braced spelling"
    );
}

/// Braced and bare produce the same tree, so neither spelling is a
/// different code path in disguise.
#[test]
fn braced_and_bare_closures_agree() {
    let bare = count_of(
        r#"fn Tag(l: string): View { Div(class="t") { Text(l) } }
           view { let tags = ["a", "b"] Div(class="p") { tags.map(|t| Tag(t)) } }"#,
        "map_agree_bare.blinc",
    );
    let braced = count_of(
        r#"fn Tag(l: string): View { Div(class="t") { Text(l) } }
           view { let tags = ["a", "b"] Div(class="p") { tags.map(|t| { Tag(t) }) } }"#,
        "map_agree_braced.blinc",
    );
    assert_eq!(bare, braced, "the two spellings must agree");
}
