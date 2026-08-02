//! A slider follows the number it is bound to, whoever writes it.
//!
//! The value can be kept in either precision, so a number input holding
//! an `f64` and a slider can share one number rather than two that are
//! kept in step. Writing it moves the thumb without rebuilding the
//! track: the spring is retargeted, and the value label is subscribed on
//! its own.
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::Arc;

const WIDTH: f32 = 300.0;
/// Medium thumb, and what the travel is `width - thumb`.
const TRAVEL: f32 = WIDTH - 18.0;

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

fn build(value: impl Into<blinc_cn::NumberValue>) {
    let host = div().w(400.0).h(80.0).child(
        blinc_cn::slider(value)
            .min(0.0)
            .max(100.0)
            .w(WIDTH)
            .label("Volume"),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 80.0);
}

/// The slider's own spring. A non-creating lookup on purpose: if the
/// widget didn't persist one, this is `None` and the test fails rather
/// than measuring a spring the test minted itself.
fn thumb(id: blinc_core::reactive::SignalId) -> blinc_layout::motion::SharedAnimatedValue {
    blinc_layout::stateful::try_persisted_animated_value(&format!(
        "cn-slider:{}:thumb",
        id.to_raw()
    ))
    .expect("the slider must persist its thumb spring")
}

/// Where the thumb is heading, which is what a write changes. Its
/// current position lags behind while the spring travels.
fn aiming_at(id: blinc_core::reactive::SignalId) -> f32 {
    thumb(id).lock().unwrap().target()
}

/// The case that did not work: a field bound to the same number is
/// typed into, and the slider has to follow it.
#[test]
fn a_write_from_elsewhere_moves_the_thumb() {
    init();
    let value = State::new(signal::<f64>(0.0), global_graph(), global_dirty_flag());
    build(&value);
    let id = value.signal_id();
    assert_eq!(aiming_at(id), 0.0, "starts at the floor");

    // Nothing rebuilds here. This is the write a number input makes.
    value.set(75.0);

    let expected = 0.75 * TRAVEL;
    let actual = aiming_at(id);
    assert!(
        (actual - expected).abs() < 1.0,
        "the thumb aims at three quarters of the travel: {actual} vs {expected}"
    );
}

/// An `f32` state works the same, which is what Rust callers pass.
#[test]
fn either_precision_drives_the_thumb() {
    init();
    let value = State::new(signal::<f32>(0.0), global_graph(), global_dirty_flag());
    build(&value);
    let id = value.signal_id();

    value.set(50.0);

    let expected = 0.5 * TRAVEL;
    let actual = aiming_at(id);
    assert!(
        (actual - expected).abs() < 1.0,
        "half the travel: {actual} vs {expected}"
    );
}

/// The spring has to be the SAME one across rebuilds. The binding that
/// drives it is registered once, so a fresh spring per build would leave
/// it steering a value nothing paints.
#[test]
fn the_spring_survives_a_rebuild() {
    init();
    let value = State::new(signal::<f64>(20.0), global_graph(), global_dirty_flag());
    build(&value);
    let first = thumb(value.signal_id());
    build(&value);
    let second = thumb(value.signal_id());
    assert!(
        Arc::ptr_eq(&first, &second),
        "the same spring, not a fresh one per build"
    );

    value.set(60.0);
    let expected = 0.6 * TRAVEL;
    let actual = aiming_at(value.signal_id());
    assert!(
        (actual - expected).abs() < 1.0,
        "and it still follows writes after a rebuild: {actual} vs {expected}"
    );
}
