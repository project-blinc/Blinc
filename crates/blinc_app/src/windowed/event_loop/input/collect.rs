//! The collectors shared by the input collectors.

use blinc_layout::LayoutNodeId;
use blinc_layout::prelude::*;

/// The collectors an input handler fills while the event router is borrowed.
pub(super) struct Collect<'a> {
    pub(super) pending_events: &'a mut Vec<PendingEvent>,
    pub(super) keyboard_events: &'a mut Vec<PendingEvent>,
    pub(super) scroll_ended: &'a mut bool,
    pub(super) gesture_ended: &'a mut bool,
    pub(super) scroll_info: &'a mut Option<(f32, f32, f32, f32)>,
    pub(super) scroll_cancel_hit:
        &'a mut Option<(blinc_layout::LayoutNodeId, Vec<blinc_layout::LayoutNodeId>)>,
}

impl Collect<'_> {
    pub(super) fn reborrow(&mut self) -> Collect<'_> {
        Collect {
            pending_events: &mut *self.pending_events,
            keyboard_events: &mut *self.keyboard_events,
            scroll_ended: &mut *self.scroll_ended,
            gesture_ended: &mut *self.gesture_ended,
            scroll_info: &mut *self.scroll_info,
            scroll_cancel_hit: &mut *self.scroll_cancel_hit,
        }
    }
}

#[derive(Clone)]
pub(super) struct PendingEvent {
    pub(super) node_id: LayoutNodeId,
    pub(super) event_type: u32,
    pub(super) mouse_x: f32,
    pub(super) mouse_y: f32,
    /// Local coordinates relative to element bounds
    pub(super) local_x: f32,
    pub(super) local_y: f32,
    /// Absolute position of element bounds (top-left corner)
    pub(super) bounds_x: f32,
    pub(super) bounds_y: f32,
    /// Computed bounds dimensions of the element
    pub(super) bounds_width: f32,
    pub(super) bounds_height: f32,
    pub(super) scroll_delta_x: f32,
    pub(super) scroll_delta_y: f32,
    /// Drag delta for DRAG/DRAG_END events
    pub(super) drag_delta_x: f32,
    pub(super) drag_delta_y: f32,
    pub(super) key_char: Option<char>,
    pub(super) key_code: u32,
    pub(super) shift: bool,
    pub(super) ctrl: bool,
    pub(super) alt: bool,
    pub(super) meta: bool,
    /// Pinch scale or rotation delta
    pub(super) pinch_scale: f32,
}

impl Default for PendingEvent {
    fn default() -> Self {
        Self {
            node_id: LayoutNodeId::default(),
            event_type: 0,
            mouse_x: 0.0,
            mouse_y: 0.0,
            local_x: 0.0,
            local_y: 0.0,
            bounds_x: 0.0,
            bounds_y: 0.0,
            bounds_width: 0.0,
            bounds_height: 0.0,
            scroll_delta_x: 0.0,
            scroll_delta_y: 0.0,
            drag_delta_x: 0.0,
            drag_delta_y: 0.0,
            key_char: None,
            key_code: 0,
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            pinch_scale: 1.0,
        }
    }
}
