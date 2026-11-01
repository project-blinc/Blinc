//! Wheel stepping shared by the numeric controls.

/// One wheel line is this many layout units, as the runners normalise it.
pub(crate) const WHEEL_STEP_UNITS: f32 = 40.0;

/// The whole steps a wheel delta adds up to, wheel up positive.
///
/// Trackpad deltas accumulate in `remainder` until a whole step, and
/// reversing the direction clears what is left; the caller clears it when the
/// pointer leaves. A mostly horizontal gesture is not a step and returns
/// `None`, so the caller lets it pass on.
pub(crate) fn wheel_steps(remainder: &mut f32, delta_x: f32, delta_y: f32) -> Option<i32> {
    if delta_y == 0.0 || delta_x.abs() > delta_y.abs() {
        return None;
    }
    if *remainder * delta_y < 0.0 {
        *remainder = 0.0;
    }
    *remainder += delta_y;
    let steps = (*remainder / WHEEL_STEP_UNITS).trunc();
    *remainder -= steps * WHEEL_STEP_UNITS;
    Some(steps as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trackpad_deltas_add_up_to_a_step() {
        let mut left = 0.0;
        assert_eq!(wheel_steps(&mut left, 0.0, 25.0), Some(0));
        assert_eq!(wheel_steps(&mut left, 0.0, 25.0), Some(1));
        assert_eq!(left, 10.0);
    }

    #[test]
    fn reversing_drops_the_remainder() {
        let mut left = 30.0;
        assert_eq!(wheel_steps(&mut left, 0.0, -10.0), Some(0));
        assert_eq!(left, -10.0);
    }

    #[test]
    fn a_sideways_gesture_is_not_a_step() {
        let mut left = 0.0;
        assert_eq!(wheel_steps(&mut left, 50.0, 10.0), None);
        assert_eq!(left, 0.0);
    }
}
