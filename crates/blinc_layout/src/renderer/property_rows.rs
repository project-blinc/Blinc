//! Retain paint for reactive translation writes while keeping their live RenderProps.

use super::RenderTree;
use crate::tree::LayoutNodeId;
use blinc_core::Transform;

fn translation(transform: Option<&Transform>) -> Option<(f32, f32)> {
    match transform {
        None => Some((0.0, 0.0)),
        Some(Transform::Affine2D(matrix)) => {
            let [a, b, c, d, x, y] = matrix.elements;
            (a == 1.0 && b == 0.0 && c == 0.0 && d == 1.0 && x.is_finite() && y.is_finite())
                .then_some((x, y))
        }
        Some(Transform::Mat4(_)) => None,
    }
}

impl RenderTree {
    /// The clip before a bound node began painting, including retained inner clips.
    pub fn binding_clip_bounds(&self, node: LayoutNodeId) -> Option<[f32; 4]> {
        self.binding_clip_bounds.borrow().get(&node).copied()
    }

    /// Changes when a property drain needs fresh paint. Compare before and after draining,
    /// including writes applied by nested region reconciles.
    pub fn property_paint_revision(&self) -> u64 {
        self.property_paint_revision
    }

    /// The live translation of a reactive node carried by retained rows.
    pub fn property_row_translation(&self, node: LayoutNodeId) -> Option<(f32, f32)> {
        if !self.property_translation_nodes.contains(&node)
            || !self
                .motion_bindings
                .get(&node)
                .is_some_and(|b| b.is_empty())
        {
            return None;
        }
        translation(self.render_nodes.get(&node)?.props.transform.as_ref())
    }

    /// Whether retained rows carry any reactive translations.
    pub fn has_property_translation_rows(&self) -> bool {
        !self.property_translation_nodes.is_empty()
    }

    fn can_retain_property_translation(&self, node: LayoutNodeId) -> bool {
        if self.motion_promotion.get()
            || self
                .motion_bindings
                .get(&node)
                .is_some_and(|b| !self.property_translation_nodes.contains(&node) || !b.is_empty())
        {
            return false;
        }
        // Rows translate in window axes. Scaled or turned ancestors need a fresh paint.
        for n in std::iter::once(node).chain(self.layout_tree.ancestors(node)) {
            let Some(render) = self.render_nodes.get(&n) else {
                return false;
            };
            if translation(render.props.transform.as_ref()).is_none()
                || render.props.motion.is_some()
                || render.props.rotate_x.is_some()
                || render.props.rotate_y.is_some()
                || render.props.perspective.is_some()
                || render.props.translate_z.is_some()
                || render.props.depth.unwrap_or(0.0) > 0.0
                || render.props.shape_3d.is_some()
                || self.motion_bindings.get(&n).is_some_and(|b| {
                    b.scale.is_some()
                        || b.scale_x.is_some()
                        || b.scale_y.is_some()
                        || b.rotation.is_some()
                        || b.rotation_timeline.is_some()
                })
            {
                return false;
            }
        }
        if let Some(stable) = self.stable_id(node) {
            let Ok(store) = self.css_anim_store.lock() else {
                return false;
            };
            if store.animations.get(&stable).is_some_and(|a| a.is_playing)
                || store.transitions.get(&stable).is_some_and(|t| t.is_playing)
            {
                return false;
            }
        }
        true
    }

    pub(super) fn retain_property_translation(&mut self, node: LayoutNodeId) -> bool {
        if !self.can_retain_property_translation(node) {
            if self.property_translation_nodes.remove(&node)
                && self
                    .motion_bindings
                    .get(&node)
                    .is_some_and(|b| b.is_empty())
            {
                self.motion_bindings.remove(&node);
            }
            return false;
        }
        if self.property_translation_nodes.insert(node) || !self.motion_bindings.contains_key(&node)
        {
            // An empty binding gives the painter an owner and ordinal. The transform itself
            // stays in RenderProps, so painting and hit testing apply it exactly once.
            self.motion_bindings.insert(node, Default::default());
            return false;
        }
        self.composite_bindings
            .borrow()
            .get(&node)
            .is_some_and(|m| m.dynamic_batch)
    }
}
