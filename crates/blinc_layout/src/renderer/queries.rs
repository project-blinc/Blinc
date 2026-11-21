//! Read-only queries against the `RenderTree`.
//!
//! Pure accessors and lookups — no mutation, no animation tick, no
//! state changes. The caller asks "what's the current value?" and
//! gets the answer with a single map / cache hit. Larger query
//! surfaces that overlay animation state on layout (`get_render_bounds`,
//! `get_animated_bounds`, `get_visual_render_bounds`) live with the
//! animation modules — those need to interleave layout reads with
//! visual / FLIP / motion bookkeeping, so splitting them here would
//! either fragment the dependency or duplicate the overlay logic.

use std::collections::HashMap;

use crate::diff::DivHash;
use crate::div::ElementBuilder;
use crate::element::ElementBounds;
use crate::tree::LayoutNodeId;

use super::{NodeStateStorage, RenderNode, RenderTree, RenderTreeDebugStats};
use crate::tree::NodeMap;

impl RenderTree {
    /// Get the tree hash for this render tree
    pub fn tree_hash(&self) -> Option<DivHash> {
        self.tree_hash
    }

    /// Check if a new element tree would produce the same render tree
    ///
    /// Returns true if the element tree hash matches, meaning no rebuild is needed.
    pub fn matches_element<E: ElementBuilder>(&self, element: &E) -> bool {
        match self.tree_hash {
            Some(hash) => {
                hash == DivHash::compute_element_tree(element)
                    && self.tree_topology_hash == Some(DivHash::compute_topology_tree(element))
            }
            None => false,
        }
    }

    /// Snapshot of stats useful for debugging the render tree's
    /// animation / cache state.
    ///
    /// Returns counts of active animations and other debug info.
    /// Used by `BLINC_DEBUG=motion` to display animation stats.
    pub fn debug_stats(&self) -> RenderTreeDebugStats {
        RenderTreeDebugStats {
            visual_animation_count: self.visual_animations.len(),
            visual_animation_config_count: self.visual_animation_configs.len(),
            layout_animation_count: self.layout_animations_by_key.len(),
            animated_bounds_count: self.animated_render_bounds.len(),
            render_node_count: self.render_nodes.len(),
            scroll_physics_count: self.scroll_physics.len(),
        }
    }

    /// Query an element by ID
    ///
    /// Returns the node ID if an element with the given ID exists.
    pub fn query_by_id(&self, id: &str) -> Option<LayoutNodeId> {
        self.element_registry.get(id)
    }

    /// Get the node states map (for transferring to a new tree)
    pub fn node_states(&self) -> &NodeMap<NodeStateStorage> {
        &self.node_states
    }

    /// Get bounds for a specific node
    pub fn get_bounds(&self, node: LayoutNodeId) -> Option<ElementBounds> {
        self.layout_tree.get_bounds(node, (0.0, 0.0))
    }

    /// Get absolute bounds for a node (traversing up the tree, accounting for scroll)
    pub fn get_absolute_bounds(&self, node: LayoutNodeId) -> Option<ElementBounds> {
        let mut bounds = self.layout_tree.get_absolute_bounds(node)?;
        // Walk up ancestors and apply scroll offsets from scroll containers.
        //
        // Touch scrolling on mobile (and momentum / bounce on desktop)
        // updates the per-container `scroll_physics` state, which is the
        // SOURCE OF TRUTH for "where this container is currently
        // scrolled to". The legacy `scroll_offsets` HashMap is only
        // written for code paths that don't use physics
        // (`set_scroll_offset`, immediate scroll commands). Reading
        // only the HashMap here would return stale offsets after a
        // touch scroll, which silently broke
        // `scroll_focused_text_input_above_keyboard` — the helper
        // saw the focused input's *original* on-screen position
        // (pre-scroll) and concluded it was already visible above the
        // keyboard, even though the user had scrolled it under the
        // keyboard.
        //
        // We mirror `get_scroll_offset`'s precedence: physics first,
        // HashMap fallback. The `try_lock` is intentional — under
        // contention we'd rather get a slightly-stale value than
        // block on a paint thread; the next frame catches up.
        for ancestor in self.layout_tree.ancestors(node) {
            let Some((sx, sy)) = self.container_scroll(ancestor) else {
                continue;
            };
            bounds.x += sx;
            bounds.y += sy;
        }
        Some(bounds)
    }

    /// The offset of `node` as a scroll container, `None` when it is not one: the physics first,
    /// the stored offset as a fallback, as `get_scroll_offset` reads them.
    pub(crate) fn container_scroll(&self, node: LayoutNodeId) -> Option<(f32, f32)> {
        if let Some(physics) = self.scroll_physics.get(&node) {
            if let Ok(p) = physics.try_lock() {
                Some((p.offset_x, p.offset_y))
            } else {
                Some(
                    self.scroll_offsets
                        .get(&node)
                        .copied()
                        .unwrap_or((0.0, 0.0)),
                )
            }
        } else {
            self.scroll_offsets.get(&node).copied()
        }
    }

    /// The absolute bounds of `child` from those of its parent: the parent's, the child's offset in
    /// the parent, and the parent's own scroll. What `get_absolute_bounds` adds up one ancestor at
    /// a time, taken in one step by a walk that is going down the tree anyway.
    pub(crate) fn child_absolute_bounds(
        &self,
        parent: LayoutNodeId,
        parent_bounds: Option<ElementBounds>,
        child: LayoutNodeId,
    ) -> Option<ElementBounds> {
        let Some(parent_bounds) = parent_bounds else {
            return self.get_absolute_bounds(child);
        };
        let local = self.layout_tree.get_bounds(child, (0.0, 0.0))?;
        let (sx, sy) = self.container_scroll(parent).unwrap_or((0.0, 0.0));
        Some(ElementBounds {
            x: parent_bounds.x + local.x + sx,
            y: parent_bounds.y + local.y + sy,
            width: local.width,
            height: local.height,
        })
    }

    /// Get render node data
    pub fn get_render_node(&self, node: LayoutNodeId) -> Option<&RenderNode> {
        self.render_nodes.get(&node)
    }

    /// Get the resolved padding for a layout node as [top, right, bottom, left] in px.
    pub fn get_node_padding(&self, node: LayoutNodeId) -> [f32; 4] {
        if let Some(style) = self.layout_tree.get_style(node) {
            let to_px = |lp: &taffy::LengthPercentage| match lp.expand() {
                taffy::style::ExpandedLengthPercentage::Length(v) => v,
                _ => 0.0, // percent: approx
            };
            [
                to_px(&style.padding.top),
                to_px(&style.padding.right),
                to_px(&style.padding.bottom),
                to_px(&style.padding.left),
            ]
        } else {
            [0.0; 4]
        }
    }

    /// Iterate over all nodes with their bounds and render props
    pub fn iter_nodes(&self) -> impl Iterator<Item = (LayoutNodeId, &RenderNode)> {
        self.render_nodes.iter().map(|(&id, node)| (id, node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::div::div;
    use crate::scroll::scroll;

    fn close(a: &ElementBounds, b: &ElementBounds) -> bool {
        (a.x - b.x).abs() < 0.01
            && (a.y - b.y).abs() < 0.01
            && (a.width - b.width).abs() < 0.01
            && (a.height - b.height).abs() < 0.01
    }

    fn walk(
        tree: &RenderTree,
        node: LayoutNodeId,
        bounds: Option<ElementBounds>,
        seen: &mut usize,
    ) {
        for child in tree.layout_tree.children(node) {
            let carried = tree.child_absolute_bounds(node, bounds, child);
            let summed = tree.get_absolute_bounds(child);
            match (carried, summed) {
                (Some(a), Some(b)) => assert!(close(&a, &b), "{a:?} against {b:?}"),
                (None, None) => {}
                (a, b) => panic!("{a:?} against {b:?}"),
            }
            *seen += 1;
            walk(tree, child, carried, seen);
        }
    }

    /// A node's bounds taken from its parent's equal what adding up every ancestor gives, under
    /// nested containers that are scrolled.
    #[test]
    fn child_bounds_carried_down_equal_the_sum_over_the_ancestors() {
        let ui = div().w(300.0).h(300.0).p(7.0).child(
            scroll().w(280.0).h(200.0).child(
                div().w(260.0).flex_col().gap(5.0).p(3.0).child(
                    scroll().w(200.0).h(120.0).child(
                        div()
                            .flex_col()
                            .gap(4.0)
                            .child(div().w(150.0).h(40.0))
                            .child(div().w(150.0).h(40.0))
                            .child(div().w(150.0).h(400.0)),
                    ),
                ),
            ),
        );
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(300.0, 300.0);
        // Scroll both containers.
        let mut scrolls = Vec::new();
        let mut stack = vec![tree.root().unwrap()];
        while let Some(n) = stack.pop() {
            if tree.scroll_physics.contains_key(&n) {
                scrolls.push(n);
            }
            stack.extend(tree.layout_tree.children(n));
        }
        assert!(scrolls.len() >= 2, "{} scroll containers", scrolls.len());
        for (i, node) in scrolls.iter().enumerate() {
            tree.set_scroll_offset(*node, 0.0, -(10.0 + 13.0 * i as f32));
        }
        let root = tree.root().unwrap();
        let mut seen = 0;
        walk(&tree, root, tree.get_absolute_bounds(root), &mut seen);
        assert!(seen > 6, "only {seen} nodes were compared");
    }
}
