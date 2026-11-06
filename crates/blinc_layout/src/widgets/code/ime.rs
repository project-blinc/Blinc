use super::*;
use crate::ime::{ImeBinding, ImeContent, char_offset, line_offset, line_position, utf16_offset};

impl CodeEditorData {
    fn ime_entry(&self) -> UndoEntry {
        UndoEntry {
            lines: self.lines.clone(),
            cursor: self.cursor,
            selection_start: self.selection_start,
        }
    }

    fn finish_ime(&mut self) -> Option<(OnChangeCallback, String)> {
        self.ime.composing = None;
        let before = self.ime.original.take()?;
        if before.lines == self.lines {
            return None;
        }
        self.undo_stack.push(before);
        if self.undo_stack.len() > 200 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.on_change
            .clone()
            .map(|callback| (callback, self.value()))
    }
}

pub(super) fn finish_on_blur(data: &SharedCodeEditorState) {
    let callback = {
        let mut d = data.lock().unwrap();
        if d.focused {
            return;
        }
        d.ime.blur();
        d.finish_ime()
    };
    if let Some((callback, value)) = callback {
        callback(&value);
    }
}

pub(super) fn commit(data: &SharedCodeEditorState) {
    let callback = {
        let mut d = data.lock().unwrap();
        if d.ime.original.is_none() {
            return;
        }
        let callback = d.finish_ime();
        d.ime.blur();
        callback
    };
    if let Some((callback, value)) = callback {
        callback(&value);
    }
    search::refresh_editor(data);
}

pub(super) fn binding(data: &SharedCodeEditorState) -> ImeBinding {
    let source = data.clone();
    let target = data.clone();
    ImeBinding {
        read: Arc::new(move || {
            finish_on_blur(&source);
            let mut d = source.lock().unwrap();
            if !d.focused {
                return None;
            }
            let text = d.value();
            let selection = [
                utf16_offset(
                    &text,
                    line_offset(&d.lines, d.selection_start.unwrap_or(d.cursor)),
                ),
                utf16_offset(&text, line_offset(&d.lines, d.cursor)),
            ];
            Some(ImeContent {
                edit: d.ime.snapshot(true, text, selection),
                focused: true,
                disabled: false,
                secure: false,
                multiline: true,
                autocorrect: false,
            })
        }),
        edit: Arc::new(move |edit| {
            let callback = {
                let mut d = target.lock().unwrap();
                if !d.focused || !d.ime.receive(edit) {
                    return false;
                }
                if !crate::ime::valid_edit(edit) {
                    return true;
                }
                if d.ime.original.is_none() {
                    d.ime.original = Some(d.ime_entry());
                }
                if d.value() != edit.text {
                    d.lines = edit.text.split('\n').map(str::to_owned).collect();
                    d.invalidate_all_highlights();
                }
                d.cursor = line_position(
                    &edit.text,
                    char_offset(&edit.text, edit.selection[1]).unwrap(),
                );
                let anchor = line_position(
                    &edit.text,
                    char_offset(&edit.text, edit.selection[0]).unwrap(),
                );
                d.selection_start = (anchor != d.cursor).then_some(anchor);
                d.ime.composing = edit.composing.filter(|[a, b]| a < b);
                d.ensure_cursor_visible();
                if let Ok(mut cursor) = d.cursor_state.lock() {
                    cursor.reset_blink();
                }
                if d.ime.composing.is_none() {
                    d.finish_ime()
                } else {
                    None
                }
            };
            if let Some((callback, value)) = callback {
                callback(&value);
            }
            search::refresh_editor(&target);
            crate::stateful::request_redraw();
            true
        }),
    }
}
