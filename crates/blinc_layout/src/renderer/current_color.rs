//! CSS `currentColor` in shadows: a shadow that names no colour takes the text colour of the
//! element casting it, put in as it is painted.

use std::borrow::Cow;

use blinc_animation::KeyframeProperties;
use blinc_core::{Color, GlassStyle, Shadow};

use super::{ElementType, RenderTree};
use crate::element::RenderProps;
use crate::tree::LayoutNodeId;

impl RenderTree {
    /// The CSS `color` of a node: its own, else the nearest ancestor's. With none set, the colour
    /// the node's own text draws in, and black for a node that has no text.
    pub fn current_color(&self, node: LayoutNodeId) -> Color {
        let rgba = |c: [f32; 4]| Color::rgba(c[0], c[1], c[2], c[3]);
        let mut at = Some(node);
        while let Some(id) = at {
            if let Some(color) = self.render_nodes.get(&id).and_then(|n| n.props.text_color) {
                return rgba(color);
            }
            at = self.element_registry.get_parent(id);
        }
        match self.render_nodes.get(&node).map(|n| &n.element_type) {
            Some(ElementType::Text(text)) => rgba(text.color),
            Some(ElementType::StyledText(text)) => rgba(text.default_color),
            _ => Color::BLACK,
        }
    }

    /// `shadows` as `node` casts them, with every `currentColor` replaced by its colour.
    pub fn resolved_shadows<'a>(
        &self,
        node: LayoutNodeId,
        shadows: &'a [Shadow],
    ) -> Cow<'a, [Shadow]> {
        if !shadows.iter().any(Shadow::uses_current_color) {
            return Cow::Borrowed(shadows);
        }
        let current = self.current_color(node);
        Cow::Owned(
            shadows
                .iter()
                .map(|shadow| shadow.with_current_color(current))
                .collect(),
        )
    }

    /// The glass style of `node` at `depth`, its shadow's `currentColor` resolved.
    pub(crate) fn glass_of(
        &self,
        node: LayoutNodeId,
        props: &RenderProps,
        depth: u32,
    ) -> Option<GlassStyle> {
        let mut glass = props.glass_style(depth)?;
        if let Some(shadow) = glass.shadow {
            glass.shadow = Some(self.resolved_shadows(node, &[shadow])[0]);
        }
        Some(glass)
    }

    /// The colour of a `drop-shadow()` effect as `node` casts it.
    pub(crate) fn resolved_effect_color(&self, node: LayoutNodeId, color: Color) -> Color {
        let shadow = Shadow::new(0.0, 0.0, 0.0, color);
        if shadow.uses_current_color() {
            shadow.with_current_color(self.current_color(node)).color
        } else {
            color
        }
    }
}

/// Puts `current` in for every `currentColor` a snapshot or keyframe holds, so that animations
/// run between colours.
pub(crate) fn resolve_keyframe_colors(properties: &mut KeyframeProperties, current: Color) {
    let resolve = |c: &mut [f32]| {
        if c[0] < 0.0 {
            c.copy_from_slice(&[current.r, current.g, current.b, current.a * c[3]]);
        }
    };
    if let Some(color) = &mut properties.shadow_color {
        resolve(color);
    }
    if let Some(color) = &mut properties.text_shadow_color {
        resolve(color);
    }
    for layer in properties.shadow_stack.iter_mut().flatten() {
        resolve(&mut layer[4..]);
    }
}
