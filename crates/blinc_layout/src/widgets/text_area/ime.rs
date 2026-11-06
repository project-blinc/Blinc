use super::*;
use crate::ime::{ImeBinding, ImeContent, char_offset, line_offset, line_position, utf16_offset};

#[derive(Clone)]
pub(crate) struct ImeEntry {
    value: String,
    cursor: TextPosition,
    selection: Option<TextPosition>,
}

pub(super) fn commit(state: &SharedTextAreaState) {
    let (callback, signal) = {
        let mut d = state.lock().unwrap();
        if d.ime.original.is_none() {
            return;
        }
        d.ime.blur();
        d.ime.composing = None;
        if d.ime
            .original
            .take()
            .is_some_and(|before| before.value != d.value())
        {
            d.change_version.fetch_add(1, Ordering::SeqCst);
            (
                d.on_change.clone().map(|callback| (callback, d.value())),
                d.change_signal_id,
            )
        } else {
            (None, None)
        }
    };
    if let Some((callback, value)) = callback {
        callback(&value);
    }
    if let Some(signal) = signal {
        crate::stateful::check_stateful_deps(&[signal]);
    }
    super::focus::notify_text_area(state);
}

pub(super) fn finish_on_blur(state: &SharedTextAreaState) {
    let (callback, signal) = {
        let mut d = state.lock().unwrap();
        if d.visual.is_focused() && !d.disabled {
            return;
        }
        d.ime.blur();
        d.ime.composing = None;
        let changed = d.ime.original.take().is_some_and(|before| {
            if d.disabled {
                d.lines = before.value.split('\n').map(str::to_owned).collect();
                d.cursor = before.cursor;
                d.selection_start = before.selection;
                false
            } else {
                before.value != d.value()
            }
        });
        if changed {
            d.change_version.fetch_add(1, Ordering::SeqCst);
            (
                d.on_change.clone().map(|callback| (callback, d.value())),
                d.change_signal_id,
            )
        } else {
            (None, None)
        }
    };
    if let Some((callback, value)) = callback {
        callback(&value);
    }
    if let Some(signal) = signal {
        crate::stateful::check_stateful_deps(&[signal]);
    }
}

pub(super) fn row_run(d: &TextAreaState, base: &TextAreaConfig, row: usize, part: usize) -> String {
    let shown = super::content::row_text(d, base, row);
    let Some([start, end]) = d.ime.composing else {
        return if part == 0 { shown } else { String::new() };
    };
    let Some(line) = d.visual_lines.get(row) else {
        return String::new();
    };
    let value = d.value();
    let start = char_offset(&value, start).unwrap_or(0);
    let end = char_offset(&value, end).unwrap_or(start);
    let row_start = d
        .lines
        .iter()
        .take(line.logical_line)
        .map(|s| s.chars().count() + 1)
        .sum::<usize>()
        + line.start_char;
    let count = shown.chars().count();
    let from = start.saturating_sub(row_start).min(count);
    let to = end.saturating_sub(row_start).min(count);
    match part {
        0 => shown.chars().take(from).collect(),
        1 => shown.chars().skip(from).take(to - from).collect(),
        _ => shown.chars().skip(to).collect(),
    }
}

pub(super) fn binding(state: &SharedTextAreaState) -> ImeBinding {
    let source = state.clone();
    let target = state.clone();
    ImeBinding {
        read: Arc::new(move || {
            let mut d = source.lock().unwrap();
            if !d.visual.is_focused() {
                d.ime.snapshot(false, String::new(), [0, 0]);
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
                disabled: d.disabled,
                secure: false,
                multiline: true,
                autocorrect: true,
            })
        }),
        edit: Arc::new(move |edit| {
            let (callback, change_signal) = {
                let mut d = target.lock().unwrap();
                if d.disabled || !d.visual.is_focused() || !d.ime.receive(edit) {
                    return false;
                }
                if !crate::ime::valid_edit(edit) {
                    return true;
                }
                let composing = edit.composing.filter(|[start, end]| start < end);
                let before = ImeEntry {
                    value: d.value(),
                    cursor: d.cursor,
                    selection: d.selection_start,
                };
                if composing.is_some() && d.ime.original.is_none() {
                    d.ime.original = Some(before.clone());
                }
                d.lines = edit.text.split('\n').map(str::to_owned).collect();
                d.cursor = line_position(
                    &edit.text,
                    char_offset(&edit.text, edit.selection[1]).unwrap(),
                );
                let anchor = line_position(
                    &edit.text,
                    char_offset(&edit.text, edit.selection[0]).unwrap(),
                );
                d.selection_start = (anchor != d.cursor).then_some(anchor);
                d.ime.composing = composing;
                d.reset_cursor_blink();
                d.compute_visual_lines();
                let (line_height, viewport_height) = (d.line_height, d.viewport_height);
                d.ensure_cursor_visible(line_height, viewport_height);
                if composing.is_none() && d.ime.original.take().unwrap_or(before).value != edit.text
                {
                    d.change_version.fetch_add(1, Ordering::SeqCst);
                    (
                        d.on_change.clone().map(|callback| (callback, d.value())),
                        d.change_signal_id,
                    )
                } else {
                    (None, None)
                }
            };
            if let Some((callback, value)) = callback {
                callback(&value);
            }
            if let Some(signal) = change_signal {
                crate::stateful::check_stateful_deps(&[signal]);
            }
            super::focus::notify_text_area(&target);
            true
        }),
    }
}
