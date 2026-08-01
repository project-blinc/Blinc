//! `ref pages: Scroll` — a handle, bound to an element, driven by
//! method calls. No key: the declaration's own span is the identity.
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

/// `ref = pages` binds the declared handle to that element, and driving
/// the handle moves that container.
#[test]
fn a_declared_ref_binds_to_the_element_and_drives_it() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal shut: bool = false
           ref pages: Scroll

           view {
             cn.Sidebar(collapsed = shut) {
               cn.SidebarItem(label = "Top", icon = "house",
                              on_click = || pages.scroll_to_top())
               cn.SidebarContent(ref = pages) {
                 cn.Skeleton(w = 300.0, h = 900.0)
               }
             }
           }"#,
        "scroll_ref.blinc",
    )
    .expect("compile");

    let host = div().w(800.0).h(300.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(800.0, 300.0);
    tree.process_pending_scroll_refs();

    let mut viewport = None;
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if tree.is_scroll_container(id) {
            viewport = Some(id);
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    let viewport = viewport.expect("the content area is a scroll container");

    // The handle the declaration minted, resolved through the node the
    // `ref =` prop bound it to — which is the claim under test.
    let bound = tree
        .scroll_ref(viewport)
        .cloned()
        .expect("`ref = pages` bound a handle to the content area");

    assert!(
        bound.is_bound(),
        "the handle resolved to a live node, so a command has somewhere to go"
    );

    // And a command on it is accepted rather than queued forever.
    bound.scroll_to_top();
    tree.process_pending_scroll_refs();
    let (_, offset) = tree.get_scroll_offset(viewport);
    assert_eq!(offset, 0.0, "scroll_to_top left it at the top");
}
