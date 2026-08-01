//! `cn.Dialog` — the signal is the handle.
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

fn build(dsl: &BlincDsl) {
    let host = div().w(600.0).h(400.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 400.0);
}

/// Opening happens from inside a build, which reaches into the overlay
/// stack. A closed dialog stays off, an open one goes up exactly once
/// however many times the tree is built, and clearing the signal takes
/// it down.
#[test]
fn the_signal_drives_the_modal_and_building_twice_does_not_stack_it() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal confirm: bool = false

           view {
             Div {
               cn.Button("Delete", on_click = || confirm.set(true))
               cn.Dialog(open = confirm, title = "Delete this row?",
                         description = "This cannot be undone.",
                         destructive = true) {
                 cn.Label("The row and its history go with it.")
               }
             }
           }"#,
        "dialog.blinc",
    )
    .expect("compile");

    // Live, not merely present: closing marks an overlay exiting and
    // leaves it in the stack while its motion plays out, so counting
    // entries would call a closing dialog open.
    let open_count = || {
        blinc_layout::overlay_state::overlay_stack()
            .lock()
            .map(|s| s.iter_top_down().filter(|e| !e.exiting).count())
            .unwrap_or(0)
    };

    build(&dsl);
    assert_eq!(open_count(), 0, "a closed dialog puts nothing up");

    dsl.set_signal_bool("confirm", true);
    build(&dsl);
    let after_open = open_count();
    assert!(after_open > 0, "setting the signal raised the modal");

    // The watcher runs on every build, so this is the shape that would
    // stack a second copy behind the first.
    build(&dsl);
    build(&dsl);
    assert_eq!(
        open_count(),
        after_open,
        "building again does not stack another copy"
    );

    dsl.set_signal_bool("confirm", false);
    build(&dsl);
    assert_eq!(open_count(), 0, "clearing the signal takes it down");
}
