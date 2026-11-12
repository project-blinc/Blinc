//! Scroll containers painted as nodes moved by rows: the content once, the scroll as a row.
//!
//! The walker paints a qualifying container's content into the dynamic batch at the offset it has
//! at that paint, and a frame in which only the offset changed writes a row for the change instead
//! of walking and painting the content again. The scrollbar is not part of the content: the
//! renderer draws it each frame from the physics. Content that cannot be moved this way
//! (something fixed or sticky, an SVG, a canvas, a group, anything already moving) keeps the
//! container painted as before, and so does a container inside another that is on rows.

use super::ElementType;
use super::RenderTree;
use crate::tree::LayoutNodeId;
use blinc_core::{DrawContext, Transform};

/// How far, in logical pixels, a container on rows may be scrolled from where its content was
/// painted before the content is painted again: the part of the painted window past the viewport
/// that is not kept for shadows and edges.
const SPARE: f32 = 80.0;

impl RenderTree {
    /// Allow or forbid painting scroll containers as nodes moved by rows. Set before painting.
    pub fn set_scroll_rows(&self, on: bool) {
        self.scroll_rows.set(on);
    }

    /// Whether scroll containers may be painted as nodes moved by rows.
    pub fn scroll_rows_enabled(&self) -> bool {
        self.scroll_rows.get()
    }

    /// Whether the last paint put the scroll container `node` on rows.
    pub fn is_scroll_row_node(&self, node: LayoutNodeId) -> bool {
        self.scroll_row_nodes.borrow().contains_key(&node)
    }

    /// How far past its viewport the content of the scroll container `node` was painted, if it is
    /// on rows. What is collected for it, text and images, is cut at the same distance.
    pub fn scroll_row_overscan(&self, node: LayoutNodeId) -> Option<f32> {
        self.scroll_row_nodes.borrow().get(&node).map(|(o, _)| *o)
    }

    /// Set how far past a cull rect content is drawn for the walk in progress, returning the
    /// distance it had, to put back.
    pub fn set_cull_overscan(&self, overscan: f32) -> f32 {
        self.cull_overscan.replace(overscan)
    }

    /// The offset of the scroll container `node` now.
    pub fn scroll_row_pose(&self, node: LayoutNodeId) -> Option<(f32, f32)> {
        self.is_scroll_row_node(node)
            .then(|| self.get_scroll_offset(node))
    }

    /// Whether the offset of a scroll container on rows has changed since its rows were written.
    pub fn scroll_rows_moved(&self) -> bool {
        let applied = self.scroll_applied.borrow();
        self.scroll_row_nodes.borrow().keys().any(|node| {
            let now = self.get_scroll_offset(*node);
            applied
                .get(node)
                .is_none_or(|a| (a.0 - now.0).abs() > 0.01 || (a.1 - now.1).abs() > 0.01)
        })
    }

    /// Say the rows have been written for the offsets the containers on rows have now.
    pub fn note_scroll_rows_applied(&self) {
        let mut applied = self.scroll_applied.borrow_mut();
        for node in self.scroll_row_nodes.borrow().keys() {
            applied.insert(*node, self.get_scroll_offset(*node));
        }
    }

    /// Whether a scroll container that is not on rows has an offset other than the one it was
    /// painted at: the paint is stale and has to be made again.
    pub fn scroll_off_rows_changed(&self) -> bool {
        let rows = self.scroll_row_nodes.borrow();
        let painted = self.scroll_painted.borrow();
        self.scroll_physics.keys().any(|node| {
            if rows.contains_key(node) {
                return false;
            }
            let now = self.get_scroll_offset(*node);
            painted
                .get(node)
                .is_none_or(|p| (p.0 - now.0).abs() > 0.01 || (p.1 - now.1).abs() > 0.01)
        })
    }

    /// Whether, at the last tick of the scroll physics, a scroll container that is not on rows was
    /// gliding, bouncing or fading its scrollbar.
    pub fn scroll_off_rows_animating(&self) -> bool {
        self.scroll_off_rows_animating.get()
    }

    pub(crate) fn set_scroll_off_rows_animating(&self, on: bool) {
        self.scroll_off_rows_animating.set(on);
    }

    /// How far the cull rect is widened for the walk in progress.
    pub(crate) fn cull_overscan(&self) -> f32 {
        self.cull_overscan.get()
    }

    pub(crate) fn clear_scroll_rows_record(&self) {
        self.scroll_row_nodes.borrow_mut().clear();
        self.scroll_painted.borrow_mut().clear();
        self.scroll_applied.borrow_mut().clear();
    }

    pub(crate) fn record_scroll_painted(&self, node: LayoutNodeId, offset: (f32, f32)) {
        self.scroll_painted.borrow_mut().insert(node, offset);
    }

    /// Record that `node` was painted on rows, with `overscan` of content past its viewport.
    /// Returns the margin its offset may move by.
    pub(crate) fn record_scroll_row_node(
        &self,
        node: LayoutNodeId,
        offset: (f32, f32),
        overscan: f32,
    ) -> f32 {
        let margin = (overscan - SPARE).max(0.0);
        self.scroll_row_nodes
            .borrow_mut()
            .insert(node, (overscan, margin));
        self.scroll_applied.borrow_mut().insert(node, offset);
        margin
    }

    /// Whether the content of the scroll container `node` can be painted once and moved by a row:
    /// nothing under it moves on its own, draws apart from the batch, or is positioned against
    /// the scroll.
    pub(crate) fn scroll_content_qualifies(&self, node: LayoutNodeId) -> bool {
        self.layout_tree
            .children(node)
            .into_iter()
            .all(|child| self.scroll_subtree_qualifies(child))
    }

    fn scroll_subtree_qualifies(&self, node: LayoutNodeId) -> bool {
        let Some(render) = self.render_nodes.get(&node) else {
            return true;
        };
        let props = &render.props;
        let simple = !self.moves_its_content(node)
            && !props.is_overlay_root
            && !props.is_glass()
            && props.opacity >= 0.999
            && props.layer_effects.is_empty()
            && props.clip_path.is_none()
            && !matches!(
                render.element_type,
                ElementType::Svg(_) | ElementType::Canvas(_)
            )
            && !matches!(
                self.current_animation_status.borrow().get(&node),
                Some(super::AnimationStatus::Animating(_))
            );
        simple
            && self
                .layout_tree
                .children(node)
                .into_iter()
                .all(|child| self.scroll_subtree_qualifies(child))
    }

    /// Draw the scrollbars of the containers on rows, as the walker draws them for the others:
    /// after the content, in the container's own space. Their offsets change every frame, which is
    /// not a change of the content.
    pub fn draw_scrollbars(&self, ctx: &mut dyn DrawContext) {
        let nodes: Vec<LayoutNodeId> = self.scroll_row_nodes.borrow().keys().copied().collect();
        for node in nodes {
            let Some(physics) = self.scroll_physics.get(&node) else {
                continue;
            };
            let Some(bounds) = self.layout_tree.get_absolute_bounds(node) else {
                continue;
            };
            let info = match physics.try_lock() {
                Ok(p) => p.scrollbar_render_info(),
                Err(_) => continue,
            };
            if info.opacity <= 0.01 {
                continue;
            }
            // Where the container sits on screen: its layout place moved by the scroll above it.
            let mut shift = (0.0, 0.0);
            for ancestor in self.layout_tree.ancestors(node) {
                if self.scroll_physics.contains_key(&ancestor) {
                    let (x, y) = self.get_scroll_offset(ancestor);
                    shift = (shift.0 + x, shift.1 + y);
                }
            }
            ctx.push_transform(Transform::translate(bounds.x + shift.0, bounds.y + shift.1));
            self.render_scrollbar(ctx, bounds.width, bounds.height, &info);
            ctx.pop_transform();
        }
    }
}
