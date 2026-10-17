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
//! triggered by a private `version: State<u32>` signal that the click
//! and key handlers bump after every mutation. The Stateful's `on_state`
//! callback re-walks the line geometry index against the current
//! document and emits the cursor / selection overlay.

use std::sync::Arc;

use blinc_core::State;
use blinc_core::context_state::BlincContextState;

use crate::div::{Div, div};

use super::block_ops::{indent_blocks, outdent_blocks, toggle_block_kind};
use super::content::{content, ensure_geometry};
use super::cursor::{DocPosition, step_backward, step_forward};
use super::document::BlockKind;
use super::edit::{
    delete_backward, delete_forward, delete_selection, insert_char, insert_text, soft_break,
    split_block,
};
use super::format::{Mark, apply_mark_to_selection};
use super::render::RichTextTheme;
use super::state::RichTextState;

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
    let version: State<u32> = blinc.use_state_keyed(&version_key, || 0);

    // Pre-populate the line index so the very first frame can already
    // hit-test clicks before any state mutation has happened.
    ensure_geometry(state, &theme, content_width, version.try_get().unwrap_or(0));

    // Clones for the various closures
    let state_for_click = Arc::clone(state);
    let state_for_drag = Arc::clone(state);
    let state_for_key = Arc::clone(state);
    let state_for_text = Arc::clone(state);
    let version_for_click = version.clone();
    let version_for_drag = version.clone();
    let version_for_key = version.clone();
    let version_for_text = version.clone();

    let content = content(state, &version, &theme, content_width);

    div()
        .w_full()
        .on_mouse_down(move |ctx| {
            let mut data = state_for_click.lock().unwrap();
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
            crate::widgets::text_input::bump_focus_tap_generation();
            let blur_state = Arc::clone(&state_for_click);
            let blur_version = version_for_click.clone();
            crate::widgets::text_input::set_focused_editable_node(
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
            //
            // We can't use a coordinate-based hit test here because
            // bubbling events carry the *button's* local coordinates,
            // not the editor's (Blinc's event router caches
            // `last_hit_local_*` once and forwards them up the chain).
            if data.suppress_next_outer_click {
                data.suppress_next_outer_click = false;
                return;
            }

            // Detect double-click within the standard 400ms window.
            let now = web_time::Instant::now();
            let is_double = data
                .last_click_time
                .map(|t| now.duration_since(t).as_millis() < 400)
                .unwrap_or(false);
            data.last_click_time = Some(now);

            let touch = crate::widgets::text_input::is_touch_input();

            if let Some(pos) = data.position_from_click(ctx.local_x, ctx.local_y) {
                if is_double {
                    // Double-click — select the word under the cursor.
                    if let Some(line) = data
                        .document
                        .blocks
                        .get(pos.block)
                        .and_then(|b| b.lines.get(pos.line))
                    {
                        let (start_col, end_col) =
                            crate::widgets::text_edit::word_at_position(&line.text, pos.col);
                        data.cursor = super::cursor::DocPosition::new(pos.block, pos.line, end_col);
                        data.selection = Some(super::cursor::Selection {
                            anchor: super::cursor::DocPosition::new(pos.block, pos.line, start_col),
                            head: super::cursor::DocPosition::new(pos.block, pos.line, end_col),
                        });
                        data.reset_cursor_blink();
                        if touch {
                            crate::widgets::text_edit::haptic_impact_light();
                            use crate::widgets::text_edit::edit_menu_actions;
                            crate::widgets::text_edit::show_edit_menu(
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
                    crate::widgets::text_edit::haptic_selection();
                    crate::widgets::text_edit::hide_edit_menu();
                    let captured_pos = pos;
                    let state_for_long_press = Arc::clone(&state_for_click);
                    let version_for_long_press = version_for_click.clone();
                    crate::widgets::text_input::arm_long_press_timer(
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
                                    crate::widgets::text_edit::word_at_position(
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
        .on_drag(move |ctx| {
            let mut data = state_for_drag.lock().unwrap();
            if !data.focused {
                return;
            }
            // Touch drag = move cursor (no selection extension), with
            // a subtle haptic per character boundary.
            // Mouse drag = extend selection from anchor to pointer.
            let touch = crate::widgets::text_input::is_touch_input();
            if let Some(pos) = data.position_from_click(ctx.local_x, ctx.local_y) {
                if touch {
                    // Drift-cancel any armed long-press so a real
                    // cursor drag doesn't also fire the paste menu.
                    crate::widgets::text_input::check_long_press_drift(ctx.mouse_x, ctx.mouse_y);
                    if data.cursor != pos {
                        data.move_cursor(pos, false);
                        crate::widgets::text_edit::haptic_selection();
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
            if !data.focused {
                return;
            }
            // If the link prompt is open, route the keystroke into the
            // URL draft instead of inserting into the document.
            if let super::state::PickerState::Link { ref mut draft } = data.picker {
                draft.push(ch);
                drop(data);
                version_for_text.set(version_for_text.get().wrapping_add(1));
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
        })
        .on_key_down(move |ctx| {
            let mut data = state_for_key.lock().unwrap();
            if !data.focused {
                return;
            }

            // Link-prompt key handling — Backspace edits the draft,
            // Enter commits, Esc cancels. Other keys fall through so
            // arrow keys still navigate the document underneath.
            if matches!(data.picker, super::state::PickerState::Link { .. }) {
                match ctx.key_code {
                    8 => {
                        // Backspace — pop the last character from the draft
                        if let super::state::PickerState::Link { ref mut draft } = data.picker {
                            draft.pop();
                            drop(data);
                            version_for_key.set(version_for_key.get().wrapping_add(1));
                            return;
                        }
                    }
                    13 => {
                        // Enter — commit the link
                        if let super::state::PickerState::Link { draft } = data.picker.clone() {
                            if !draft.is_empty() {
                                if let Some(sel) = data.selection {
                                    if !sel.is_empty() {
                                        data.push_undo();
                                        super::format::apply_mark_to_selection(
                                            &mut data.document,
                                            sel,
                                            super::format::Mark::Link(Some(draft)),
                                        );
                                    }
                                }
                            }
                            data.picker = super::state::PickerState::None;
                            drop(data);
                            version_for_key.set(version_for_key.get().wrapping_add(1));
                            return;
                        }
                    }
                    27 => {
                        // Esc — cancel the prompt
                        data.picker = super::state::PickerState::None;
                        drop(data);
                        version_for_key.set(version_for_key.get().wrapping_add(1));
                        return;
                    }
                    _ => {}
                }
            }

            let mod_key = ctx.meta || ctx.ctrl;
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
                    // Cmd+E — inline code
                    69 => {
                        changed |= toggle_mark(&mut data, Mark::Code);
                    }
                    // Cmd+C — copy selected text to system clipboard
                    67 => {
                        if let Some(sel) = data.selection {
                            if !sel.is_empty() {
                                let (s, e) = sel.ordered();
                                let plain = data.document.plain_text_range(s, e);
                                if !plain.is_empty() {
                                    crate::widgets::text_edit::clipboard_write(&plain);
                                }
                            }
                        }
                        // Copy never mutates the document, so we don't
                        // bump the rebuild signal.
                    }
                    // Cmd+X — cut: copy then delete
                    88 if !ctx.shift => {
                        if let Some(sel) = data.selection {
                            if !sel.is_empty() {
                                let (s, e) = sel.ordered();
                                let plain = data.document.plain_text_range(s, e);
                                if !plain.is_empty() {
                                    crate::widgets::text_edit::clipboard_write(&plain);
                                }
                                data.push_undo();
                                data.selection = None;
                                let new_pos = delete_selection(&mut data.document, sel);
                                data.set_cursor(new_pos);
                                changed = true;
                            }
                        }
                    }
                    // Cmd+V — paste: replace selection (if any) with
                    // clipboard text. Newlines in the clipboard become
                    // soft breaks via `insert_text`.
                    86 => {
                        if let Some(text) = crate::widgets::text_edit::clipboard_read() {
                            if !text.is_empty() {
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
                    }
                    // Cmd+Alt+0 — paragraph
                    48 if ctx.alt => {
                        changed |= apply_block_kind(&mut data, BlockKind::Paragraph);
                    }
                    // Cmd+Alt+1..6 — headings
                    49 if ctx.alt => {
                        changed |= apply_block_kind(&mut data, BlockKind::Heading(1));
                    }
                    50 if ctx.alt => {
                        changed |= apply_block_kind(&mut data, BlockKind::Heading(2));
                    }
                    51 if ctx.alt => {
                        changed |= apply_block_kind(&mut data, BlockKind::Heading(3));
                    }
                    52 if ctx.alt => {
                        changed |= apply_block_kind(&mut data, BlockKind::Heading(4));
                    }
                    53 if ctx.alt => {
                        changed |= apply_block_kind(&mut data, BlockKind::Heading(5));
                    }
                    54 if ctx.alt => {
                        changed |= apply_block_kind(&mut data, BlockKind::Heading(6));
                    }
                    // Cmd+Shift+7 — toggle numbered list
                    55 if ctx.shift => {
                        changed |= apply_block_kind(&mut data, BlockKind::NumberedItem);
                    }
                    // Cmd+Shift+8 — toggle bullet list
                    56 if ctx.shift => {
                        changed |= apply_block_kind(&mut data, BlockKind::BulletItem);
                    }
                    // Cmd+Shift+9 — toggle quote
                    57 if ctx.shift => {
                        changed |= apply_block_kind(&mut data, BlockKind::Quote);
                    }
                    _ => {}
                }
                if changed {
                    drop(data);
                    version_for_key.set(version_for_key.get().wrapping_add(1));
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
                // Tab — indent the current block (or selected blocks).
                // Shift+Tab outdents. Only paragraphs and list items
                // are affected; headings, dividers, and quotes are
                // skipped by `indent_blocks` / `outdent_blocks`.
                9 => {
                    let range = block_selection(&data);
                    let did_change = if ctx.shift {
                        data.push_undo();
                        let r = outdent_blocks(&mut data.document, range);
                        if !r {
                            data.undo_stack.pop();
                        }
                        r
                    } else {
                        data.push_undo();
                        let r = indent_blocks(&mut data.document, range);
                        if !r {
                            data.undo_stack.pop();
                        }
                        r
                    };
                    if did_change {
                        let cursor = data.cursor;
                        data.set_cursor(cursor);
                        changed = true;
                    }
                }
                _ => {}
            }

            drop(data);
            if changed {
                version_for_key.set(version_for_key.get().wrapping_add(1));
            }
        })
        .child(content)
}

/// Build a `Selection` covering whatever block range the editor's
/// current cursor / selection covers — used by block-level ops that
/// need a range even when the user has no explicit selection.
fn block_selection(data: &super::state::RichTextData) -> super::cursor::Selection {
    if let Some(sel) = data.selection {
        if !sel.is_empty() {
            return sel;
        }
    }
    super::cursor::Selection {
        anchor: data.cursor,
        head: data.cursor,
    }
}

/// Toggle every block in the editor's current cursor range to `kind`.
/// If every block in the range already has `kind`, they all revert
/// to `BlockKind::Paragraph`. Pushes undo before mutating; pops the
/// undo entry if nothing actually changed.
fn apply_block_kind(data: &mut super::state::RichTextData, kind: BlockKind) -> bool {
    let range = block_selection(data);
    data.push_undo();
    let changed = toggle_block_kind(&mut data.document, range, kind);
    if !changed {
        data.undo_stack.pop();
        return false;
    }
    let cursor = data.cursor;
    data.set_cursor(cursor);
    true
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
    if let Some(sel) = data.selection {
        if !sel.is_empty() {
            data.push_undo();
            let changed = apply_mark_to_selection(&mut data.document, sel, mark.clone());
            // Refresh the active format from the cursor location so the
            // toolbar (Phase 7) sees the new state.
            data.active_format =
                super::cursor::ActiveFormat::from_position(&data.document, data.cursor);
            data.reset_cursor_blink();
            return changed;
        }
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
    // Find the cursor's current visual line in the index.
    let current_idx = data.line_index.iter().position(|g| {
        if g.start.block != cursor.block || g.start.line != cursor.line {
            return false;
        }
        let end_col = g.start.col + g.total_chars();
        cursor.col >= g.start.col && cursor.col <= end_col
    })?;
    let target_idx = if dir < 0 {
        current_idx.checked_sub(1)?
    } else {
        let n = current_idx + 1;
        if n >= data.line_index.len() {
            return None;
        }
        n
    };
    let g = &data.line_index[target_idx];
    let local_col = cursor
        .col
        .saturating_sub(data.line_index[current_idx].start.col);
    let new_col = g.start.col + local_col.min(g.total_chars());
    Some(DocPosition::new(g.start.block, g.start.line, new_col))
}

/// Position at the start of the cursor's current visual line.
fn home_of(data: &super::state::RichTextData) -> DocPosition {
    let cursor = data.cursor;
    for g in &data.line_index {
        if g.start.block == cursor.block && g.start.line == cursor.line {
            let end_col = g.start.col + g.total_chars();
            if cursor.col >= g.start.col && cursor.col <= end_col {
                return DocPosition::new(g.start.block, g.start.line, g.start.col);
            }
        }
    }
    cursor
}

/// Position at the end of the cursor's current visual line.
fn end_of(data: &super::state::RichTextData) -> DocPosition {
    let cursor = data.cursor;
    for g in &data.line_index {
        if g.start.block == cursor.block && g.start.line == cursor.line {
            let end_col = g.start.col + g.total_chars();
            if cursor.col >= g.start.col && cursor.col <= end_col {
                return DocPosition::new(g.start.block, g.start.line, end_col);
            }
        }
    }
    cursor
}
