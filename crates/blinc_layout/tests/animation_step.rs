//! A frame that comes late pauses a motion rather than jumping it ahead.

use std::sync::{Arc, Mutex};

use blinc_layout::element::{MotionAnimation, MotionKeyframe};
use blinc_layout::render_state::{MAX_ANIMATION_STEP_MS, RenderState};

#[test]
fn a_late_frame_moves_a_motion_by_at_most_the_longest_step() {
    let animations = Arc::new(Mutex::new(blinc_animation::AnimationScheduler::new()));
    let mut rs = RenderState::new(animations);
    let fade_in = MotionAnimation {
        enter_from: Some(MotionKeyframe {
            opacity: Some(0.0),
            ..Default::default()
        }),
        enter_duration_ms: 300,
        enter_delay_ms: 0,
        exit_to: None,
        exit_duration_ms: 0,
    };
    rs.start_stable_motion("probe", fade_in, false);
    rs.tick(1_000);
    let start = rs.motion_samples()[0].progress;

    // The next frame comes 200ms later.
    rs.tick(1_200);
    let progress = rs.motion_samples()[0].progress;
    assert!(
        progress - start <= MAX_ANIMATION_STEP_MS / 300.0 + 1e-3,
        "a 200ms frame moved the motion from {start} to {progress}"
    );
}
