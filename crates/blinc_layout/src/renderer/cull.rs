//! Culling: whether a subtree can draw anything inside a visible rect.

use super::RenderTree;
use crate::tree::LayoutNodeId;
use blinc_core::Transform;

/// How far past a cull rect content is still drawn, for shadows and edges entering on scroll.
pub const CULL_OVERSCAN: f32 = 200.0;

/// A rect as `[x, y, width, height]`.
pub type CullRect = [f32; 4];

fn intersects(b: CullRect, view: CullRect) -> bool {
    let (x0, y0) = (view[0] - CULL_OVERSCAN, view[1] - CULL_OVERSCAN);
    let (x1, y1) = (
        view[0] + view[2] + CULL_OVERSCAN,
        view[1] + view[3] + CULL_OVERSCAN,
    );
    b[0] + b[2] > x0 && b[0] < x1 && b[1] + b[3] > y0 && b[1] < y1
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
        if self.moves_its_content(node) {
            return false;
        }
        let Some(b) = self.layout_tree.get_absolute_bounds(node) else {
            return false;
        };
        if intersects([b.x + shift.0, b.y + shift.1, b.width, b.height], view) {
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
            .children(node)
            .into_iter()
            .all(|child| self.draws_outside(child, shift, view))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overscan_keeps_near_edges() {
        let view = [0.0, 0.0, 100.0, 100.0];
        assert!(intersects([0.0, 250.0, 10.0, 10.0], view));
        assert!(!intersects([0.0, 350.0, 10.0, 10.0], view));
    }
}
