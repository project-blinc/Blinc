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
    for level in [3, 4, 8] {
        assert_eq!(report(&[(0, level)]).first_mismatch(), None);
    }
    let small_roundoff: Vec<_> = (0..8).map(|pixel| (pixel, 4)).collect();
    assert_eq!(report(&small_roundoff).first_mismatch(), None);
    let at_limit: Vec<_> = (0..24).map(|pixel| (pixel, 8)).collect();
    assert_eq!(report(&at_limit).first_mismatch(), None);
    let too_many: Vec<_> = (0..25).map(|pixel| (pixel, 3)).collect();
    assert_eq!(report(&too_many).first_mismatch(), Some((0, (25, 3))),);
    assert_eq!(report(&[(0, 9)]).first_mismatch(), Some((0, (1, 9))),);
    let changes: Vec<_> = (0..506).map(|pixel| (pixel, 37)).collect();
    assert_eq!(report(&changes).first_mismatch(), Some((0, (506, 37))));
}
