//! `cn.ResizableGroup` — panels split by handles the group draws.
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
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// Both panel bodies render, and the fixed panel takes its declared
/// width — which is also the proof the group laid the panels out rather
/// than dropping them.
#[test]
fn panels_render_at_their_declared_sizes() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"view {
             Div {
               cn.ResizableGroup(direction = "horizontal", h = 160.0) {
                 cn.ResizablePanel(default_size = 180.0) {
                   cn.Label("left pane")
                 }
                 cn.ResizablePanel {
                   cn.Label("right pane")
                 }
               }
             }
           }"#,
        "resizable.blinc",
    )
    .expect("compile");

    let host = div().w(600.0).h(200.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 200.0);

    let mut texts = Vec::new();
    let mut saw_declared_width = false;
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            texts.push(t.content.clone());
        }
        if let Some(l) = tree.layout_tree.get_layout(id)
            && (l.size.width - 180.0).abs() < 1.0
        {
            saw_declared_width = true;
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }

    for needle in ["left pane", "right pane"] {
        assert!(
            texts.iter().any(|t| t.contains(needle)),
            "{needle} rendered: {texts:?}"
        );
    }
    assert!(
        saw_declared_width,
        "a node takes the declared 180px along the axis"
    );
}
