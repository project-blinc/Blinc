//! Culling: whether a subtree can draw anything inside a visible rect.

use super::RenderTree;
use super::subtree_cache::Extent;
use crate::tree::LayoutNodeId;
use blinc_core::Transform;

/// How far past a cull rect content is still drawn, for shadows and edges entering on scroll.
pub const CULL_OVERSCAN: f32 = 200.0;

/// A rect as `[x, y, width, height]`.
pub type CullRect = [f32; 4];

fn intersects(b: CullRect, view: CullRect, overscan: f32) -> bool {
    let (x0, y0) = (view[0] - overscan, view[1] - overscan);
    let (x1, y1) = (view[0] + view[2] + overscan, view[1] + view[3] + overscan);
    b[0] + b[2] > x0 && b[0] < x1 && b[1] + b[3] > y0 && b[1] < y1
}

/// Whether the transform is one that moves what is drawn.
pub(super) fn moves_by_transform(t: &Option<Transform>) -> bool {
    !is_identity(t)
}

fn is_identity(t: &Option<Transform>) -> bool {
    match t {
        None => true,
        Some(Transform::Affine2D(a)) => {
            let [a, b, c, d, tx, ty] = a.elements;
            (a - 1.0).abs() < 1e-4
                && b.abs() < 1e-4
                && c.abs() < 1e-4
                && (d - 1.0).abs() < 1e-4
                && tx.abs() < 1e-4
                && ty.abs() < 1e-4
        }
        Some(_) => false,
    }
}

impl RenderTree {
    /// Whether this node moves its content away from where layout put it: a transform, a
    /// motion or bound transform, or a layout animation. Culling does not see past such a node.
    pub fn moves_its_content(&self, node: LayoutNodeId) -> bool {
        let Some(n) = self.render_nodes.get(&node) else {
            return false;
        };
        !is_identity(&n.props.transform)
            || n.props.motion.is_some()
            || n.props.is_fixed
            || n.props.is_sticky
            || self.motion_bindings.contains_key(&node)
            || self.is_layout_animating(node)
    }

    /// Whether `node` and everything under it draws outside `view` (plus `CULL_OVERSCAN`), with
    /// its absolute layout bounds moved by `shift` into `view`'s space. Conservative: a subtree
    /// with anything that moves its content is never outside. A descendant that clips its
    /// content stands for its whole subtree, since nothing beneath it draws past it.
    pub fn draws_outside(&self, node: LayoutNodeId, shift: (f32, f32), view: CullRect) -> bool {
        if self.index_key().is_none() {
            return self.draws_outside_at(node, shift, view, None);
        }
        let extent = self.subtree_extent(node, None);
        let Some([x0, y0, x1, y1]) = extent.rect else {
            return false;
        };
        !intersects(
            [x0 + shift.0, y0 + shift.1, x1 - x0, y1 - y0],
            view,
            self.cull_overscan(),
        )
    }

    /// The extent of `node`'s subtree. `parent` is the parent's absolute origin when the caller
    /// knows it.
    fn subtree_extent(&self, node: LayoutNodeId, parent: Option<(f32, f32)>) -> Extent {
        self.memo(
            |c| &mut c.extents,
            node,
            || self.measure_extent(node, parent),
        )
    }

    fn measure_extent(&self, node: LayoutNodeId, parent: Option<(f32, f32)>) -> Extent {
        const NEVER: Extent = Extent { rect: None };
        if self.moves_its_content(node) {
            return NEVER;
        }
        let bounds = match parent {
            Some(origin) => self.layout_tree.get_bounds(node, origin),
            None => self.layout_tree.get_absolute_bounds(node),
        };
        let Some(b) = bounds else {
            return NEVER;
        };
        let mut rect = [b.x, b.y, b.x + b.width, b.y + b.height];
        let clips = self
            .render_nodes
            .get(&node)
            .is_some_and(|n| n.props.clips_content);
        if !clips {
            for child in self.layout_tree.children_iter(node) {
                let Some(r) = self.subtree_extent(child, Some((b.x, b.y))).rect else {
                    return NEVER;
                };
                rect = [
                    rect[0].min(r[0]),
                    rect[1].min(r[1]),
                    rect[2].max(r[2]),
                    rect[3].max(r[3]),
                ];
            }
        }
        Extent { rect: Some(rect) }
    }

    /// `draws_outside` without the cache, for while a layout animation moves content by the
    /// clock, which no revision records.
    fn draws_outside_at(
        &self,
        node: LayoutNodeId,
        shift: (f32, f32),
        view: CullRect,
        parent_offset: Option<(f32, f32)>,
    ) -> bool {
        if self.moves_its_content(node) {
            return false;
        }
        let bounds = match parent_offset {
            Some(offset) => self.layout_tree.get_bounds(node, offset),
            None => self.layout_tree.get_absolute_bounds(node),
        };
        let Some(b) = bounds else {
            return false;
        };
        let overscan = self.cull_overscan();
        if intersects(
            [b.x + shift.0, b.y + shift.1, b.width, b.height],
            view,
            overscan,
        ) {
            return false;
        }
        let clips = self
            .render_nodes
            .get(&node)
            .is_some_and(|n| n.props.clips_content);
        if clips {
            return true;
        }
        self.layout_tree
            .children_iter(node)
            .all(|child| self.draws_outside_at(child, shift, view, Some((b.x, b.y))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overscan_keeps_near_edges() {
        let view = [0.0, 0.0, 100.0, 100.0];
        assert!(intersects([0.0, 250.0, 10.0, 10.0], view, CULL_OVERSCAN));
        assert!(!intersects([0.0, 350.0, 10.0, 10.0], view, CULL_OVERSCAN));
        assert!(intersects([0.0, 550.0, 10.0, 10.0], view, 500.0));
    }
}
