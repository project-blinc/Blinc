//! `cn.Collapsible` — container wrapper driven by a bound signal.
//!
//! Geometry-checked, like the other containers: a wrapper can report
//! the right shape and render nothing of the sort.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};

/// A collapsible animates its fold, so it needs the scheduler and the
/// context state a real app stands up at launch, on top of the theme
/// every cn widget reads while building.
fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        // TWO globals, and a collapsible reads the second: the
        // animation crate keeps a `OnceLock` for widgets that ask it
        // directly, and `blinc_layout::render_state` keeps its own that
        // a real app populates from `RenderState::new`. Setting only
        // the first leaves `get_global_scheduler` returning None and
        // the widget panicking on a message that names neither.
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        // Leaked on purpose: the handles above borrow from it and the
        // scheduler has to outlive every test in the binary.
        Box::leak(Box::new(s));
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                std::sync::Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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

/// Text contents of the laid-out tree, in no particular order.
fn texts(src: &str, name: &str) -> Vec<String> {
    let dsl = compiled(src, name);
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

/// The body renders when the bound signal is open.
#[test]
fn an_open_collapsible_shows_its_body() {
    let found = texts(
        r#"signal col_open: bool = true
           view { cn.Collapsible(open = col_open) { Text("inside") } }"#,
        "col_open.blinc",
    );
    assert!(
        found.iter().any(|t| t == "inside"),
        "body renders when open: {found:?}"
    );
}

/// Closed folds by animating scale and opacity rather than unmounting,
/// so the body is still in the tree. Pinned because it is the property
/// that lets the body be built once and handed to the builder — if
/// closing ever unmounted, that would have to change.
#[test]
fn a_closed_collapsible_keeps_its_body_mounted() {
    let found = texts(
        r#"signal col_shut: bool = false
           view { cn.Collapsible(open = col_shut) { Text("inside") } }"#,
        "col_shut.blinc",
    );
    assert!(
        found.iter().any(|t| t == "inside"),
        "body stays mounted when closed: {found:?}"
    );
}

/// Several children share one content element rather than competing,
/// the same rule the other containers follow.
#[test]
fn several_children_all_render() {
    let found = texts(
        r#"signal col_many: bool = true
           view { cn.Collapsible(open = col_many) { Text("a") Text("b") Text("c") } }"#,
        "col_many.blinc",
    );
    for want in ["a", "b", "c"] {
        assert!(found.iter().any(|t| t == want), "{want} renders: {found:?}");
    }
}

/// A literal pins the section rather than binding it.
#[test]
fn a_literal_open_is_accepted() {
    let found = texts(
        r#"view { cn.Collapsible(open = true) { Text("pinned") } }"#,
        "col_literal.blinc",
    );
    assert!(
        found.iter().any(|t| t == "pinned"),
        "a literal works: {found:?}"
    );
}
