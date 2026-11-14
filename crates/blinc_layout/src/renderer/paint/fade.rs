//! Which nodes a bound opacity can fade without a group.
//!
//! A group composites a node's primitives together in an offscreen layer and fades the result.
//! Where the node paints one primitive, fading that primitive's colours is the same picture, and
//! a row of the node table can carry it: the node is painted once and each frame writes a value.

use blinc_core::{BlendMode, Brush};

use super::super::{ElementType, RenderTree};
use crate::element::RenderLayer;
use crate::tree::LayoutNodeId;

impl RenderTree {
    /// Whether `node` paints a single primitive whose opacity a motion binding or a CSS animation
    /// can drive on its own: a childless div with a solid background and nothing else drawn (no
    /// border, outline, shadow, effect, blend, mask, filter, 3D or glass), on the background layer.
    /// Anything else paints in more than one draw whose overlaps a fade has to treat as one
    /// picture, which is a group.
    pub(crate) fn fades_without_group(&self, node: LayoutNodeId) -> bool {
        let Some(render) = self.render_nodes.get(&node) else {
            return false;
        };
        let p = &render.props;
        matches!(render.element_type, ElementType::Div)
            && matches!(p.background, Some(Brush::Solid(_)))
            && p.layer == RenderLayer::Background
            && p.border_width <= 0.0
            && !p.border_sides.has_any()
            && p.outline_width <= 0.0
            && p.shadow.is_empty()
            && p.inner_shadow.is_empty()
            && p.layer_effects.is_empty()
            && p.mix_blend_mode
                .is_none_or(|mode| mode == BlendMode::Normal)
            && p.material.is_none()
            && p.rotate_x.is_none()
            && p.rotate_y.is_none()
            && p.shape_3d.is_none()
            && p.depth.is_none_or(|d| d <= 0.0)
            && p.clip_path.is_none()
            && p.filter.is_none()
            && p.mask_image.is_none()
            && p.motion.is_none()
            && !p.clips_content
            && self.layout_tree.children(node).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::div::div;
    use blinc_core::Color;

    /// The one child of a root div.
    fn only_child(ui: crate::div::Div) -> (RenderTree, LayoutNodeId) {
        let tree = RenderTree::from_element(&div().child(ui));
        let root = tree.root().unwrap();
        let child = tree.layout_tree.children(root)[0];
        (tree, child)
    }

    #[test]
    fn a_bare_filled_div_fades_without_a_group() {
        let (tree, node) = only_child(div().w(20.0).h(10.0).bg(Color::RED));
        assert!(tree.fades_without_group(node));
    }

    #[test]
    fn what_paints_more_than_one_draw_keeps_its_group() {
        let cases: Vec<(&str, crate::div::Div)> = vec![
            ("border", div().bg(Color::RED).border(1.0, Color::WHITE)),
            (
                "shadow",
                div()
                    .bg(Color::RED)
                    .shadow_params(0.0, 2.0, 4.0, Color::BLACK),
            ),
            ("no background", div()),
            ("child", div().bg(Color::RED).child(div())),
        ];
        for (what, ui) in cases {
            let (tree, node) = only_child(ui);
            assert!(!tree.fades_without_group(node), "{what}");
        }
    }

    #[test]
    fn a_blend_or_a_3d_turn_keeps_its_group() {
        type Set = fn(&mut crate::element::RenderProps);
        let sets: [(&str, Set); 3] = [
            ("blend", |p| p.mix_blend_mode = Some(BlendMode::Multiply)),
            ("rotate_x", |p| p.rotate_x = Some(10.0)),
            ("glass", |p| {
                p.material = Some(crate::element::Material::Glass(Default::default()))
            }),
        ];
        for (what, set) in sets {
            let (mut tree, node) = only_child(div().w(20.0).h(10.0).bg(Color::RED));
            set(&mut tree.render_nodes.get_mut(&node).unwrap().props);
            assert!(!tree.fades_without_group(node), "{what}");
        }
    }
}
