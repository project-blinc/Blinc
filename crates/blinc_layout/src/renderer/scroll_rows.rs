//! Scroll containers painted as nodes moved by rows: the content once, the scroll as a row.
//!
//! The walker paints a qualifying container's content into the dynamic batch at the offset it has
//! at that paint, and a frame in which only the offset changed writes a row for the change instead
//! of walking and painting the content again. The scrollbar is not part of the content: the
//! renderer draws it each frame from the physics. Content that cannot be moved this way
//! (something fixed or sticky, an SVG, a canvas, a group, anything already moving) keeps the
//! container painted as before, and so does a container inside another that is on rows.

use super::RenderTree;
use crate::tree::LayoutNodeId;
use blinc_core::{DrawContext, Transform};

/// How far, in logical pixels, a container on rows may be scrolled from where its content was
/// painted before the content is painted again: the part of the painted window past the viewport
/// that is not kept for shadows and edges.
const SPARE: f32 = 80.0;

/// How long a scroll container stays on rows after it last moved, in milliseconds.
pub const SCROLL_IDLE_MS: u64 = 500;

impl RenderTree {
    /// Allow or forbid painting scroll containers as nodes moved by rows. Set before painting.
    pub fn set_scroll_rows(&self, on: bool) {
        self.scroll_rows.set(on);
    }

    /// Whether scroll containers may be painted as nodes moved by rows.
    pub fn scroll_rows_enabled(&self) -> bool {
        self.scroll_rows.get()
    }

    /// Whether the last paint put the scroll container `node` on rows.
    pub fn is_scroll_row_node(&self, node: LayoutNodeId) -> bool {
        self.scroll_row_nodes.borrow().contains_key(&node)
    }

    /// How far past its viewport the content of the scroll container `node` was painted, if it is
    /// on rows. What is collected for it, text and images, is cut at the same distance.
    pub fn scroll_row_overscan(&self, node: LayoutNodeId) -> Option<f32> {
        self.scroll_row_nodes.borrow().get(&node).map(|(o, _)| *o)
    }

    /// Set how far past a cull rect content is drawn for the walk in progress, returning the
    /// distance it had, to put back.
    pub fn set_cull_overscan(&self, overscan: f32) -> f32 {
        self.cull_overscan.replace(overscan)
    }

    /// The offset of the scroll container `node` now.
    pub fn scroll_row_pose(&self, node: LayoutNodeId) -> Option<(f32, f32)> {
        self.is_scroll_row_node(node)
            .then(|| self.get_scroll_offset(node))
    }

    /// Whether the offset of a scroll container on rows has changed since its rows were written.
    pub fn scroll_rows_moved(&self) -> bool {
        let applied = self.scroll_applied.borrow();
        self.scroll_row_nodes.borrow().keys().any(|node| {
            let now = self.get_scroll_offset(*node);
            applied
                .get(node)
                .is_none_or(|a| (a.0 - now.0).abs() > 0.01 || (a.1 - now.1).abs() > 0.01)
        })
    }

    /// Say the rows have been written for the offsets the containers on rows have now.
    pub fn note_scroll_rows_applied(&self) {
        let mut applied = self.scroll_applied.borrow_mut();
        for node in self.scroll_row_nodes.borrow().keys() {
            applied.insert(*node, self.get_scroll_offset(*node));
        }
    }

    /// Whether a scroll container that is not on rows has an offset other than the one it was
    /// painted at: the paint is stale and has to be made again. A container that was not painted
    /// at all, because it is culled, hidden or in a closed overlay, has no paint to be stale.
    pub fn scroll_off_rows_changed(&self) -> bool {
        let rows = self.scroll_row_nodes.borrow();
        let painted = self.scroll_painted.borrow();
        self.scroll_physics.keys().any(|node| {
            if rows.contains_key(node) {
                return false;
            }
            let now = self.get_scroll_offset(*node);
            painted
                .get(node)
                .is_some_and(|p| (p.0 - now.0).abs() > 0.01 || (p.1 - now.1).abs() > 0.01)
        })
    }

    /// Changes with scroll offsets or gesture ownership, so a still pointer refreshes its hover.
    pub fn scroll_signature(&self) -> u64 {
        let mut signature = self
            .latched_scroll_target()
            .map_or(0, |node| node.to_raw().wrapping_mul(0x94D0_49BB_1331_11EB));
        for (node, physics) in &self.scroll_physics {
            if let Ok(p) = physics.try_lock() {
                let mixed = node.to_raw().wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    ^ (p.offset_x.to_bits() as u64)
                    ^ ((p.offset_y.to_bits() as u64) << 32);
                signature = signature.wrapping_add(mixed.wrapping_mul(0xBF58_476D_1CE4_E5B9));
            }
        }
        signature
    }

    /// Whether the layout has changed since the containers on rows were painted: rows mounted or
    /// removed, or restyled, as a virtual list does when its window moves. What was painted is
    /// then not what is there, and moving it by a row shows the old.
    pub fn scroll_rows_tree_changed(&self) -> bool {
        !self.scroll_row_nodes.borrow().is_empty()
            && self.scroll_painted_epoch.get() != Some(self.layout_tree.epoch())
    }

    /// Whether the paint of a scroll container is stale and has to be made again: one that is not
    /// on rows has an offset other than the one it was painted at, and one that is has had its
    /// content change.
    pub fn scroll_paint_stale(&self) -> bool {
        self.scroll_off_rows_changed() || self.scroll_rows_tree_changed() || self.scroll_rows_idle()
    }

    /// Whether the scroll container `node` has moved recently. Content
    /// is moved by rows only while its container scrolls; at rest it is painted into the static
    /// layer, and a frame draws only what moves on its own. Without a scroll clock (headless, no
    /// physics tick yet) every container counts as moving.
    pub fn scroll_moved_recently(&self, node: LayoutNodeId) -> bool {
        let Some(now) = self.scroll_clock_ms.get() else {
            return true;
        };
        let offset = self.get_scroll_offset(node);
        let mut seen = self.scroll_seen.borrow_mut();
        match seen.get_mut(&node) {
            // First seen at rest: nothing has scrolled it yet.
            None => {
                seen.insert(node, (offset, 0));
                false
            }
            Some((last, at)) => {
                if (last.0 - offset.0).abs() > 0.01 || (last.1 - offset.1).abs() > 0.01 {
                    *last = offset;
                    *at = now;
                    return true;
                }
                *at > 0 && now.saturating_sub(*at) < SCROLL_IDLE_MS
            }
        }
    }

    /// Whether a container on rows has come to rest: its content is painted again, into the
    /// static layer.
    fn scroll_rows_idle(&self) -> bool {
        let rows: Vec<LayoutNodeId> = self.scroll_row_nodes.borrow().keys().copied().collect();
        rows.into_iter()
            .any(|node| !self.scroll_moved_recently(node))
    }

    /// Whether, at the last tick of the scroll physics, a scroll container that is not on rows was
    /// gliding, bouncing or fading its scrollbar.
    pub fn scroll_off_rows_animating(&self) -> bool {
        self.scroll_off_rows_animating.get()
    }

    pub(crate) fn set_scroll_off_rows_animating(&self, on: bool) {
        self.scroll_off_rows_animating.set(on);
    }

    /// How far the cull rect is widened for the walk in progress.
    pub(crate) fn cull_overscan(&self) -> f32 {
        self.cull_overscan.get()
    }

    pub(crate) fn clear_scroll_rows_record(&self) {
        self.scroll_row_nodes.borrow_mut().clear();
        self.scroll_painted.borrow_mut().clear();
        self.scroll_applied.borrow_mut().clear();
        self.scroll_painted_epoch.set(None);
    }

    pub(crate) fn record_scroll_painted(&self, node: LayoutNodeId, offset: (f32, f32)) {
        self.scroll_painted.borrow_mut().insert(node, offset);
    }

    /// Record that `node` was painted on rows, with `overscan` of content past its viewport.
    /// Returns the margin its offset may move by.
    pub(crate) fn record_scroll_row_node(
        &self,
        node: LayoutNodeId,
        offset: (f32, f32),
        overscan: f32,
    ) -> f32 {
        let margin = (overscan - SPARE).max(0.0);
        self.scroll_painted_epoch
            .set(Some(self.layout_tree.epoch()));
        self.scroll_row_nodes
            .borrow_mut()
            .insert(node, (overscan, margin));
        self.scroll_applied.borrow_mut().insert(node, offset);
        margin
    }

    /// Whether the content of the scroll container `node` can be painted once and moved by a row:
    /// nothing under it moves on its own, draws apart from the batch, or is positioned against
    /// the scroll.
    pub(crate) fn scroll_content_qualifies(&self, node: LayoutNodeId) -> bool {
        self.children_qualify(node)
    }

    fn children_qualify(&self, node: LayoutNodeId) -> bool {
        // A container with very many children keeps the answer to the checks that do not change
        // by the frame; what is animating is asked each time.
        if let Some(qualifies) = self.indexed_qualification(node) {
            return qualifies && !self.animating_under(node);
        }
        self.layout_tree
            .children(node)
            .into_iter()
            .all(|child| self.scroll_subtree_qualifies(child))
    }

    fn scroll_subtree_qualifies(&self, node: LayoutNodeId) -> bool {
        self.scroll_node_qualifies(node)
            // A canvas is replayed into the rows' batch each frame, and a bound node is moved by
            // a row of its own chained to the container's, so neither counts.
            && !matches!(
                self.current_animation_status.borrow().get(&node),
                Some(super::AnimationStatus::Animating(super::AnimatedKind::Css))
            )
            && self.children_qualify(node)
    }

    /// Draw the scrollbars of the containers on rows, as the walker draws them for the others:
    /// after the content, in the container's own space. Their offsets change every frame, which is
    /// not a change of the content.
    pub fn draw_scrollbars(&self, ctx: &mut dyn DrawContext) {
        let nodes: Vec<LayoutNodeId> = self.scroll_row_nodes.borrow().keys().copied().collect();
        for node in nodes {
            let Some(physics) = self.scroll_physics.get(&node) else {
                continue;
            };
            let Some(bounds) = self.layout_tree.get_absolute_bounds(node) else {
                continue;
            };
            let info = match physics.try_lock() {
                Ok(p) => p.scrollbar_render_info(),
                Err(_) => continue,
            };
            if info.opacity <= 0.01 {
                continue;
            }
            // Where the container sits on screen: its layout place moved by the scroll above it.
            let mut shift = (0.0, 0.0);
            for ancestor in self.layout_tree.ancestors(node) {
                if self.scroll_physics.contains_key(&ancestor) {
                    let (x, y) = self.get_scroll_offset(ancestor);
                    shift = (shift.0 + x, shift.1 + y);
                }
            }
            ctx.push_transform(Transform::translate(bounds.x + shift.0, bounds.y + shift.1));
            self.render_scrollbar(ctx, bounds.width, bounds.height, &info);
            ctx.pop_transform();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::div::div;
    use crate::widgets::scroll::scroll;

    fn scrolled() -> (RenderTree, LayoutNodeId) {
        let mut tree = RenderTree::from_element(
            &div()
                .w(200.0)
                .h(200.0)
                .child(scroll().w(200.0).h(200.0).child(div().w(200.0).h(2000.0))),
        );
        tree.compute_layout(200.0, 200.0);
        let root = tree.root().unwrap();
        let node = tree.layout_tree.children(root)[0];
        assert!(tree.is_scroll_container(node));
        (tree, node)
    }

    #[test]
    fn a_container_is_moving_only_until_it_has_rested_a_while() {
        let (mut tree, node) = scrolled();
        // No scroll clock yet: every container counts as moving, as headless rendering has it.
        assert!(tree.scroll_moved_recently(node));
        tree.tick_scroll_physics(1_000);
        assert!(!tree.scroll_moved_recently(node), "first seen at rest");
        tree.scroll_physics[&node].lock().unwrap().offset_y = -50.0;
        tree.tick_scroll_physics(1_100);
        assert!(tree.scroll_moved_recently(node), "it just scrolled");
        tree.tick_scroll_physics(1_100 + SCROLL_IDLE_MS - 10);
        assert!(tree.scroll_moved_recently(node));
        tree.tick_scroll_physics(1_100 + SCROLL_IDLE_MS + 10);
        assert!(!tree.scroll_moved_recently(node), "at rest again");
    }
}
