//! `cn.Input(ref = email)` — the field's value, reachable from source.
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

fn texts(tree: &RenderTree) -> Vec<String> {
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

/// The ref reaches the field's own state: what it writes is what the
/// field renders, and `clear()` from a handler empties it.
#[test]
fn an_input_ref_reads_and_writes_the_field_it_binds() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"ref email: Input

           view {
             cn.Input(ref = email, key = "email", placeholder = "you@example.com")
           }"#,
        "input_ref.blinc",
    )
    .expect("compile");

    // Build once so the widget constructs and binds.
    let host = div().w(400.0).h(200.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);

    // The handle the declaration minted. Its id is span-derived, so the
    // test asks the registry the same way a rewritten method call does.
    let handle = blinc_dsl_core::refs::declared_input_refs()
        .into_iter()
        .find(|r| r.is_bound())
        .expect("the field bound its ref");

    handle.set_value("typed@example.com");
    let host = div().w(400.0).h(200.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    let found = texts(&tree);
    assert!(
        found.iter().any(|t| t == "typed@example.com"),
        "what the ref wrote is what the field shows: {found:?}"
    );

    handle.clear();
    assert_eq!(handle.value().as_deref(), Some(""), "and clear empties it");
    let _ = tree;
}

/// A `Textarea` ref reaches the multi-line field's own state, and
/// newlines survive the round trip as lines.
#[test]
fn a_textarea_ref_reads_and_writes_the_field_it_binds() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"ref bio: Textarea

           view {
             cn.Textarea(ref = bio, label = "Bio", rows = 3)
           }"#,
        "textarea_ref.blinc",
    )
    .expect("compile");

    let host = div().w(400.0).h(300.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 300.0);

    let handle = blinc_dsl_core::refs::declared_textarea_refs()
        .into_iter()
        .find(|r| r.is_bound())
        .expect("the field bound its ref");

    handle.set_value("first line\nsecond line");
    let host = div().w(400.0).h(300.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 300.0);
    let found = texts(&tree);
    assert!(
        found.iter().any(|t| t == "first line"),
        "the field shows what the ref wrote: {found:?}"
    );
    assert!(
        found.iter().any(|t| t == "second line"),
        "including the line after the newline: {found:?}"
    );

    handle.clear();
    assert_eq!(handle.value().as_deref(), Some(""));
}
