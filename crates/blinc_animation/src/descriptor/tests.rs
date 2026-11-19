use super::*;
use crate::{Easing, Spring};

const CONFIGS: [SpringConfig; 6] = [
    SpringConfig {
        stiffness: 300.0,
        damping: 20.0,
        mass: 1.0,
    },
    SpringConfig {
        stiffness: 100.0,
        damping: 20.0,
        mass: 1.0,
    },
    SpringConfig {
        stiffness: 100.0,
        damping: 10.0,
        mass: 1.0,
    },
    SpringConfig {
        stiffness: 170.0,
        damping: 26.0,
        mass: 1.0,
    },
    SpringConfig {
        stiffness: 3000.0,
        damping: 110.0,
        mass: 1.0,
    },
    SpringConfig {
        stiffness: 50.0,
        damping: 40.0,
        mass: 2.0,
    },
];

#[test]
fn a_timed_descriptor_holds_before_it_starts_and_after_it_ends() {
    let d = AnimationDescriptor::timed(100.0, 200.0, 10.0, 30.0, Curve::Linear);
    assert_eq!(d.evaluate(0.0), 10.0);
    assert_eq!(d.evaluate(100.0), 10.0);
    assert!((d.evaluate(200.0) - 20.0).abs() < 1e-5);
    assert_eq!(d.evaluate(300.0), 30.0);
    assert_eq!(d.evaluate(10_000.0), 30.0);
    assert!(!d.is_finished(299.0) && d.is_finished(300.0));
    assert_eq!(d.velocity(10_000.0), 0.0);
}

/// Each polynomial easing is the power curve `to_curve` names, to the last bit: nothing about an
/// animation changes by being described as data.
#[test]
fn an_easing_and_its_curve_agree() {
    let easings = [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
        Easing::EaseInQuad,
        Easing::EaseOutQuad,
        Easing::EaseInOutQuad,
        Easing::EaseInCubic,
        Easing::EaseOutCubic,
        Easing::EaseInOutCubic,
        Easing::EaseInQuart,
        Easing::EaseOutQuart,
        Easing::EaseInOutQuart,
        Easing::CubicBezier(0.25, 0.1, 0.25, 1.0),
        Easing::CubicBezier(0.42, 0.0, 0.58, 1.0),
        Easing::CubicBezier(0.34, 1.56, 0.64, 1.0),
    ];
    for easing in easings {
        let curve = easing.to_curve();
        for i in 0..=200 {
            let t = i as f32 / 200.0;
            let (want, got) = (easing.apply(t), curve.progress(t));
            assert!(
                (want - got).abs() < 1e-6,
                "{easing:?} at {t}: {want} against {got}"
            );
        }
    }
}

#[test]
fn steps_jump_where_css_says() {
    let at = |n, jump, t| steps(t, n, jump);
    // steps(4, end): 0, .25, .5, .75, then 1 at the end.
    assert_eq!(at(4, StepJump::End, 0.0), 0.0);
    assert_eq!(at(4, StepJump::End, 0.26), 0.25);
    assert_eq!(at(4, StepJump::End, 1.0), 1.0);
    // steps(4, start): the first jump is at 0.
    assert_eq!(at(4, StepJump::Start, 0.0), 0.25);
    assert_eq!(at(4, StepJump::Start, 0.99), 1.0);
    // steps(3, none): 0, .5, 1 with no jump at either end.
    assert_eq!(at(3, StepJump::None, 0.0), 0.0);
    assert_eq!(at(3, StepJump::None, 0.34), 0.5);
    assert_eq!(at(3, StepJump::None, 1.0), 1.0);
    // steps(2, both): .333, .667, 1.
    assert!((at(2, StepJump::Both, 0.0) - 1.0 / 3.0).abs() < 1e-6);
    assert_eq!(at(2, StepJump::Both, 1.0), 1.0);
}

/// The closed form is the same motion as the integrator, for under, critical and over damped
/// springs alike, from rest and with a velocity.
#[test]
fn a_closed_form_spring_matches_the_integrator() {
    for config in CONFIGS {
        for (from, to, v0) in [(0.0, 100.0, 0.0), (50.0, -20.0, 0.0), (0.0, 1.0, 5.0)] {
            let d = AnimationDescriptor::spring(0.0, from, to, v0, config);
            let mut spring = Spring::new(config, from);
            spring.set_target(to);
            spring.velocity_for_test(v0);
            let scale = (to - from)
                .abs()
                .max(v0.abs() / config.stiffness.sqrt())
                .max(1e-3);
            for frame in 1..=240 {
                // The integrator snaps to the target once it is within an absolute 0.01, which for
                // a small move is a large part of it. The closed form does not; compare up to there.
                if spring.is_settled() {
                    break;
                }
                spring.step(1.0 / 120.0);
                let now = frame as f64 * 1000.0 / 120.0;
                let (a, b) = (d.evaluate(now), spring.value());
                assert!(
                    (a - b).abs() < 2e-3 * scale,
                    "{config:?} {from}->{to} v0 {v0} at {now} ms: closed {a}, integrated {b}"
                );
            }
        }
    }
}

#[test]
fn a_spring_ends_at_its_target_with_no_velocity() {
    for config in CONFIGS {
        let d = AnimationDescriptor::spring(0.0, 0.0, 100.0, 0.0, config);
        assert!(
            d.duration_ms > 0.0 && d.duration_ms < 60_000.0,
            "{config:?}: {}",
            d.duration_ms
        );
        assert_eq!(d.evaluate(d.end_ms() + 1.0), 100.0);
        assert_eq!(d.velocity(d.end_ms() + 1.0), 0.0);
        // Just before it settles it is within a thousandth or two of the distance it travelled.
        assert!(
            (d.evaluate(d.end_ms() - 1.0) - 100.0).abs() <= 0.25,
            "{config:?}"
        );
    }
}

#[test]
fn a_spring_that_starts_at_its_target_with_no_velocity_is_finished() {
    let d = AnimationDescriptor::spring(5.0, 3.0, 3.0, 0.0, CONFIGS[0]);
    assert_eq!(d.duration_ms, 0.0);
    assert!(d.is_finished(5.0));
    assert_eq!(d.evaluate(5.0), 3.0);
}

/// Settling is relative to the motion, so the same spring over a distance of a thousand
/// takes as long as over a distance of one.
#[test]
fn settling_does_not_depend_on_the_units() {
    let small = AnimationDescriptor::spring(0.0, 0.0, 1.0, 0.0, CONFIGS[2]);
    let large = AnimationDescriptor::spring(0.0, 0.0, 1000.0, 0.0, CONFIGS[2]);
    assert!((small.duration_ms - large.duration_ms).abs() < 1.0);
}

/// The velocity is the slope of the value, for every kind of curve.
#[test]
fn velocity_is_the_slope_of_the_value() {
    let curves = [
        Curve::Linear,
        Curve::CubicBezier(0.42, 0.0, 0.58, 1.0),
        Curve::Power(3, PowerMode::InOut),
        Curve::Power(2, PowerMode::Out),
    ];
    for curve in curves {
        let d = AnimationDescriptor::timed(0.0, 400.0, 10.0, 90.0, curve);
        for now in [40.0, 123.0, 200.0, 311.0, 380.0] {
            let h = 0.05;
            let slope = (d.evaluate(now + h) - d.evaluate(now - h)) as f64 / (2.0 * h) * 1000.0;
            let v = d.velocity(now) as f64;
            assert!(
                (v - slope).abs() < 0.02 * slope.abs().max(1.0),
                "{curve:?} at {now}: {v} against {slope}"
            );
        }
    }
    for config in CONFIGS {
        let d = AnimationDescriptor::spring(0.0, 0.0, 100.0, 30.0, config);
        for now in [10.0, 55.0, 130.0, 260.0] {
            let h = 0.02;
            let slope = (d.evaluate(now + h) - d.evaluate(now - h)) as f64 / (2.0 * h) * 1000.0;
            let v = d.velocity(now) as f64;
            assert!(
                (v - slope).abs() < 0.02 * slope.abs().max(1.0),
                "{config:?} at {now}: {v} against {slope}"
            );
        }
    }
}

/// Retargeting a spring is continuous in value and in velocity, at any time, to any target.
#[test]
fn retargeting_a_spring_keeps_value_and_velocity() {
    for config in CONFIGS {
        let d = AnimationDescriptor::spring(0.0, 0.0, 100.0, 0.0, config);
        for now in [7.0, 31.0, 90.0, 177.0] {
            let r = d.retarget(now, -40.0);
            assert!(
                (r.evaluate(now) - d.evaluate(now)).abs() < 1e-3,
                "{config:?} value at {now}"
            );
            let (a, b) = (r.velocity(now), d.velocity(now));
            assert!(
                (a - b).abs() < 1e-3 * b.abs().max(1.0),
                "{config:?} velocity at {now}: {a} against {b}"
            );
            assert_eq!(r.to, -40.0);
        }
    }
}

/// Retargeting a timed animation starts a new run from where it is, over the whole duration
/// again, with no velocity carried: a CSS transition's rule.
#[test]
fn retargeting_a_timed_animation_restarts_from_the_current_value() {
    let d = AnimationDescriptor::timed(
        0.0,
        300.0,
        0.0,
        100.0,
        Curve::CubicBezier(0.42, 0.0, 0.58, 1.0),
    );
    let now = 100.0;
    let r = d.retarget(now, 40.0);
    assert_eq!(r.start_ms, now);
    assert_eq!(r.duration_ms, 300.0);
    assert!((r.from - d.evaluate(now)).abs() < 1e-6);
    assert_eq!(r.to, 40.0);
    assert_eq!(r.initial_velocity, 0.0);
    assert!((r.evaluate(now) - d.evaluate(now)).abs() < 1e-6);
}

#[test]
fn a_spring_taken_from_the_scheduler_describes_the_motion_it_is_in() {
    let mut spring = Spring::new(CONFIGS[3], 0.0);
    spring.set_target(100.0);
    for _ in 0..30 {
        spring.step(1.0 / 60.0);
    }
    let d = spring.to_descriptor(1000.0);
    assert!((d.evaluate(1000.0) - spring.value()).abs() < 1e-4);
    assert!(
        (d.velocity(1000.0) - spring.velocity()).abs() < 1e-3 * spring.velocity().abs().max(1.0)
    );
    assert_eq!(d.to, 100.0);
    // The descriptor carries on where the integrator would.
    let mut later = spring;
    for _ in 0..30 {
        later.step(1.0 / 60.0);
    }
    assert!(
        (d.evaluate(1500.0) - later.value()).abs() < 0.3,
        "{} against {}",
        d.evaluate(1500.0),
        later.value()
    );
}

#[test]
fn colours_mix_as_the_keyframe_lerp_does() {
    let a = [1.0, 0.0, 0.0, 1.0];
    let b = [0.0, 0.0, 1.0, 0.5];
    for i in 0..=10 {
        let t = i as f32 / 10.0;
        let mixed = lerp_color_premultiplied(a, b, t);
        let alpha = 1.0 + (0.5 - 1.0) * t;
        assert!((mixed[3] - alpha).abs() < 1e-6);
        // A fade from transparent keeps the opaque side's colour instead of going through black.
        let fade = lerp_color_premultiplied([0.0, 0.0, 0.0, 0.0], [1.0, 0.5, 0.25, 1.0], t);
        if t > 0.0 {
            assert!(
                (fade[0] - 1.0).abs() < 1e-5 && (fade[1] - 0.5).abs() < 1e-5,
                "{fade:?}"
            );
        }
    }
}
