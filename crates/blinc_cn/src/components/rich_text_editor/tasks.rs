//! The boxes of a checklist's items.
//!
//! A tick is not a change to a block's content, which would rebuild the block and cut the box's
//! own transition short. Each box has a signal, and ticks made otherwise, by a shortcut, undo or
//! the application, are written to it.

use std::sync::Arc;

use blinc_core::{Signal, reactive::signal};
use blinc_layout::div::{Div, div};

use super::document::BlockKind;
use super::state::{RichTextData, RichTextState, report_change};
use crate::components::checkbox::{CheckboxSize, checkbox};

/// The box of the task at `index`, which a press ticks.
pub(super) fn task_box(
    state: &RichTextState,
    version: Signal<u32>,
    index: usize,
    checked: bool,
) -> Div {
    let ticked = signal(checked);
    if let Ok(mut data) = state.lock() {
        data.tasks.insert(index, ticked);
    }
    let (on_press, on_change) = (Arc::clone(state), Arc::clone(state));
    div()
        // The press is the box's: the editor keeps its cursor and selection.
        .on_mouse_down(move |_| {
            if let Ok(mut data) = on_press.lock() {
                data.suppress_next_outer_click = true;
            }
        })
        .child(
            checkbox(ticked)
                .size(CheckboxSize::Small)
                .on_change(move |checked| {
                    let changed = on_change
                        .lock()
                        .is_ok_and(|mut data| set_checked(&mut data, index, checked));
                    if changed {
                        version.update(|n| n.wrapping_add(1));
                        report_change(&on_change);
                    }
                })
                .build_component()
                .class("cn-rich-text-editor-task"),
        )
}

/// Tick or untick the task at `index`, as one undo step. Returns whether it changed.
fn set_checked(data: &mut RichTextData, index: usize, to: bool) -> bool {
    data.change(|doc| match doc.blocks.get_mut(index).map(|b| &mut b.kind) {
        Some(BlockKind::TaskItem { checked }) if *checked != to => {
            *checked = to;
            true
        }
        _ => false,
    })
}

/// Tick the task under the cursor, or untick it. Returns whether there is one.
pub(super) fn toggle_at_cursor(data: &mut RichTextData) -> bool {
    let index = data.cursor.block;
    match data.document.blocks.get(index).map(|b| &b.kind) {
        Some(BlockKind::TaskItem { checked }) => {
            let to = !checked;
            set_checked(data, index, to)
        }
        _ => false,
    }
}

/// Write the ticks of the document to the boxes that show others. Called with the state unlocked,
/// outside any reactive computation.
pub(super) fn sync(state: &RichTextState) {
    let Ok(mut data) = state.lock() else {
        return;
    };
    let data = &mut *data;
    let mut stale = Vec::new();
    let blocks = &data.document.blocks;
    // A box whose signal is gone was rebuilt or removed.
    data.tasks.retain(|index, ticked| {
        let shown = ticked.try_get();
        if let (Some(shown), Some(BlockKind::TaskItem { checked })) =
            (shown, blocks.get(*index).map(|b| &b.kind))
            && shown != *checked
        {
            stale.push((*ticked, *checked));
        }
        shown.is_some()
    });
    for (ticked, checked) in stale {
        ticked.set(checked);
    }
}
