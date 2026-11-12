use super::super::{init_context_state, init_overlay_context};
use blinc_app::windowed::WindowedContext;
use std::sync::{Arc, Barrier};

/// Sessions that start together set the process-wide state once between them: the check and the
/// set are one step, so none of them panics on a state another has just set.
#[test]
fn sessions_starting_together_set_the_context_state_once() {
    let sessions = 16;
    let start = Arc::new(Barrier::new(sessions));
    let threads: Vec<_> = (0..sessions)
        .map(|_| {
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                let ctx = WindowedContext::headless_with_scale(64.0, 64.0, 1.0);
                start.wait();
                init_context_state(&ctx);
                init_overlay_context(&ctx);
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("a session panicked while starting");
    }
    assert!(blinc_core::BlincContextState::is_initialized());
    assert!(blinc_layout::overlay_state::OverlayContext::is_initialized());
}
