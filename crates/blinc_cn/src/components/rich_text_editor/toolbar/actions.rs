//! What the toolbar's controls do to the editor's state.

use blinc_core::Color;

use crate::components::rich_text_editor::cursor::ActiveFormat;
use crate::components::rich_text_editor::format::{Mark, apply_mark_to_selection};
use crate::components::rich_text_editor::state::RichTextData;

/// Toggle a boolean mark: over the selection, or, with a collapsed cursor, for what is typed next.
pub(super) fn toggle_mark(data: &mut RichTextData, mark: Mark) {
    match data.selection.filter(|s| !s.is_empty()) {
        Some(sel) => {
            data.push_undo();
            apply_mark_to_selection(&mut data.document, sel, mark);
            data.active_format = ActiveFormat::from_position(&data.document, data.cursor);
        }
        None => {
            let f = &mut data.active_format;
            match mark {
                Mark::Bold => f.bold = !f.bold,
                Mark::Italic => f.italic = !f.italic,
                Mark::Underline => f.underline = !f.underline,
                Mark::Strikethrough => f.strikethrough = !f.strikethrough,
                Mark::Code => f.code = !f.code,
                Mark::Color(_) | Mark::Link(_) => {}
            }
        }
    }
}

/// Whether the mark is on under the cursor, or for what is typed next.
pub(super) fn mark_on(format: &ActiveFormat, mark: &Mark) -> bool {
    match mark {
        Mark::Bold => format.bold,
        Mark::Italic => format.italic,
        Mark::Underline => format.underline,
        Mark::Strikethrough => format.strikethrough,
        Mark::Code => format.code,
        Mark::Color(_) | Mark::Link(_) => false,
    }
}

/// Colour the selection, and what is typed next. Transparent takes the editor's text colour.
pub(super) fn set_color(data: &mut RichTextData, color: Color) {
    if let Some(sel) = data.selection.filter(|s| !s.is_empty()) {
        data.push_undo();
        apply_mark_to_selection(&mut data.document, sel, Mark::Color(color));
    }
    data.active_format.color = Some(color);
}

/// Link the selection to `url`, or remove its link with `None`.
pub(super) fn set_link(data: &mut RichTextData, url: Option<String>) {
    if let Some(sel) = data.selection.filter(|s| !s.is_empty()) {
        data.push_undo();
        apply_mark_to_selection(&mut data.document, sel, Mark::Link(url));
        data.active_format = ActiveFormat::from_position(&data.document, data.cursor);
    }
}

/// The link under the start of the selection, to edit it without typing it again.
pub(super) fn existing_link(data: &RichTextData) -> Option<String> {
    let sel = data.selection?;
    let (start, _end) = sel.ordered();
    let block = data.document.blocks.get(start.block)?;
    let line = block.lines.get(start.line)?;
    let byte = crate::components::rich_text_editor::document::char_to_byte(&line.text, start.col);
    line.spans
        .iter()
        .find(|s| s.start <= byte && byte < s.end)
        .and_then(|s| s.link_url.clone())
}

/// Collapse the selection to its end, which closes the floating toolbar.
pub(super) fn close(data: &mut RichTextData) {
    if let Some(sel) = data.selection.take() {
        data.set_cursor(sel.head);
    }
    data.picker = Default::default();
}

/// The colour picker's swatches: the text colour (transparent), then a few highlight colours.
pub(super) fn palette() -> Vec<Color> {
    vec![
        Color::TRANSPARENT,
        Color::rgba(0.94, 0.39, 0.39, 1.0),
        Color::rgba(0.97, 0.59, 0.20, 1.0),
        Color::rgba(0.96, 0.85, 0.40, 1.0),
        Color::rgba(0.50, 0.85, 0.50, 1.0),
        Color::rgba(0.40, 0.78, 1.00, 1.0),
        Color::rgba(0.66, 0.55, 1.00, 1.0),
        Color::rgba(0.55, 0.55, 0.65, 1.0),
    ]
}

/// Apply a heading level to the current selection. If the selection
/// is partial-within-a-line, the containing block is split into up to
/// three blocks (prefix paragraph, middle heading, suffix paragraph)
/// so that only the *selected* run becomes the heading. With a
/// collapsed cursor, a full-line selection, or a multi-block
/// selection, every block in the range is converted in place.
///
/// `None` reverts to plain paragraph.
pub(super) fn apply_heading_level(data: &mut RichTextData, level: Option<u8>) {
    use crate::components::rich_text_editor::block_ops::convert_selection_to_block;
    use crate::components::rich_text_editor::document::BlockKind;

    let range = data
        .selection
        .unwrap_or(crate::components::rich_text_editor::cursor::Selection {
            anchor: data.cursor,
            head: data.cursor,
        });
    let kind = match level {
        Some(n) => BlockKind::Heading(n),
        None => BlockKind::Paragraph,
    };

    let before = data.document.clone();
    data.push_undo();
    let middle_block_idx = convert_selection_to_block(&mut data.document, range, kind);
    if data.document == before {
        // Nothing actually changed — drop the undo entry we just pushed.
        data.undo_stack.pop();
        return;
    }

    // Re-anchor the cursor and selection inside the converted block.
    // When the block was split, `middle_block_idx` is the index of the
    // new heading block; otherwise the cursor's existing block is fine
    // (its index is unchanged for in-place conversions).
    if let Some(idx) = middle_block_idx {
        let middle_chars = data
            .document
            .blocks
            .get(idx)
            .map(|b| b.char_len())
            .unwrap_or(0);
        let new_start = crate::components::rich_text_editor::cursor::DocPosition::new(idx, 0, 0);
        let new_end =
            crate::components::rich_text_editor::cursor::DocPosition::new(idx, 0, middle_chars);
        data.selection = Some(crate::components::rich_text_editor::cursor::Selection {
            anchor: new_start,
            head: new_end,
        });
        data.set_cursor(new_end);
    } else {
        let cursor = data.cursor;
        data.set_cursor(cursor);
    }
    data.reset_cursor_blink();
}
