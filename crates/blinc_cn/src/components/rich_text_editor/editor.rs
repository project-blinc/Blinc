//! `rich_text_editor(state, theme, content_width)` — interactive editor.
//!
//! Phases 3 + 3.5 + 4 in this file:
//!
//! - Click to place cursor, drag to select, arrow / Home / End nav.
//! - Canvas-based blinking visual cursor (no rebuilds for the blink).
//! - Typing inserts characters at the cursor with the current
//!   `ActiveFormat`; Backspace / Delete / Enter / Shift+Enter all work
//!   as expected. Undo / redo are wired to Cmd-Z / Cmd-Shift-Z and use
//!   a snapshot stack capped at 200 entries.
//!
//! The factory wraps the renderer in a `Stateful` whose rebuild is
//! triggered by a private `version: Signal<u32>` that the click
//! and key handlers bump after every mutation. The Stateful's `on_state`
//! callback re-walks the line geometry index against the current
//! document and emits the cursor / selection overlay.

use std::sync::Arc;

use blinc_core::Signal;
use blinc_core::context_state::BlincContextState;

use blinc_layout::div::{Div, div};

use super::block_ops::{indent_blocks, outdent_blocks, toggle_block_kind};
use super::content::{content, ensure_geometry};
use super::cursor::{DocPosition, step_backward, step_forward};
use super::document::{BlockAlign, BlockKind};
use super::edit::{
    delete_backward, delete_forward, delete_selection, insert_char, insert_text, soft_break,
    split_block,
};
use super::format::{Mark, apply_mark_to_selection};
use super::render::RichTextTheme;
use super::state::{RichTextState, report_change};

/// Build an interactive rich text editor element.
///
/// `state` is an externally-owned `RichTextState` that survives across
/// rebuilds. `theme` controls visual style. `content_width` is the
/// pixel width of the column the document will be rendered into — same
/// caveat as [`super::render::render_document`].
///
/// The element is built once. Every edit bumps a per-editor version
/// signal, and what the editor shows is bound to it: the blocks are a
/// keyed list, so an edit rebuilds only the block it changed, and the
/// selection, cursor and toolbar follow in place. Use a parent with
/// explicit width (e.g. inside a centered column) to control the
/// editor's footprint.
pub fn rich_text_editor(state: &RichTextState, theme: RichTextTheme, content_width: f32) -> Div {
    // Per-editor signal that bumps on every state mutation.
    let blinc = BlincContextState::get();
    let version_key = format!("rich_text_editor:{:p}", Arc::as_ptr(state));
    let version: Signal<u32> = blinc.use_signal_keyed(&version_key, || 0);

    // Pre-populate the line index so the very first frame can already
    // hit-test clicks before any state mutation has happened.
    // The given width until the editor has been laid out; its own width after.
    if let Ok(mut data) = state.lock()
        && data.wrap_width <= 0.0
    {
        data.wrap_width = content_width;
    }
    ensure_geometry(state, &theme, version.try_get().unwrap_or(0));

    // Clones for the various closures
    let state_for_click = Arc::clone(state);
    let state_for_drag = Arc::clone(state);
    let state_for_key = Arc::clone(state);
    let state_for_text = Arc::clone(state);
    let state_for_release = Arc::clone(state);
    let version_for_release = version;
    let version_for_click = version;
    let version_for_drag = version;
    let version_for_key = version;
    let version_for_text = version;

    let content = content(state, &version, &theme);

    div()
        .w_full()
        .ime(super::ime::binding(state, version))
        .on_mouse_down(move |ctx| {
            let mut data = state_for_click.lock().unwrap();
            if data.ime.original.is_some() {
                data.finish_ime();
                data.ime.blur();
            }
            data.set_focus(true);
            data.editor_bounds = (
                ctx.bounds_x,
                ctx.bounds_y,
                ctx.bounds_width,
                ctx.bounds_height,
            );

            // Tell the mobile platform runner that the user just tapped
            // the rich-text editor. The tap-generation counter
            // (`focus_tap_generation`) drives scroll-into-view on
            // re-taps and cross-input focus swaps; the
            // generic-editable-node atomic
            // (`set_focused_editable_node`) is what the
            // `RenderTree::scroll_focused_text_input_above_keyboard`
            // helper looks up to find the editor's bounds. Both are
            // populated unconditionally — even when the click ends up
            // being suppressed below for a toolbar button — so the
            // user always sees the keyboard re-engage with the editor.
            //
            // The blur callback captures the editor state so when the
            // user taps outside, `blur_all_text_inputs` can call
            // `set_focus(false)`, which both clears the local
            // `focused` flag AND decrements the global focus count
            // (the global hook lives inside `RichTextEditorState::set_focus`).
            blinc_layout::widgets::text_input::bump_focus_tap_generation();
            let blur_state = Arc::clone(&state_for_click);
            let blur_version = version_for_click;
            blinc_layout::widgets::text_input::set_focused_editable_node(
                ctx.node_id,
                Some(Box::new(move || {
                    if let Ok(mut data) = blur_state.lock() {
                        data.set_focus(false);
                    }
                    // The selection, cursor and toolbar follow focus.
                    blur_version.set(blur_version.get().wrapping_add(1));
                })),
            );

            // A toolbar button just consumed the click — skip cursor
            // placement so we don't collapse the selection underneath.
            // Events dispatch deepest-first, so the button's handler
            // already ran and set this flag before we got here.
            if data.suppress_next_outer_click {
                data.suppress_next_outer_click = false;
                return;
            }
            data.pressing = true;

            // Detect double-click within the standard 400ms window.
            let now = web_time::Instant::now();
            let is_double = data
                .last_click_time
                .map(|t| now.duration_since(t).as_millis() < 400)
                .unwrap_or(false);
            data.last_click_time = Some(now);

            let touch = blinc_layout::widgets::text_input::is_touch_input();

            if let Some(pos) = data.position_from_click(
                ctx.local_x - data.content_left,
                ctx.local_y - data.content_top,
            ) {
                if is_double {
                    // Double-click — select the word under the cursor.
                    if let Some(line) = data
                        .document
                        .blocks
                        .get(pos.block)
                        .and_then(|b| b.lines.get(pos.line))
                    {
                        let (start_col, end_col) =
                            blinc_layout::widgets::text_edit::word_at_position(&line.text, pos.col);
                        data.cursor = super::cursor::DocPosition::new(pos.block, pos.line, end_col);
                        data.selection = Some(super::cursor::Selection {
                            anchor: super::cursor::DocPosition::new(pos.block, pos.line, start_col),
                            head: super::cursor::DocPosition::new(pos.block, pos.line, end_col),
                        });
                        data.reset_cursor_blink();
                        if touch {
                            blinc_layout::widgets::text_edit::haptic_impact_light();
                            use blinc_layout::widgets::text_edit::edit_menu_actions;
                            blinc_layout::widgets::text_edit::show_edit_menu(
                                ctx.bounds_x + ctx.local_x,
                                ctx.bounds_y + ctx.local_y,
                                ctx.bounds_x + ctx.local_x,
                                ctx.bounds_y + ctx.local_y,
                                0.0,
                                24.0,
                                edit_menu_actions::CUT
                                    | edit_menu_actions::COPY
                                    | edit_menu_actions::PASTE
                                    | edit_menu_actions::SELECT_ALL,
                            );
                        }
                    }
                } else if touch {
                    // Touch single-tap: position the caret without
                    // starting a selection (touch drag will move it).
                    // Light haptic to mirror UITextField focus feedback.
                    // Also arm the long-press timer for the
                    // press-and-hold edit menu, with a callback that
                    // selects the word under the press position so
                    // long-press matches the double-tap UX above.
                    data.move_cursor(pos, false);
                    blinc_layout::widgets::text_edit::haptic_selection();
                    blinc_layout::widgets::text_edit::hide_edit_menu();
                    let captured_pos = pos;
                    let state_for_long_press = Arc::clone(&state_for_click);
                    let version_for_long_press = version_for_click;
                    blinc_layout::widgets::text_input::arm_long_press_timer(
                        ctx.bounds_x + ctx.local_x,
                        ctx.bounds_y + ctx.local_y,
                        24.0,
                        Some(Box::new(move || {
                            let did_update = {
                                let mut data = match state_for_long_press.lock() {
                                    Ok(d) => d,
                                    Err(_) => return,
                                };
                                if !data.focused {
                                    return;
                                }
                                let line_text = data
                                    .document
                                    .blocks
                                    .get(captured_pos.block)
                                    .and_then(|b| b.lines.get(captured_pos.line))
                                    .map(|l| l.text.clone());
                                let Some(line_text) = line_text else {
                                    return;
                                };
                                let (start_col, end_col) =
                                    blinc_layout::widgets::text_edit::word_at_position(
                                        &line_text,
                                        captured_pos.col,
                                    );
                                if start_col == end_col {
                                    return;
                                }
                                data.cursor = super::cursor::DocPosition::new(
                                    captured_pos.block,
                                    captured_pos.line,
                                    end_col,
                                );
                                data.selection = Some(super::cursor::Selection {
                                    anchor: super::cursor::DocPosition::new(
                                        captured_pos.block,
                                        captured_pos.line,
                                        start_col,
                                    ),
                                    head: super::cursor::DocPosition::new(
                                        captured_pos.block,
                                        captured_pos.line,
                                        end_col,
                                    ),
                                });
                                data.reset_cursor_blink();
                                true
                            };
                            if did_update {
                                version_for_long_press
                                    .set(version_for_long_press.get().wrapping_add(1));
                            }
                        })),
                    );
                } else {
                    let extend = ctx.shift;
                    data.move_cursor(pos, extend);
                }
            }
            drop(data);
            version_for_click.set(version_for_click.get().wrapping_add(1));
        })
        .on_mouse_up(move |_| {
            // The release reveals the floating toolbar over what the press selected.
            let released = state_for_release
                .lock()
                .map(|mut d| std::mem::take(&mut d.pressing))
                .unwrap_or(false);
            if released {
                version_for_release.update(|n| n.wrapping_add(1));
            }
        })
        .on_drag(move |ctx| {
            let mut data = state_for_drag.lock().unwrap();
            if !data.focused {
                return;
            }
            // Touch drag = move cursor (no selection extension), with
            // a subtle haptic per character boundary.
            // Mouse drag = extend selection from anchor to pointer.
            let touch = blinc_layout::widgets::text_input::is_touch_input();
            if let Some(pos) = data.position_from_click(
                ctx.local_x - data.content_left,
                ctx.local_y - data.content_top,
            ) {
                if touch {
                    // Drift-cancel any armed long-press so a real
                    // cursor drag doesn't also fire the paste menu.
                    blinc_layout::widgets::text_input::check_long_press_drift(
                        ctx.mouse_x,
                        ctx.mouse_y,
                    );
                    if data.cursor != pos {
                        data.move_cursor(pos, false);
                        blinc_layout::widgets::text_edit::haptic_selection();
                    }
                } else {
                    data.move_cursor(pos, true);
                }
            }
            drop(data);
            version_for_drag.set(version_for_drag.get().wrapping_add(1));
        })
        .on_text_input(move |ctx| {
            // Printable characters arrive here (post-IME). The
            // on_key_down handler intentionally doesn't insert anything
            // for typed characters; it only handles editing keys and
            // Cmd-shortcuts.
            let ch: char = match ctx.key_char {
                Some(c) => c,
                None => return,
            };
            // Skip control chars — those come via on_key_down.
            if ch.is_control() {
                return;
            }
            let mut data = state_for_text.lock().unwrap();
            if data.ime.original.is_some()
                && (matches!(ctx.key_code, 8 | 13 | 27 | 35..=40 | 46)
                    || (ctx.command_modifier()
                        && matches!(ctx.key_code, 65 | 67 | 86 | 88 | 89 | 90)))
            {
                data.finish_ime();
                data.ime.blur();
            }
            if !data.focused {
                return;
            }
            // If there's a selection, replace it first.
            let mut pos = data.cursor;
            if let Some(sel) = data.selection.take() {
                pos = delete_selection(&mut data.document, sel);
            }
            data.push_undo();
            let fmt = data.active_format.clone();
            let new_pos = insert_char(&mut data.document, pos, ch, &fmt);
            data.set_cursor(new_pos);
            drop(data);
            version_for_text.set(version_for_text.get().wrapping_add(1));
            report_change(&state_for_text);
        })
        .on_key_down(move |ctx| {
            let mut data = state_for_key.lock().unwrap();
            if data.ime.original.is_some() {
                data.finish_ime();
                data.ime.blur();
            }
            if !data.focused {
                return;
            }

            let mod_key = ctx.command_modifier();
            let extend = ctx.shift;
            let mut changed = false;

            // Modifier shortcuts first
            if mod_key {
                match ctx.key_code {
                    // Cmd+Z — undo
                    90 if !ctx.shift => {
                        changed |= data.undo();
                    }
                    // Cmd+Shift+Z — redo
                    90 if ctx.shift => {
                        changed |= data.redo();
                    }
                    // Cmd+A — select all
                    65 => {
                        let last_block = data.document.blocks.len().saturating_sub(1);
                        let last_line = data.document.blocks[last_block]
                            .lines
                            .len()
                            .saturating_sub(1);
                        let last_col = data.document.blocks[last_block].lines[last_line]
                            .text
                            .chars()
                            .count();
                        data.selection = Some(super::cursor::Selection {
                            anchor: DocPosition::ZERO,
                            head: DocPosition::new(last_block, last_line, last_col),
                        });
                        data.cursor = DocPosition::new(last_block, last_line, last_col);
                        data.reset_cursor_blink();
                        changed = true;
                    }
                    // Cmd+B — bold
                    66 => {
                        changed |= toggle_mark(&mut data, Mark::Bold);
                    }
                    // Cmd+I — italic
                    73 => {
                        changed |= toggle_mark(&mut data, Mark::Italic);
                    }
                    // Cmd+U — underline
                    85 => {
                        changed |= toggle_mark(&mut data, Mark::Underline);
                    }
                    // Cmd+Shift+X — strikethrough
                    88 if ctx.shift => {
                        changed |= toggle_mark(&mut data, Mark::Strikethrough);
                    }
                    // Cmd+Shift+L, E, R, J — align to the start, the centre, the end, both
                    76 if ctx.shift => changed |= data.set_align(BlockAlign::Start),
                    69 if ctx.shift => changed |= data.set_align(BlockAlign::Center),
                    82 if ctx.shift => changed |= data.set_align(BlockAlign::End),
                    74 if ctx.shift => changed |= data.set_align(BlockAlign::Justify),
                    // Cmd+E — inline code
                    69 => {
                        changed |= toggle_mark(&mut data, Mark::Code);
                    }
                    // Cmd+C — copy selected text to system clipboard
                    67 => {
                        if let Some(sel) = data.selection
                            && !sel.is_empty()
                        {
                            let (s, e) = sel.ordered();
                            let plain = data.document.plain_text_range(s, e);
                            if !plain.is_empty() {
                                blinc_layout::widgets::text_edit::clipboard_write(&plain);
                            }
                        }
                        // Copy never mutates the document, so we don't
                        // bump the rebuild signal.
                    }
                    // Cmd+X — cut: copy then delete
                    88 if !ctx.shift => {
                        if let Some(sel) = data.selection
                            && !sel.is_empty()
                        {
                            let (s, e) = sel.ordered();
                            let plain = data.document.plain_text_range(s, e);
                            if !plain.is_empty() {
                                blinc_layout::widgets::text_edit::clipboard_write(&plain);
                            }
                            data.push_undo();
                            data.selection = None;
                            let new_pos = delete_selection(&mut data.document, sel);
                            data.set_cursor(new_pos);
                            changed = true;
                        }
                    }
                    // Cmd+V — paste: replace selection (if any) with
                    // clipboard text. Newlines in the clipboard become
                    // soft breaks via `insert_text`.
                    86 => {
                        if let Some(text) = blinc_layout::widgets::text_edit::clipboard_read()
                            && !text.is_empty()
                        {
                            data.push_undo();
                            let mut pos = data.cursor;
                            if let Some(sel) = data.selection.take() {
                                pos = delete_selection(&mut data.document, sel);
                            }
                            let fmt = data.active_format.clone();
                            let new_pos = insert_text(&mut data.document, pos, &text, &fmt);
                            data.set_cursor(new_pos);
                            changed = true;
                        }
                    }
                    // Cmd+Alt+0 — paragraph
                    48 if ctx.alt => {
                        changed |= data.toggle_kind(BlockKind::Paragraph);
                    }
                    // Cmd+Alt+1..6 — headings
                    49 if ctx.alt => {
                        changed |= data.toggle_kind(BlockKind::Heading(1));
                    }
                    50 if ctx.alt => {
                        changed |= data.toggle_kind(BlockKind::Heading(2));
                    }
                    51 if ctx.alt => {
                        changed |= data.toggle_kind(BlockKind::Heading(3));
                    }
                    52 if ctx.alt => {
                        changed |= data.toggle_kind(BlockKind::Heading(4));
                    }
                    53 if ctx.alt => {
                        changed |= data.toggle_kind(BlockKind::Heading(5));
                    }
                    54 if ctx.alt => {
                        changed |= data.toggle_kind(BlockKind::Heading(6));
                    }
                    // Cmd+Shift+7 — toggle numbered list
                    55 if ctx.shift => {
                        changed |= data.toggle_kind(BlockKind::NumberedItem);
                    }
                    // Cmd+Shift+8 — toggle bullet list
                    56 if ctx.shift => {
                        changed |= data.toggle_kind(BlockKind::BulletItem);
                    }
                    // Cmd+Shift+6 — toggle checklist
                    54 if ctx.shift => {
                        changed |= data.toggle_kind(BlockKind::TaskItem { checked: false });
                    }
                    // Cmd+Enter — tick the task under the cursor
                    13 => changed |= super::tasks::toggle_at_cursor(&mut data),
                    // Cmd+Shift+9 — toggle quote
                    57 if ctx.shift => {
                        changed |= data.toggle_quote();
                    }
                    _ => {}
                }
                if changed {
                    drop(data);
                    version_for_key.set(version_for_key.get().wrapping_add(1));
                    report_change(&state_for_key);
                    return;
                }
            }

            match ctx.key_code {
                // Left
                37 => {
                    if mod_key {
                        // Cmd+Left jumps to start of line
                        let pos = home_of(&data);
                        data.move_cursor(pos, extend);
                        changed = true;
                    } else if let Some(pos) = step_backward(&data.document, data.cursor) {
                        data.move_cursor(pos, extend);
                        changed = true;
                    }
                }
                // Right
                39 => {
                    if mod_key {
                        let pos = end_of(&data);
                        data.move_cursor(pos, extend);
                        changed = true;
                    } else if let Some(pos) = step_forward(&data.document, data.cursor) {
                        data.move_cursor(pos, extend);
                        changed = true;
                    }
                }
                // Up
                38 => {
                    if let Some(pos) = move_vertical(&data, -1) {
                        data.move_cursor(pos, extend);
                        changed = true;
                    }
                }
                // Down
                40 => {
                    if let Some(pos) = move_vertical(&data, 1) {
                        data.move_cursor(pos, extend);
                        changed = true;
                    }
                }
                // Home
                36 => {
                    let pos = home_of(&data);
                    data.move_cursor(pos, extend);
                    changed = true;
                }
                // End
                35 => {
                    let pos = end_of(&data);
                    data.move_cursor(pos, extend);
                    changed = true;
                }
                // Backspace
                8 => {
                    let pos = data.cursor;
                    let sel = data.selection.take();
                    data.push_undo();
                    let new_pos = if let Some(s) = sel {
                        delete_selection(&mut data.document, s)
                    } else {
                        delete_backward(&mut data.document, pos)
                    };
                    data.set_cursor(new_pos);
                    changed = true;
                }
                // Delete (forward)
                127 => {
                    let pos = data.cursor;
                    let sel = data.selection.take();
                    data.push_undo();
                    let new_pos = if let Some(s) = sel {
                        delete_selection(&mut data.document, s)
                    } else {
                        delete_forward(&mut data.document, pos)
                    };
                    data.set_cursor(new_pos);
                    changed = true;
                }
                // Enter — Shift+Enter is a soft break, plain Enter splits the block
                13 => {
                    // Replace selection first
                    let mut pos = data.cursor;
                    if let Some(s) = data.selection.take() {
                        pos = delete_selection(&mut data.document, s);
                    }
                    data.push_undo();
                    let new_pos = if ctx.shift {
                        soft_break(&mut data.document, pos)
                    } else {
                        split_block(&mut data.document, pos)
                    };
                    data.set_cursor(new_pos);
                    changed = true;
                }
                // Escape — blur
                27 => {
                    data.set_focus(false);
                    changed = true;
                }
                // Tab — the next cell in a table, else indent the current block (or selected blocks).
                // Shift+Tab outdents. Only paragraphs and list items
                // are affected; headings, dividers, and quotes are
                // skipped by `indent_blocks` / `outdent_blocks`.
                9 => {
                    if super::table::Cell::at(&data.document, data.cursor).is_some() {
                        data.tab_in_table(!ctx.shift);
                        drop(data);
                        version_for_key.update(|n| n.wrapping_add(1));
                        report_change(&state_for_key);
                        return;
                    }
                    let range = data.block_range();
                    changed |= data.change(|doc| {
                        if ctx.shift {
                            outdent_blocks(doc, range)
                        } else {
                            indent_blocks(doc, range)
                        }
                    });
                }
                _ => {}
            }

            drop(data);
            if changed {
                version_for_key.set(version_for_key.get().wrapping_add(1));
                report_change(&state_for_key);
            }
        })
        .child(content)
}

/// Toggle an inline `mark`.
///
/// - With a non-empty selection, applies the toggle to the selected
///   range via [`apply_mark_to_selection`] and pushes an undo entry.
/// - With no selection, flips the corresponding flag on the cursor's
///   `ActiveFormat` so the next typed character carries the mark.
///   This matches every other rich editor's "click bold then start
///   typing" behaviour.
///
/// Returns `true` if anything changed (selection mark applied OR
/// active-format flag flipped), so the caller knows whether to bump
/// the rebuild signal.
fn toggle_mark(data: &mut super::state::RichTextData, mark: Mark) -> bool {
    if let Some(sel) = data.selection
        && !sel.is_empty()
    {
        data.push_undo();
        let changed = apply_mark_to_selection(&mut data.document, sel, mark.clone());
        // Refresh the active format from the cursor location so the
        // toolbar (Phase 7) sees the new state.
        data.active_format =
            super::cursor::ActiveFormat::from_position(&data.document, data.cursor);
        data.reset_cursor_blink();
        return changed;
    }

    // No selection — flip the active format flag in place.
    let fmt = &mut data.active_format;
    match mark {
        Mark::Bold => fmt.bold = !fmt.bold,
        Mark::Italic => fmt.italic = !fmt.italic,
        Mark::Underline => fmt.underline = !fmt.underline,
        Mark::Strikethrough => fmt.strikethrough = !fmt.strikethrough,
        Mark::Code => fmt.code = !fmt.code,
        Mark::Color(c) => fmt.color = Some(c),
        Mark::Link(url) => fmt.link = url,
    }
    data.reset_cursor_blink();
    true
}

/// Move the cursor up (-1) or down (+1) one visual line by stepping in
/// the line-geometry index. Preserves the cursor's column where possible.
fn move_vertical(data: &super::state::RichTextData, dir: i32) -> Option<DocPosition> {
    let cursor = data.cursor;
    let current_idx = data.cursor_line()?;
    let g = line_beyond(&data.line_index, current_idx, dir)?;
    let local_col = cursor
        .col
        .saturating_sub(data.line_index[current_idx].start.col);
    let new_col = (g.start.col + local_col).min(data.last_col(g));
    Some(DocPosition::new(g.start.block, g.start.line, new_col))
}

/// The visual line above or below the one at `index`. In a table that is in the same cell, then in
/// the cell of the same column, and from the first or last row, outside it.
fn line_beyond(
    lines: &[super::state::LineGeometry],
    index: usize,
    dir: i32,
) -> Option<&super::state::LineGeometry> {
    let here = &lines[index];
    let next = if dir < 0 {
        index.checked_sub(1)
    } else {
        Some(index + 1)
    };
    let next = lines.get(next?);
    let same_cell =
        next.is_some_and(|g| (g.start.block, g.start.line) == (here.start.block, here.start.line));
    if same_cell || (here.cell.is_none() && next.is_none_or(|g| g.cell.is_none())) {
        return next;
    }
    // Into, across or out of a table: the nearest row of it, or line outside it, that way.
    let edge = |g: &super::state::LineGeometry| match g.cell {
        Some([_, y, _, h]) => (y, y + h),
        None => (g.y, g.y + g.height),
    };
    let (top, bottom) = edge(here);
    let beyond = |g: &&super::state::LineGeometry| {
        let (t, b) = edge(g);
        if dir < 0 {
            b <= top + 0.5
        } else {
            t >= bottom - 0.5
        }
    };
    let distance = |g: &super::state::LineGeometry| {
        let (t, b) = edge(g);
        if dir < 0 { top - b } else { t - bottom }
    };
    let nearest = lines
        .iter()
        .filter(beyond)
        .map(distance)
        .min_by(f32::total_cmp)?;
    let middle = here.cell.map_or(here.x, |[x, _, w, _]| x + w * 0.5);
    let aside = |g: &super::state::LineGeometry| match g.cell {
        Some([x, _, w, _]) => (x - middle).max(middle - (x + w)).max(0.0),
        None => 0.0,
    };
    let row = || {
        lines
            .iter()
            .filter(beyond)
            .filter(|g| distance(g) - nearest < 0.5)
    };
    let closest = row().map(aside).min_by(f32::total_cmp)?;
    let mut cell = row().filter(|g| aside(g) - closest < 0.5);
    // The end of the cell that is nearer.
    if dir < 0 {
        cell.next_back()
    } else {
        cell.next()
    }
}

/// Position at the start of the cursor's current visual line.
fn home_of(data: &super::state::RichTextData) -> DocPosition {
    let cursor = data.cursor;
    data.cursor_line().map_or(cursor, |line| {
        let g = &data.line_index[line];
        DocPosition::new(g.start.block, g.start.line, g.start.col)
    })
}

/// Position at the end of the cursor's current visual line.
fn end_of(data: &super::state::RichTextData) -> DocPosition {
    let cursor = data.cursor;
    data.cursor_line().map_or(cursor, |line| {
        let g = &data.line_index[line];
        DocPosition::new(g.start.block, g.start.line, data.last_col(g))
    })
}
