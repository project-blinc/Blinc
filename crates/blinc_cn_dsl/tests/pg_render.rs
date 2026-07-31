//! The playground must build a real subtree, headlessly.
//!
//! Regression guard for a self-deadlock: feeding a `computed { }` into
//! a cn widget put a `Computed` read inside another compute closure, so
//! `Computed::try_get` re-locked the graph mutex on a thread that
//! already held it. The app hung during element-tree hashing, before
//! the window ever painted, with no error.

use blinc_dsl_core::BlincDsl;
use blinc_layout::div::ElementBuilder;
use blinc_layout::tree::LayoutTree;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let scheduler = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(scheduler.handle());
            // Two globals hold a scheduler: this one, and
            // `blinc_layout::render_state`'s, which a real app fills
            // from `RenderState::new`. Widgets that animate read the
            // second, so a test that sets only the first panics with a
            // message naming neither.
            blinc_layout::render_state::set_global_scheduler(scheduler.handle());
            Box::leak(Box::new(scheduler));
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

#[test]
fn playground_builds_a_subtree_without_deadlocking() {
    let _ = tracing_subscriber::fmt::try_init();
    init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile");

    let widget = dsl.view_widget();
    let mut tree = LayoutTree::new();
    let root_id = widget.build(&mut tree);
    let mut n = 0;
    let mut stack = vec![root_id];
    while let Some(id) = stack.pop() {
        n += 1;
        stack.extend(tree.children(id));
    }
    assert!(
        n > 30,
        "playground should build a large subtree, got {n} nodes"
    );
}
