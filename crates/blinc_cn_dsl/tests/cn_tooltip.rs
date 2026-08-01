//! `cn.Tooltip` — the body is the trigger.
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

/// The trigger renders, and the tooltip's own text does not — it lives
/// in the overlay and only appears on hover, so a build that showed it
/// would be showing it always.
#[test]
fn the_trigger_renders_and_the_tip_stays_hidden() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"view {
             Div {
               cn.Tooltip(text = "Removes the row", side = "bottom") {
                 cn.Button("Delete", variant = "destructive")
               }
               cn.Tooltip(text = "second tip", side = "left", align = "start",
                          open_delay = 100.0, offset = 10.0) {
                 cn.Label("hover me")
               }
             }
           }"#,
        "tooltip.blinc",
    )
    .expect("compile");

    let found = texts(&dsl);
    for trigger in ["Delete", "hover me"] {
        assert!(
            found.iter().any(|t| t == trigger),
            "the trigger renders: {found:?}"
        );
    }
    for tip in ["Removes the row", "second tip"] {
        assert!(
            !found.iter().any(|t| t == tip),
            "the tip waits for a hover rather than painting at rest: {found:?}"
        );
    }
}
