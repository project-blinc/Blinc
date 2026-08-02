//! Signal-driven overlays: dialog, sheet, drawer.
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
        // The app installs this; without it a signal write reaches
        // property bindings but never the statefuls that declared deps
        // on it, so a subscribed widget looks unsubscribed.
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

/// One tree, rebuilt in place — what a running app does.
///
/// Building a FRESH tree on every check hides the bug the app hit: the
/// watcher would run on the new build regardless of whether anything
/// subscribed to the signal, so a dialog that never subscribes still
/// looked like it opened.
struct Harness {
    tree: RenderTree,
}

impl Harness {
    fn new(dsl: &BlincDsl) -> Self {
        let host = div().w(600.0).h(400.0).child_box(dsl.view_widget());
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(600.0, 400.0);
        Self { tree }
    }

    /// Apply whatever a signal write queued, as a frame would.
    fn frame(&mut self) {
        self.tree.process_pending_subtree_rebuilds();
        self.tree.compute_layout(600.0, 400.0);
    }
}

/// Every signal-driven overlay, in one test: they share one process-wide
/// overlay stack, so running them in parallel would have each one
/// counting the others' modals.
///
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

    let mut app = Harness::new(&dsl);
    assert_eq!(open_count(), 0, "a closed dialog puts nothing up");

    // The write alone has to reach the watcher — nothing else in this
    // view depends on `confirm`, so if the dialog does not subscribe,
    // no rebuild happens and no modal appears.
    dsl.set_signal_bool("confirm", true);
    app.frame();
    let after_open = open_count();
    assert!(after_open > 0, "setting the signal raised the modal");

    // The watcher re-renders on every frame, so this is the shape that
    // would stack a second copy behind the first.
    app.frame();
    app.frame();
    assert_eq!(
        open_count(),
        after_open,
        "later frames do not stack another copy"
    );

    dsl.set_signal_bool("confirm", false);
    app.frame();
    assert_eq!(open_count(), 0, "clearing the signal takes it down");

    a_sheet_and_a_drawer_follow_their_own_signals();
    an_unwound_overlay_does_not_reopen_itself();
}

/// A sheet and a drawer follow their signals the same way, and two
/// overlays on two signals do not interfere.
fn a_sheet_and_a_drawer_follow_their_own_signals() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal filters: bool = false
           signal nav: bool = false

           view {
             Div {
               cn.Sheet(open = filters, side = "right", title = "Filters") {
                 cn.Label("narrow it down")
               }
               cn.Drawer(open = nav, title = "Navigation") {
                 cn.Label("go somewhere")
               }
             }
           }"#,
        "sheet_drawer.blinc",
    )
    .expect("compile");

    let live = || {
        blinc_layout::overlay_state::overlay_stack()
            .lock()
            .map(|s| s.iter_top_down().filter(|e| !e.exiting).count())
            .unwrap_or(0)
    };

    let mut app = Harness::new(&dsl);
    assert_eq!(live(), 0);

    dsl.set_signal_bool("filters", true);
    app.frame();
    assert_eq!(live(), 1, "the sheet is out");

    dsl.set_signal_bool("nav", true);
    app.frame();
    assert_eq!(live(), 2, "and the drawer joins it");

    // Closing the TOP one leaves what is under it.
    dsl.set_signal_bool("nav", false);
    app.frame();
    assert_eq!(live(), 1, "closing the drawer leaves the sheet");

    dsl.set_signal_bool("filters", false);
    app.frame();
    assert_eq!(live(), 0);
}

/// Closing an overlay unwinds everything stacked above it — correct
/// overlay semantics, and a problem for signal-driven ones: the
/// unwound overlay's signal is still set, so it must not creep back on
/// the next frame.
fn an_unwound_overlay_does_not_reopen_itself() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal lower: bool = false
           signal upper: bool = false

           view {
             Div {
               cn.Sheet(open = lower, title = "Lower") { cn.Label("under") }
               cn.Drawer(open = upper, title = "Upper") { cn.Label("over") }
             }
           }"#,
        "unwind.blinc",
    )
    .expect("compile");

    let live = || {
        blinc_layout::overlay_state::overlay_stack()
            .lock()
            .map(|s| s.iter_top_down().filter(|e| !e.exiting).count())
            .unwrap_or(0)
    };

    let mut app = Harness::new(&dsl);
    dsl.set_signal_bool("lower", true);
    app.frame();
    dsl.set_signal_bool("upper", true);
    app.frame();
    assert_eq!(live(), 2, "both are up");

    // Closing the lower one takes the upper with it.
    dsl.set_signal_bool("lower", false);
    app.frame();
    assert_eq!(live(), 0, "the unwind took both");

    // The upper's signal was never written, so this is the frame where
    // a naive watcher would see want=true, live=false, and raise it
    // again on top of nothing.
    app.frame();
    app.frame();
    assert_eq!(live(), 0, "and it stays down");
}
