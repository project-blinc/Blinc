use super::{CapturedFrame, LEVELS, Report, difference};

fn report(changes: &[(usize, u8)]) -> Report {
    let reference = CapturedFrame::new([20, 30, 40, 255].repeat(1024), 32, 32);
    let mut painted = reference.clone();
    for &(pixel, delta) in changes {
        painted.data[pixel * 4] += delta;
    }
    let diff = difference(&reference, &painted, LEVELS);
    Report {
        name: "edge noise",
        steps: 1,
        per_step: vec![(diff.differing, diff.max_channel)],
    }
}

#[test]
fn rounding_allowance_keeps_larger_rendering_changes_visible() {
    for level in [3, 4] {
        assert_eq!(report(&[(0, level)]).first_row_mismatch(), None);
    }
    assert_eq!(
        report(&[(0, 3), (1, 3)]).first_row_mismatch(),
        Some((0, (2, 3))),
    );
    assert_eq!(report(&[(0, 5)]).first_row_mismatch(), Some((0, (1, 5))));
    let changes: Vec<_> = (0..506).map(|pixel| (pixel, 37)).collect();
    assert_eq!(report(&changes).first_row_mismatch(), Some((0, (506, 37))));
}
