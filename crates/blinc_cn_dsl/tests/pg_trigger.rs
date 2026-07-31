//! The entry file's FSM must stay addressable by its source name.
//!
//! `compile_project` used to namespace every file including the entry,
//! so `fsm Play` registered as `main$Play`. Nothing looking it up by
//! `"Play"` could find it: not the host, and not a `Play.trigger(...)`
//! handler in the entry's own view. Buttons did nothing.
//!
//! Also serves as a headless trigger probe -- firing each transition and
//! re-rendering exercises the ctx-signal -> derived -> rebuild path,
//! which is where a deadlock would show up.

use blinc_dsl_core::BlincDsl;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// `@stateful` builds through `BlincContextState`, so the playground
/// needs the app singletons a real host would have installed.
fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            // Two globals hold a scheduler: this one, and
            // `blinc_layout::render_state`'s, which a real app fills
            // from `RenderState::new`. Widgets that animate read the
            // second, so a test that sets only the first panics with a
            // message naming neither.
            blinc_layout::render_state::set_global_scheduler(s.handle());
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

#[test]
fn entry_fsm_is_addressable_by_source_name() {
    let _ = tracing_subscriber::fmt::try_init();
    init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile");

    for ev in ["Grow", "Grow", "Busy", "Reset"] {
        let dispatched = blinc_runtime::fsm::dispatch_default("Play", ev);
        assert!(
            dispatched.is_some(),
            "`{ev}` must dispatch against the unmangled entry FSM name"
        );
        // Re-render so the derived reads and any rebuild actually run.
        drop(dsl.view_widget());
    }
}
