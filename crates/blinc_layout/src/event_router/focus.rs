use super::{EventRouter, KEYBOARD_FOCUSED};
use crate::tree::{LayoutNodeId, StableNodeId};

thread_local! {
    static VISIBLE: std::cell::Cell<Option<LayoutNodeId>> = const { std::cell::Cell::new(None) };
}

pub(super) fn visible_node() -> Option<LayoutNodeId> {
    VISIBLE.with(std::cell::Cell::get)
}

pub(crate) struct FocusScope {
    pub key: String,
    pub return_to: Option<StableNodeId>,
    pub members: Vec<StableNodeId>,
}

impl EventRouter {
    pub(crate) fn publish_focus(&self) {
        KEYBOARD_FOCUSED.with(|v| v.set(self.focused.filter(|_| self.keyboard_focus)));
        VISIBLE.with(|v| v.set(self.focused.filter(|_| self.keyboard_focus)));
    }

    /// Keyboard input reveals the ring without moving focus.
    pub fn note_keyboard(&mut self, key: u32) {
        if key != 0 && !matches!(key, 16..=18 | 91..=92) {
            self.keyboard_focus = true;
        }
        self.publish_focus();
    }

    pub fn focus_visible(&self, node: LayoutNodeId) -> bool {
        self.focused == Some(node) && self.keyboard_focus
    }
}
