//! Identify native groups and their collected-content requirements.
use super::super::{DeferredContent, ElementType, RenderTree};
use crate::element::{Material, RenderLayer};
use crate::tree::LayoutNodeId;

impl RenderTree {
    pub(super) fn plan_cross_stream_groups(
        &self,
        capabilities: DeferredContent,
        glass_layers: bool,
    ) {
        #[derive(Clone, Copy, Default)]
        struct Scope {
            foreground: bool,
            glass: bool,
            portal: bool,
        }
        fn visit(
            tree: &RenderTree,
            node: LayoutNodeId,
            scope: Scope,
            capabilities: DeferredContent,
            glass_layers: bool,
            groups: &mut Vec<(LayoutNodeId, DeferredContent)>,
        ) -> (bool, bool, DeferredContent, bool, bool) {
            let Some(render) = tree.render_nodes.get(&node) else {
                return (false, false, DeferredContent::default(), false, false);
            };
            let foreground = scope.foreground || render.props.layer == RenderLayer::Foreground;
            let props = &render.props;
            let is_glass = matches!(props.material, Some(Material::Glass(_)));
            let mut content = DeferredContent {
                glass: false,
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
                && (!is_glass || (glass_layers && !scope.glass && !scope.portal))
                && ((capabilities.image
                    && matches!(render.element_type, ElementType::Div)
                    && matches!(props.background, Some(blinc_core::Brush::Image(_))))
                    || matches!(
                        props.background,
                        None | Some(blinc_core::Brush::Solid(_) | blinc_core::Brush::Gradient(_))
                    ))
                && (props.layer != RenderLayer::Glass || is_glass)
                && props.flow.is_none()
                && props.flow_graph.is_none()
                && props.rotate_x.is_none()
                && props.rotate_y.is_none()
                && props.depth.unwrap_or(0.0) == 0.0
                && props.shape_3d.is_none();
            let own_native = native;
            let mut has_foreground = foreground;
            let mut has_canvas = matches!(render.element_type, ElementType::Canvas(_));
            let mut has_portal = props.is_overlay_root;
            let start = groups.len();
            let children = tree.layout_tree.children(node);
            for child in children.iter().copied() {
                let (child_native, child_foreground, child_content, child_canvas, child_portal) =
                    visit(
                        tree,
                        child,
                        Scope {
                            foreground: foreground || is_glass,
                            glass: scope.glass || is_glass || !own_native,
                            portal: scope.portal || props.is_overlay_root,
                        },
                        capabilities,
                        glass_layers,
                        groups,
                    );
                native &= child_native;
                has_foreground |= child_foreground;
                has_canvas |= child_canvas;
                has_portal |= child_portal;
                content.text |= child_content.text;
                content.svg |= child_content.svg;
                content.image |= child_content.image;
                content.glass |= child_content.glass;
            }
            // Transparent unary wrappers can share the panel's foreground owner.
            native |= own_native
                && !is_glass
                && content.glass
                && children.len() == 1
                && matches!(render.element_type, ElementType::Div)
                && props.background.is_none()
                && props.material.is_none()
                && props.shadow.is_empty()
                && props.border_width == 0.0
                && !props.border_sides.has_any()
                && props.outline_width == 0.0;
            native &= !(is_glass && has_portal);
            content.glass |= is_glass && native;
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
            content.glass &= native;
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
                && (content.glass
                    || has_portal
                    || (!foreground && has_foreground)
                    || needs_content_group
                    || needs_canvas_group)
            {
                groups.push((node, content));
            }
            // Glass keeps its own foreground owner; ordinary ancestors remain separate.
            (
                native && !content.glass,
                has_foreground,
                content,
                has_canvas,
                has_portal,
            )
        }
        let mut groups = self.cross_stream_groups.borrow_mut();
        groups.clear();
        if let Some(root) = self.root {
            let mut planned = Vec::new();
            visit(
                self,
                root,
                Scope::default(),
                capabilities,
                glass_layers,
                &mut planned,
            );
            groups.extend(planned);
        }
    }
}
