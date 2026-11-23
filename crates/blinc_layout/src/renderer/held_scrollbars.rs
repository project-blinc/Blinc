//! Scrollbars that fade by themselves, drawn each frame apart from the batch.
//!
//! An auto-hiding scrollbar changes its opacity while the content under it stays still. Painted
//! into the batch, every frame of its fade would be a full paint. The walker instead records
//! where such a scrollbar is (its transform and the clip in force) and leaves it out, and the
//! renderer draws it from the physics each frame, as it does for containers on rows. It is drawn
//! after the content, so only a container nothing above it can move and nothing inside it can
//! cover is held: no scroll container, bound node, animation or overlay above it, nothing under
//! it raised to a higher layer (a z-index, a stack layer), and no overlay open.

use super::RenderTree;
use crate::tree::LayoutNodeId;
use blinc_core::{Affine2D, ClipShape, CornerRadius, DrawContext, Rect, Transform};

/// Where a held scrollbar is drawn: the transform and clip of its container's viewport at the
/// last full paint, both in screen space, and the viewport's size.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HeldScrollbar {
    affine: [f32; 6],
    clip: Option<([f32; 4], [f32; 4])>,
    size: (f32, f32),
}

impl RenderTree {
    /// Whether the last full paint left the scrollbar of `node` out of the batch.
    pub fn scrollbar_held(&self, node: LayoutNodeId) -> bool {
        self.held_scrollbars.borrow().contains_key(&node)
    }

    /// How long until a shown scrollbar starts to fade. A window with nothing else to draw must
    /// still draw a frame then, or the scrollbar stays until something does.
    pub fn scrollbar_wake_in(&self) -> Option<std::time::Duration> {
        self.scroll_physics
            .values()
            .filter_map(|p| p.try_lock().ok().and_then(|p| p.scrollbar_hide_in()))
            .min_by(f32::total_cmp)
            .map(std::time::Duration::from_secs_f32)
    }

    /// Whether a shown scrollbar's fade is due by now, counting the time since the last scroll
    /// tick. For a runner that polls each display frame whether to draw.
    pub fn scrollbar_fade_due(&self) -> bool {
        let Some(wait) = self.scrollbar_wake_in() else {
            return false;
        };
        let since_tick = self.last_scroll_tick_us.map_or(0, |last| {
            crate::widgets::text_input::elapsed_micros().saturating_sub(last)
        });
        wait <= std::time::Duration::from_micros(since_tick)
    }

    /// At the scrollbar of the scroll container `node`, in its viewport's space: record it to be
    /// drawn each frame if it fades by itself and may be held. Returns whether it was, in which
    /// case the walker leaves it out.
    pub(crate) fn hold_scrollbar(
        &self,
        ctx: &dyn DrawContext,
        node: LayoutNodeId,
        size: (f32, f32),
    ) -> bool {
        if !self.scroll_rows_enabled() || !self.may_hold_scrollbar(node) {
            return false;
        }
        self.held_scrollbars.borrow_mut().insert(
            node,
            HeldScrollbar {
                affine: ctx.current_affine_elements(),
                clip: ctx.current_clip_rounded(),
                size,
            },
        );
        true
    }

    fn may_hold_scrollbar(&self, node: LayoutNodeId) -> bool {
        let fades = self
            .scroll_physics
            .get(&node)
            .and_then(|p| p.try_lock().ok().map(|p| p.scrollbar_fades()))
            .unwrap_or(false);
        if !fades || overlay_open() {
            return false;
        }
        let animating = self.current_animation_status.borrow();
        !self
            .layout_tree
            .ancestors(node)
            .into_iter()
            .any(|ancestor| {
                self.scroll_physics.contains_key(&ancestor)
                    || self.motion_bindings.contains_key(&ancestor)
                    || matches!(
                        animating.get(&ancestor),
                        Some(super::AnimationStatus::Animating(_))
                    )
                    || self
                        .render_nodes
                        .get(&ancestor)
                        .is_some_and(|r| r.props.is_overlay_root || r.props.motion.is_some())
            })
    }

    pub(crate) fn clear_held_scrollbars(&self) {
        self.held_scrollbars.borrow_mut().clear();
    }

    /// Draw the held scrollbars. `ctx` is in screen space, with no transform or clip pushed.
    pub fn draw_held_scrollbars(&self, ctx: &mut dyn DrawContext) {
        let held: Vec<(LayoutNodeId, HeldScrollbar)> = self
            .held_scrollbars
            .borrow()
            .iter()
            .map(|(node, held)| (*node, *held))
            .collect();
        for (node, held) in held {
            let Some(physics) = self.scroll_physics.get(&node) else {
                continue;
            };
            let info = match physics.try_lock() {
                Ok(p) => p.scrollbar_render_info(),
                Err(_) => continue,
            };
            if info.opacity <= 0.01 {
                continue;
            }
            if let Some(([x, y, w, h], [tl, tr, br, bl])) = held.clip {
                let rect = Rect::new(x, y, w, h);
                ctx.push_clip(if [tl, tr, br, bl].iter().all(|r| *r <= 0.0) {
                    ClipShape::rect(rect)
                } else {
                    ClipShape::rounded_rect(rect, CornerRadius::new(tl, tr, br, bl))
                });
            }
            ctx.push_transform(Transform::Affine2D(Affine2D {
                elements: held.affine,
            }));
            self.render_scrollbar(ctx, held.size.0, held.size.1, &info);
            ctx.pop_transform();
            if held.clip.is_some() {
                ctx.pop_clip();
            }
        }
    }
}

fn overlay_open() -> bool {
    crate::overlay_state::overlay_stack()
        .lock()
        .map(|s| s.has_visible_overlays())
        .unwrap_or(false)
}
