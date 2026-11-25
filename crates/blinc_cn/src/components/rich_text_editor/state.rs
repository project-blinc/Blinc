//! Editor state — document, cursor, selection, focus, and the visual line
//! index used for hit-testing and cursor positioning.
//!
//! `RichTextState` is the externally-visible handle. Like `TextInputData`
//! it's an `Arc<Mutex<…>>` so it survives across UI rebuilds. Phase 3 only
//! reads/writes the cursor and selection — Phase 4 will add the edit ops
//! that mutate the document.

use std::sync::{Arc, Mutex};
use web_time::Instant;

use blinc_animation::{TickCallbackId, try_get_scheduler};

use blinc_layout::div::FontWeight;
use blinc_layout::styled_text::StyledLine;
use blinc_layout::widgets::cursor::{SharedCursorState, cursor_state};

use super::cursor::{ActiveFormat, DocPosition, Selection};
use super::document::RichDocument;

/// Per-run geometry inside a single visual line.
///
/// A line is composed of one or more contiguous runs, each with its
/// own font (family / size / weight / italic) — splitting on inline
/// code is the canonical reason for multiple runs, but the same
/// machinery generalizes to any per-span font override.
///
/// Each run records its *measured* pixel x and width so cursor
/// placement and click hit-testing don't have to re-measure with the
/// wrong font.
#[derive(Clone, Debug)]
pub struct RunGeometry {
    /// Source character column where this run starts (relative to the
    /// source line, not the visual line).
    pub source_col: usize,
    /// Substring of the source line that this run covers.
    pub text: String,
    /// Pixel x offset *within the visual line* (i.e. measured from
    /// `LineGeometry.x`).
    pub x_in_line: f32,
    /// Pixel width of the run at its declared font.
    pub width: f32,
    /// Font family used to render and measure this run.
    pub font_family: blinc_layout::div::FontFamily,
    /// Font size in px.
    pub font_size: f32,
    /// Font weight.
    pub weight: FontWeight,
    /// Italic flag.
    pub italic: bool,
}

/// Geometry for a single visual line in the rendered document.
///
/// Built by the renderer at frame-build time and stored on the editor
/// state. Click handling and cursor positioning both walk this index.
///
/// One source `StyledLine` may produce many `LineGeometry` entries
/// (one per pre-wrapped chunk). Each visual line is itself a list of
/// runs ([`RunGeometry`]), so per-span font / weight / size variation
/// inside a line is fully captured.
#[derive(Clone, Debug)]
pub struct LineGeometry {
    /// Document position of the *first* character in this visual line.
    /// Cursor placement on this line is `(block, line, col)` for cols
    /// `0..=total_chars`.
    pub start: DocPosition,
    /// X offset of the line within the editor's content rect (px).
    /// Lists/quotes/indents add to this; plain paragraphs use 0.
    pub x: f32,
    /// Y top of the line within the editor's content rect (px).
    pub y: f32,
    /// Pixel width allocated for this line (used for selection rects).
    pub width: f32,
    /// Pixel height of one line (font_size * line_height).
    pub height: f32,
    /// Runs that make up this visual line, in source order.
    pub runs: Vec<RunGeometry>,
    /// The box of the table cell the line is in (x, y, width, height).
    pub cell: Option<[f32; 4]>,
}

impl LineGeometry {
    /// Concatenated text across all runs — useful for tests / dbg.
    pub fn full_text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Total character count across all runs.
    pub fn total_chars(&self) -> usize {
        self.runs.iter().map(|r| r.text.chars().count()).sum()
    }

    /// True if `(local_x, local_y)` falls inside this line's rect.
    pub fn contains(&self, local_x: f32, local_y: f32) -> bool {
        local_y >= self.y
            && local_y < self.y + self.height
            && local_x >= self.x
            && local_x < self.x + self.width.max(1.0)
    }

    /// True if `local_y` falls inside this line's vertical band, ignoring
    /// horizontal position. Used by the click handler so clicking past
    /// the right edge of a short line still selects its end column.
    pub fn contains_y(&self, local_y: f32) -> bool {
        local_y >= self.y && local_y < self.y + self.height
    }
}

/// A snapshot of editable state for the undo/redo stack.
#[derive(Clone, Debug)]
pub struct UndoEntry {
    pub document: RichDocument,
    pub cursor: DocPosition,
    pub selection: Option<Selection>,
}

/// Editor data that survives across UI rebuilds.
///
/// Held inside `RichTextState = Arc<Mutex<RichTextData>>`. Public fields
/// are read-only outside the edit ops in `edit.rs` — use the helper
/// methods to mutate.
#[derive(Debug)]
pub struct RichTextData {
    /// The document being edited.
    pub document: RichDocument,
    /// Current cursor position. Always clamped to a valid location.
    pub cursor: DocPosition,
    /// Optional selection. When set, `head == cursor` and `anchor` is
    /// the other end.
    pub selection: Option<Selection>,
    /// Active formatting that will be applied to the next typed character.
    pub active_format: ActiveFormat,
    /// Focus flag — set on first mouse-down inside the editor.
    pub focused: bool,
    pub(super) ime: blinc_layout::ime::ImeSession<UndoEntry>,
    /// Visual line geometry index, kept current by the editor as the
    /// document changes.
    pub line_index: Vec<LineGeometry>,
    /// The editor version the line index was computed for.
    pub(crate) geometry_stamp: Option<u32>,
    /// The width the text wraps to: the editor's laid-out text width, or the width it was built
    /// with until it has been laid out.
    pub(super) wrap_width: f32,
    /// Shared cursor blink state — used by the canvas-based cursor
    /// overlay so blinking doesn't require tree rebuilds.
    pub cursor_state: SharedCursorState,
    /// Animation-scheduler tick-callback ID, registered while the editor
    /// is focused. The presence of any tick callback in the scheduler
    /// drives `needs_redraw = true` on the animation thread → wakes the
    /// event loop → only the GPU paint pass runs (no full rebuild).
    /// This is how the editor drives cursor blinking without touching
    /// the global text-input continuous-redraw flag.
    pub tick_callback_id: Option<TickCallbackId>,
    /// Cached editor bounds (x, y, width, height) in screen coords,
    /// captured from the most recent pointer event. The selection
    /// toolbar uses this to position itself in absolute space.
    pub editor_bounds: (f32, f32, f32, f32),
    /// How far below the editor's top the text starts: the padding, and the fixed toolbar's height.
    pub(super) content_top: f32,
    /// How far right of the editor's left edge the text starts: the padding.
    pub(super) content_left: f32,
    /// A press on the text is held: the floating toolbar waits for the release.
    pub(super) pressing: bool,
    /// The fixed toolbar's scroll position, kept while the toolbar is rebuilt.
    pub(super) bar_scroll: blinc_layout::selector::ScrollRef,
    /// The signal of each checklist box shown, by its block.
    pub(super) tasks: std::collections::HashMap<usize, blinc_core::Signal<bool>>,
    /// Called after an edit changes the document; set by [`RichTextStateExt::on_change`].
    on_change: Option<ChangeCallback>,
    /// The document `on_change` last saw.
    reported: Option<RichDocument>,
    /// The link popover's text and URL fields, kept while the toolbar is rebuilt.
    pub(super) link_text: blinc_layout::widgets::text_input::SharedTextInputData,
    pub(super) link_url: blinc_layout::widgets::text_input::SharedTextInputData,
    /// The link popover is open: the selection stays shown while its fields have the focus.
    pub(super) link_open: bool,
    /// The colour the toolbar's picker shows, made on first use and owned by no build, so the
    /// picker keeps it while the toolbar is rebuilt around it.
    swatch: Option<blinc_core::Signal<blinc_core::Color>>,
    /// Timestamp of the most recent mouse-down (used for double-click
    /// detection in the editor's click handler).
    pub last_click_time: Option<Instant>,
    /// Bounding rect of the floating selection toolbar in
    /// editor-content-rect coordinates, written by `toolbar.rs` whenever
    /// the toolbar is built. Currently used for diagnostics — the
    /// click-swallow path uses `suppress_next_outer_click` instead,
    /// because pointer-down events bubble up to the editor's outer
    /// handler with `local_x`/`local_y` in *button-local* coords, not
    /// editor-content coords, so a rect-based check is unreliable.
    pub toolbar_rect: Option<(f32, f32, f32, f32)>,
    /// One-shot flag set by a toolbar button's `on_mouse_down` to tell
    /// the editor's bubbling `on_mouse_down` handler to skip cursor /
    /// selection placement for this event. The flag is consumed
    /// immediately by the outer handler so it never affects subsequent
    /// clicks. This works because Blinc dispatches events deepest-first
    /// then bubbles up, so the button's handler runs before the
    /// editor's outer handler in the same event.
    pub suppress_next_outer_click: bool,
    /// Undo stack — newest entry at the back. Capped at 200 entries to
    /// match the code editor's default.
    pub undo_stack: Vec<UndoEntry>,
    /// Redo stack — populated when undo is invoked, cleared on any new
    /// edit.
    pub redo_stack: Vec<UndoEntry>,
}

impl Default for RichTextData {
    fn default() -> Self {
        Self::new(RichDocument::new())
    }
}

impl Drop for RichTextData {
    fn drop(&mut self) {
        // Make sure we don't leak the tick callback if the editor data
        // is dropped while still focused.
        if let Some(id) = self.tick_callback_id.take()
            && let Some(scheduler) = try_get_scheduler()
        {
            scheduler.remove_tick_callback(id);
        }
    }
}

impl RichTextData {
    pub fn new(document: RichDocument) -> Self {
        Self {
            document,
            cursor: DocPosition::ZERO,
            selection: None,
            active_format: ActiveFormat::default(),
            focused: false,
            ime: Default::default(),
            line_index: Vec::new(),
            geometry_stamp: None,
            wrap_width: 0.0,
            cursor_state: cursor_state(),
            tick_callback_id: None,
            editor_bounds: (0.0, 0.0, 0.0, 0.0),
            content_top: 0.0,
            content_left: 0.0,
            pressing: false,
            bar_scroll: blinc_layout::selector::ScrollRef::new(),
            link_text: Arc::new(Mutex::new(
                blinc_layout::widgets::text_input::TextInputData::with_placeholder("Text"),
            )),
            link_url: Arc::new(Mutex::new(
                blinc_layout::widgets::text_input::TextInputData::with_placeholder("Paste a link"),
            )),
            link_open: false,
            tasks: Default::default(),
            on_change: None,
            reported: None,
            swatch: None,
            last_click_time: None,
            toolbar_rect: None,
            suppress_next_outer_click: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Compute the bounding rectangle of the current selection in
    /// editor-content-rect coordinates. Returns `None` when no
    /// selection exists or it's collapsed. The rect is the union of
    /// every per-line selection slice.
    pub fn selection_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let sel = self.selection?;
        if sel.is_empty() {
            return None;
        }
        let (start, end) = sel.ordered();
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for g in &self.line_index {
            let line_chars = g.total_chars();
            let line_end_col = g.start.col + line_chars;
            let on_block = g.start.block;
            let on_line = g.start.line;
            let after_start =
                (on_block, on_line, line_end_col) >= (start.block, start.line, start.col);
            let before_end = (on_block, on_line, g.start.col) <= (end.block, end.line, end.col);
            if !(after_start && before_end) {
                continue;
            }
            let line_start_pos = (on_block, on_line, g.start.col);
            let line_end_pos = (on_block, on_line, line_end_col);
            let sel_start_pos = (start.block, start.line, start.col);
            let sel_end_pos = (end.block, end.line, end.col);
            let sx = sel_start_pos.max(line_start_pos);
            let ex = sel_end_pos.min(line_end_pos);
            if sx >= ex {
                continue;
            }
            let local_start = sx.2 - g.start.col;
            let local_end = ex.2 - g.start.col;
            let prefix_w = pixel_x_for_local_col(g, local_start);
            let end_w = pixel_x_for_local_col(g, local_end);
            let mid_w = end_w - prefix_w;
            if mid_w <= 0.0 {
                continue;
            }
            let x0 = g.x + prefix_w;
            let x1 = x0 + mid_w;
            let y0 = g.y;
            let y1 = g.y + g.height;
            if x0 < min_x {
                min_x = x0;
            }
            if y0 < min_y {
                min_y = y0;
            }
            if x1 > max_x {
                max_x = x1;
            }
            if y1 > max_y {
                max_y = y1;
            }
        }
        if !min_x.is_finite() {
            return None;
        }
        Some((min_x, min_y, max_x - min_x, max_y - min_y))
    }

    /// Set focus state and register / unregister a per-frame tick on
    /// the animation scheduler.
    ///
    /// The tick callback itself is empty — its mere presence in the
    /// scheduler's `tick_callbacks` slotmap is enough to make the
    /// scheduler thread set `needs_redraw = true` and wake the event
    /// loop on every frame. This drives the cursor blink animation
    /// (the cursor canvas reads `current_opacity()` from `Instant::now()`
    /// on each redraw) without touching the global text-input
    /// continuous-redraw flag, so other widgets in the app are not
    /// affected.
    ///
    /// Calling with the same value twice is a no-op so handlers can
    /// invoke this freely.
    ///
    /// Also drives the global text-editable focus tracker so the
    /// mobile soft-keyboard show/hide path picks the editor up. We
    /// route through `increment_focus_count` / `decrement_focus_count`
    /// in `widgets::text_input` because they're the central
    /// focus-counting hooks every editable widget shares — they fire
    /// `take_keyboard_state_change()` on the next frame when the
    /// global focus count crosses `0 → 1` / `1 → 0`, which the
    /// platform runners forward to `keyboard.show` / `keyboard.hide`.
    pub fn set_focus(&mut self, focused: bool) {
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        if !focused {
            self.finish_ime();
            self.ime.blur();
        }
        self.set_cursor_visible(focused);
        if focused {
            blinc_layout::widgets::text_input::increment_focus_count();
            if self.tick_callback_id.is_none()
                && let Some(scheduler) = try_get_scheduler()
            {
                // Empty tick — we just need the scheduler to keep
                // ticking so it raises needs_redraw each frame.
                self.tick_callback_id = scheduler.register_tick_callback(|_dt| {});
            }
        } else {
            blinc_layout::widgets::text_input::decrement_focus_count();
            blinc_layout::widgets::text_input::clear_focused_editable_node();
            if let Some(id) = self.tick_callback_id.take()
                && let Some(scheduler) = try_get_scheduler()
            {
                scheduler.remove_tick_callback(id);
            }
        }
    }

    /// Replace the line index. Called by the renderer at the end of each
    /// build pass.
    pub fn set_line_index(&mut self, index: Vec<LineGeometry>) {
        self.line_index = index;
    }

    /// Reset the cursor blink so it's visible immediately after typing.
    pub fn reset_cursor_blink(&self) {
        if let Ok(mut cs) = self.cursor_state.lock() {
            cs.reset_blink();
        }
    }

    /// Set the visible flag of the underlying cursor blink state.
    pub fn set_cursor_visible(&self, visible: bool) {
        if let Ok(mut cs) = self.cursor_state.lock() {
            cs.set_visible(visible);
        }
    }

    /// Snapshot current document + cursor + selection onto the undo
    /// stack and clear the redo stack. Call this *before* any text-
    /// modifying op.
    pub fn push_undo(&mut self) {
        const MAX_UNDO: usize = 200;
        self.undo_stack.push(UndoEntry {
            document: self.document.clone(),
            cursor: self.cursor,
            selection: self.selection,
        });
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Pop the most recent undo entry into the document, pushing the
    /// current state onto the redo stack. Returns `true` if anything
    /// was undone.
    pub fn undo(&mut self) -> bool {
        let Some(entry) = self.undo_stack.pop() else {
            return false;
        };
        self.redo_stack.push(UndoEntry {
            document: self.document.clone(),
            cursor: self.cursor,
            selection: self.selection,
        });
        self.document = entry.document;
        self.cursor = entry.cursor.clamp(&self.document);
        self.selection = entry.selection;
        self.active_format = ActiveFormat::from_position(&self.document, self.cursor);
        self.sync_swatch();
        self.reset_cursor_blink();
        true
    }

    /// Pop from the redo stack, mirroring `undo`.
    pub fn redo(&mut self) -> bool {
        let Some(entry) = self.redo_stack.pop() else {
            return false;
        };
        self.undo_stack.push(UndoEntry {
            document: self.document.clone(),
            cursor: self.cursor,
            selection: self.selection,
        });
        self.document = entry.document;
        self.cursor = entry.cursor.clamp(&self.document);
        self.selection = entry.selection;
        self.active_format = ActiveFormat::from_position(&self.document, self.cursor);
        self.sync_swatch();
        self.reset_cursor_blink();
        true
    }

    /// The blocks a block-level change applies to: those of the selection, or the cursor's.
    pub(super) fn block_range(&self) -> Selection {
        self.selection
            .filter(|s| !s.is_empty())
            .unwrap_or(Selection::empty(self.cursor))
    }

    /// Change the document with `op`, which says whether it did, as one undo step.
    pub(super) fn change(&mut self, op: impl FnOnce(&mut RichDocument) -> bool) -> bool {
        self.push_undo();
        if !op(&mut self.document) {
            self.undo_stack.pop();
            return false;
        }
        let cursor = self.cursor;
        self.set_cursor(cursor);
        true
    }

    /// Toggle the blocks of the selection or the cursor to `kind`: where all have it, they become
    /// paragraphs.
    pub(super) fn toggle_kind(&mut self, kind: super::document::BlockKind) -> bool {
        let range = self.block_range();
        self.change(|doc| super::block_ops::toggle_block_kind(doc, range, kind))
    }

    /// Move to the next cell of the table the cursor is in, or the one before it. Past the last
    /// cell there is a new row.
    pub(super) fn tab_in_table(&mut self, forward: bool) {
        use super::table::{Cell, insert_row, next_cell};
        let here = self.cursor;
        let to = next_cell(&self.document, here, forward);
        if let Some(to) = to {
            self.move_cursor(to, false);
        } else if let Some(cell) = Cell::at(&self.document, here).filter(|_| forward) {
            self.change(|doc| insert_row(doc, here, cell.rows).is_some());
            let first = cell.rows * cell.columns;
            self.move_cursor(super::cursor::DocPosition::new(here.block, first, 0), false);
        }
    }

    /// Quote the blocks of the selection or the cursor, or unquote them where all are quotes.
    pub(super) fn toggle_quote(&mut self) -> bool {
        let range = self.block_range();
        self.change(|doc| super::block_ops::toggle_quote(doc, range))
    }

    /// Put a table at the cursor, with the cursor in its first cell.
    pub(super) fn insert_table(&mut self, rows: usize, columns: usize) {
        let here = self.cursor;
        let mut to = here;
        self.change(|doc| {
            to = super::table::insert_table(doc, here, rows, columns);
            true
        });
        self.move_cursor(to, false);
    }

    /// Change the table the cursor is in with `op`, which says where the cursor goes.
    pub(super) fn change_table(
        &mut self,
        op: fn(&mut RichDocument, DocPosition, &super::table::Cell) -> Option<DocPosition>,
    ) {
        let here = self.cursor;
        let Some(cell) = super::table::Cell::at(&self.document, here) else {
            return;
        };
        let mut to = None;
        self.change(|doc| {
            to = op(doc, here, &cell);
            to.is_some()
        });
        if let Some(to) = to {
            self.move_cursor(to, false);
        }
    }

    /// Align the blocks of the selection or the cursor.
    pub(super) fn set_align(&mut self, align: super::document::BlockAlign) -> bool {
        let range = self.block_range();
        self.change(|doc| super::block_ops::set_block_align(doc, range, align))
    }

    /// The colour under the caret, as the picker shows it: text without its own colour takes the
    /// theme's text colour.
    pub(super) fn caret_color(&self) -> blinc_core::Color {
        match self.active_format.color {
            Some(c) if c.a > 0.0 => c,
            _ => blinc_theme::ThemeState::get().color(blinc_theme::ColorToken::TextPrimary),
        }
    }

    /// The picker's colour signal, made on first use.
    pub(super) fn swatch(&mut self) -> blinc_core::Signal<blinc_core::Color> {
        let color = self.caret_color();
        *self.swatch.get_or_insert_with(|| {
            blinc_core::owner::Owner::detached(|| blinc_core::reactive::signal(color))
        })
    }

    /// Show the caret's colour in the picker.
    fn sync_swatch(&self) {
        if let Some(swatch) = self.swatch {
            let color = self.caret_color();
            if swatch.get() != color {
                swatch.set(color);
            }
        }
    }

    /// Set the cursor position (clamped to valid bounds).
    pub fn set_cursor(&mut self, pos: DocPosition) {
        let clamped = pos.clamp(&self.document);
        self.cursor = clamped;
        self.active_format = ActiveFormat::from_position(&self.document, clamped);
        self.sync_swatch();
        self.reset_cursor_blink();
    }

    /// Move the cursor and update the selection head if `extend` is true.
    pub fn move_cursor(&mut self, pos: DocPosition, extend: bool) {
        let clamped = pos.clamp(&self.document);
        if extend {
            // Establish or extend selection from the previous cursor.
            let anchor = self.selection.map(|s| s.anchor).unwrap_or(self.cursor);
            self.selection = Some(Selection {
                anchor,
                head: clamped,
            });
        } else {
            self.selection = None;
        }
        self.cursor = clamped;
        self.active_format = ActiveFormat::from_position(&self.document, clamped);
        self.sync_swatch();
        self.reset_cursor_blink();
    }

    /// The visual line a point is on: in a table, a line of the cell it is in; elsewhere the line
    /// whose band it is in, the first above the text and the last below it.
    pub fn line_at(&self, x: f32, y: f32) -> Option<&LineGeometry> {
        let inside = |[cx, cy, cw, ch]: [f32; 4]| x >= cx && x < cx + cw && y >= cy && y < cy + ch;
        let away = |g: &LineGeometry| (g.y - y).max(y - (g.y + g.height)).max(0.0);
        let in_cell = self
            .line_index
            .iter()
            .filter(|g| g.cell.is_some_and(inside))
            .min_by(|a, b| away(a).total_cmp(&away(b)));
        if in_cell.is_some() {
            return in_cell;
        }
        // Beside a table, the nearest cell of the row.
        let aside = |g: &LineGeometry| (g.x - x).max(x - (g.x + g.width)).max(0.0);
        let band = |g: &&LineGeometry| match g.cell {
            Some([_, cy, _, ch]) => y >= cy && y < cy + ch,
            None => g.contains_y(y),
        };
        let nearest = self.line_index.iter().filter(band).min_by(|a, b| {
            let cells = a.cell.is_some() && b.cell.is_some();
            let across = if cells {
                aside(a).total_cmp(&aside(b))
            } else {
                std::cmp::Ordering::Equal
            };
            across.then(away(a).total_cmp(&away(b)))
        });
        if nearest.is_some() {
            return nearest;
        }
        // Between blocks, above the first and below the last.
        let first = self.line_index.first()?;
        if y < first.y {
            return Some(first);
        }
        self.line_index.last()
    }

    /// The index of the visual line the cursor is on. Where a line wraps, the column the two parts
    /// share is the start of the second.
    pub(super) fn cursor_line(&self) -> Option<usize> {
        let cursor = self.cursor;
        let part = |g: &LineGeometry| g.start.block == cursor.block && g.start.line == cursor.line;
        self.line_index
            .iter()
            .rposition(|g| part(g) && g.start.col <= cursor.col)
            .or_else(|| self.line_index.iter().position(part))
    }

    /// The last column on the visual line `g`: where a line wraps, the one before the space it
    /// wraps at.
    pub(super) fn last_col(&self, g: &LineGeometry) -> usize {
        let end = g.start.col + g.total_chars();
        let wraps = self.line_index.iter().any(|next| {
            (next.start.block, next.start.line) == (g.start.block, g.start.line)
                && next.start.col > g.start.col
                && next.start.col <= end
        });
        if wraps {
            end.saturating_sub(1).max(g.start.col)
        } else {
            end
        }
    }

    /// The cursor's `(x, y)` in the content and its line's height, for the caret.
    pub fn cursor_geometry(&self) -> Option<(f32, f32, f32)> {
        let cursor = self.cursor;
        let g = &self.line_index[self.cursor_line()?];
        let local_col = cursor.col.saturating_sub(g.start.col);
        // Walk runs left-to-right summing widths until we find the run
        // containing the local column. Each run is measured with its
        // own font, so the cursor x stays correct across font changes
        // inside a single line (e.g. proportional + monospace mixes).
        let mut consumed = 0usize;
        for run in &g.runs {
            let run_chars = run.text.chars().count();
            if local_col <= consumed + run_chars {
                let in_run = local_col - consumed;
                let prefix: String = run.text.chars().take(in_run).collect();
                let prefix_w = measure_width(
                    &prefix,
                    run.font_size,
                    run.weight,
                    run.italic,
                    Some(&run.font_family),
                );
                return Some((g.x + run.x_in_line + prefix_w, g.y, g.height));
            }
            consumed += run_chars;
        }
        // Past the last run — drop the cursor at the right edge of the
        // last run if there is one, else at the line origin.
        if let Some(last) = g.runs.last() {
            return Some((g.x + last.x_in_line + last.width, g.y, g.height));
        }
        Some((g.x, g.y, g.height))
    }

    /// Convert a click at `(local_x, local_y)` to a `DocPosition` and
    /// return it. Snaps to the nearest line if no line is directly under
    /// the click.
    ///
    /// The click x is matched against each run's measured pixel range,
    /// then column-scanned within the matching run using that run's
    /// own font. This is what makes mixed-font lines (e.g. body text
    /// with inline code in monospace) place the cursor where the user
    /// actually pointed.
    pub fn position_from_click(&self, local_x: f32, local_y: f32) -> Option<DocPosition> {
        let g = self.line_at(local_x, local_y)?.clone();
        let last_col = self.last_col(&g);
        let inside_x = (local_x - g.x).max(0.0);

        // Walk runs to find which one the click landed in (or past).
        let mut consumed_chars = 0usize;
        for run in &g.runs {
            let run_chars = run.text.chars().count();
            let run_left = run.x_in_line;
            let run_right = run_left + run.width;
            if inside_x < run_right || run_chars == 0 {
                // Hit (or before) this run — column-scan inside it.
                let target = (inside_x - run_left).max(0.0);
                let in_run = column_at_x(
                    &run.text,
                    target,
                    run.font_size,
                    run.weight,
                    run.italic,
                    Some(&run.font_family),
                );
                return Some(DocPosition::new(
                    g.start.block,
                    g.start.line,
                    (g.start.col + consumed_chars + in_run).min(last_col),
                ));
            }
            consumed_chars += run_chars;
        }
        // Click past the last run — drop the cursor at the line end.
        Some(DocPosition::new(
            g.start.block,
            g.start.line,
            (g.start.col + consumed_chars).min(last_col),
        ))
    }
}

/// Shared handle to editor state.
pub type RichTextState = Arc<Mutex<RichTextData>>;

#[derive(Clone)]
struct ChangeCallback(Arc<dyn Fn(&RichDocument) + Send + Sync>);

impl std::fmt::Debug for ChangeCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChangeCallback")
    }
}

/// Callbacks on the shared editor state.
pub trait RichTextStateExt {
    /// Call `f` with the document after an edit made in the editor changes it: typing, formatting,
    /// paste, undo and redo. Moving the cursor, selecting and focus do not. Replaces an earlier
    /// callback. The state is unlocked while `f` runs, so it may read or change the document.
    fn on_change(&self, f: impl Fn(&RichDocument) + Send + Sync + 'static);
}

impl RichTextStateExt for RichTextState {
    fn on_change(&self, f: impl Fn(&RichDocument) + Send + Sync + 'static) {
        if let Ok(mut data) = self.lock() {
            data.reported = Some(data.document.clone());
            data.on_change = Some(ChangeCallback(Arc::new(f)));
        }
    }
}

/// Follow a change: show its ticks in the checklist boxes, and report the document to `on_change`
/// if it differs from the one last reported.
///
/// Called by the edit handlers with the state unlocked and outside any reactive computation, as
/// the callback may lock the state and set signals.
pub(in crate::components::rich_text_editor) fn report_change(state: &RichTextState) {
    super::tasks::sync(state);
    let changed = state.lock().ok().and_then(|mut data| {
        let f = data.on_change.clone()?;
        if data.reported.as_ref() == Some(&data.document) {
            return None;
        }
        let document = data.document.clone();
        data.reported = Some(document.clone());
        Some((f, document))
    });
    if let Some((ChangeCallback(f), document)) = changed {
        f(&document);
        // What the callback itself changed is known to it.
        if let Ok(mut data) = state.lock() {
            data.reported = Some(data.document.clone());
        }
    }
}

/// Convenience: create a new shared state from a document.
pub fn rich_text_state(document: RichDocument) -> RichTextState {
    Arc::new(Mutex::new(RichTextData::new(document)))
}

// =====================================================================
// Helpers — text measurement / character math
// =====================================================================

fn take_chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// Walk a `LineGeometry`'s runs and return the pixel x of `local_col`
/// (a character index inside the visual line, not the source line),
/// measuring with each run's own font.
pub(crate) fn pixel_x_for_local_col(g: &LineGeometry, local_col: usize) -> f32 {
    let mut consumed = 0usize;
    for run in &g.runs {
        let run_chars = run.text.chars().count();
        if local_col <= consumed + run_chars {
            let in_run = local_col - consumed;
            let prefix: String = run.text.chars().take(in_run).collect();
            let prefix_w = measure_width(
                &prefix,
                run.font_size,
                run.weight,
                run.italic,
                Some(&run.font_family),
            );
            return run.x_in_line + prefix_w;
        }
        consumed += run_chars;
    }
    g.runs.last().map(|r| r.x_in_line + r.width).unwrap_or(0.0)
}

/// Measure the pixel width of `text` at the given font properties.
///
/// `font_family` is optional — when `None`, the default font is used.
/// Pass the actual run font when measuring inside a multi-font line so
/// the cursor x lines up with the rendered glyphs.
pub(crate) fn measure_width(
    text: &str,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
    font_family: Option<&blinc_layout::div::FontFamily>,
) -> f32 {
    let mut options = blinc_layout::text_measure::TextLayoutOptions::new();
    options.font_weight = weight.weight();
    options.italic = italic;
    if let Some(family) = font_family {
        options.font_name = family.name.clone();
        options.generic_font = family.generic;
    }
    blinc_layout::text_measure::measure_text_with_options(text, font_size, &options).width
}

/// Find the character column inside `text` whose left edge is closest to
/// `target_x` (in pixels, measured from the line's left edge). The
/// returned column is in `0..=text.chars().count()` so that clicking past
/// the end of a line places the cursor at the end.
pub(crate) fn column_at_x(
    text: &str,
    target_x: f32,
    font_size: f32,
    weight: FontWeight,
    italic: bool,
    font_family: Option<&blinc_layout::div::FontFamily>,
) -> usize {
    if target_x <= 0.0 || text.is_empty() {
        return 0;
    }
    // Linear scan — fine for editor lines, which are short. We bisect
    // each character's left/right edge against the target and pick the
    // closer side.
    let mut prev_width = 0.0;
    let mut col = 0;
    for (i, _ch) in text.char_indices() {
        let upto = &text[..i];
        let after_idx = next_char_index(text, i);
        let upto_inclusive = &text[..after_idx];
        let w_before = measure_width(upto, font_size, weight, italic, font_family);
        let w_after = measure_width(upto_inclusive, font_size, weight, italic, font_family);
        let mid = (w_before + w_after) * 0.5;
        if target_x < mid {
            return col;
        }
        prev_width = w_after;
        col += 1;
        if w_after >= target_x && col > 0 {
            // Already past target — return the col that puts the cursor
            // before this character.
            // Actually we already advanced; bail and return col directly.
            return col;
        }
    }
    let _ = prev_width; // explicit drop, silences "unused"
    text.chars().count()
}

fn next_char_index(text: &str, byte_idx: usize) -> usize {
    text[byte_idx..]
        .char_indices()
        .nth(1)
        .map(|(i, _)| byte_idx + i)
        .unwrap_or(text.len())
}

/// Used by the line index to source the relevant `StyledLine` for a
/// click. The renderer registers entries with their source `StyledLine`
/// reference but stores only the wrapped text in `LineGeometry`; this
/// helper exists for tests that want to reconstruct geometry.
pub(crate) fn synth_line(text: &str, color: blinc_core::Color) -> StyledLine {
    StyledLine::plain(text, color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::rich_text_editor::document::Block;
    use blinc_core::Color;

    fn make_run(text: &str, source_col: usize, x_in_line: f32) -> RunGeometry {
        let width = measure_width(text, 14.0, FontWeight::Normal, false, None);
        RunGeometry {
            source_col,
            text: text.to_string(),
            x_in_line,
            width,
            font_family: blinc_layout::div::FontFamily::default(),
            font_size: 14.0,
            weight: FontWeight::Normal,
            italic: false,
        }
    }

    fn sample_state() -> RichTextData {
        let doc = RichDocument::from_blocks(vec![
            Block::paragraph("hello world", Color::WHITE),
            Block::paragraph("second block", Color::WHITE),
        ]);
        let mut state = RichTextData::new(doc);
        // Synthesize a tiny line index — two single-line blocks, each
        // a single run.
        state.set_line_index(vec![
            LineGeometry {
                start: DocPosition::new(0, 0, 0),
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 20.0,
                runs: vec![make_run("hello world", 0, 0.0)],
                cell: None,
            },
            LineGeometry {
                start: DocPosition::new(1, 0, 0),
                x: 0.0,
                y: 24.0,
                width: 200.0,
                height: 20.0,
                runs: vec![make_run("second block", 0, 0.0)],
                cell: None,
            },
        ]);
        state
    }

    #[test]
    fn click_inside_first_line_finds_block_zero() {
        let state = sample_state();
        let pos = state.position_from_click(40.0, 5.0).unwrap();
        assert_eq!(pos.block, 0);
        assert_eq!(pos.line, 0);
    }

    #[test]
    fn click_in_second_line_finds_block_one() {
        let state = sample_state();
        let pos = state.position_from_click(40.0, 30.0).unwrap();
        assert_eq!(pos.block, 1);
    }

    #[test]
    fn click_above_first_line_snaps_to_start() {
        let state = sample_state();
        let pos = state.position_from_click(40.0, -100.0).unwrap();
        assert_eq!(pos.block, 0);
    }

    #[test]
    fn click_below_last_line_snaps_to_end() {
        let state = sample_state();
        let pos = state.position_from_click(40.0, 9999.0).unwrap();
        assert_eq!(pos.block, 1);
    }

    #[test]
    fn click_at_x_zero_returns_col_zero() {
        let state = sample_state();
        let pos = state.position_from_click(0.0, 5.0).unwrap();
        assert_eq!(pos.col, 0);
    }

    #[test]
    fn click_past_right_edge_returns_end_col() {
        let state = sample_state();
        let pos = state.position_from_click(10000.0, 5.0).unwrap();
        // "hello world" is 11 chars
        assert_eq!(pos.col, 11);
    }

    #[test]
    fn move_cursor_extends_selection_when_requested() {
        let mut state = sample_state();
        state.set_cursor(DocPosition::new(0, 0, 0));
        state.move_cursor(DocPosition::new(0, 0, 5), true);
        assert!(state.selection.is_some());
        let sel = state.selection.unwrap();
        assert_eq!(sel.anchor, DocPosition::new(0, 0, 0));
        assert_eq!(sel.head, DocPosition::new(0, 0, 5));
        // Subsequent extend keeps the same anchor
        state.move_cursor(DocPosition::new(0, 0, 8), true);
        let sel = state.selection.unwrap();
        assert_eq!(sel.anchor, DocPosition::new(0, 0, 0));
        assert_eq!(sel.head, DocPosition::new(0, 0, 8));
    }

    #[test]
    fn move_cursor_clears_selection_when_not_extending() {
        let mut state = sample_state();
        state.move_cursor(DocPosition::new(0, 0, 5), true);
        assert!(state.selection.is_some());
        state.move_cursor(DocPosition::new(0, 0, 7), false);
        assert!(state.selection.is_none());
    }

    #[test]
    fn cursor_geometry_returns_position_for_known_line() {
        let mut state = sample_state();
        state.set_cursor(DocPosition::new(0, 0, 5));
        let (x, y, h) = state.cursor_geometry().unwrap();
        assert!(x > 0.0);
        assert_eq!(y, 0.0);
        assert!(h > 0.0);
    }

    #[test]
    fn synth_line_helper_used_for_test_round_trip() {
        let _l = synth_line("a", Color::WHITE);
    }
}
