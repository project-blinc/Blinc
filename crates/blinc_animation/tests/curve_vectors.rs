//! The descriptor maths against `vectors/curves.txt`, which is computed in float64 from the maths
//! alone (see `vectors/generate.py`). The shader is checked against the same file in blinc_gpu.

use blinc_animation::{AnimationDescriptor, Curve, PowerMode, StepJump};

pub struct Case {
    pub descriptor: AnimationDescriptor,
    pub elapsed_ms: f64,
    pub value: f32,
    pub velocity: f32,
}

fn curve(kind: u32, p: [f32; 4], n: u32, jump: u32) -> Curve {
    match kind {
        0 => Curve::Linear,
        1 => Curve::CubicBezier(p[0], p[1], p[2], p[3]),
        2 => Curve::Steps(
            n,
            [
                StepJump::Start,
                StepJump::End,
                StepJump::None,
                StepJump::Both,
            ][jump as usize],
        ),
        3 => Curve::Power(
            p[0] as u8,
            [PowerMode::In, PowerMode::Out, PowerMode::InOut][jump as usize],
        ),
        4 => Curve::Spring {
            stiffness: p[0],
            damping: p[1],
            mass: p[2],
        },
        _ => panic!("kind {kind}"),
    }
}

pub fn cases() -> Vec<Case> {
    include_str!("vectors/curves.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|line| {
            let f: Vec<f64> = line
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            assert_eq!(f.len(), 14, "{line}");
            Case {
                descriptor: AnimationDescriptor {
                    start_ms: 0.0,
                    duration_ms: f[9] as f32,
                    from: f[7] as f32,
                    to: f[8] as f32,
                    curve: curve(
                        f[0] as u32,
                        [f[1] as f32, f[2] as f32, f[3] as f32, f[4] as f32],
                        f[5] as u32,
                        f[6] as u32,
                    ),
                    initial_velocity: f[10] as f32,
                },
                elapsed_ms: f[11],
                value: f[12] as f32,
                velocity: f[13] as f32,
            }
        })
        .collect()
}

/// The error allowed: a hundred-thousandth of the distance moved, and at least of one unit.
fn scale(d: &AnimationDescriptor) -> f32 {
    (d.to - d.from).abs().max(1.0)
}

#[test]
fn the_cpu_evaluation_matches_the_vectors() {
    let cases = cases();
    assert!(cases.len() > 700);
    let mut worst: f32 = 0.0;
    for case in &cases {
        let d = &case.descriptor;
        let got = d.evaluate(case.elapsed_ms);
        let err = (got - case.value).abs() / scale(d);
        worst = worst.max(err);
        assert!(
            err < 1e-5,
            "{:?} at {} ms: value {got}, expected {}",
            d.curve,
            case.elapsed_ms,
            case.value
        );
    }
    eprintln!("worst value error {worst:e} over {} cases", cases.len());
}

#[test]
fn the_cpu_velocity_matches_the_vectors() {
    for case in cases() {
        let d = &case.descriptor;
        let got = d.velocity(case.elapsed_ms);
        let allowed = 1e-4 * case.velocity.abs().max(scale(d) * 10.0);
        assert!(
            (got - case.velocity).abs() < allowed,
            "{:?} at {} ms: velocity {got}, expected {}",
            d.curve,
            case.elapsed_ms,
            case.velocity
        );
    }
}
