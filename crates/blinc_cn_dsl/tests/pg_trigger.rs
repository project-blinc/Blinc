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

#[test]
fn entry_fsm_is_addressable_by_source_name() {
    let _ = tracing_subscriber::fmt::try_init();
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
