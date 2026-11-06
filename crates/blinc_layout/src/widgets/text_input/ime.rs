use super::*;
use crate::ime::{ImeBinding, ImeContent, ImeEdit, char_offset, utf16_offset};

impl TextInputData {
    fn ime_entry(&self) -> UndoEntry {
        UndoEntry {
            value: self.value.clone(),
            cursor: self.cursor,
            selection_start: self.selection_start,
        }
    }

    fn accepts_ime(&self, text: &str) -> bool {
        !text.contains(['\n', '\r'])
            && self
                .constraints
                .max_length
                .is_none_or(|max| text.chars().count() <= max)
            && text.chars().all(|ch| match self.input_type {
                InputType::Number => ch.is_ascii_digit() || matches!(ch, '.' | '-'),
                InputType::Integer => ch.is_ascii_digit() || ch == '-',
                InputType::Tel => ch.is_ascii_digit() || matches!(ch, '+' | '-' | ' '),
                _ => true,
            })
    }

    fn finish_ime(&mut self, before: UndoEntry) -> Option<(OnChangeCallback, String)> {
        self.ime.composing = None;
        if self.disabled || !self.accepts_ime(&self.value) {
            self.value = before.value;
            self.cursor = before.cursor;
            self.selection_start = before.selection_start;
            self.validate();
            return None;
        }
        self.validate();
        if before.value == self.value {
            return None;
        }
        self.undo_stack.push(before);
        if self.undo_stack.len() > UNDO_HISTORY_MAX {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.on_change_callback
            .clone()
            .map(|callback| (callback, self.value.clone()))
    }
}

pub(super) fn finish_on_blur(data: &SharedTextInputData) {
    let callback = {
        let mut d = data.lock().unwrap();
        if d.visual.is_focused() && !d.disabled {
            return;
        }
        d.ime.blur();
        d.ime
            .original
            .take()
            .and_then(|before| d.finish_ime(before))
    };
    if let Some((callback, value)) = callback {
        callback(&value);
    }
}

pub(super) fn commit(data: &SharedTextInputData) {
    let callback = {
        let mut d = data.lock().unwrap();
        let Some(before) = d.ime.original.take() else {
            return;
        };
        let callback = d.finish_ime(before);
        d.ime.blur();
        callback
    };
    if let Some((callback, value)) = callback {
        callback(&value);
    }
    super::focus::notify_text_input(data);
}

pub(super) fn binding(data: &SharedTextInputData) -> ImeBinding {
    let source = data.clone();
    let target = data.clone();
    ImeBinding {
        read: Arc::new(move || {
            let mut d = source.lock().unwrap();
            if !d.visual.is_focused() {
                d.ime.snapshot(false, String::new(), [0, 0]);
                return None;
            }
            let text = d.value.clone();
            let selection = [
                utf16_offset(&text, d.selection_start.unwrap_or(d.cursor)),
                utf16_offset(&text, d.cursor),
            ];
            let focused = d.visual.is_focused();
            Some(ImeContent {
                edit: d.ime.snapshot(focused, text, selection),
                focused,
                disabled: d.disabled,
                secure: d.masked,
                multiline: false,
                autocorrect: false,
            })
        }),
        edit: Arc::new(move |edit| {
            let callback = {
                let mut d = target.lock().unwrap();
                if d.disabled || !d.visual.is_focused() || !d.ime.receive(edit) {
                    return false;
                }
                if !crate::ime::valid_edit(edit) {
                    return true;
                }
                let composing = edit.composing.filter(|[start, end]| start < end);
                let before = d.ime_entry();
                if composing.is_some() && d.ime.original.is_none() {
                    d.ime.original = Some(before.clone());
                }
                d.value = edit.text.clone();
                d.cursor = char_offset(&d.value, edit.selection[1]).unwrap();
                let anchor = char_offset(&d.value, edit.selection[0]).unwrap();
                d.selection_start = (anchor != d.cursor).then_some(anchor);
                d.reset_cursor_blink();
                d.ime.composing = composing;
                if composing.is_none() {
                    let original = d.ime.original.take().unwrap_or(before);
                    d.finish_ime(original)
                } else {
                    None
                }
            };
            if let Some((callback, value)) = callback {
                callback(&value);
            }
            super::focus::notify_text_input(&target);
            true
        }),
    }
}
