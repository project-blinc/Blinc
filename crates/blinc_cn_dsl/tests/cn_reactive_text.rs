//! Text props that accept a signal must follow it.
//!
//! Text content has no property writer, so these rebuild through
//! `deps()`. The rebuilt text also has to carry its own colour: content
//! created inside a stateful callback cannot inherit from an ancestor's
//! class, because the stylesheet pass has already walked the tree by
//! then.

use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
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
    });
}

/// The pending-rebuild queue and the stateful-deps registry are
/// process-global, so one test's `Signal::set` can queue work another
/// test's `process_pending_subtree_rebuilds` drains. Take turns.
fn rebuild_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

/// Widest leaf: containers stretch to the host, so only a childless
/// node reports the text's own measured width.
fn widest_leaf(tree: &RenderTree) -> f32 {
    let mut max = 0.0f32;
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        let kids = tree.layout_tree.children(id);
        if kids.is_empty()
            && let Some(l) = tree.layout_tree.get_layout(id)
        {
            max = max.max(l.size.width);
        }
        stack.extend(kids);
    }
    max
}

/// Compile `src`, set `signal` short then long, and return the widest
/// leaf before and after.
fn widths(src: &str, file: &str, signal: &str) -> (f32, f32) {
    let _guard = rebuild_lock();
    init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_source(src, file).expect("compile");
    dsl.set_signal_string(signal, "ab");

    let host = div().w(600.0).h(200.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 200.0);
    let before = widest_leaf(&tree);

    dsl.set_signal_string(signal, "a considerably longer string");
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(600.0, 200.0);
    (before, widest_leaf(&tree))
}

#[test]
fn label_text_follows_its_signal() {
    let (before, after) = widths(
        r#"
        signal lbl_text: string
        view { Div { cn.Label(lbl_text) } }
        "#,
        "label_bound.blinc",
        "lbl_text",
    );
    assert!(
        after > before,
        "cn.Label must re-render: {before} -> {after}"
    );
}

#[test]
fn alert_message_follows_its_signal() {
    let (before, after) = widths(
        r#"
        signal alert_msg: string
        view { Div { cn.Alert(alert_msg, variant = "warning") } }
        "#,
        "alert_bound.blinc",
        "alert_msg",
    );
    assert!(
        after > before,
        "cn.Alert must re-render: {before} -> {after}"
    );
}

/// A literal still works, and takes the non-stateful path.
#[test]
fn literal_text_still_renders() {
    init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_source(
        r#"view { Div { cn.Label("static") cn.Alert("static", variant = "success") } }"#,
        "text_literal.blinc",
    )
    .expect("compile");
    let host = div().w(600.0).h(200.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 200.0);
    assert!(
        widest_leaf(&tree) > 0.0,
        "a literal label must still measure"
    );
}
