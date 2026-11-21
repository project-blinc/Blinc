//! What a node is, for what is derived from the render nodes: the child index, the plan of native
//! groups and the scan for layers and canvases.
//!
//! A change to the colour of a node's fill or border cannot change any of that, so a write that
//! leaves the structure as it was does not move the revision of the nodes. Whatever such a pass
//! reads of a node has to be in [`Structure`]: add to it before reading a new property there. The
//! font a text node is measured at is among it, since the layout is brought up to date with it only
//! when the nodes have changed.

use super::RenderNode;
use super::cull::moves_by_transform;
use crate::element::RenderLayer;
use blinc_core::Brush;

/// A value with a part of it that derived data reads.
pub(crate) trait Structural {
    type Structure: PartialEq;

    fn structure(&self) -> Self::Structure;

    /// Whether what a text node takes its font from changed between two structures.
    fn font_changed(before: &Self::Structure, after: &Self::Structure) -> bool;
}

/// What the passes derived from the render nodes read of one.
#[derive(PartialEq)]
pub(crate) struct Structure {
    /// Moves what it draws: a transform that is not the identity, a motion, fixed or sticky
    /// placement.
    moves: bool,
    motion: bool,
    fixed: bool,
    sticky: bool,
    overlay_root: bool,
    visible: bool,
    glass: bool,
    /// Whether the opacity is full, and whether it is less than 1.
    opaque: bool,
    faded: bool,
    effects: bool,
    clip_path: bool,
    clips_content: bool,
    layer: RenderLayer,
    blends: bool,
    /// The kind of fill: none, solid, gradient, image, glass or other.
    fill: u8,
    material: bool,
    shadow: bool,
    border: bool,
    /// A border with a width and a colour, which draws over what is under it.
    border_painted: bool,
    border_sides: bool,
    outline: bool,
    rotated_x: bool,
    rotated_y: bool,
    deep: bool,
    shaped_3d: bool,
    element: std::mem::Discriminant<super::ElementType>,
    /// The font a text node takes from its own properties and its ancestors'.
    font: Font,
}

#[derive(PartialEq)]
struct Font {
    size: Option<f32>,
    line_height: Option<f32>,
    weight: Option<crate::div::FontWeight>,
    italic: Option<bool>,
    letter_spacing: Option<f32>,
    family: Option<crate::div::FontFamily>,
}

impl Structural for RenderNode {
    type Structure = Structure;

    fn font_changed(before: &Structure, after: &Structure) -> bool {
        before.font != after.font
    }

    fn structure(&self) -> Structure {
        let p = &self.props;
        Structure {
            moves: moves_by_transform(&p.transform),
            motion: p.motion.is_some(),
            fixed: p.is_fixed,
            sticky: p.is_sticky,
            overlay_root: p.is_overlay_root,
            visible: p.visible,
            glass: p.is_glass(),
            opaque: p.opacity >= 0.999,
            faded: p.opacity < 1.0,
            effects: !p.layer_effects.is_empty(),
            clip_path: p.clip_path.is_some(),
            clips_content: p.clips_content,
            layer: p.layer,
            blends: p
                .mix_blend_mode
                .is_some_and(|m| m != blinc_core::BlendMode::Normal),
            fill: match &p.background {
                None => 0,
                Some(Brush::Solid(_)) => 1,
                Some(Brush::Gradient(_)) => 2,
                Some(Brush::Image(_)) => 3,
                Some(Brush::Glass(_)) => 4,
                Some(_) => 5,
            },
            material: p.material.is_some(),
            shadow: !p.shadow.is_empty() || !p.inner_shadow.is_empty(),
            border: p.border_width != 0.0,
            border_painted: p.border_width > 0.0 && p.border_color.is_some(),
            border_sides: p.border_sides.has_any(),
            outline: p.outline_width != 0.0,
            rotated_x: p.rotate_x.is_some(),
            rotated_y: p.rotate_y.is_some(),
            deep: p.depth.unwrap_or(0.0) != 0.0,
            shaped_3d: p.shape_3d.is_some(),
            element: std::mem::discriminant(&self.element_type),
            font: Font {
                size: p.font_size,
                line_height: p.line_height,
                weight: p.font_weight,
                italic: p.font_style.map(|s| s.is_italic()),
                letter_spacing: p.letter_spacing,
                family: p.font_family.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::div::div;
    use crate::renderer::RenderTree;
    use blinc_core::{Brush, Color, Transform};

    /// A tree of one node, and the revision of its nodes after a write to its props.
    fn revision_after(write: impl FnOnce(&mut crate::element::RenderProps)) -> (u64, u64) {
        let mut tree = RenderTree::from_element(&div().w(10.0).h(10.0).bg(Color::RED));
        let node = tree.root().unwrap();
        let before = tree.render_nodes.revision();
        tree.update_render_props(node, write);
        (before, tree.render_nodes.revision())
    }

    #[test]
    fn a_change_of_colour_leaves_the_revision() {
        let (before, after) = revision_after(|p| p.background = Some(Brush::Solid(Color::BLUE)));
        assert_eq!(before, after);
        let (before, after) = revision_after(|p| p.border_color = Some(Color::GREEN));
        assert_eq!(before, after);
    }

    #[test]
    fn a_change_a_pass_reads_moves_it() {
        type Write = Box<dyn FnOnce(&mut crate::element::RenderProps)>;
        let cases: Vec<(&str, Write)> = vec![
            ("opacity", Box::new(|p| p.opacity = 0.5)),
            (
                "transform",
                Box::new(|p| p.transform = Some(Transform::translate(5.0, 0.0))),
            ),
            ("clip", Box::new(|p| p.clips_content = !p.clips_content)),
            ("fixed", Box::new(|p| p.is_fixed = true)),
            ("sticky", Box::new(|p| p.is_sticky = true)),
            ("overlay root", Box::new(|p| p.is_overlay_root = true)),
            ("fill dropped", Box::new(|p| p.background = None)),
            ("border", Box::new(|p| p.border_width = 2.0)),
            ("hidden", Box::new(|p| p.visible = false)),
            ("rotation", Box::new(|p| p.rotate_x = Some(10.0))),
        ];
        for (what, write) in cases {
            let (before, after) = revision_after(write);
            assert_ne!(before, after, "{what} did not move the revision");
        }
    }
}
