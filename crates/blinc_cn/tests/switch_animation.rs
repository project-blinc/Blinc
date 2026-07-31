//! The switch thumb must travel, not teleport.
//!
//! A bound `checked` takes the deps() rebuild path, so every toggle
//! reconstructs the switch. Springs built fresh at the destination on
//! each rebuild are already arrived, and the thumb jumps — once because
//! the in-flight spring from the click was discarded, and again when
//! something else writes the signal so no click handler ran at all.
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::Arc;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

fn build(checked: &State<bool>) {
    let host = div()
        .w(200.0)
        .h(60.0)
        .child(blinc_cn::switch(checked).label("busy"));
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(200.0, 60.0);
}

fn thumb_key(checked: &State<bool>) -> String {
    format!("cn-switch:{}:thumb", checked.signal_id().to_raw())
}

/// The switch's own spring. A non-creating lookup on purpose: if the
/// widget didn't persist one, this is `None` and the test fails rather
/// than silently measuring a spring the test minted itself.
fn thumb(checked: &State<bool>) -> blinc_layout::motion::SharedAnimatedValue {
    blinc_layout::stateful::try_persisted_animated_value(&thumb_key(checked))
        .expect("the switch must persist its thumb spring")
}

fn thumb_position(checked: &State<bool>) -> f32 {
    thumb(checked).lock().unwrap().get()
}

#[test]
fn a_switch_that_mounts_on_does_not_animate_into_place() {
    init();
    let checked = State::new(signal::<bool>(true), global_graph(), global_dirty_flag());
    build(&checked);
    let at_mount = thumb_position(&checked);
    assert!(
        at_mount > 0.0,
        "a switch built `on` starts at the far end, not sliding in: {at_mount}"
    );
}

/// The case that jumped: the signal is written by something other than
/// the switch, then the switch is rebuilt. It must be aiming at the new
/// end, not already sitting there.
#[test]
fn an_externally_toggled_switch_animates_rather_than_jumping() {
    init();
    let checked = State::new(signal::<bool>(false), global_graph(), global_dirty_flag());
    build(&checked);
    let start = thumb_position(&checked);
    assert_eq!(start, 0.0, "starts off, at the near end");

    // Something else flips it — an FSM transition, another control
    // bound to the same signal — and the switch rebuilds.
    checked.set(true);
    build(&checked);

    let after = thumb_position(&checked);
    assert!(
        after < 1.0,
        "the thumb must still be near the start, travelling: {after}"
    );
}

/// The spring has to be the SAME one across rebuilds, or each rebuild
/// restarts the travel from wherever the new one was constructed.
#[test]
fn the_spring_survives_a_rebuild() {
    init();
    let checked = State::new(signal::<bool>(false), global_graph(), global_dirty_flag());
    build(&checked);
    let first = thumb(&checked);
    build(&checked);
    let second = thumb(&checked);
    assert!(
        Arc::ptr_eq(&first, &second),
        "the same spring, not a fresh one per build"
    );
}
