//! Tab traversal and Enter/Space activation.
//!
//! An element is a tab stop when it carries the `tabindex=0` attribute (or
//! `tabindex=text`, for a text input, which Tab focuses by clicking it) and
//! is neither `disabled` nor empty. Tab and Shift+Tab move focus through
//! the stops in tree order, wrapping at the ends; Enter and Space click the
//! focused control. Tab leaves a text input or area; a code or
//! rich-text editor keeps it.

mod focus_scope;
mod roving;

use blinc_core::events::event_types;

use crate::event_router::EventRouter;
use crate::tree::LayoutNodeId;

use super::RenderTree;

pub const KEY_TAB: u32 = 9;
pub const KEY_ENTER: u32 = 13;
pub const KEY_SPACE: u32 = 32;

impl RenderTree {
    fn is_tab_stop(&self, node: LayoutNodeId) -> bool {
        matches!(
            self.element_registry()
                .attribute(node, "tabindex")
                .as_deref(),
            Some("0" | "text")
        ) && self.is_focusable(node)
    }

    /// The tab stops in tree order.
    pub fn tab_stops(&self) -> Vec<LayoutNodeId> {
        self.tab_walk(None).0
    }

    fn tab_stop_visible(&self, node: LayoutNodeId) -> bool {
        let Some(mut bounds) = self.get_absolute_bounds(node) else {
            return false;
        };
        for ancestor in self.layout_tree.ancestors(node) {
            if (self.is_scroll_container(ancestor) || Some(ancestor) == self.root())
                && let Some(view) = self.get_absolute_bounds(ancestor)
            {
                let left = bounds.x.max(view.x);
                let top = bounds.y.max(view.y);
                let right = (bounds.x + bounds.width).min(view.x + view.width);
                let bottom = (bounds.y + bounds.height).min(view.y + view.height);
                if right <= left || bottom <= top {
                    return false;
                }
                bounds.x = left;
                bounds.y = top;
                bounds.width = right - left;
                bounds.height = bottom - top;
            }
        }
        true
    }

    /// The stops in tree order, and how many of them precede `at` (counted
    /// before `at` itself, whether or not it is one).
    fn tab_walk(&self, at: Option<LayoutNodeId>) -> (Vec<LayoutNodeId>, usize) {
        let mut stops = Vec::new();
        let mut before = 0;
        let Some(root) = self.root() else {
            return (stops, before);
        };
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if Some(node) == at {
                before = stops.len();
            }
            if self.is_tab_stop(node) {
                stops.push(node);
            }
            stack.extend(self.layout_tree.children(node).into_iter().rev());
        }
        if let Some((_, scope)) = self.active_focus_scopes().last() {
            let inside =
                |n: LayoutNodeId| n == *scope || self.layout_tree.ancestors(n).contains(scope);
            before = stops[..before].iter().filter(|n| inside(**n)).count();
            stops.retain(|n| inside(*n));
        }
        (stops, before)
    }

    /// Scroll every scroll container around `node` just far enough to show
    /// it, nearest container first. Returns whether any offset moved.
    pub fn scroll_into_view(&mut self, node: LayoutNodeId) -> bool {
        const MARGIN: f32 = 8.0;
        let mut moved = false;
        for container in self.layout_tree.ancestors(node) {
            if !self.is_scroll_container(container) {
                continue;
            }
            let (Some(n), Some(v)) = (
                self.get_absolute_bounds(node),
                self.get_absolute_bounds(container),
            ) else {
                continue;
            };
            let (ox, oy) = self.get_scroll_offset(container);
            // The far edge first, so the near edge wins when the node is
            // larger than the viewport.
            let mut dx = 0.0;
            if n.x + n.width > v.x + v.width - MARGIN {
                dx = (v.x + v.width - MARGIN) - (n.x + n.width);
            }
            if n.x < v.x + MARGIN + dx {
                dx = (v.x + MARGIN) - n.x;
            }
            let mut dy = 0.0;
            if n.y + n.height > v.y + v.height - MARGIN {
                dy = (v.y + v.height - MARGIN) - (n.y + n.height);
            }
            if n.y < v.y + MARGIN + dy {
                dy = (v.y + MARGIN) - n.y;
            }
            if dx == 0.0 && dy == 0.0 {
                continue;
            }
            let (content_w, content_h) = self
                .layout_tree
                .get_content_size(container)
                .unwrap_or((v.width, v.height));
            let min_x = -(content_w - v.width).max(0.0);
            let min_y = -(content_h - v.height).max(0.0);
            let nx = (ox + dx).clamp(min_x, 0.0);
            let ny = (oy + dy).clamp(min_y, 0.0);
            if (nx - ox).abs() < 0.5 && (ny - oy).abs() < 0.5 {
                continue;
            }
            self.set_scroll_offset(container, nx, ny);
            if let Some(physics) = self.scroll_physics.get(&container)
                && let Ok(mut p) = physics.try_lock()
            {
                p.offset_x = nx;
                p.offset_y = ny;
                p.velocity_x = 0.0;
                p.velocity_y = 0.0;
            }
            moved = true;
        }
        moved
    }

    /// Press and release the centre of `node`.
    fn click_node(&mut self, node: LayoutNodeId) {
        let (bx, by, bw, bh) = self
            .get_absolute_bounds(node)
            .map(|b| (b.x, b.y, b.width, b.height))
            .unwrap_or((0.0, 0.0, 0.0, 0.0));
        let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
        for event in [event_types::POINTER_DOWN, event_types::POINTER_UP] {
            self.dispatch_event_full(
                node,
                event,
                cx,
                cy,
                bw / 2.0,
                bh / 2.0,
                bx,
                by,
                bw,
                bh,
                0.0,
                0.0,
                1.0,
                false,
                false,
                false,
                false,
            );
        }
    }

    /// Handle Tab, Shift+Tab, Enter and Space. Returns whether the key was
    /// used.
    pub fn handle_navigation_key(
        &mut self,
        router: &mut EventRouter,
        key_code: u32,
        shift: bool,
    ) -> bool {
        use crate::widgets::text_input::{
            blur_all_text_inputs, focused_editable_node_id, focused_text_area_node_id,
            focused_text_input_node_id,
        };
        self.sync_keyboard_focus(router);
        router.note_keyboard(key_code);
        if key_code == 27 && self.dismiss_focus_scope(router) {
            return true;
        }
        router.focus_stable = router.focused().and_then(|n| self.stable_id(n));
        let editable = focused_editable_node_id();
        let typed = focused_text_input_node_id().or_else(focused_text_area_node_id);
        let in_text = editable.is_some() || typed.is_some();
        // A plain input or area gives Tab up; an editor keeps it.
        let leavable = typed.is_some() && (editable.is_none() || editable == typed);
        if in_text && !(key_code == KEY_TAB && leavable) {
            return false;
        }
        if self.handle_roving_key(router, key_code, shift) {
            return true;
        }
        match key_code {
            KEY_TAB => {
                let focused = typed
                    .or(router.focused())
                    .map(|n| self.roving_group(n).unwrap_or(n));
                let (stops, before) = self.tab_walk(focused);
                if stops.is_empty() {
                    return !self.active_focus_scopes().is_empty();
                }
                let len = stops.len();
                let on_stop = focused.is_some_and(|f| stops.contains(&f));
                let next = if !focused.is_some_and(|node| self.tab_stop_visible(node)) {
                    let mut visible = stops
                        .iter()
                        .enumerate()
                        .filter(|(_, node)| self.tab_stop_visible(**node));
                    if shift {
                        visible
                            .next_back()
                            .map(|(index, _)| index)
                            .unwrap_or(len - 1)
                    } else {
                        visible.next().map(|(index, _)| index).unwrap_or(0)
                    }
                } else if shift {
                    (before + len - 1) % len
                } else if on_stop {
                    (before + 1) % len
                } else {
                    before % len
                };
                let target = stops[next];
                let target = self.roving_items(target).first().copied().unwrap_or(target);
                self.move_focus(router, target);
                true
            }
            KEY_ENTER | KEY_SPACE if router.keyboard_focus() => {
                let Some(node) = router.focused() else {
                    return false;
                };
                if !self.is_focusable(node) {
                    return false;
                }
                self.click_node(node);
                true
            }
            _ => false,
        }
    }
}
