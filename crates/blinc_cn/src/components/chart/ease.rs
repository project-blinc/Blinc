//! The values a chart shows while it eases from what it drew to new data.

use std::sync::{Arc, Mutex};

use blinc_animation::{SharedAnimatedValue, SpringConfig};

struct State {
    from: Vec<Vec<f64>>,
    to: Vec<Vec<f64>>,
    /// What the drawing is heading to, to tell new data.
    seen: Option<Vec<Vec<f64>>>,
}

/// Rows of values easing from those on screen to the latest given.
pub(super) struct Easer {
    state: Mutex<State>,
    /// From 0 to 1 while easing; none without a scheduler, which shows the
    /// new values at once.
    progress: Option<SharedAnimatedValue>,
    /// Whether the first values grow from zero rather than appear.
    sweep_in: bool,
}

impl Easer {
    pub fn new(sweep_in: bool) -> Self {
        let progress = blinc_layout::render_state::get_global_scheduler().map(|handle| {
            // Critically damped: it settles on the new values without
            // passing them.
            Arc::new(Mutex::new(blinc_animation::AnimatedValue::new(
                handle,
                1.0,
                SpringConfig::new(120.0, 22.0, 1.0),
            )))
        });
        Self {
            state: Mutex::new(State {
                from: Vec::new(),
                to: Vec::new(),
                seen: None,
            }),
            progress,
            sweep_in,
        }
    }

    fn progress(&self) -> f64 {
        self.progress
            .as_ref()
            .map_or(1.0, |p| p.lock().unwrap().get().clamp(0.0, 1.0) as f64)
    }

    /// Start easing to `target` from what is on screen, if it is new.
    pub fn note(&self, target: &[Vec<f64>]) {
        let mut s = self.state.lock().unwrap();
        if s.seen.as_deref() == Some(target) {
            return;
        }
        let first = s.seen.is_none();
        s.from = if first && !self.sweep_in {
            target.to_vec()
        } else {
            blend(&s.from, &s.to, self.progress())
        };
        s.to = target.to_vec();
        s.seen = Some(target.to_vec());
        if let Some(p) = &self.progress {
            let mut p = p.lock().unwrap();
            p.set_immediate(0.0);
            p.set_target(1.0);
        }
    }

    /// The values to draw now.
    pub fn shown(&self) -> Vec<Vec<f64>> {
        let s = self.state.lock().unwrap();
        blend(&s.from, &s.to, self.progress())
    }

    /// Whether the drawing has to go on: still easing, or `target` is new.
    pub fn wants_frames(&self, target: Option<&[Vec<f64>]>) -> bool {
        let easing = self
            .progress
            .as_ref()
            .is_some_and(|p| p.lock().unwrap().is_animating());
        easing || self.state.lock().unwrap().seen.as_deref() != target
    }
}

/// `from` moved `t` of the way to `to`; a value with nothing before it
/// starts from zero.
fn blend(from: &[Vec<f64>], to: &[Vec<f64>], t: f64) -> Vec<Vec<f64>> {
    to.iter()
        .enumerate()
        .map(|(s, row)| {
            row.iter()
                .enumerate()
                .map(|(i, b)| {
                    let a = from.get(s).and_then(|r| r.get(i)).copied().unwrap_or(0.0);
                    a + (b - a) * t
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_blend_towards_the_new_ones() {
        let from = vec![vec![0.0, 10.0]];
        let to = vec![vec![10.0, 20.0, 30.0]];
        assert_eq!(blend(&from, &to, 0.5), vec![vec![5.0, 15.0, 15.0]]);
    }

    #[test]
    fn without_a_scheduler_new_values_show_at_once() {
        let easer = Easer::new(true);
        if easer.progress.is_some() {
            return;
        }
        let (first, next) = (vec![vec![4.0]], vec![vec![9.0, 1.0]]);
        easer.note(&first);
        assert_eq!(easer.shown(), first);
        easer.note(&next);
        assert_eq!(easer.shown(), next);
        assert!(!easer.wants_frames(Some(&next)));
        assert!(easer.wants_frames(Some(&first)), "other data is new");
    }
}
