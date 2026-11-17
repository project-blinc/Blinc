//! Keep overlapping bound siblings in one ordered dynamic scope.

use crate::renderer::{ElementType, RenderTree};
use crate::tree::LayoutNodeId;

impl RenderTree {
    pub(super) fn shared_motion_scope(&self, node: LayoutNodeId) -> bool {
        if self.motion_promotion.get() {
            return false;
        }
        let Some(render) = self.render_nodes.get(&node) else {
            return false;
        };
        if !render.props.clips_content
            || self.is_scroll_container(node)
            || !self.transparent_motion_wrapper(node)
        {
            return false;
        }
        let children = self.layout_tree.children(node);
        children.len() > 1
            && children
                .iter()
                .all(|child| self.bound_motion_branch(*child))
            && self.plain_motion_content(node)
    }

    fn bound_motion_branch(&self, node: LayoutNodeId) -> bool {
        if self.motion_bindings.contains_key(&node) {
            return true;
        }
        if !self.transparent_motion_wrapper(node) {
            return false;
        }
        let children = self.layout_tree.children(node);
        !children.is_empty()
            && children
                .into_iter()
                .all(|child| self.bound_motion_branch(child))
    }

    fn transparent_motion_wrapper(&self, node: LayoutNodeId) -> bool {
        self.render_nodes.get(&node).is_some_and(|render| {
            let p = &render.props;
            matches!(render.element_type, ElementType::Div)
                && p.background.is_none()
                && p.border_width == 0.0
                && p.shadow.is_empty()
                && p.opacity == 1.0
                && p.motion.is_none()
                && p.layer_effects.is_empty()
        })
    }

    fn plain_motion_content(&self, node: LayoutNodeId) -> bool {
        self.render_nodes.get(&node).is_some_and(|render| {
            let p = &render.props;
            matches!(render.element_type, ElementType::Div | ElementType::Text(_))
                && p.layer == crate::element::RenderLayer::Background
                && !p.is_glass()
                && !p.is_overlay_root
                && !p.is_fixed
                && !p.is_sticky
                && p.motion.is_none()
                && !matches!(p.transform, Some(blinc_core::Transform::Mat4(_)))
                && p.clip_path.is_none()
                && p.mask_image.is_none()
                && p.filter.is_none()
                && p.layer_effects.is_empty()
                && p.rotate_x.is_none()
                && p.rotate_y.is_none()
                && p.perspective.is_none()
                && p.shape_3d.is_none()
                && p.depth.is_none()
                && p.translate_z.is_none()
                && p.mix_blend_mode
                    .is_none_or(|m| m == blinc_core::BlendMode::Normal)
        }) && self
            .layout_tree
            .children(node)
            .into_iter()
            .all(|child| self.plain_motion_content(child))
    }
}
