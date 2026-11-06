use super::*;
use crate::ime::{ImeBinding, ImeContent, char_offset, utf16_offset};
use std::sync::Arc;

fn position(doc: &RichDocument, mut offset: usize) -> DocPosition {
    for (block, b) in doc.blocks.iter().enumerate() {
        for (line, text) in b.lines.iter().enumerate() {
            let count = text.text.chars().count();
            if offset <= count {
                return DocPosition::new(block, line, offset);
            }
            offset = offset.saturating_sub(count + 1);
        }
    }
    DocPosition::end_of(doc, doc.blocks.len().saturating_sub(1))
}

fn offset(doc: &RichDocument, pos: DocPosition) -> usize {
    let mut offset = 0;
    for (block, b) in doc.blocks.iter().enumerate() {
        for (line, text) in b.lines.iter().enumerate() {
            if block == pos.block && line == pos.line {
                return offset + pos.col;
            }
            offset += text.text.chars().count() + 1;
        }
    }
    offset.saturating_sub(1)
}

impl RichTextData {
    pub(super) fn finish_ime(&mut self) {
        self.ime.composing = None;
        let Some(original) = self.ime.original.take() else {
            return;
        };
        if original.document.plain_text() != self.document.plain_text() {
            self.undo_stack.push(original);
            if self.undo_stack.len() > 200 {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
        }
    }
}

pub(super) fn binding(
    data: &RichTextState,
    version: blinc_core::reactive::Signal<u32>,
) -> ImeBinding {
    let source = data.clone();
    let target = data.clone();
    ImeBinding {
        read: Arc::new(move || {
            let mut d = source.lock().unwrap();
            if !d.focused {
                d.ime.blur();
                return None;
            }
            if let PickerState::Link { draft } = d.picker.clone() {
                d.finish_ime();
                d.ime.blur();
                let length = draft.encode_utf16().count();
                let selection = d
                    .link_ime_selection
                    .unwrap_or([length, length])
                    .map(|n| n.min(length));
                return Some(ImeContent {
                    edit: d.link_ime.snapshot(true, draft, selection),
                    focused: true,
                    disabled: false,
                    secure: false,
                    multiline: false,
                    autocorrect: false,
                });
            }
            d.link_ime.blur();
            d.link_ime.composing = None;
            d.link_ime_selection = None;
            let text = d.document.plain_text();
            let selection = [
                utf16_offset(
                    &text,
                    offset(&d.document, d.selection.map_or(d.cursor, |s| s.anchor)),
                ),
                utf16_offset(&text, offset(&d.document, d.cursor)),
            ];
            Some(ImeContent {
                edit: d.ime.snapshot(true, text, selection),
                focused: true,
                disabled: false,
                secure: false,
                multiline: true,
                autocorrect: true,
            })
        }),
        edit: Arc::new(move |edit| {
            {
                let mut d = target.lock().unwrap();
                if d.focused && matches!(d.picker, PickerState::Link { .. }) {
                    if !d.link_ime.receive(edit) {
                        return false;
                    }
                    if crate::ime::valid_edit(edit) && !edit.text.contains(['\n', '\r']) {
                        d.picker = PickerState::Link {
                            draft: edit.text.clone(),
                        };
                        d.link_ime_selection = Some(edit.selection);
                        d.link_ime.composing = edit.composing.filter(|[a, b]| a < b);
                    }
                    drop(d);
                    version.update(|n| n.wrapping_add(1));
                    crate::stateful::request_redraw();
                    return true;
                }
                if !d.focused || !d.ime.receive(edit) {
                    return false;
                }
                if !crate::ime::valid_edit(edit) {
                    return true;
                }
                if d.ime.original.is_none() {
                    d.ime.original = Some(UndoEntry {
                        document: d.document.clone(),
                        cursor: d.cursor,
                        selection: d.selection,
                    });
                }
                // Rebuild only the replacement against the original rich document.
                let original = d.ime.original.as_ref().unwrap().document.clone();
                let before: Vec<_> = original.plain_text().chars().collect();
                let after: Vec<_> = edit.text.chars().collect();
                let prefix = before
                    .iter()
                    .zip(&after)
                    .take_while(|(a, b)| a == b)
                    .count();
                let suffix = before[prefix..]
                    .iter()
                    .rev()
                    .zip(after[prefix..].iter().rev())
                    .take_while(|(a, b)| a == b)
                    .count();
                let start = position(&original, prefix);
                let end = position(&original, before.len() - suffix);
                d.document = original;
                let caret = delete_selection(
                    &mut d.document,
                    Selection {
                        anchor: start,
                        head: end,
                    },
                );
                let format = d.active_format.clone();
                insert_text(
                    &mut d.document,
                    caret,
                    &after[prefix..after.len() - suffix]
                        .iter()
                        .collect::<String>(),
                    &format,
                );
                d.cursor = position(
                    &d.document,
                    char_offset(&edit.text, edit.selection[1]).unwrap(),
                );
                let anchor = position(
                    &d.document,
                    char_offset(&edit.text, edit.selection[0]).unwrap(),
                );
                d.selection = (anchor != d.cursor).then_some(Selection {
                    anchor,
                    head: d.cursor,
                });
                d.ime.composing = edit.composing.filter(|[a, b]| a < b);
                if d.ime.composing.is_none() {
                    d.finish_ime();
                    d.active_format = ActiveFormat::from_position(&d.document, d.cursor);
                }
                d.reset_cursor_blink();
            }
            version.update(|n| n.wrapping_add(1));
            crate::stateful::request_redraw();
            true
        }),
    }
}
