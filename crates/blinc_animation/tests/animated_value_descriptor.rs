//! An animated value describes the motion it is in, as data.

use blinc_animation::{AnimatedValue, AnimationScheduler, SpringConfig};

#[test]
fn a_value_in_motion_describes_that_motion() {
    let scheduler = AnimationScheduler::new();
    let mut value = AnimatedValue::new(scheduler.handle(), 0.0, SpringConfig::wobbly());
    assert!(
        value.descriptor().is_none(),
        "a value at rest has no motion to describe"
    );

    value.set_target(100.0);
    for _ in 0..10 {
        scheduler.tick_by(1.0 / 60.0);
    }
    let d = value.descriptor().expect("a value in motion");
    assert_eq!(d.to, 100.0);
    assert!(
        (d.evaluate(d.start_ms) - value.get()).abs() < 1e-3,
        "starts where the value is: {} against {}",
        d.evaluate(d.start_ms),
        value.get()
    );

    // The descriptor goes on where the spring goes.
    for _ in 0..30 {
        scheduler.tick_by(1.0 / 60.0);
    }
    let later = d.evaluate(d.start_ms + 500.0);
    assert!(
        (later - value.get()).abs() < 0.5,
        "after 500 ms: {later} against {}",
        value.get()
    );
}

#[test]
fn a_value_that_has_settled_describes_nothing() {
    let scheduler = AnimationScheduler::new();
    let mut value = AnimatedValue::new(scheduler.handle(), 0.0, SpringConfig::stiff());
    value.set_target(1.0);
    for _ in 0..600 {
        scheduler.tick_by(1.0 / 60.0);
    }
    assert!((value.get() - 1.0).abs() < 0.02);
    assert!(
        value
            .descriptor()
            .is_none_or(|d| d.is_finished(d.start_ms + 1.0))
    );
}
