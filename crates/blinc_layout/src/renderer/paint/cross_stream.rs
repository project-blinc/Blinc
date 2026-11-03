//! Identify native groups and their collected-content requirements.
use super::super::{DeferredContent, ElementType, RenderTree};
use crate::element::RenderLayer;
use crate::tree::LayoutNodeId;
use std::collections::HashSet;

impl RenderTree {
    pub(super) fn plan_cross_stream_groups(
        &self,
        capabilities: DeferredContent,
        glass_layers: bool,
    ) {
        #[derive(Clone, Copy, Default)]
        struct Scope {
            background_group: bool,
            foreground: bool,
            blocked_glass: bool,
            portal: bool,
        }
        fn visit(
            tree: &RenderTree,
            node: LayoutNodeId,
            scope: Scope,
            capabilities: DeferredContent,
            glass_layers: bool,
            css_opacity: &HashSet<LayoutNodeId>,
            groups: &mut Vec<(LayoutNodeId, DeferredContent)>,
        ) -> (bool, bool, DeferredContent, bool, bool) {
            let Some(render) = tree.render_nodes.get(&node) else {
                return (false, false, DeferredContent::default(), false, false);
            };
            let foreground = scope.foreground || render.props.layer == RenderLayer::Foreground;
            let props = &render.props;
            let isolates = props.opacity < 1.0
                || !props.layer_effects.is_empty()
                || props
                    .mix_blend_mode
                    .is_some_and(|m| m != blinc_core::BlendMode::Normal)
                || props.motion.is_some()
                || tree.motion_bindings.contains_key(&node)
                || css_opacity.contains(&node);
            let is_glass = props.is_glass();
            let mut content = DeferredContent {
                glass: false,
                background_glass: false,
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
                && (!is_glass || (glass_layers && !scope.blocked_glass && !scope.portal))
                && ((is_glass && matches!(props.background, Some(blinc_core::Brush::Glass(_))))
                    || (capabilities.image
                        && matches!(render.element_type, ElementType::Div)
                        && matches!(props.background, Some(blinc_core::Brush::Image(_))))
                    || matches!(
                        props.background,
                        None | Some(blinc_core::Brush::Solid(_) | blinc_core::Brush::Gradient(_))
                    ))
                && (props.layer != RenderLayer::Glass || is_glass)
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
            let mut compatible_children = true;
            for child in children.iter().copied() {
                let (child_native, child_foreground, child_content, child_canvas, child_portal) =
                    visit(
                        tree,
                        child,
                        Scope {
                            background_group: scope.background_group || (!foreground && isolates),
                            foreground: foreground || is_glass,
                            blocked_glass: scope.blocked_glass || !own_native,
                            portal: scope.portal || props.is_overlay_root,
                        },
                        capabilities,
                        glass_layers,
                        css_opacity,
                        groups,
                    );
                native &= child_native;
                compatible_children &= child_native || child_content.glass;
                has_foreground |= child_foreground;
                has_canvas |= child_canvas;
                has_portal |= child_portal;
                content.text |= child_content.text;
                content.svg |= child_content.svg;
                content.image |= child_content.image;
                content.glass |= child_content.glass;
                content.background_glass |= child_content.background_glass;
            }
            let transparent_wrapper = children.len() == 1
                && props.background.is_none()
                && props.material.is_none()
                && props.shadow.is_empty()
                && props.border_width == 0.0
                && !props.border_sides.has_any()
                && props.outline_width == 0.0;
            // Painted background isolation stays at its background anchor.
            native |= own_native
                && content.glass
                && compatible_children
                && matches!(render.element_type, ElementType::Div)
                && (is_glass
                    || foreground
                    || transparent_wrapper
                    || isolates
                    || scope.background_group
                    || content.background_glass);
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
            content.background_glass = content.glass
                && !is_glass
                && !foreground
                && (!transparent_wrapper || content.background_glass);
            let needs_content_group = content.any()
                && ((foreground && props.background.is_some())
                    || props.opacity < 1.0
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
            // Compatible ancestors choose the glass scope's final anchor.
            (
                native && !content.glass,
                has_foreground,
                content,
                has_canvas,
                has_portal,
            )
        }
        // Keep glass ownership at settled CSS opacity endpoints.
        let css_opacity = self
            .css_anim_store
            .lock()
            .ok()
            .map(|store| {
                store
                    .animations
                    .iter()
                    .chain(&store.transitions)
                    .filter(|(_, animation)| animation.current_properties.opacity.is_some())
                    .filter_map(|(stable, _)| self.stable_to_layout.get(stable).copied())
                    .collect()
            })
            .unwrap_or_default();
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
                &css_opacity,
                &mut planned,
            );
            groups.extend(planned);
        }
    }
}
