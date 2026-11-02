//! Scroll gestures: which scrolls belong together, which are momentum, and
//! which element a gesture belongs to.
//!
//! A gesture runs from the fingers touching a trackpad until the momentum
//! after them stops. Its scrolls carry its number and whether they are
//! momentum. A gesture that a scroll handler with no physics of its own
//! takes stays with that handler, wherever the pointer goes, so a swipe
//! started on one element never drives another. Wheels report no phases, so
//! their scrolls are one long gesture that latches nothing.

use std::sync::Mutex;

use web_time::Instant;

use super::RenderTree;
use crate::tree::LayoutNodeId;

/// How soon after the fingers lift a touch is the platform starting
/// momentum, not a new touch: some platforms report both the same way.
const MOMENTUM_START_MS: u128 = 80;
/// How long after a lift scrolls can still be its momentum. A lift with none
/// after it ends the gesture this long after, so a wheel used later is not
/// mistaken for momentum.
const MOMENTUM_MAX_MS: u128 = 3000;

/// A change of phase the platform reports for a scroll gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollGesturePhase {
    /// Fingers touched, or momentum began.
    Touch,
    /// Fingers lifted, or momentum ended.
    Lift,
    /// Momentum ended.
    MomentumEnd,
}

#[derive(Default)]
pub(crate) struct ScrollGesture {
    id: u64,
    momentum: bool,
    lifted_at: Option<Instant>,
    latch: Option<LayoutNodeId>,
}

pub(crate) type SharedScrollGesture = Mutex<ScrollGesture>;

impl RenderTree {
    /// Follow the platform's scroll phase.
    pub fn on_scroll_gesture(&self, phase: ScrollGesturePhase) {
        let mut g = self.scroll_gesture.lock().unwrap();
        match phase {
            ScrollGesturePhase::Touch => {
                let momentum_starts = g
                    .lifted_at
                    .is_some_and(|t| t.elapsed().as_millis() < MOMENTUM_START_MS);
                if !momentum_starts {
                    Self::begin(&mut g);
                }
            }
            ScrollGesturePhase::Lift if g.momentum => Self::over(&mut g),
            ScrollGesturePhase::Lift => {
                g.momentum = true;
                g.lifted_at = Some(Instant::now());
            }
            ScrollGesturePhase::MomentumEnd => Self::over(&mut g),
        }
    }

    /// Start a new gesture, as fingers touching do.
    pub fn begin_scroll_gesture(&self) {
        Self::begin(&mut self.scroll_gesture.lock().unwrap());
    }

    fn begin(g: &mut ScrollGesture) {
        g.id += 1;
        g.momentum = false;
        g.lifted_at = None;
        g.latch = None;
    }

    fn over(g: &mut ScrollGesture) {
        g.momentum = false;
        g.lifted_at = None;
        g.latch = None;
    }

    /// The gesture's number and whether its scrolls are momentum now.
    pub(crate) fn scroll_gesture_now(&self) -> (u64, bool) {
        let mut g = self.scroll_gesture.lock().unwrap();
        if g.momentum
            && g.lifted_at
                .is_some_and(|t| t.elapsed().as_millis() > MOMENTUM_MAX_MS)
        {
            Self::over(&mut g);
        }
        (g.id, g.momentum)
    }

    /// The handler the gesture under way belongs to, while it is in the tree.
    pub(crate) fn latched_scroll_target(&self) -> Option<LayoutNodeId> {
        let latch = self.scroll_gesture.lock().unwrap().latch?;
        self.render_nodes.contains_key(&latch).then_some(latch)
    }

    /// Give the gesture under way to `node`, a scroll handler with no physics
    /// of its own, if it belongs to nothing yet.
    pub(crate) fn latch_scroll_target(&self, node: LayoutNodeId) {
        let mut g = self.scroll_gesture.lock().unwrap();
        if g.id > 0 && g.latch.is_none() && !self.scroll_physics.contains_key(&node) {
            g.latch = Some(node);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::div::div;

    #[test]
    fn a_touch_right_after_a_lift_is_momentum_and_a_later_one_a_new_gesture() {
        let tree = RenderTree::from_element(&div());
        tree.on_scroll_gesture(ScrollGesturePhase::Touch);
        let (first, momentum) = tree.scroll_gesture_now();
        assert!(!momentum);

        tree.on_scroll_gesture(ScrollGesturePhase::Lift);
        // The platform starts momentum at once, reported as a touch.
        tree.on_scroll_gesture(ScrollGesturePhase::Touch);
        assert_eq!(tree.scroll_gesture_now(), (first, true), "still momentum");

        // Momentum ends; fingers touch again later.
        tree.on_scroll_gesture(ScrollGesturePhase::Lift);
        std::thread::sleep(std::time::Duration::from_millis(
            MOMENTUM_START_MS as u64 + 20,
        ));
        tree.on_scroll_gesture(ScrollGesturePhase::Touch);
        let (second, momentum) = tree.scroll_gesture_now();
        assert!(second > first && !momentum, "a new gesture");
    }
}
