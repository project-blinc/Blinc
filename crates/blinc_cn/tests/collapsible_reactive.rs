//! A collapsible follows its bound state.
//!
//! It samples `is_open` once for its initial spring values, so before
//! this it only moved through `set_open` / `toggle` on the builder — a
//! section bound to a signal written from anywhere else changed the
//! signal and stayed put. That is the whole point of taking a
//! `State<bool>` rather than an internal flag.
use blinc_core::reactive::{State, global_graph};
use std::sync::{Arc, Mutex};

fn bool_state(initial: bool) -> State<bool> {
    State::new(
        blinc_core::reactive::signal::<bool>(initial),
        global_graph(),
        blinc_core::reactive::global_dirty_flag(),
    )
}

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
                global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// Writing the state retargets the fold springs.
#[test]
fn writing_the_state_retargets_the_springs() {
    init();
    let open = bool_state(false);
    let c = blinc_cn::CollapsibleBuilder::new(&open);
    let scale = c.scale_anim();

    assert_eq!(
        scale.lock().unwrap().target(),
        0.0,
        "starts folded, matching the state"
    );

    open.set(true);
    assert_eq!(
        scale.lock().unwrap().target(),
        1.0,
        "the write reached the spring without anyone calling set_open"
    );

    open.set(false);
    assert_eq!(scale.lock().unwrap().target(), 0.0, "and back");
}

/// An initially-open section starts open rather than animating in.
#[test]
fn an_initially_open_section_starts_open() {
    init();
    let open = bool_state(true);
    let c = blinc_cn::CollapsibleBuilder::new(&open);
    assert_eq!(c.scale_anim().lock().unwrap().target(), 1.0);
}
