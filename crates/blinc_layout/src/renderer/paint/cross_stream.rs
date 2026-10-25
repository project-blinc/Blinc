//! Identify native groups and the text-resolution capability they require.
use super::super::{ElementType, RenderTree};
use crate::element::{Material, RenderLayer};
use crate::tree::LayoutNodeId;

impl RenderTree {
    pub(super) fn plan_cross_stream_groups(&self, deferred_text: bool, deferred_svg: bool) {
        fn visit(
            tree: &RenderTree,
            node: LayoutNodeId,
            foreground: bool,
            deferred_text: bool,
            deferred_svg: bool,
            groups: &mut Vec<(LayoutNodeId, (bool, bool))>,
        ) -> (bool, bool, bool, bool) {
            let Some(render) = tree.render_nodes.get(&node) else {
                return (false, false, false, false);
            };
            let foreground = foreground || render.props.layer == RenderLayer::Foreground;
            let props = &render.props;
            let mut has_text = deferred_text
                && matches!(
                    render.element_type,
                    ElementType::Text(_) | ElementType::StyledText(_)
                );
            let mut has_svg = deferred_svg && matches!(render.element_type, ElementType::Svg(_));
            let mut native = (has_text
                || has_svg
                || matches!(
                    render.element_type,
                    ElementType::Div | ElementType::Canvas(_)
                ))
                && !matches!(props.material, Some(Material::Glass(_)))
                && matches!(
                    props.background,
                    None | Some(blinc_core::Brush::Solid(_) | blinc_core::Brush::Gradient(_))
                )
                && props.layer != RenderLayer::Glass
                && !props.is_overlay_root
                && props.flow.is_none()
                && props.flow_graph.is_none()
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
                let (child_native, child_foreground, child_text, child_svg) =
                    visit(tree, child, foreground, deferred_text, deferred_svg, groups);
                native &= child_native;
                has_foreground |= child_foreground;
                has_text |= child_text;
                has_svg |= child_svg;
            }
            // Native children must stay with an unsupported moving owner's later pass.
            if !native
                && (tree.motion_bindings.contains_key(&node)
                    || props.motion.is_some()
                    || props.is_overlay_root)
            {
                groups.truncate(start);
            }
            let needs_content_group = (has_text || has_svg)
                && (props.opacity < 1.0
                    || props.motion.is_some()
                    || tree
                        .motion_bindings
                        .get(&node)
                        .is_some_and(|b| b.opacity.is_some())
                    || groups.len() > start);
            if native && ((!foreground && has_foreground) || needs_content_group) {
                groups.push((node, (has_text, has_svg)));
            }
            (native, has_foreground, has_text, has_svg)
        }
        let mut groups = self.cross_stream_groups.borrow_mut();
        groups.clear();
        if let Some(root) = self.root {
            let mut planned = Vec::new();
            visit(self, root, false, deferred_text, deferred_svg, &mut planned);
            groups.extend(planned);
        }
    }
}
