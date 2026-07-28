//! Bound content must land on the FIRST frame after the signal moves.
//!
//! `cn.Button`'s label and `cn.Kbd` / `cn.Avatar`'s content all take the
//! deps() rebuild path, so they have to settle together. A widget whose
//! Stateful is nested inside a non-stateful widget shell used to land a
//! frame later than one whose Stateful is the widget's own root.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(AtomicBool::new(false)),
            );
        }
        // What the windowed runner installs. Without it a bare
        // `Signal::set` never reaches `check_stateful_deps`, so no
        // bound widget refreshes and the test measures nothing.
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
    });
    blinc_core::BlincContextState::get().set_viewport_size(720.0, 820.0);
}

fn texts(tree: &RenderTree) -> Vec<String> {
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

/// One frame of the windowed loop: apply whatever the last event
/// queued, then lay out.
fn frame(tree: &mut RenderTree) {
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(720.0, 820.0);
}

#[test]
fn bound_content_settles_on_the_first_frame() {
    init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    // The playground's shape, reduced: one FSM field driving a widget
    // whose Stateful is its own root (Button) and two whose Stateful is
    // nested inside a plain shell (Kbd, Avatar).
    dsl.compile_source(
        r#"
fsm Play {
    context { caption: string = "Save" }
    state Idle
    initial Idle
    on Idle.Busy -> Idle { ctx.caption = "Saving..." }
}
view {
    Div {
        cn.Button(label = Play.caption)
        cn.Kbd(text = Play.caption)
        cn.Avatar(fallback = Play.caption)
    }
}
"#,
        "latency.blinc",
    )
    .expect("compile");

    let host = div().w(720.0).h(820.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(720.0, 820.0);
    let before = texts(&tree);
    println!("BEFORE {before:?}");
    assert_eq!(
        before.iter().filter(|t| *t == "Save").count(),
        3,
        "all three start on the initial value: {before:?}"
    );

    blinc_runtime::fsm::dispatch_default("Play", "Busy").expect("Busy must dispatch");
    frame(&mut tree);
    let after = texts(&tree);
    println!("AFTER  {after:?}");

    assert_eq!(
        after.iter().filter(|t| *t == "Saving...").count(),
        3,
        "every bound widget follows on the same frame: {after:?}"
    );
}
