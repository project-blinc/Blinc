//! Keep DOM wheel gestures distinct while coalescing their frame updates.

use std::collections::VecDeque;

const GESTURE_GAP_MS: f64 = 40.0;

pub(crate) struct WheelDelta {
    pub x: f32,
    pub y: f32,
    pub starts_gesture: bool,
}

#[derive(Default)]
pub(crate) struct WheelInput {
    last: Option<(f64, f32, f32)>,
    interval: Option<f64>,
    peak: f32,
    tail_min: Option<f32>,
    pending: VecDeque<WheelDelta>,
}

impl WheelInput {
    pub fn push(&mut self, x: f32, y: f32, timestamp: f64) {
        if !x.is_finite() || !y.is_finite() || !timestamp.is_finite() || (x == 0.0 && y == 0.0) {
            return;
        }
        let magnitude = x.hypot(y);
        let quiet_gap = self
            .interval
            .map_or(120.0, |interval| (interval * 1.5).max(GESTURE_GAP_MS));
        // DOM wheels omit touch/lift phases. A pause, reversal, or renewed
        // acceleration after a momentum tail marks another finger stroke.
        let starts_gesture = self.last.is_none_or(|(at, dx, dy)| {
            timestamp < at
                || timestamp - at >= quiet_gap
                || (magnitude >= 3.0 && x * dx + y * dy < 0.0)
                || self
                    .tail_min
                    .is_some_and(|minimum| magnitude >= 3.0 && magnitude >= minimum * 2.0)
        });
        if starts_gesture {
            self.peak = magnitude;
            self.tail_min = None;
        } else {
            if let Some((at, _, _)) = self.last {
                let elapsed = timestamp - at;
                if elapsed > 0.0 {
                    self.interval = Some(
                        self.interval
                            .map_or(elapsed, |interval| interval * 0.75 + elapsed * 0.25),
                    );
                }
            }
            self.peak = self.peak.max(magnitude);
            if let Some(minimum) = &mut self.tail_min {
                *minimum = minimum.min(magnitude);
            } else if magnitude < self.peak / 3.0 {
                self.tail_min = Some(magnitude);
            }
        }
        self.last = Some((timestamp, x, y));
        if !starts_gesture && let Some(delta) = self.pending.back_mut() {
            delta.x += x;
            delta.y += y;
        } else {
            self.pending.push_back(WheelDelta {
                x,
                y,
                starts_gesture,
            });
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn pop(&mut self) -> Option<WheelDelta> {
        self.pending.pop_front()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_strokes_keep_their_boundaries_before_a_frame_runs() {
        let mut input = WheelInput::default();
        for start in [0.0, 200.0, 400.0, 600.0] {
            for i in 0..8 {
                input.push(-20.0, 0.0, start + i as f64 * 16.0);
            }
        }
        for _ in 0..4 {
            let delta = input.pop().unwrap();
            assert!(delta.starts_gesture);
            assert_eq!(delta.x, -160.0);
        }
        assert!(input.is_empty());
    }

    #[test]
    fn a_momentum_tail_does_not_start_another_gesture() {
        let mut input = WheelInput::default();
        for (i, x) in [3.0, 8.0, 16.0, 30.0, 24.0, 18.0, 9.0, 4.0, 2.0, 1.0]
            .into_iter()
            .enumerate()
        {
            input.push(x, 0.0, i as f64 * 16.0);
        }
        assert!(input.pop().unwrap().starts_gesture);
        assert!(input.is_empty());
    }

    #[test]
    fn lower_frequency_wheel_events_still_form_whole_strokes() {
        let mut input = WheelInput::default();
        for start in [0.0, 432.0, 864.0, 1296.0] {
            for i in 0..8 {
                input.push(-20.0, 0.0, start + i as f64 * 49.0);
            }
        }
        for _ in 0..4 {
            let delta = input.pop().unwrap();
            assert!(delta.starts_gesture);
            assert_eq!(delta.x, -160.0);
        }
        assert!(input.is_empty());
    }

    #[test]
    fn another_stroke_can_interrupt_the_momentum_tail_without_a_pause() {
        let mut input = WheelInput::default();
        for (i, x) in [20.0, 30.0, 12.0, 4.0, 1.0, 4.0, 12.0, 24.0]
            .into_iter()
            .enumerate()
        {
            input.push(x, 0.0, i as f64 * 16.0);
        }
        assert_eq!(input.pop().unwrap().x, 67.0);
        let next = input.pop().unwrap();
        assert!(next.starts_gesture);
        assert_eq!(next.x, 40.0);
        assert!(input.is_empty());
    }

    #[test]
    fn gradual_strokes_after_a_tail_restart_without_a_click_or_idle_gap() {
        let mut input = WheelInput::default();
        let profile = [
            1.0, 1.25, 1.6, 2.0, 2.5, 3.1, 3.8, 4.7, 5.7, 7.0, 8.6, 10.5, 12.8, 15.6, 19.0, 23.0,
            28.0, 34.0, 28.0, 20.0, 14.0, 9.0, 6.0, 4.0, 3.0, 2.0, 1.0,
        ];
        let mut starts = 0;
        for stroke in 0..4 {
            for (i, dx) in profile.into_iter().enumerate() {
                input.push(dx, 0.0, (stroke * profile.len() + i) as f64 * 16.0);
                // Drain each event, as a responsive browser frame loop does.
                starts += usize::from(input.pop().unwrap().starts_gesture);
            }
        }
        assert_eq!(
            starts, 4,
            "each finger stroke starts a fresh calendar swipe"
        );
    }

    #[test]
    fn reversing_direction_preserves_both_strokes() {
        let mut input = WheelInput::default();
        input.push(-60.0, 0.0, 0.0);
        input.push(60.0, 0.0, 16.0);
        assert_eq!(input.pop().unwrap().x, -60.0);
        assert_eq!(input.pop().unwrap().x, 60.0);
    }

    #[test]
    fn frame_drains_do_not_split_a_gesture_and_leave_discards_pending_input() {
        let mut input = WheelInput::default();
        input.push(0.0, -10.0, 0.0);
        assert!(input.pop().unwrap().starts_gesture);
        input.push(0.0, -12.0, 16.0);
        assert!(!input.pop().unwrap().starts_gesture);
        input.push(0.0, -8.0, 32.0);
        input.reset();
        assert!(input.is_empty());
        input.push(0.0, -4.0, 48.0);
        assert!(input.pop().unwrap().starts_gesture);
    }
}
