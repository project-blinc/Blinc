//! Driving an editor from outside its own event handlers, as a toolbar or an application does.

use std::sync::Arc;

use super::find::notify_changed;
use super::{CodeEditorData, SharedCodeEditorState};
use crate::stateful::{StateTransitions, refresh_stateful};
use crate::widgets::text_area::TextPosition;
use crate::widgets::text_input::{
    blur_all_text_inputs, decrement_focus_count, increment_focus_count,
    request_continuous_redraw_pub,
};

impl CodeEditorData {
    /// Replace all the text, as the application's own change: it is not undone, and `on_change`
    /// does not report it. The cursor stays where it can, and an open search runs again.
    pub fn set_value(&mut self, text: impl AsRef<str>) {
        let mut lines: Vec<String> = text.as_ref().lines().map(str::to_owned).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        if lines == self.lines {
            return;
        }
        self.lines = lines;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.selection_start = None;
        self.drag_anchor = None;
        let line = self.cursor.line.min(self.lines.len() - 1);
        let column = self.cursor.column.min(self.lines[line].chars().count());
        self.cursor = TextPosition::new(line, column);
        self.folded_regions.clear();
        self.invalidate_all_highlights();
        self.sync_highlight_cache();
        if !self.search_query.is_empty() {
            self.execute_search();
        }
    }
}

/// [`CodeEditorData::set_value`], and the editor drawn again.
pub fn set_code_value(state: &SharedCodeEditorState, text: impl AsRef<str>) {
    if let Ok(mut d) = state.lock() {
        d.set_value(text);
    }
    refresh_code_editor(state);
}

/// Draw the editor again after its state was changed from outside, as a search from a toolbar is.
pub fn refresh_code_editor(state: &SharedCodeEditorState) {
    let stateful = state.lock().ok().and_then(|d| d.editor_stateful.clone());
    if let Some(stateful) = stateful {
        refresh_stateful(&stateful);
    }
}

/// [`CodeEditorData::replace_current`], told to `on_change` and drawn. Returns whether the text
/// changed.
pub fn replace_code_match(state: &SharedCodeEditorState) -> bool {
    let changed = state.lock().is_ok_and(|mut d| d.replace_current());
    after_replace(state, changed);
    changed
}

/// [`CodeEditorData::replace_all`], told to `on_change` and drawn. Returns how many matches were
/// replaced.
pub fn replace_all_code_matches(state: &SharedCodeEditorState) -> usize {
    let replaced = state.lock().map_or(0, |mut d| d.replace_all());
    after_replace(state, replaced > 0);
    replaced
}

fn after_replace(state: &SharedCodeEditorState, changed: bool) {
    refresh_code_editor(state);
    if changed {
        notify_changed(state);
    }
}

/// Give the editor the keyboard, as a press on it does, taking it from any text field.
pub fn focus_code_editor(state: &SharedCodeEditorState) {
    // The fields let go first: their blur must not undo this focus.
    blur_all_text_inputs();
    let (stateful, node) = {
        let Ok(mut d) = state.lock() else {
            return;
        };
        if !d.focused {
            d.focused = true;
            increment_focus_count();
            request_continuous_redraw_pub();
        }
        d.reset_cursor_blink();
        (d.editor_stateful.clone(), d.editor_node)
    };
    if let Some(stateful) = &stateful
        && let Ok(mut shared) = stateful.lock()
        && !shared.state.is_focused()
        && let Some(next) = shared
            .state
            .on_event(blinc_core::events::event_types::FOCUS)
    {
        shared.state = next;
        shared.needs_visual_update = true;
    }
    if let Some(node) = node {
        let blurred = Arc::clone(state);
        crate::widgets::text_input::set_focused_editable_node(
            node,
            Some(Box::new(move || {
                if let Ok(mut d) = blurred.lock()
                    && d.focused
                {
                    d.focused = false;
                    d.selection_start = None;
                    if let Ok(mut cursor) = d.cursor_state.lock() {
                        cursor.visible = false;
                    }
                    decrement_focus_count();
                }
            })),
        );
    }
    refresh_code_editor(state);
}
