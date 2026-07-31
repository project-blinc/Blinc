//! `@stateful @fsm([X])` must subscribe to X's context fields.
//!
//! A self transition (`Idle -> Idle`, the shape of an action that only
//! mutates context) leaves the FSM's state value unchanged, so a
//! stateful bound solely to the shared state never fires. The context
//! writes are the actual signal, so they have to be in the dep list.
//!
//! The opposite extreme -- subscribing to every declared signal -- made
//! unrelated signals re-render this stateful, and made the FSM's own
//! fields notify alongside the shared state, so one transition rendered
//! twice.

use blinc_dsl_core::BlincDsl;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

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
fn self_transition_updates_a_bound_stateful() {
    let _ = tracing_subscriber::fmt::try_init();
    init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_source(
        r#"
        fsm Flip {
            context { on: bool = false }
            state Idle
            initial Idle
            on Idle.Go -> Idle { ctx.on = true }
        }
        component Panel {
            @stateful @fsm([Flip]) view {
                Div {
                    if Flip.on.get() {
                        cn.Badge("on")
                    } else {
                        cn.Badge("off")
                    }
                }
            }
        }
        view { Panel() }
        "#,
        "flip.blinc",
    )
    .expect("compile");

    // The context field must be readable and must change on a self
    // transition -- that is what the stateful subscribes to.
    assert_eq!(
        blinc_runtime::signal::get_bool("__fsm_ctx_Flip_on"),
        Some(false)
    );
    assert!(
        blinc_runtime::fsm::dispatch_default("Flip", "Go").is_some(),
        "Flip must dispatch"
    );
    assert_eq!(
        blinc_runtime::signal::get_bool("__fsm_ctx_Flip_on"),
        Some(true),
        "a self transition must still write its context field"
    );
    drop(dsl.view_widget());
}
