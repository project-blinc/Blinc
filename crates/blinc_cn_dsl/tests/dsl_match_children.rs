//! `match` in a widget body — the arms are children.
//!
//! A match lowers to an expression-form `if` chain wrapped in a `Block`,
//! and neither shape used to reach child collection, so every arm was
//! dropped and the body rendered empty.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

fn compiled(src: &str, name: &str) -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

fn texts(dsl: &BlincDsl) -> Vec<String> {
    let host = div().w(400.0).h(400.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 400.0);
    let mut out = Vec::new();
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    out
}

/// The matching arm renders and the others do not.
#[test]
fn the_matching_arm_becomes_the_child() {
    let dsl = compiled(
        r#"view {
             Div {
               match "forms" {
                 "forms" -> Text("the forms page"),
                 "media" -> Text("the media page"),
                 _ -> Text("some other page"),
               }
             }
           }"#,
        "match_const.blinc",
    );
    let found = texts(&dsl);
    assert!(
        found.iter().any(|t| t == "the forms page"),
        "the matching arm renders: {found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|t| t == "the media page" || t == "some other page"),
        "and no other arm does: {found:?}"
    );
}

/// The shape a navigation view uses: a region re-renders the match when
/// the page signal is written.
#[test]
fn a_match_over_a_signal_swaps_children_on_a_write() {
    let dsl = compiled(
        r#"signal page: string = "forms"

           view {
             with {
               Div {
                 match page.get() {
                   "forms" -> cn.Label("the forms page"),
                   "media" -> cn.Label("the media page"),
                   _ -> cn.Label("some other page"),
                 }
               }
             }
           }"#,
        "match_signal.blinc",
    );
    assert!(
        texts(&dsl).iter().any(|t| t == "the forms page"),
        "starts on the first arm"
    );

    dsl.set_signal_string("page", "media");
    let found = texts(&dsl);
    assert!(
        found.iter().any(|t| t == "the media page"),
        "swapped: {found:?}"
    );
    assert!(
        !found.iter().any(|t| t == "the forms page"),
        "and the old arm is gone: {found:?}"
    );

    dsl.set_signal_string("page", "nothing-matches-this");
    assert!(
        texts(&dsl).iter().any(|t| t == "some other page"),
        "the wildcard catches the rest"
    );
}

/// An arm may hold several children, and a brace-block arm behaves the
/// same as a bare one.
#[test]
fn an_arm_may_hold_more_than_one_child() {
    let dsl = compiled(
        r#"view {
             Div {
               match "many" {
                 "many" -> {
                   Text("first")
                   Text("second")
                 },
                 _ -> Text("none"),
               }
             }
           }"#,
        "match_multi.blinc",
    );
    let found = texts(&dsl);
    for want in ["first", "second"] {
        assert!(found.iter().any(|t| t == want), "{want} renders: {found:?}");
    }
}
