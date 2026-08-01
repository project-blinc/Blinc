//! `cn.Popover` — two named slots, and only the trigger renders at rest.
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

fn texts(dsl: &BlincDsl) -> Vec<String> {
    let host = div().w(500.0).h(300.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(500.0, 300.0);
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

/// The trigger renders; the panel waits to be opened. A loose child
/// belongs to neither slot, so it is dropped rather than shown in one.
#[test]
fn the_trigger_renders_and_the_panel_waits() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"view {
             cn.Popover(side = "bottom", align = "start") {
               cn.PopoverTrigger { cn.Button("Options") }
               cn.PopoverContent {
                 cn.Label("inside the panel")
               }
               cn.Label("loose")
             }
           }"#,
        "popover.blinc",
    )
    .expect("compile");

    let found = texts(&dsl);
    assert!(
        found.iter().any(|t| t == "Options"),
        "the trigger renders: {found:?}"
    );
    assert!(
        !found.iter().any(|t| t == "inside the panel"),
        "the panel waits to be opened rather than painting at rest: {found:?}"
    );
    assert!(
        !found.iter().any(|t| t == "loose"),
        "a child in neither slot is dropped: {found:?}"
    );
}
