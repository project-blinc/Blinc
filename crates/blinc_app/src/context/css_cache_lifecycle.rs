//! Keep cached output aligned with active CSS sources and paint ownership.
use super::{RenderContext, RenderTree};
use slotmap::Key;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CssCacheOwner {
    animation: bool,
    transition: bool,
    promoted: bool,
}

impl RenderContext {
    /// Repaint when CSS changes owner or visible output has no captured paint state.
    pub(super) fn refresh_css_cache_ownership(&mut self, tree: &RenderTree) -> bool {
        let mut next = std::mem::take(&mut self.scratch_css_cache_owners);
        next.clear();
        {
            let store = tree.css_anim_store();
            let store = store.lock().unwrap();
            for (stable, animation) in &store.animations {
                if animation.is_playing
                    && let Some(node) = tree.layout_id(*stable)
                {
                    next.entry(node).or_default().animation = true;
                }
            }
            for (stable, transition) in &store.transitions {
                if transition.is_playing
                    && let Some(node) = tree.layout_id(*stable)
                {
                    next.entry(node).or_default().transition = true;
                }
            }
        }
        let missing_paint = {
            let promotion = tree.composite_promotion();
            let painted = tree.painted_node_ids();
            let records = tree.css_anim_paint_records();
            for (node, owner) in &mut next {
                owner.promoted = promotion.contains(node);
            }
            next.keys().any(|node| {
                painted.contains(node)
                    && tree
                        .get_render_node(*node)
                        .is_some_and(|n| n.props.opacity > 0.001)
                    && !records.contains_key(node)
                    && !self
                        .css_composited_textures
                        .contains_key(&node.data().as_ffi())
            })
        };
        let repaint = next != self.css_cache_owners || missing_paint;
        if repaint {
            self.invalidate_render_cache_tagged("css_ownership");
        }
        std::mem::swap(&mut self.css_cache_owners, &mut next);
        self.scratch_css_cache_owners = next;
        repaint
    }
}
