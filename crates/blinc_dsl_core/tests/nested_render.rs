//! Render dispatch through the JIT renderer.
//!
//! NOTE: there is no test here for the nested case -- a component
//! rendered from inside another render -- because nothing in the DSL
//! calls back into the JIT from within a render yet. The first thing
//! that will is a scoped `@stateful`, and the test belongs with it. An
//! earlier version of this file claimed to cover it and did not: the
//! two renders ran in sequence, not nested, so it passed with the
//! reentrancy hazard fully present.
use blinc_dsl_core::BlincDsl;
use std::sync::Arc;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();

        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

const SRC: &str = r#"
component Panel {
    view {
        Div {
            Text("panel")
        }
    }
}
view { Div { Panel() } }
"#;

/// The lock must still serialise: two renders in sequence both work,
/// and the runtime is not left poisoned or half-locked.
#[test]
fn repeated_renders_still_work() {
    init();
    let dsl = BlincDsl::new().expect("dsl");
    dsl.compile_source(SRC, "repeat.blinc").expect("compile");
    let renderer = dsl.view_renderer();
    for i in 0..3 {
        assert!(
            blinc_runtime::view::render_main(&renderer).is_ok(),
            "render {i} must succeed"
        );
    }
}
