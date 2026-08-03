//! `cn.HoverCard` — trigger and content slots.
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

fn texts(dsl: &BlincDsl) -> Vec<String> {
    let host = div().w(600.0).h(300.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 300.0);
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

/// The trigger renders inline; the card does not exist until hovered,
/// which a layout test cannot do — so its absence IS the assertion.
#[test]
fn the_trigger_shows_and_the_card_waits() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"view {
             Div {
               cn.HoverCard {
                 cn.HoverCardTrigger { cn.Label("handle") }
                 cn.HoverCardContent { cn.Label("the card body") }
               }
             }
           }"#,
        "hover_card.blinc",
    )
    .expect("compile");

    let found = texts(&dsl);
    assert!(
        found.iter().any(|t| t.contains("handle")),
        "the trigger is in the tree: {found:?}"
    );
    assert!(
        !found.iter().any(|t| t.contains("the card body")),
        "the card is not raised before a hover: {found:?}"
    );
}

/// A slot outside a hover card renders its body inline rather than
/// disappearing.
#[test]
fn a_stray_slot_renders_inline() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"view {
             Div {
               cn.HoverCardContent { cn.Label("stranded body") }
             }
           }"#,
        "hover_card_stray.blinc",
    )
    .expect("compile");

    let found = texts(&dsl);
    assert!(
        found.iter().any(|t| t.contains("stranded body")),
        "the stray slot's body still shows: {found:?}"
    );
}
