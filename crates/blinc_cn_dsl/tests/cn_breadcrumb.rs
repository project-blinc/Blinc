//! `cn.Breadcrumb` — the first cn widget with a collection prop.
//!
//! Proves the whole path from DSL source: a list literal parses to
//! `TypedExpression::Array`, Zyntax lowers it to `List<T>`, the prop
//! crosses as one pointer, and the widget renders one entry per
//! element.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::Arc;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            // Two globals hold a scheduler: this one, and
            // `blinc_layout::render_state`'s, which a real app fills
            // from `RenderState::new`. Widgets that animate read the
            // second, so a test that sets only the first panics with a
            // message naming neither.
            blinc_layout::render_state::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

fn texts(src: &str) -> Vec<String> {
    init();
    let dsl = BlincDsl::new().expect("dsl");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, "crumb.blinc").expect("compile");
    let host = div().w(600.0).h(120.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 120.0);

    let mut out = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(id));
    }
    out
}

#[test]
fn every_item_in_the_list_is_rendered() {
    let found = texts(r#"view { cn.Breadcrumb(items = ["Home", "Docs", "Guide"]) }"#);
    for want in ["Home", "Docs", "Guide"] {
        assert!(
            found.contains(&want.to_string()),
            "{want:?} must render: {found:?}"
        );
    }
}

/// A literal separator, so `separator = "→"` needs no new keyword.
#[test]
fn a_text_separator_renders_between_items() {
    let found = texts(r#"view { cn.Breadcrumb(items = ["A", "B"], separator = "/") }"#);
    assert!(
        found.contains(&"/".to_string()),
        "the separator renders: {found:?}"
    );
}

/// An empty list is a trail with nothing in it, not a panic.
#[test]
fn an_empty_list_renders_nothing() {
    let found = texts(r#"view { cn.Breadcrumb(items = []) }"#);
    assert!(found.is_empty(), "no entries: {found:?}");
}

/// An omitted collection prop behaves like an empty one.
#[test]
fn an_omitted_list_is_empty() {
    let found = texts(r#"view { cn.Breadcrumb() }"#);
    assert!(found.is_empty(), "no entries: {found:?}");
}

/// One element is just the current page.
#[test]
fn a_single_item_renders() {
    let found = texts(r#"view { cn.Breadcrumb(items = ["Only"]) }"#);
    assert!(found.contains(&"Only".to_string()), "{found:?}");
}
