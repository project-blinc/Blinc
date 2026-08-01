//! `ref card: Div` — a handle onto an ordinary element, from DSL source.
//!
//! One program: two compiles in a process fight over `render_view`.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
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

/// Two declarations bind two elements, and both become addressable —
/// which is what `card.focus()` needs to reach one.
///
/// The handle ids come from the declarations' spans, so nothing here
/// can name them; the observable is that exactly the bound elements
/// carry a ref-derived id, and the unbound sibling does not.
#[test]
fn declared_div_refs_bind_their_elements() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    dsl.compile_source(
        r#"ref card: Div
           ref panel: Div

           view {
             Div {
               Div(ref = card) { Text("bound") }
               Div(ref = panel) { Text("also bound") }
               Div { Text("not bound") }
             }
           }"#,
        "div_ref.blinc",
    )
    .expect("compile");

    let host = div().w(300.0).h(300.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(300.0, 300.0);

    let registry = tree.element_registry();
    let mut ref_ids = Vec::new();
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(element_id) = registry.get_id(id)
            && element_id.starts_with("__blinc_ref_")
        {
            ref_ids.push(element_id.to_string());
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }

    assert_eq!(
        ref_ids.len(),
        2,
        "one element per declaration, and no more: {ref_ids:?}"
    );
    ref_ids.sort();
    ref_ids.dedup();
    assert_eq!(ref_ids.len(), 2, "and they are distinct: {ref_ids:?}");
}

/// Every ref kind parses, binds where it can, and only answers to its
/// own methods.
#[test]
fn each_kind_declares_and_binds() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"ref panel: Div
           ref email: Input

           view {
             Div {
               Div(ref = panel, on_click = || panel.scroll_into_view()) {
                 Text("a panel")
               }
               Div(ref = email, on_click = || email.select_all()) {
                 Text("a field")
               }
             }
           }"#,
        "kinds.blinc",
    )
    .expect("every kind compiles, and each method belongs to its kind");

    let host = div().w(300.0).h(300.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(300.0, 300.0);

    // A `Div` ref addresses its element through the registry, so it
    // takes an id. An `Input` ref routes through the field's own focus
    // path and leaves the element as it was built — binding must not
    // change an element that is already handling its own input.
    let registry = tree.element_registry();
    let mut stamped = 0;
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if registry
            .get_id(id)
            .is_some_and(|s| s.starts_with("__blinc_ref_"))
        {
            stamped += 1;
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    assert_eq!(
        stamped, 1,
        "the Div ref stamps an id, the Input ref does not"
    );
}
