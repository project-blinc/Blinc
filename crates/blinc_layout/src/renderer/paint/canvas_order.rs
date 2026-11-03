//! Whether a canvas has to paint in tree order.
//!
//! An independent canvas is replayed over the static cache, after everything in it. That is the
//! order tree order asks for unless a box later in the tree is drawn over the canvas's box; then
//! the canvas has to paint with the rest, in order. Glass panels and the foreground layer have
//! passes of their own and do not count.

use crate::renderer::{ElementType, RenderTree};
use crate::tree::LayoutNodeId;

impl RenderTree {
    /// Whether a box painted after `node` in tree order, into the same cache, overlaps its box.
    ///
    /// Overlay subtrees, glass and the foreground layer are drawn in passes of their own, and
    /// other canvases are ordered among themselves by z-layer; none of them counts.
    pub(crate) fn later_content_overlaps(&self, node: LayoutNodeId) -> bool {
        let Some(own) = self.get_absolute_bounds(node) else {
            return false;
        };
        let rect = [own.x, own.y, own.width, own.height];
        let mut current = node;
        while let Some(parent) = self.element_registry.get_parent(current) {
            let siblings = self.layout_tree.children(parent);
            let Some(at) = siblings.iter().position(|s| *s == current) else {
                return false;
            };
            if siblings[at + 1..]
                .iter()
                .any(|sibling| self.subtree_paints_over(*sibling, rect))
            {
                return true;
            }
            current = parent;
        }
        false
    }

    fn subtree_paints_over(&self, node: LayoutNodeId, rect: [f32; 4]) -> bool {
        let Some(render) = self.render_nodes.get(&node) else {
            return false;
        };
        let props = &render.props;
        if props.is_overlay_root || !props.visible {
            return false;
        }
        let bounds = self.get_absolute_bounds(node);
        let overlaps = bounds.is_some_and(|b| {
            b.width > 0.0
                && b.height > 0.0
                && b.x < rect[0] + rect[2]
                && b.x + b.width > rect[0]
                && b.y < rect[1] + rect[3]
                && b.y + b.height > rect[1]
        });
        // Boxes and their fills, the case a HUD or a panel over a canvas is. Text, SVG and image
        // content over a canvas draws after it as it always has, since a canvas that has to
        // repaint in order costs the static cache a repaint each frame.
        let paints = matches!(render.element_type, ElementType::Div)
            && (props.background.is_some()
                || !props.shadow.is_empty()
                || (props.border_width > 0.0 && props.border_color.is_some()));
        if paints && overlaps && props.layer == crate::element::RenderLayer::Background {
            return true;
        }
        // What a node that clips does not contain cannot reach the box.
        if props.clips_content && !overlaps {
            return false;
        }
        self.layout_tree
            .children(node)
            .into_iter()
            .any(|child| self.subtree_paints_over(child, rect))
    }
}
