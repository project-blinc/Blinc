//! When a mobile frame may reuse the cached static layer.
//!
//! The compositor path repaints only what moves over a cached layer: canvases and motion
//! bindings that the scheduler drives. Anything else that can change the picture needs a full
//! paint first. Each such change calls [`request_full_paint`]; a frame takes the incremental
//! path only when none was raised since the last one painted.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static FULL: AtomicBool = AtomicBool::new(true);
static LAST_FULL_MS: AtomicU64 = AtomicU64::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// The longest run of incremental frames before one full paint. A change that nothing flagged
/// then shows within this long, not until the next touch.
const REPAINT_AFTER_MS: u64 = 1000;

/// The next frame must paint the whole tree.
pub(crate) fn request_full_paint() {
    FULL.store(true, Ordering::Relaxed);
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Counts the requests so far. Equal values mean nothing that changes the picture was raised in
/// between, so a result derived from the tree can be reused.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub(crate) fn paint_generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// Whether this frame must paint the whole tree. Lowers the request.
pub(crate) fn take_full_paint(now_ms: u64) -> bool {
    let requested = FULL.swap(false, Ordering::Relaxed);
    let stale = now_ms.saturating_sub(LAST_FULL_MS.load(Ordering::Relaxed)) > REPAINT_AFTER_MS;
    let full = requested || stale;
    if full {
        LAST_FULL_MS.store(now_ms, Ordering::Relaxed);
    }
    full
}

/// Decides when the pointer and focus styles (`:hover`, `:active`, `:focus`) need applying.
///
/// The pass walks every styled element, so it runs when the event router's hover, press or focus
/// state differs from the last pass, or after something changed the tree: a rebuild, a prop or
/// subtree update, a new stylesheet. A frame that only advances animations skips it.
#[derive(Debug, Default)]
pub(crate) struct StateStyleGate {
    last: Option<u64>,
    force: bool,
}

impl StateStyleGate {
    /// The next pass runs whatever the router state is.
    pub(crate) fn force(&mut self) {
        self.force = true;
    }

    /// Whether to run the pass for this router state fingerprint. Records it as applied.
    pub(crate) fn should_apply(&mut self, fingerprint: u64) -> bool {
        let apply = self.force || self.last != Some(fingerprint);
        self.last = Some(fingerprint);
        self.force = false;
        apply
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paint_request_advances_the_generation() {
        let before = paint_generation();
        request_full_paint();
        assert!(paint_generation() > before);
    }

    #[test]
    fn the_first_pass_runs_then_an_unchanged_state_is_skipped() {
        let mut gate = StateStyleGate::default();
        assert!(gate.should_apply(7));
        assert!(!gate.should_apply(7));
        assert!(!gate.should_apply(7));
    }

    #[test]
    fn a_changed_router_state_runs_the_pass() {
        let mut gate = StateStyleGate::default();
        assert!(gate.should_apply(1));
        assert!(gate.should_apply(2));
        assert!(!gate.should_apply(2));
        assert!(gate.should_apply(1));
    }

    #[test]
    fn a_forced_pass_runs_once_with_an_unchanged_state() {
        let mut gate = StateStyleGate::default();
        assert!(gate.should_apply(3));
        gate.force();
        assert!(gate.should_apply(3));
        assert!(!gate.should_apply(3));
    }
}
