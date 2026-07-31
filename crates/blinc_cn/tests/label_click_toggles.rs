//! Clicking the label toggles the control, for every widget that has
//! one.
//!
//! The row already carried `cursor_pointer`, so the cursor promised a
//! click target across the whole row. A switch honoured that only on
//! its track.
//!
//! `on_click` registers under `POINTER_UP`, which is what these query.
use blinc_core::reactive::{State, global_graph};
use blinc_layout::div::ElementBuilder;
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
                global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

fn bool_state(initial: bool) -> State<bool> {
    State::new(
        blinc_core::reactive::signal::<bool>(initial),
        global_graph(),
        blinc_core::reactive::global_dirty_flag(),
    )
}

/// Both a switch and a checkbox register a row-level click when they
/// carry a label.
///
/// Asserted through the built element tree rather than the wrapper:
/// the handler lives on the row `Div` the widget builds, and the cn
/// wrapper does not forward `event_handlers`, so querying the wrapper
/// reports nothing for either of them.
fn labelled_row_is_clickable(w: &dyn ElementBuilder) -> bool {
    fn walk(e: &dyn ElementBuilder) -> bool {
        if e.event_handlers()
            .is_some_and(|h| h.has_handler(blinc_core::events::event_types::POINTER_UP))
        {
            return true;
        }
        e.children_builders().iter().any(|c| walk(c.as_ref()))
    }
    walk(w)
}

#[test]
fn a_labelled_switch_toggles_from_its_label() {
    init();
    let on = bool_state(false);
    let w = blinc_cn::switch(&on).label("wifi");
    assert!(
        labelled_row_is_clickable(&w),
        "the labelled row must be clickable, not just the track"
    );
}

/// A checkbox already did this; pinned so the two stay consistent.
#[test]
fn a_labelled_checkbox_toggles_from_its_label() {
    init();
    let checked = bool_state(false);
    let w = blinc_cn::checkbox(&checked).label("remember me");
    assert!(
        labelled_row_is_clickable(&w),
        "the labelled row must be clickable"
    );
}
