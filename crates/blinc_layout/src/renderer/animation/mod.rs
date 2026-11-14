//! Animation systems on `RenderTree`.
//!
//! Three loosely-coupled subsystems live here:
//!
//! - [`flip`] — FLIP-style position transitions for elements whose
//!   layout bounds changed across a subtree rebuild. Keyed by stable
//!   element id so they survive node-id churn.
//! - (TODO) `visual` — `animate_bounds`-driven offsets layered on
//!   top of taffy bounds without modifying the layout tree itself.
//! - (TODO) `css` — keyframe / transition machinery that interprets
//!   CSS `animation:` and `transition:` declarations, with a
//!   companion store on `RenderTree`.
//!
//! Methods in each submodule are extra `impl RenderTree` blocks; the
//! struct definition + fields stay in `renderer/mod.rs`.

pub mod css;
pub mod flip;
pub mod visual;

use crate::renderer::RenderTree;

impl RenderTree {
    /// Whether an animation, transition or FLIP slide that is running, or has just stopped,
    /// keeps the frame from reusing the cached paint. One a binding carries is moved by rows, and
    /// a CSS animation baked into a layer is blitted, so neither does; a CSS animation or
    /// transition that is neither, one that lays out, a stopped one whose final values are still
    /// to be written, and a FLIP slide no binding carries, do. False when nothing is running.
    pub fn animation_needs_full_paint(&self) -> bool {
        self.has_flip_animations_off_rows() || !self.css_active_all_composite_promotable()
    }
}
