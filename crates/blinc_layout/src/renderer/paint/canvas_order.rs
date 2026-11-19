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
        self.later_content_overlaps_with(node, false)
    }

    /// As [`Self::later_content_overlaps`], for content that has to stay below what is painted
    /// after it in every stream: the foreground layer counts, and so does anything that is not a
    /// box, such as text, an image or a canvas. Overlay subtrees and glass still do not, which are
    /// drawn after everything.
    pub(crate) fn later_content_overlaps_any(&self, node: LayoutNodeId) -> bool {
        self.later_content_overlaps_with(node, true)
    }

    /// Whether `node` has an image or an SVG somewhere below it, content that is drawn apart
    /// from the primitive batch and takes a group's opacity in a pass of its own.
    pub(crate) fn subtree_has_media(&self, node: LayoutNodeId) -> bool {
        self.layout_tree.children_iter(node).any(|child| {
            self.render_nodes.get(&child).is_some_and(|render| {
                matches!(
                    render.element_type,
                    ElementType::Image(_) | ElementType::Svg(_)
                )
            }) || self.subtree_has_media(child)
        })
    }

    /// Whether `node` has an SVG somewhere below it.
    pub(crate) fn subtree_has_svg(&self, node: LayoutNodeId) -> bool {
        self.layout_tree.children_iter(node).any(|child| {
            self.render_nodes
                .get(&child)
                .is_some_and(|render| matches!(render.element_type, ElementType::Svg(_)))
                || self.subtree_has_svg(child)
        })
    }

    fn later_content_overlaps_with(&self, node: LayoutNodeId, any: bool) -> bool {
        let Some(own) = self.get_absolute_bounds(node) else {
            return false;
        };
        let rect = [own.x, own.y, own.width, own.height];
        let mut current = node;
        while let Some(parent) = self.element_registry.get_parent(current) {
            let mut siblings = self.layout_tree.children_iter(parent);
            if !siblings.any(|s| s == current) {
                return false;
            }
            if siblings.any(|sibling| self.subtree_paints_over(sibling, rect, any)) {
                return true;
            }
            current = parent;
        }
        false
    }

    fn subtree_paints_over(&self, node: LayoutNodeId, rect: [f32; 4], any: bool) -> bool {
        self.subtree_paints_over_at(node, rect, any, self.get_absolute_bounds(node))
    }

    /// [`Self::subtree_paints_over`] with the absolute bounds of `node` known, which a walk down
    /// carries from each node to its children instead of adding up every ancestor again.
    fn subtree_paints_over_at(
        &self,
        node: LayoutNodeId,
        rect: [f32; 4],
        any: bool,
        bounds: Option<crate::element::ElementBounds>,
    ) -> bool {
        let Some(render) = self.render_nodes.get(&node) else {
            return false;
        };
        let props = &render.props;
        if props.is_overlay_root || !props.visible {
            return false;
        }
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
        let paints = if matches!(render.element_type, ElementType::Div) {
            props.background.is_some()
                || !props.shadow.is_empty()
                || (props.border_width > 0.0 && props.border_color.is_some())
        } else {
            any
        };
        let layer_counts = any || props.layer == crate::element::RenderLayer::Background;
        if paints && overlaps && layer_counts {
            return true;
        }
        // What a node that clips does not contain cannot reach the box.
        if props.clips_content && !overlaps {
            return false;
        }
        self.layout_tree.children_iter(node).any(|child| {
            self.subtree_paints_over_at(
                child,
                rect,
                any,
                self.child_absolute_bounds(node, bounds, child),
            )
        })
    }
}
