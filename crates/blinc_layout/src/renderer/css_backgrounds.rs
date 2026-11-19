//! Retain the owning fill of a background-only CSS animation or transition.

use super::RenderTree;
use crate::{element::RenderLayer, tree::LayoutNodeId};
use blinc_animation::KeyframeProperties;
use blinc_core::{Brush, DrawContext};

pub(super) struct Painted {
    owner: u32,
    alpha: f32,
    dynamic: bool,
}

fn background_only(props: &KeyframeProperties) -> bool {
    props.background_color.is_some()
        && KeyframeProperties {
            background_color: None,
            ..props.clone()
        } == KeyframeProperties::default()
}

impl RenderTree {
    pub(super) fn retains_css_background(
        &self,
        node: LayoutNodeId,
        props: &KeyframeProperties,
    ) -> bool {
        background_only(props) && self.css_backgrounds.borrow().contains_key(&node)
    }

    pub(super) fn record_css_background(
        &self,
        ctx: &mut dyn DrawContext,
        node: LayoutNodeId,
        start: usize,
        alpha: f32,
    ) {
        let owner = node.to_raw() as u32;
        if owner == 0
            || owner > 0x00ff_ffff
            || ctx.bg_batch_is_composite()
            || self.motion_promotion.get()
        {
            return;
        }
        let Some(render) = self.render_nodes.get(&node) else {
            return;
        };
        if render.props.layer != RenderLayer::Background
            || render.props.is_glass()
            || !matches!(render.props.background, Some(Brush::Solid(_)))
            || self.inside_overlay_root(node)
            || self
                .motion_bindings
                .get(&node)
                .is_some_and(|b| b.opacity.is_some())
        {
            return;
        }
        let Some(stable) = self.stable_id(node) else {
            return;
        };
        let source = {
            let store = self.css_anim_store.lock().unwrap();
            let mut active = store
                .animations
                .get(&stable)
                .into_iter()
                .chain(store.transitions.get(&stable))
                .filter(|a| a.is_playing)
                .peekable();
            active.peek().is_some() && active.all(|a| background_only(&a.current_properties))
        };
        if source
            && let Some(baked_alpha) =
                ctx.tag_background_color(start, ctx.bg_primitive_count(), owner)
        {
            self.css_backgrounds.borrow_mut().insert(
                node,
                Painted {
                    owner,
                    alpha: alpha * baked_alpha,
                    dynamic: ctx.bg_batch_is_dynamic(),
                },
            );
        }
    }

    /// Current fill colours and their cached owners. Tags survive native group rebasing.
    /// Returns `None` when a source now requires a full paint.
    pub fn retained_background_colors(&self) -> Option<Vec<(u32, [f32; 4], bool)>> {
        let painted = self.css_backgrounds.borrow();
        let store = self.css_anim_store.lock().unwrap();
        let mut colors = Vec::with_capacity(painted.len());
        for (node, record) in painted.iter() {
            if self
                .motion_bindings
                .get(node)
                .is_some_and(|b| b.opacity.is_some())
            {
                return None;
            }
            let Some(stable) = self.stable_id(*node) else {
                continue;
            };
            if store
                .animations
                .get(&stable)
                .into_iter()
                .chain(store.transitions.get(&stable))
                .any(|a| a.is_playing && !background_only(&a.current_properties))
            {
                return None;
            }
            let Some(render) = self.render_nodes.get(node) else {
                continue;
            };
            let Some(Brush::Solid(color)) = &render.props.background else {
                return None;
            };
            colors.push((
                record.owner,
                [color.r, color.g, color.b, color.a * record.alpha],
                record.dynamic,
            ));
        }
        Some(colors)
    }
}
