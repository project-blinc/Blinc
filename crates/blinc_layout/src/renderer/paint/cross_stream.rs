//! Identify native groups and their collected-content requirements.
use super::super::{DeferredContent, ElementType, RenderTree};
use crate::element::{Material, RenderLayer};
use crate::tree::LayoutNodeId;

impl RenderTree {
    pub(super) fn plan_cross_stream_groups(&self, capabilities: DeferredContent) {
        fn visit(
            tree: &RenderTree,
            node: LayoutNodeId,
            foreground: bool,
            capabilities: DeferredContent,
            groups: &mut Vec<(LayoutNodeId, DeferredContent)>,
        ) -> (bool, bool, DeferredContent, bool, bool) {
            let Some(render) = tree.render_nodes.get(&node) else {
                return (false, false, DeferredContent::default(), false, false);
            };
            let foreground = foreground || render.props.layer == RenderLayer::Foreground;
            let props = &render.props;
            let mut content = DeferredContent {
                text: capabilities.text
                    && matches!(
                        render.element_type,
                        ElementType::Text(_) | ElementType::StyledText(_)
                    ),
                svg: capabilities.svg && matches!(render.element_type, ElementType::Svg(_)),
                image: capabilities.image
                    && (matches!(render.element_type, ElementType::Image(_))
                        || (matches!(render.element_type, ElementType::Div)
                            && matches!(props.background, Some(blinc_core::Brush::Image(_))))),
            };
            let mut native = (content.any()
                || matches!(
                    render.element_type,
                    ElementType::Div | ElementType::Canvas(_)
                ))
                && !matches!(props.material, Some(Material::Glass(_)))
                && ((capabilities.image
                    && matches!(render.element_type, ElementType::Div)
                    && matches!(props.background, Some(blinc_core::Brush::Image(_))))
                    || matches!(
                        props.background,
                        None | Some(blinc_core::Brush::Solid(_) | blinc_core::Brush::Gradient(_))
                    ))
                && props.layer != RenderLayer::Glass
                && props.flow.is_none()
                && props.flow_graph.is_none()
                && props.rotate_x.is_none()
                && props.rotate_y.is_none()
                && props.depth.unwrap_or(0.0) == 0.0
                && props.shape_3d.is_none();
            let mut has_foreground = foreground;
            let mut has_canvas = matches!(render.element_type, ElementType::Canvas(_));
            let mut has_portal = props.is_overlay_root;
            let start = groups.len();
            for child in tree.layout_tree.children(node) {
                let (child_native, child_foreground, child_content, child_canvas, child_portal) =
                    visit(tree, child, foreground, capabilities, groups);
                native &= child_native;
                has_foreground |= child_foreground;
                has_canvas |= child_canvas;
                has_portal |= child_portal;
                content.text |= child_content.text;
                content.svg |= child_content.svg;
                content.image |= child_content.image;
            }
            // Unsupported moving or effect owners retain their descendants' fallback pass.
            if !native
                && (tree.motion_bindings.contains_key(&node)
                    || !props.layer_effects.is_empty()
                    || props
                        .mix_blend_mode
                        .is_some_and(|mode| mode != blinc_core::BlendMode::Normal)
                    || props.motion.is_some()
                    || props.is_overlay_root)
            {
                groups.truncate(start);
            }
            let needs_content_group = content.any()
                && (props.opacity < 1.0
                    || !props.layer_effects.is_empty()
                    || props
                        .mix_blend_mode
                        .is_some_and(|mode| mode != blinc_core::BlendMode::Normal)
                    || props.motion.is_some()
                    || tree
                        .motion_bindings
                        .get(&node)
                        .is_some_and(|b| b.opacity.is_some())
                    || groups.len() > start);
            // Keep callback ownership at bound-opacity endpoints, including a
            // zero frame whose canvas is culled by the walker.
            let needs_canvas_group = has_canvas
                && tree
                    .motion_bindings
                    .get(&node)
                    .is_some_and(|b| b.opacity.is_some());
            if native
                && (has_portal
                    || (!foreground && has_foreground)
                    || needs_content_group
                    || needs_canvas_group)
            {
                groups.push((node, content));
            }
            // Native ancestors retain state; the paint context gives portals a late scope.
            (native, has_foreground, content, has_canvas, has_portal)
        }
        let mut groups = self.cross_stream_groups.borrow_mut();
        groups.clear();
        if let Some(root) = self.root {
            let mut planned = Vec::new();
            visit(self, root, false, capabilities, &mut planned);
            groups.extend(planned);
        }
    }
}
