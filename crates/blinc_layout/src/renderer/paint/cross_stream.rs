//! Identify native subtrees whose foreground can be local to an opacity group.
use super::super::{ElementType, RenderTree};
use crate::element::{Material, RenderLayer};
use crate::tree::LayoutNodeId;

impl RenderTree {
    pub(super) fn plan_cross_stream_groups(&self) {
        fn visit(
            tree: &RenderTree,
            node: LayoutNodeId,
            foreground: bool,
            groups: &mut Vec<LayoutNodeId>,
        ) -> (bool, bool) {
            let Some(render) = tree.render_nodes.get(&node) else {
                return (false, false);
            };
            let foreground = foreground || render.props.layer == RenderLayer::Foreground;
            let props = &render.props;
            let mut native = matches!(
                render.element_type,
                ElementType::Div | ElementType::Canvas(_)
            ) && !matches!(props.material, Some(Material::Glass(_)))
                && matches!(
                    props.background,
                    None | Some(blinc_core::Brush::Solid(_) | blinc_core::Brush::Gradient(_))
                )
                && props.layer != RenderLayer::Glass
                && !props.is_overlay_root
                && props.layer_effects.is_empty()
                && props
                    .mix_blend_mode
                    .unwrap_or(blinc_core::BlendMode::Normal)
                    == blinc_core::BlendMode::Normal
                && props.rotate_x.is_none()
                && props.rotate_y.is_none()
                && props.depth.unwrap_or(0.0) == 0.0
                && props.shape_3d.is_none();
            let mut has_foreground = foreground;
            let start = groups.len();
            for child in tree.layout_tree.children(node) {
                let (child_native, child_foreground) = visit(tree, child, foreground, groups);
                native &= child_native;
                has_foreground |= child_foreground;
            }
            // Native children must stay with an unsupported moving owner's later pass.
            if !native
                && (tree.motion_bindings.contains_key(&node)
                    || props.motion.is_some()
                    || props.is_overlay_root)
            {
                groups.truncate(start);
            }
            if native && !foreground && has_foreground {
                groups.push(node);
            }
            (native, has_foreground)
        }
        let mut groups = self.cross_stream_groups.borrow_mut();
        groups.clear();
        if let Some(root) = self.root {
            let mut planned = Vec::new();
            visit(self, root, false, &mut planned);
            groups.extend(planned);
        }
    }
}
