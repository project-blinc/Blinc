//! Code block widget with syntax highlighting
//!
//! Two modes of operation:
//!
//! - **Read-only** via `code("content")` — lightweight display widget, no Stateful overhead
//! - **Editable** via `code_editor(&state)` — full editor with Stateful incremental updates
//!
//! # Read-only Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_layout::syntax::{SyntaxConfig, RustHighlighter};
//!
//! code(r#"fn main() { println!("Hello"); }"#)
//!     .syntax(SyntaxConfig::new(RustHighlighter::new()))
//!     .line_numbers(true)
//!     .font_size(14.0)
//! ```
//!
//! # Editable Example
//!
//! ```ignore
//! let state = code_editor_state("let x = 42;");
//! code_editor(&state)
//!     .syntax(SyntaxConfig::new(RustHighlighter::new()))
//!     .line_numbers(true)
//!     .h(400.0)
//!     .on_change(|new_content| {
//!         println!("Content changed: {}", new_content);
//!     })
//! ```
//!
//! # Editor Features
//!
//! **Editing:** Type, Enter (auto-indent), Backspace, Delete, Tab/Shift+Tab (indent/dedent),
//! Cmd+Backspace/Delete (delete word), Cmd+Z/Shift+Z (undo/redo)
//!
//! **Navigation:** Arrow keys, Cmd+Left/Right (word jump), Home (smart: first non-whitespace
//! then col 0), End, Page Up/Down, mouse click, mouse drag selection, double-click word select
//!
//! **Clipboard:** Cmd+A (select all), Cmd+C (copy), Cmd+X (cut), Cmd+V (paste)
//!
//! **Visual:** Syntax highlighting (cached per-line), line numbers, selection highlight,
//! current line highlight, cursor with blink animation, vertical scroll (overflow_y_scroll)

use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use blinc_core::{Brush, Color, CornerRadius, Rect};
use blinc_theme::{ColorToken, ThemeState};

use crate::canvas::canvas;
use crate::div::{Div, ElementBuilder, ElementTypeId, div};
use crate::element::RenderProps;
use crate::styled_text::StyledText;
use crate::syntax::{SyntaxConfig, SyntaxHighlighter, TokenHit};
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::cursor::{CursorAnimation, SharedCursorState, cursor_state};
use crate::widgets::scroll::{ScrollPhysics, SharedScrollPhysics};
use crate::widgets::text_area::TextPosition;
use crate::widgets::text_edit;
use crate::widgets::text_input::{
    SharedTextInputData, text_input, text_input_state_with_placeholder,
};
use crate::widgets::text_input::{
    decrement_focus_count, increment_focus_count, request_continuous_redraw_pub,
};

// ============================================================================
// Configuration
// ============================================================================

/// Code block configuration
#[derive(Clone)]
pub struct CodeConfig {
    /// Font size in pixels
    pub font_size: f32,
    /// Line height multiplier
    pub line_height: f32,
    /// Show line numbers in gutter
    pub line_numbers: bool,
    /// Gutter width (for line numbers)
    pub gutter_width: f32,
    /// Padding inside the code block
    pub padding: f32,
    /// Corner radius
    pub corner_radius: f32,
    /// Background color
    pub bg_color: Color,
    /// Text color (default, when no syntax highlighting)
    pub text_color: Color,
    /// Line number color
    pub line_number_color: Color,
    /// Cursor color (when editable)
    pub cursor_color: Color,
    /// Selection color (when editable)
    pub selection_color: Color,
    /// Gutter background color
    pub gutter_bg_color: Color,
    /// Gutter separator color
    pub gutter_separator_color: Color,
    /// Show minimap on the right side
    pub minimap: bool,
    /// Minimap width in pixels
    pub minimap_width: f32,
    /// Show indentation guides
    pub indent_guides: bool,
    /// Indentation guide color
    pub indent_guide_color: Color,
    /// Enable code folding
    pub code_folding: bool,
    /// Enable the find / replace search overlay (Cmd+F to open,
    /// Cmd+G / Cmd+Shift+G next/prev, Cmd+H toggle replace,
    /// Cmd+R toggle regex). Disabled by default — the search bar
    /// is pushed via the legacy overlay manager (a sibling
    /// top-level overlay, not nested under the host's content),
    /// and clicks inside it dismiss the host popover via the
    /// click_outside registry. Opt in for hosts that own the full
    /// viewport; leave off when embedding inside a `cn::popover`
    /// or any host overlay that uses click_outside dismissal.
    pub search_enabled: bool,
}

impl Default for CodeConfig {
    fn default() -> Self {
        let theme = ThemeState::get();
        Self {
            font_size: 13.0,
            line_height: 1.5,
            line_numbers: false,
            gutter_width: 48.0,
            padding: 16.0,
            corner_radius: 8.0,
            bg_color: theme.color(ColorToken::Surface),
            text_color: theme.color(ColorToken::TextPrimary),
            line_number_color: theme.color(ColorToken::TextTertiary),
            cursor_color: theme.color(ColorToken::Accent),
            selection_color: theme.color(ColorToken::Selection),
            gutter_bg_color: theme.color(ColorToken::SurfaceOverlay),
            gutter_separator_color: theme.color(ColorToken::Border),
            minimap: false,
            minimap_width: 60.0,
            indent_guides: false,
            indent_guide_color: theme.color(ColorToken::Border).with_alpha(0.15),
            code_folding: false,
            search_enabled: false,
        }
    }
}

// ============================================================================
// Shared Editor State
// ============================================================================

/// Callback type for content changes
type OnChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Internal state for code editing
#[derive(Clone)]
pub struct CodeEditorData {
    /// Lines of text
    pub lines: Vec<String>,
    /// Cursor position
    pub cursor: TextPosition,
    /// Selection start (if selecting)
    pub selection_start: Option<TextPosition>,
    /// Whether currently focused
    pub focused: bool,
    /// Canvas-based cursor state
    pub cursor_state: SharedCursorState,
    /// On-change callback
    pub on_change: Option<OnChangeCallback>,
    /// Syntax highlighter
    pub highlighter: Option<Arc<dyn SyntaxHighlighter>>,
    /// Configuration snapshot (updated from builder)
    pub config: CodeConfig,
    /// Undo stack
    pub undo_stack: Vec<UndoEntry>,
    /// Redo stack
    pub redo_stack: Vec<UndoEntry>,
    /// Cached monospace character width for current font_size.
    /// Avoids calling measure_text() for every cursor/selection calculation.
    mono_char_width: f32,
    /// Font size that mono_char_width was computed for
    mono_char_width_font_size: f32,
    /// Cached per-line syntax highlight results. Indexed by line number.
    /// Cleared when content changes on that line.
    highlight_cache: Vec<Option<crate::styled_text::StyledLine>>,
    /// Drag selection anchor (mouse-down position, set to selection_start on drag)
    pub drag_anchor: Option<TextPosition>,
    /// Last click time for double-click detection (ms since epoch)
    last_click_time: f64,
    /// Last click position for double-click detection
    last_click_pos: TextPosition,
    /// Search state
    pub search_active: bool,
    /// Current search query
    pub search_query: String,
    /// Shared text input state for the search bar
    pub search_input_state: SharedTextInputData,
    /// Shared text input state for the replace bar
    pub replace_input_state: SharedTextInputData,
    /// Current replace text
    pub replace_text: String,
    /// Whether regex mode is enabled
    pub search_regex: bool,
    /// Case sensitive search
    pub search_case_sensitive: bool,
    /// Whole word match
    pub search_whole_word: bool,
    /// Whether replace row is visible
    pub replace_active: bool,
    /// Overlay handle for the search bar (if open)
    pub search_overlay_handle: Option<crate::widgets::overlay::OverlayHandle>,
    /// Editor bounds (updated on mouse events for overlay positioning)
    pub editor_bounds: (f32, f32, f32, f32), // (x, y, width, height)
    /// Signal ID for search state changes (triggers overlay Stateful rebuild)
    pub search_signal_id: Option<blinc_core::reactive::SignalId>,
    /// All match positions: (line, start_col, end_col)
    pub search_matches: Vec<(usize, usize, usize)>,
    /// Index of the currently focused match
    pub search_match_idx: usize,
    /// Scroll physics for vertical scrolling
    pub scroll_physics: SharedScrollPhysics,
    /// Cached viewport height (content area minus padding)
    pub viewport_height: f32,
    /// Folded line ranges: each entry is (start_line, end_line) where
    /// start_line is the line with `{` and end_line is the matching `}`.
    /// Lines start_line+1..end_line are hidden when folded.
    pub folded_regions: Vec<(usize, usize)>,
}

/// A foldable region detected from bracket matching
#[derive(Debug, Clone, Copy)]
pub struct FoldRegion {
    /// Line containing the opening bracket
    pub start_line: usize,
    /// Line containing the closing bracket
    pub end_line: usize,
}

/// Undo/redo entry
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub lines: Vec<String>,
    pub cursor: TextPosition,
    pub selection_start: Option<TextPosition>,
}

/// Shared code editor state (thread-safe, persists across rebuilds)
pub type SharedCodeEditorState = Arc<Mutex<CodeEditorData>>;

/// Create a new shared code editor state
pub fn code_editor_state(content: impl Into<String>) -> SharedCodeEditorState {
    let content = content.into();
    let lines: Vec<String> = if content.is_empty() {
        vec![String::new()]
    } else {
        content.lines().map(|s| s.to_string()).collect()
    };

    let num_lines = lines.len();
    Arc::new(Mutex::new(CodeEditorData {
        lines,
        cursor: TextPosition::default(),
        selection_start: None,
        focused: false,
        cursor_state: cursor_state(),
        on_change: None,
        highlighter: None,
        config: CodeConfig::default(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        mono_char_width: 0.0,
        mono_char_width_font_size: 0.0,
        highlight_cache: vec![None; num_lines],
        drag_anchor: None,
        last_click_time: 0.0,
        last_click_pos: TextPosition::default(),
        search_active: false,
        search_query: String::new(),
        search_input_state: text_input_state_with_placeholder("Find..."),
        replace_input_state: text_input_state_with_placeholder("Replace..."),
        replace_text: String::new(),
        search_regex: false,
        search_case_sensitive: false,
        search_whole_word: false,
        replace_active: false,
        search_overlay_handle: None,
        editor_bounds: (0.0, 0.0, 800.0, 300.0),
        search_signal_id: None,
        search_matches: Vec::new(),
        search_match_idx: 0,
        scroll_physics: Arc::new(Mutex::new(ScrollPhysics::default())),
        viewport_height: 0.0,
        folded_regions: Vec::new(),
    }))
}

impl CodeEditorData {
    /// Get full text content
    pub fn value(&self) -> String {
        self.lines.join("\n")
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    /// Measure text width using monospace font options (matches rendered text).
    pub fn measure_mono(&self, text: &str) -> f32 {
        let opts = crate::text_measure::TextLayoutOptions {
            generic_font: crate::div::GenericFont::Monospace,
            ..crate::text_measure::TextLayoutOptions::new()
        };
        crate::text_measure::measure_text_with_options(text, self.config.font_size, &opts).width
    }

    /// Get the cached monospace character width, recomputing if font size changed.
    pub fn char_width(&mut self) -> f32 {
        let fs = self.config.font_size;
        if (self.mono_char_width_font_size - fs).abs() > 0.01 || self.mono_char_width <= 0.0 {
            self.mono_char_width = self.measure_mono("M");
            self.mono_char_width_font_size = fs;
        }
        self.mono_char_width
    }

    /// Measure the pixel width of `char_count` monospace characters.
    pub fn measure_chars(&mut self, char_count: usize) -> f32 {
        self.char_width() * char_count as f32
    }

    /// Invalidate highlight cache for a specific line
    fn invalidate_highlight_line(&mut self, line: usize) {
        if line < self.highlight_cache.len() {
            self.highlight_cache[line] = None;
        }
    }

    /// Resize highlight cache to match line count
    fn sync_highlight_cache(&mut self) {
        self.highlight_cache.resize(self.lines.len(), None);
    }

    /// Invalidate entire highlight cache
    fn invalidate_all_highlights(&mut self) {
        self.highlight_cache.fill(None);
        self.sync_highlight_cache();
    }

    /// Save current state to undo stack
    pub fn push_undo(&mut self) {
        self.undo_stack.push(UndoEntry {
            lines: self.lines.clone(),
            cursor: self.cursor,
            selection_start: self.selection_start,
        });
        self.redo_stack.clear();
        // Cap undo history
        if self.undo_stack.len() > 200 {
            self.undo_stack.remove(0);
        }
    }

    /// Undo last change
    pub fn undo(&mut self) -> bool {
        if let Some(entry) = self.undo_stack.pop() {
            self.redo_stack.push(UndoEntry {
                lines: self.lines.clone(),
                cursor: self.cursor,
                selection_start: self.selection_start,
            });
            self.lines = entry.lines;
            self.cursor = entry.cursor;
            self.selection_start = entry.selection_start;
            self.invalidate_all_highlights();
            true
        } else {
            false
        }
    }

    /// Redo last undone change
    pub fn redo(&mut self) -> bool {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(UndoEntry {
                lines: self.lines.clone(),
                cursor: self.cursor,
                selection_start: self.selection_start,
            });
            self.lines = entry.lines;
            self.cursor = entry.cursor;
            self.selection_start = entry.selection_start;
            self.invalidate_all_highlights();
            true
        } else {
            false
        }
    }

    /// Insert text at cursor position
    pub fn insert(&mut self, text: &str) {
        self.push_undo();
        self.delete_selection();
        let start_line = self.cursor.line;
        for ch in text.chars() {
            if ch == '\n' {
                self.insert_newline();
            } else {
                self.insert_char(ch);
            }
        }
        // Invalidate from start_line to end — inserting/removing lines shifts
        // all subsequent cache entries, so they're all stale
        for l in start_line..self.lines.len() {
            self.invalidate_highlight_line(l);
        }
        self.sync_highlight_cache();
    }

    fn insert_char(&mut self, ch: char) {
        if self.cursor.line < self.lines.len() {
            let line = &mut self.lines[self.cursor.line];
            let byte_pos = char_to_byte_pos(line, self.cursor.column);
            line.insert(byte_pos, ch);
            self.cursor.column += 1;
        }
    }

    fn insert_newline(&mut self) {
        if self.cursor.line < self.lines.len() {
            let current_line = &self.lines[self.cursor.line];
            // Capture leading whitespace for auto-indent
            let indent: String = current_line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();
            let byte_pos = char_to_byte_pos(current_line, self.cursor.column);
            let rest = current_line[byte_pos..].to_string();
            self.lines[self.cursor.line] = current_line[..byte_pos].to_string();
            self.cursor.line += 1;
            // Auto-indent: preserve leading whitespace from previous line
            let new_line = format!("{}{}", indent, rest);
            self.cursor.column = indent.chars().count();
            self.lines.insert(self.cursor.line, new_line);
        }
    }

    /// Delete backward (backspace)
    pub fn delete_backward(&mut self) {
        if self.delete_selection() {
            return;
        }
        self.push_undo();
        let line_removed = self.cursor.column == 0 && self.cursor.line > 0;
        if self.cursor.column > 0 {
            let line = &mut self.lines[self.cursor.line];
            let byte_pos = char_to_byte_pos(line, self.cursor.column - 1);
            let next_byte = char_to_byte_pos(line, self.cursor.column);
            line.replace_range(byte_pos..next_byte, "");
            self.cursor.column -= 1;
        } else if self.cursor.line > 0 {
            let current_line = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
            self.lines[self.cursor.line].push_str(&current_line);
        }
        if line_removed {
            // Line removal shifts all subsequent entries
            self.invalidate_all_highlights();
        } else {
            self.invalidate_highlight_line(self.cursor.line);
            self.sync_highlight_cache();
        }
    }

    /// Delete forward (delete key)
    pub fn delete_forward(&mut self) {
        if self.delete_selection() {
            return;
        }
        self.push_undo();
        let mut line_removed = false;
        if self.cursor.line < self.lines.len() {
            let line_len = self.lines[self.cursor.line].chars().count();
            if self.cursor.column < line_len {
                let line = &mut self.lines[self.cursor.line];
                let byte_pos = char_to_byte_pos(line, self.cursor.column);
                let next_byte = char_to_byte_pos(line, self.cursor.column + 1);
                line.replace_range(byte_pos..next_byte, "");
            } else if self.cursor.line + 1 < self.lines.len() {
                let next_line = self.lines.remove(self.cursor.line + 1);
                self.lines[self.cursor.line].push_str(&next_line);
                line_removed = true;
            }
        }
        if line_removed {
            self.invalidate_all_highlights();
        } else {
            self.invalidate_highlight_line(self.cursor.line);
            self.sync_highlight_cache();
        }
    }

    /// Delete selection. Returns true if a selection was deleted.
    pub fn delete_selection(&mut self) -> bool {
        if let Some(sel_start) = self.selection_start.take() {
            let (start, end) = order_positions(sel_start, self.cursor);
            if start == end {
                return false;
            }
            self.push_undo();

            if start.line == end.line {
                let line = &mut self.lines[start.line];
                let start_byte = char_to_byte_pos(line, start.column);
                let end_byte = char_to_byte_pos(line, end.column);
                line.replace_range(start_byte..end_byte, "");
            } else {
                let start_byte = char_to_byte_pos(&self.lines[start.line], start.column);
                let end_byte = char_to_byte_pos(&self.lines[end.line], end.column);
                let end_remainder = self.lines[end.line][end_byte..].to_string();
                self.lines[start.line] = self.lines[start.line][..start_byte].to_string();
                self.lines[start.line].push_str(&end_remainder);
                self.lines.drain((start.line + 1)..=end.line);
            }

            self.cursor = start;
            self.selection_start = None;
            self.invalidate_all_highlights();
            true
        } else {
            false
        }
    }

    /// Get selected text
    pub fn selected_text(&self) -> Option<String> {
        let sel_start = self.selection_start?;
        let (start, end) = order_positions(sel_start, self.cursor);
        if start == end {
            return None;
        }

        if start.line == end.line {
            let line = &self.lines[start.line];
            let s = char_to_byte_pos(line, start.column);
            let e = char_to_byte_pos(line, end.column);
            Some(line[s..e].to_string())
        } else {
            let mut result = String::new();
            for (line_idx, line) in self
                .lines
                .iter()
                .enumerate()
                .take(end.line + 1)
                .skip(start.line)
            {
                if line_idx == start.line {
                    let s = char_to_byte_pos(line, start.column);
                    result.push_str(&line[s..]);
                } else if line_idx == end.line {
                    result.push('\n');
                    let e = char_to_byte_pos(line, end.column);
                    result.push_str(&line[..e]);
                } else {
                    result.push('\n');
                    result.push_str(line);
                }
            }
            Some(result)
        }
    }

    /// Select all text
    pub fn select_all(&mut self) {
        self.selection_start = Some(TextPosition::new(0, 0));
        let last_line = self.lines.len().saturating_sub(1);
        let last_col = self.lines.last().map(|l| l.chars().count()).unwrap_or(0);
        self.cursor = TextPosition::new(last_line, last_col);
    }

    // ========================================================================
    // Cursor movement
    // ========================================================================

    pub fn move_left(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            // Collapse selection to start
            if let Some(sel) = self.selection_start.take() {
                let (start, _) = order_positions(sel, self.cursor);
                self.cursor = start;
                return;
            }
        }
        if self.cursor.column > 0 {
            self.cursor.column -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
        }
    }

    pub fn move_right(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            if let Some(sel) = self.selection_start.take() {
                let (_, end) = order_positions(sel, self.cursor);
                self.cursor = end;
                return;
            }
        }
        if self.cursor.line < self.lines.len() {
            let line_len = self.lines[self.cursor.line].chars().count();
            if self.cursor.column < line_len {
                self.cursor.column += 1;
            } else if self.cursor.line + 1 < self.lines.len() {
                self.cursor.line += 1;
                self.cursor.column = 0;
            }
        }
    }

    pub fn move_up(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            let line_len = self.lines[self.cursor.line].chars().count();
            self.cursor.column = self.cursor.column.min(line_len);
        }
    }

    pub fn move_down(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            let line_len = self.lines[self.cursor.line].chars().count();
            self.cursor.column = self.cursor.column.min(line_len);
        }
    }

    pub fn move_to_line_start(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor.column = 0;
    }

    pub fn move_to_line_end(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        if self.cursor.line < self.lines.len() {
            self.cursor.column = self.lines[self.cursor.line].chars().count();
        }
    }

    pub fn move_word_left(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        if self.cursor.column == 0 && self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
        } else if self.cursor.line < self.lines.len() {
            self.cursor.column =
                text_edit::word_boundary_left(&self.lines[self.cursor.line], self.cursor.column);
        }
    }

    pub fn move_word_right(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        if self.cursor.line < self.lines.len() {
            let line_len = self.lines[self.cursor.line].chars().count();
            if self.cursor.column >= line_len && self.cursor.line + 1 < self.lines.len() {
                self.cursor.line += 1;
                self.cursor.column = 0;
            } else {
                self.cursor.column = text_edit::word_boundary_right(
                    &self.lines[self.cursor.line],
                    self.cursor.column,
                );
            }
        }
    }

    /// Delete word backward (Ctrl/Cmd+Backspace)
    pub fn delete_word_backward(&mut self) {
        if self.delete_selection() {
            return;
        }
        self.push_undo();
        if self.cursor.column == 0 && self.cursor.line > 0 {
            // Merge with previous line
            let current_line = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
            self.lines[self.cursor.line].push_str(&current_line);
            self.invalidate_all_highlights();
        } else if self.cursor.line < self.lines.len() {
            let new_col =
                text_edit::word_boundary_left(&self.lines[self.cursor.line], self.cursor.column);
            let line = &mut self.lines[self.cursor.line];
            let start_byte = char_to_byte_pos(line, new_col);
            let end_byte = char_to_byte_pos(line, self.cursor.column);
            line.replace_range(start_byte..end_byte, "");
            self.cursor.column = new_col;
            self.invalidate_highlight_line(self.cursor.line);
            self.sync_highlight_cache();
        }
    }

    /// Delete word forward (Ctrl/Cmd+Delete)
    pub fn delete_word_forward(&mut self) {
        if self.delete_selection() {
            return;
        }
        self.push_undo();
        if self.cursor.line < self.lines.len() {
            let line_len = self.lines[self.cursor.line].chars().count();
            if self.cursor.column >= line_len && self.cursor.line + 1 < self.lines.len() {
                let next_line = self.lines.remove(self.cursor.line + 1);
                self.lines[self.cursor.line].push_str(&next_line);
                self.invalidate_all_highlights();
            } else {
                let new_col = text_edit::word_boundary_right(
                    &self.lines[self.cursor.line],
                    self.cursor.column,
                );
                let line = &mut self.lines[self.cursor.line];
                let start_byte = char_to_byte_pos(line, self.cursor.column);
                let end_byte = char_to_byte_pos(line, new_col);
                line.replace_range(start_byte..end_byte, "");
                self.invalidate_highlight_line(self.cursor.line);
                self.sync_highlight_cache();
            }
        }
    }

    /// Detect foldable regions from bracket matching
    pub fn detect_fold_regions(&self) -> Vec<FoldRegion> {
        let mut regions = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            if self.is_line_folded(i) {
                continue;
            }
            for ch in line.chars() {
                if ch == '{' {
                    stack.push(i);
                } else if ch == '}' {
                    if let Some(start) = stack.pop() {
                        if start != i {
                            regions.push(FoldRegion {
                                start_line: start,
                                end_line: i,
                            });
                        }
                    }
                }
            }
        }
        regions
    }

    /// Check if a line is inside a folded region
    pub fn is_line_folded(&self, line: usize) -> bool {
        self.folded_regions
            .iter()
            .any(|&(start, end)| line > start && line < end)
    }

    /// Check if a line is the start of a folded region
    pub fn is_fold_start(&self, line: usize) -> bool {
        self.folded_regions.iter().any(|&(start, _)| start == line)
    }

    /// Toggle fold at a line (fold if unfolded, unfold if folded)
    pub fn toggle_fold(&mut self, line: usize) {
        // If already folded at this line, unfold
        if let Some(idx) = self
            .folded_regions
            .iter()
            .position(|&(start, _)| start == line)
        {
            self.folded_regions.remove(idx);
            return;
        }
        // Otherwise, find the fold region starting at this line and fold it
        let regions = self.detect_fold_regions();
        if let Some(region) = regions.iter().find(|r| r.start_line == line) {
            self.folded_regions
                .push((region.start_line, region.end_line));
            // Move cursor to end of fold start line if it's inside the folded region
            if self.cursor.line > region.start_line && self.cursor.line <= region.end_line {
                self.cursor.line = region.start_line;
                self.cursor.column = self.lines[region.start_line].chars().count();
            }
            self.selection_start = None;
        }
    }

    /// Get the visible line indices (skipping folded lines)
    pub fn visible_lines(&self) -> Vec<usize> {
        (0..self.lines.len())
            .filter(|&i| !self.is_line_folded(i))
            .collect()
    }

    /// Smart Home: toggle between first non-whitespace and column 0
    pub fn move_to_line_start_smart(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        if self.cursor.line < self.lines.len() {
            let line = &self.lines[self.cursor.line];
            let first_non_ws = line.chars().take_while(|c| c.is_whitespace()).count();
            if self.cursor.column == first_non_ws || first_non_ws == line.chars().count() {
                self.cursor.column = 0;
            } else {
                self.cursor.column = first_non_ws;
            }
        }
    }

    /// Page Up: move cursor up by viewport_height / line_height lines
    pub fn page_up(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        let line_height = self.config.font_size * self.config.line_height;
        let page_lines = (self.viewport_height / line_height).floor() as usize;
        self.cursor.line = self.cursor.line.saturating_sub(page_lines.max(1));
        let line_len = self.lines[self.cursor.line].chars().count();
        self.cursor.column = self.cursor.column.min(line_len);
    }

    /// Page Down: move cursor down by viewport_height / line_height lines
    pub fn page_down(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        let line_height = self.config.font_size * self.config.line_height;
        let page_lines = (self.viewport_height / line_height).floor() as usize;
        let max_line = self.lines.len().saturating_sub(1);
        self.cursor.line = (self.cursor.line + page_lines.max(1)).min(max_line);
        let line_len = self.lines[self.cursor.line].chars().count();
        self.cursor.column = self.cursor.column.min(line_len);
    }

    /// Indent selected lines (Tab with selection) or indent at cursor
    pub fn indent_lines(&mut self) {
        if let Some(sel_start) = self.selection_start {
            let (start, end) = order_positions(sel_start, self.cursor);
            self.push_undo();
            for line_idx in start.line..=end.line {
                if line_idx < self.lines.len() {
                    self.lines[line_idx] = format!("    {}", self.lines[line_idx]);
                    self.invalidate_highlight_line(line_idx);
                }
            }
            // Adjust selection and cursor columns
            self.selection_start = Some(TextPosition::new(start.line, start.column + 4));
            self.cursor = TextPosition::new(end.line, end.column + 4);
            self.sync_highlight_cache();
        } else {
            self.insert("    ");
        }
    }

    /// Dedent selected lines (Shift+Tab)
    pub fn dedent_lines(&mut self) {
        let (start_line, end_line) = if let Some(sel_start) = self.selection_start {
            let (start, end) = order_positions(sel_start, self.cursor);
            (start.line, end.line)
        } else {
            (self.cursor.line, self.cursor.line)
        };
        self.push_undo();
        for line_idx in start_line..=end_line {
            if line_idx < self.lines.len() {
                let line = &self.lines[line_idx];
                let spaces = line.chars().take(4).take_while(|c| *c == ' ').count();
                if spaces > 0 {
                    self.lines[line_idx] = self.lines[line_idx][spaces..].to_string();
                    self.invalidate_highlight_line(line_idx);
                }
            }
        }
        // Adjust cursor column
        if self.cursor.line < self.lines.len() {
            let line_len = self.lines[self.cursor.line].chars().count();
            self.cursor.column = self.cursor.column.saturating_sub(4).min(line_len);
        }
        if let Some(ref mut sel) = self.selection_start {
            sel.column = sel.column.saturating_sub(4);
        }
        self.sync_highlight_cache();
    }

    /// Execute search: find all matches of query in content.
    /// Supports plain text and regex (if query starts with /).
    pub fn execute_search(&mut self) {
        self.search_matches.clear();
        self.search_match_idx = 0;

        if self.search_query.is_empty() {
            return;
        }

        if self.search_regex {
            let case_flag = if self.search_case_sensitive {
                ""
            } else {
                "(?i)"
            };
            let pattern = format!("{}{}", case_flag, self.search_query);
            if let Ok(re) = regex::Regex::new(&pattern) {
                for (line_idx, line) in self.lines.iter().enumerate() {
                    for m in re.find_iter(line) {
                        let start_col = line[..m.start()].chars().count();
                        let end_col = line[..m.end()].chars().count();
                        self.search_matches.push((line_idx, start_col, end_col));
                    }
                }
            }
        } else {
            #[allow(clippy::type_complexity)]
            let (query_cmp, make_line_cmp): (String, Box<dyn Fn(&str) -> String>) =
                if self.search_case_sensitive {
                    (self.search_query.clone(), Box::new(|s: &str| s.to_string()))
                } else {
                    (
                        self.search_query.to_lowercase(),
                        Box::new(|s: &str| s.to_lowercase()),
                    )
                };

            for (line_idx, line) in self.lines.iter().enumerate() {
                let line_cmp = make_line_cmp(line);
                let mut search_from = 0;
                while let Some(byte_pos) = line_cmp[search_from..].find(&query_cmp) {
                    let abs_byte = search_from + byte_pos;
                    let start_col = line[..abs_byte].chars().count();
                    let end_col = start_col + self.search_query.chars().count();

                    // Whole word check
                    let is_match = if self.search_whole_word {
                        let before_ok = start_col == 0
                            || !line
                                .chars()
                                .nth(start_col - 1)
                                .map(|c| c.is_alphanumeric() || c == '_')
                                .unwrap_or(false);
                        let after_ok = end_col >= line.chars().count()
                            || !line
                                .chars()
                                .nth(end_col)
                                .map(|c| c.is_alphanumeric() || c == '_')
                                .unwrap_or(false);
                        before_ok && after_ok
                    } else {
                        true
                    };

                    if is_match {
                        self.search_matches.push((line_idx, start_col, end_col));
                    }
                    search_from = abs_byte + query_cmp.len();
                }
            }
        }
    }

    /// Jump to next search match
    pub fn search_next(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_match_idx = (self.search_match_idx + 1) % self.search_matches.len();
        let (line, col, _) = self.search_matches[self.search_match_idx];
        self.cursor = TextPosition::new(line, col);
        self.selection_start = None;
        self.ensure_cursor_visible();
    }

    /// Jump to previous search match
    pub fn search_prev(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        if self.search_match_idx == 0 {
            self.search_match_idx = self.search_matches.len() - 1;
        } else {
            self.search_match_idx -= 1;
        }
        let (line, col, _) = self.search_matches[self.search_match_idx];
        self.cursor = TextPosition::new(line, col);
        self.selection_start = None;
        self.ensure_cursor_visible();
    }

    /// Insert a character into the search query
    pub fn search_insert(&mut self, ch: char) {
        self.search_query.push(ch);
        self.execute_search();
        // Jump to nearest match from cursor
        if !self.search_matches.is_empty() {
            self.search_match_idx = self
                .search_matches
                .iter()
                .position(|&(l, c, _)| {
                    l > self.cursor.line || (l == self.cursor.line && c >= self.cursor.column)
                })
                .unwrap_or(0);
            let (line, col, _) = self.search_matches[self.search_match_idx];
            self.cursor = TextPosition::new(line, col);
            self.ensure_cursor_visible();
        }
    }

    /// Delete last character from search query
    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        self.execute_search();
    }

    /// Replace the current search match with replacement text
    pub fn replace_current(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        let (line, col_start, col_end) = self.search_matches[self.search_match_idx];
        self.push_undo();

        let line_text = &mut self.lines[line];
        let start_byte = char_to_byte_pos(line_text, col_start);
        let end_byte = char_to_byte_pos(line_text, col_end);
        line_text.replace_range(start_byte..end_byte, &self.replace_text);

        self.invalidate_all_highlights();
        self.execute_search();
        if !self.search_matches.is_empty() {
            self.search_match_idx = self.search_match_idx.min(self.search_matches.len() - 1);
            let (l, c, _) = self.search_matches[self.search_match_idx];
            self.cursor = TextPosition::new(l, c);
            self.ensure_cursor_visible();
        }
    }

    /// Replace all search matches with replacement text
    pub fn replace_all(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.push_undo();

        // Replace from last to first to preserve earlier positions
        for &(line, col_start, col_end) in self.search_matches.iter().rev() {
            let line_text = &mut self.lines[line];
            let start_byte = char_to_byte_pos(line_text, col_start);
            let end_byte = char_to_byte_pos(line_text, col_end);
            line_text.replace_range(start_byte..end_byte, &self.replace_text);
        }

        self.invalidate_all_highlights();
        self.search_matches.clear();
        self.search_match_idx = 0;
    }

    /// Position cursor from click coordinates
    pub fn cursor_from_click(&mut self, x: f32, y: f32) {
        let line_height = self.config.font_size * self.config.line_height;
        let visible_row = (y / line_height).floor() as usize;
        // Map visible row to actual line index (accounts for folded lines)
        let visible = self.visible_lines();
        let line_idx = visible
            .get(visible_row)
            .copied()
            .unwrap_or(visible.last().copied().unwrap_or(0));

        let line = &self.lines[line_idx];
        let char_count = line.chars().count();
        let mut best_col = char_count;
        for col in 0..=char_count {
            let text_before: String = line.chars().take(col).collect();
            let w = self.measure_mono(&text_before);
            if w >= x {
                if col > 0 {
                    let prev: String = line.chars().take(col - 1).collect();
                    let prev_w = self.measure_mono(&prev);
                    best_col = if (x - prev_w).abs() < (x - w).abs() {
                        col - 1
                    } else {
                        col
                    };
                } else {
                    best_col = 0;
                }
                break;
            }
        }

        self.cursor = TextPosition::new(line_idx, best_col);
        self.selection_start = None;
    }

    /// Get styled content without cache mutation (for minimap etc.)
    fn get_styled_content_readonly(&self) -> StyledText {
        if let Some(ref highlighter) = self.highlighter {
            highlighter.highlight(&self.value())
        } else {
            StyledText::plain(&self.value(), self.config.text_color)
        }
    }

    /// Total content height in pixels (includes padding)
    pub fn content_height(&self) -> f32 {
        let line_height = self.config.font_size * self.config.line_height;
        let pad = self.config.padding;
        self.lines.len() as f32 * line_height + pad * 2.0
    }

    /// Ensure cursor is within the visible scroll viewport.
    /// Only scrolls when cursor would be outside the visible area.
    pub fn ensure_cursor_visible(&mut self) {
        let line_height = self.config.font_size * self.config.line_height;
        let pad = self.config.padding;
        if self.viewport_height <= 0.0 {
            return;
        }

        // Cursor Y in scroll content coordinates (includes top padding)
        let cursor_y = self.cursor.line as f32 * line_height + pad;
        let cursor_bottom = cursor_y + line_height;

        let mut physics = self.scroll_physics.lock().unwrap();
        let current_offset = -physics.offset_y;
        let visible_bottom = current_offset + self.viewport_height;

        // Only scroll if cursor is outside the visible range
        if cursor_y >= current_offset && cursor_bottom <= visible_bottom {
            return; // Cursor is visible, no scroll needed
        }

        let mut new_offset = current_offset;
        if cursor_y < current_offset {
            new_offset = cursor_y;
        }
        if cursor_bottom > visible_bottom {
            new_offset = cursor_bottom - self.viewport_height;
        }

        let max_scroll = (self.content_height() - self.viewport_height).max(0.0);
        new_offset = new_offset.clamp(0.0, max_scroll);
        physics.offset_y = -new_offset;
    }

    /// Reset cursor blink
    pub fn reset_cursor_blink(&self) {
        if let Ok(mut cs) = self.cursor_state.lock() {
            cs.reset_blink();
        }
    }

    /// Get styled content with syntax highlighting (uses per-line cache)
    fn get_styled_content(&mut self) -> StyledText {
        self.sync_highlight_cache();

        if let Some(ref highlighter) = self.highlighter {
            let mut styled_lines = Vec::with_capacity(self.lines.len());
            for (i, line) in self.lines.iter().enumerate() {
                if let Some(ref cached) = self.highlight_cache[i] {
                    styled_lines.push(cached.clone());
                } else {
                    // Highlight single line
                    let line_styled = highlighter.highlight(line);
                    let styled_line = line_styled.lines.into_iter().next().unwrap_or_else(|| {
                        crate::styled_text::StyledLine {
                            text: line.clone(),
                            spans: Vec::new(),
                        }
                    });
                    self.highlight_cache[i] = Some(styled_line.clone());
                    styled_lines.push(styled_line);
                }
            }
            StyledText {
                lines: styled_lines,
            }
        } else {
            StyledText::plain(&self.value(), self.config.text_color)
        }
    }
}

/// Convert character position to byte position
fn char_to_byte_pos(s: &str, char_pos: usize) -> usize {
    s.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// Order two positions (earlier, later)
fn order_positions(a: TextPosition, b: TextPosition) -> (TextPosition, TextPosition) {
    if a.line < b.line || (a.line == b.line && a.column <= b.column) {
        (a, b)
    } else {
        (b, a)
    }
}

// ============================================================================
// Read-only Code Widget
// ============================================================================

/// Read-only code block widget
///
/// For editable code, use `code_editor()` instead.
pub struct Code {
    inner: Div,
    content: String,
    config: CodeConfig,
    highlighter: Option<Arc<dyn SyntaxHighlighter>>,
}

impl Deref for Code {
    type Target = Div;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Code {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Code {
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        let config = CodeConfig::default();
        let mut code = Self {
            inner: Div::new(),
            content,
            config,
            highlighter: None,
        };
        code.rebuild_inner();
        code
    }

    fn rebuild_inner(&mut self) {
        self.inner = self.create_visual_structure();
    }

    pub fn line_numbers(mut self, enabled: bool) -> Self {
        self.config.line_numbers = enabled;
        self.rebuild_inner();
        self
    }

    pub fn syntax(mut self, config: SyntaxConfig) -> Self {
        let bg_color = config.highlighter().background_color();
        let text_color = config.highlighter().default_color();
        let line_number_color = config.highlighter().line_number_color();
        self.highlighter = Some(config.into_arc());
        self.config.bg_color = bg_color;
        self.config.text_color = text_color;
        self.config.line_number_color = line_number_color;
        self.rebuild_inner();
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.config.font_size = size;
        self.rebuild_inner();
        self
    }

    pub fn line_height(mut self, multiplier: f32) -> Self {
        self.config.line_height = multiplier;
        self.rebuild_inner();
        self
    }

    pub fn padding(mut self, padding: f32) -> Self {
        self.config.padding = padding;
        self.rebuild_inner();
        self
    }

    pub fn code_bg(mut self, color: Color) -> Self {
        self.config.bg_color = color;
        self
    }

    pub fn text_color(mut self, color: Color) -> Self {
        self.config.text_color = color;
        self
    }

    fn get_styled_content(&self) -> StyledText {
        if let Some(ref highlighter) = self.highlighter {
            highlighter.highlight(&self.content)
        } else {
            StyledText::plain(&self.content, self.config.text_color)
        }
    }

    fn create_visual_structure(&self) -> Div {
        let styled = self.get_styled_content();
        let line_height_px = self.config.font_size * self.config.line_height;
        let num_lines = styled.line_count().max(1);

        let mut container = div()
            .flex_row()
            .bg(self.config.bg_color)
            .rounded(self.config.corner_radius)
            .overflow_clip();

        if self.config.line_numbers {
            let visible: Vec<usize> = (0..num_lines).collect();
            container = container.child(build_gutter(
                &visible,
                line_height_px,
                &self.config,
                &[],
                &[],
                self.config.padding,
            ));
        }

        let mut code_area = div()
            .flex_col()
            .flex_grow()
            .padding_x_px(self.config.padding)
            .padding_y_px(self.config.padding);

        for styled_line in &styled.lines {
            code_area =
                code_area.child(build_styled_line(styled_line, &self.config, line_height_px));
        }

        container.child(code_area)
    }

    // Shadowed Div methods
    pub fn w(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).w(px);
        self
    }
    pub fn h(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).h(px);
        self
    }
    pub fn w_full(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).w_full();
        self
    }
    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = radius;
        self
    }
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.inner = std::mem::take(&mut self.inner).border(width, color);
        self
    }
    pub fn m(mut self, value: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).m(value);
        self
    }
    pub fn mt(mut self, value: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).mt(value);
        self
    }
    pub fn mb(mut self, value: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).mb(value);
        self
    }
}

impl ElementBuilder for Code {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }
    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }
    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }
    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("code")
    }
    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }
    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }
}

// ============================================================================
// Editable Code Editor Widget (Stateful, incremental updates)
// ============================================================================

/// Editable code editor widget using Stateful for incremental updates
pub struct CodeEditor {
    inner: crate::stateful::Stateful<crate::stateful::TextFieldState>,
    state: SharedCodeEditorState,
    /// Unique instance key for state persistence
    instance_key: String,
}

impl CodeEditor {
    #[track_caller]
    pub fn new(state: &SharedCodeEditorState) -> Self {
        use crate::stateful::{
            SharedState, StateTransitions, Stateful, StatefulInner, TextFieldState,
            refresh_stateful,
        };
        use blinc_core::events::event_types;

        let instance_key = crate::InstanceKey::new("code_editor").get().to_string();
        let state_key = format!("_code_editor_{}", instance_key);
        let shared_state = crate::stateful::use_fsm_keyed::<_, TextFieldState>(
            &state_key,
            TextFieldState::default(),
        );

        let data_for_click = Arc::clone(state);
        let data_for_drag = Arc::clone(state);
        let data_for_key = Arc::clone(state);
        let data_for_text = Arc::clone(state);
        let shared_for_click = Arc::clone(&shared_state);
        let shared_for_drag = Arc::clone(&shared_state);
        let shared_for_key = Arc::clone(&shared_state);
        let shared_for_text = Arc::clone(&shared_state);

        let mut inner = Stateful::with_shared_state(Arc::clone(&shared_state))
            .on_mouse_down(move |ctx| {
                let click_x = ctx.local_x;
                let click_y = ctx.local_y;

                {
                    let mut d = match data_for_click.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    // Store editor bounds for overlay positioning
                    d.editor_bounds = (
                        ctx.bounds_x,
                        ctx.bounds_y,
                        ctx.bounds_width,
                        ctx.bounds_height,
                    );

                    // Focus via FSM
                    {
                        let mut shared = shared_for_click.lock().unwrap();
                        if !shared.state.is_focused() {
                            if let Some(new_state) = shared
                                .state
                                .on_event(event_types::POINTER_DOWN)
                                .or_else(|| shared.state.on_event(event_types::FOCUS))
                            {
                                shared.state = new_state;
                                shared.needs_visual_update = true;
                            }
                        }
                    }

                    if !d.focused {
                        d.focused = true;
                        increment_focus_count();
                        request_continuous_redraw_pub();
                    }

                    // Bump the focus-tap generation counter and
                    // register this node id as the generic focused
                    // editable so the mobile runner's
                    // scroll-into-view helper picks this up. See
                    // `text_input::focus_tap_generation` for the
                    // rationale and `RenderTree::scroll_focused_text_input_above_keyboard`
                    // for the consumer.
                    //
                    // The blur callback captures `state` so
                    // `blur_all_text_inputs` (called when the user
                    // taps outside any editable widget) can flip
                    // `d.focused = false`, hide the cursor, and
                    // decrement the focus count — which dismisses
                    // the soft keyboard. Without this, tapping
                    // outside the editor would leave it visually
                    // focused with the keyboard still up.
                    crate::widgets::text_input::bump_focus_tap_generation();
                    let blur_state = Arc::clone(&data_for_click);
                    crate::widgets::text_input::set_focused_editable_node(
                        ctx.node_id,
                        Some(Box::new(move || {
                            if let Ok(mut d) = blur_state.lock() {
                                if d.focused {
                                    d.focused = false;
                                    d.selection_start = None;
                                    if let Ok(mut cs) = d.cursor_state.lock() {
                                        cs.visible = false;
                                    }
                                    decrement_focus_count();
                                }
                            }
                        })),
                    );

                    // Don't close search on code click — use Escape or close button

                    // Account for gutter, padding, and scroll in click coordinates
                    let gutter_w = if d.config.line_numbers || d.config.code_folding {
                        d.config.gutter_width
                    } else {
                        0.0
                    };
                    let scroll_offset = d.scroll_physics.lock().map(|p| -p.offset_y).unwrap_or(0.0);
                    let adjusted_y = (click_y - d.config.padding + scroll_offset).max(0.0);

                    // Check if click is in the gutter → toggle fold only, skip cursor logic
                    if d.config.code_folding && click_x < gutter_w {
                        let line_height = d.config.font_size * d.config.line_height;
                        let visible_row = (adjusted_y / line_height).floor() as usize;
                        let visible = d.visible_lines();
                        let line_idx = visible
                            .get(visible_row)
                            .copied()
                            .unwrap_or(visible.last().copied().unwrap_or(0));
                        d.toggle_fold(line_idx);
                        d.invalidate_all_highlights();
                        // Don't touch cursor, selection, or drag state
                    } else {
                        // Code area click — position cursor
                        let adjusted_x = (click_x - gutter_w - d.config.padding).max(0.0);
                        d.cursor_from_click(adjusted_x, adjusted_y);

                        // Double-click detection: select word
                        let now = web_time::SystemTime::now()
                            .duration_since(web_time::UNIX_EPOCH)
                            .map(|t| t.as_secs_f64() * 1000.0)
                            .unwrap_or(0.0);
                        let is_double_click =
                            (now - d.last_click_time) < 350.0 && d.last_click_pos == d.cursor;
                        d.last_click_time = now;
                        d.last_click_pos = d.cursor;

                        let touch = crate::widgets::text_input::is_touch_input();

                        if is_double_click && d.cursor.line < d.lines.len() {
                            let line = &d.lines[d.cursor.line];
                            let (start, end) = text_edit::word_at_position(
                                line,
                                d.cursor.column.min(line.chars().count().saturating_sub(1)),
                            );
                            d.selection_start = Some(TextPosition::new(d.cursor.line, start));
                            d.cursor.column = end;
                            d.drag_anchor = None;
                            // On touch: heavier impact + show native
                            // edit menu over the selected word.
                            if touch {
                                crate::widgets::text_edit::haptic_impact_light();
                                use crate::widgets::text_edit::edit_menu_actions;
                                crate::widgets::text_edit::show_edit_menu(
                                    ctx.bounds_x + click_x,
                                    ctx.bounds_y + click_y,
                                    ctx.bounds_x + click_x,
                                    ctx.bounds_y + click_y,
                                    0.0,
                                    d.config.font_size * d.config.line_height,
                                    edit_menu_actions::CUT
                                        | edit_menu_actions::COPY
                                        | edit_menu_actions::PASTE
                                        | edit_menu_actions::SELECT_ALL,
                                );
                            }
                        } else if touch {
                            // Single touch: position cursor only,
                            // no drag anchor, light haptic. Also
                            // arm the long-press timer so a press-
                            // and-hold of 500 ms selects the word
                            // under the finger and shows the edit
                            // menu — matches the iOS UITextField
                            // long-press UX and stays consistent
                            // with the double-tap behavior above.
                            d.drag_anchor = None;
                            let line_height = d.config.font_size * d.config.line_height;
                            let captured_cursor = d.cursor;
                            crate::widgets::text_edit::haptic_selection();
                            crate::widgets::text_edit::hide_edit_menu();
                            let data_for_long_press = Arc::clone(&data_for_click);
                            let shared_for_long_press = Arc::clone(&shared_for_click);
                            crate::widgets::text_input::arm_long_press_timer(
                                ctx.bounds_x + click_x,
                                ctx.bounds_y + click_y,
                                line_height,
                                Some(Box::new(move || {
                                    let did_update = {
                                        let mut d = match data_for_long_press.lock() {
                                            Ok(d) => d,
                                            Err(_) => return,
                                        };
                                        if !d.focused || captured_cursor.line >= d.lines.len() {
                                            return;
                                        }
                                        let line = &d.lines[captured_cursor.line];
                                        let line_chars = line.chars().count();
                                        if line_chars == 0 {
                                            return;
                                        }
                                        let col = captured_cursor
                                            .column
                                            .min(line_chars.saturating_sub(1));
                                        let (start, end) = text_edit::word_at_position(line, col);
                                        if start == end {
                                            return;
                                        }
                                        d.selection_start =
                                            Some(TextPosition::new(captured_cursor.line, start));
                                        d.cursor = TextPosition::new(captured_cursor.line, end);
                                        d.drag_anchor = None;
                                        true
                                    };
                                    if did_update {
                                        refresh_stateful(&shared_for_long_press);
                                    }
                                })),
                            );
                        } else {
                            d.drag_anchor = Some(d.cursor);
                        }
                        d.reset_cursor_blink();
                    }
                }

                refresh_stateful(&shared_for_click);
            })
            .on_event(event_types::DRAG, move |ctx| {
                let mut d = match data_for_drag.lock() {
                    Ok(d) => d,
                    Err(_) => return,
                };
                if !d.focused {
                    return;
                }

                let gutter_offset = if d.config.line_numbers {
                    d.config.gutter_width
                } else {
                    0.0
                };
                let adjusted_x = (ctx.local_x - gutter_offset - d.config.padding).max(0.0);
                let scroll_offset = d.scroll_physics.lock().map(|p| -p.offset_y).unwrap_or(0.0);
                let adjusted_y = (ctx.local_y - d.config.padding + scroll_offset).max(0.0);

                // Move cursor to drag position (selection_start stays at mouse-down position)
                let line_height = d.config.font_size * d.config.line_height;
                let line_idx = ((adjusted_y / line_height).floor() as usize)
                    .min(d.lines.len().saturating_sub(1));
                let line = d.lines[line_idx].clone();
                let char_count = line.chars().count();
                let mut best_col = char_count;
                for col in 0..=char_count {
                    let text_before: String = line.chars().take(col).collect();
                    let w = d.measure_mono(&text_before);
                    if w >= adjusted_x {
                        if col > 0 {
                            let prev: String = line.chars().take(col - 1).collect();
                            let prev_w = d.measure_mono(&prev);
                            best_col = if (adjusted_x - prev_w).abs() < (adjusted_x - w).abs() {
                                col - 1
                            } else {
                                col
                            };
                        } else {
                            best_col = 0;
                        }
                        break;
                    }
                }
                let new_cursor = TextPosition::new(line_idx, best_col);

                if crate::widgets::text_input::is_touch_input() {
                    // Touch drag = move cursor without starting a
                    // selection. Each step gets a subtle haptic so
                    // the user feels the cursor traveling between
                    // characters / lines, matching the iOS
                    // UITextField cursor-drag UX.
                    //
                    // Also drift-cancel the long-press timer so a
                    // real cursor drag doesn't also fire the paste
                    // menu mid-gesture.
                    crate::widgets::text_input::check_long_press_drift(ctx.mouse_x, ctx.mouse_y);
                    if new_cursor != d.cursor {
                        d.cursor = new_cursor;
                        d.selection_start = None;
                        d.drag_anchor = None;
                        crate::widgets::text_edit::haptic_selection();
                    }
                } else {
                    d.cursor = new_cursor;
                    // Set selection from drag anchor to current cursor
                    if let Some(anchor) = d.drag_anchor {
                        if anchor != d.cursor {
                            d.selection_start = Some(anchor);
                        } else {
                            d.selection_start = None;
                        }
                    }
                }

                drop(d);
                crate::stateful::refresh_stateful(&shared_for_drag);
            })
            .on_event(event_types::TEXT_INPUT, move |ctx| {
                let needs_refresh = {
                    let mut d = match data_for_text.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    if !d.focused {
                        return;
                    }
                    // Skip control characters and modifier combos (Cmd+C/V/Z etc.)
                    if ctx.meta || ctx.ctrl {
                        return;
                    }
                    if let Some(c) = ctx.key_char {
                        if c.is_control() && c != '\t' {
                            return;
                        }
                        if d.search_active {
                            // text_input widget handles search input;
                            // don't insert into code while searching
                        } else {
                            d.insert(&c.to_string());
                            d.reset_cursor_blink();
                            d.ensure_cursor_visible();
                            if let Some(ref cb) = d.on_change {
                                cb(&d.value());
                            }
                        }
                        true
                    } else {
                        false
                    }
                };
                if needs_refresh {
                    refresh_stateful(&shared_for_text);
                }
            })
            .on_key_down(move |ctx| {
                let needs_refresh = {
                    let mut d = match data_for_key.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    if !d.focused {
                        return;
                    }

                    // When search is active, only allow specific keys through
                    if d.search_active {
                        let mod_key = ctx.meta || ctx.ctrl;
                        if !mod_key {
                            if ctx.key_code == 27 {
                                close_search_overlay(&mut d);
                                drop(d);
                                refresh_stateful(&shared_for_key);
                            }
                            return;
                        }
                        // Only allow Cmd+F, Cmd+G, Cmd+H, Cmd+R through to code editor
                        // Block everything else (Cmd+A, Cmd+C etc. go to text_input)
                        match ctx.key_code {
                            70 | 71 | 72 | 82 => {} // F, G, H, R — fall through
                            _ => return,
                        }
                    }

                    let mut cursor_changed = true;
                    let mut text_changed = false;
                    let mut needs_visual_refresh = false;

                    // Platform modifier: Cmd on macOS, Ctrl elsewhere
                    let mod_key = ctx.meta || ctx.ctrl;

                    match ctx.key_code {
                        8 => {
                            // Backspace / Cmd+Backspace
                            if mod_key {
                                d.delete_word_backward();
                            } else {
                                d.delete_backward();
                            }
                            text_changed = true;
                        }
                        127 => {
                            // Delete / Cmd+Delete
                            if mod_key {
                                d.delete_word_forward();
                            } else {
                                d.delete_forward();
                            }
                            text_changed = true;
                        }
                        13 => {
                            d.insert("\n");
                            text_changed = true;
                        }
                        37 => {
                            // Left / Cmd+Left
                            if mod_key {
                                d.move_word_left(ctx.shift);
                            } else {
                                d.move_left(ctx.shift);
                            }
                        }
                        39 => {
                            // Right / Cmd+Right
                            if mod_key {
                                d.move_word_right(ctx.shift);
                            } else {
                                d.move_right(ctx.shift);
                            }
                        }
                        38 => {
                            // Up
                            d.move_up(ctx.shift);
                        }
                        40 => {
                            // Down
                            d.move_down(ctx.shift);
                        }
                        36 => {
                            // Home — smart home
                            d.move_to_line_start_smart(ctx.shift);
                        }
                        35 => d.move_to_line_end(ctx.shift),
                        33 => {
                            // Page Up
                            d.page_up(ctx.shift);
                        }
                        34 => {
                            // Page Down
                            d.page_down(ctx.shift);
                        }
                        9 => {
                            // Tab / Shift+Tab
                            if ctx.shift {
                                d.dedent_lines();
                            } else {
                                d.indent_lines();
                            }
                            text_changed = true;
                        }
                        27 => {
                            // Escape — blur editor
                            d.focused = false;
                            d.selection_start = None;
                            if let Ok(mut cs) = d.cursor_state.lock() {
                                cs.visible = false;
                            }
                            decrement_focus_count();
                            crate::widgets::text_input::clear_focused_editable_node();
                        }
                        _ => {
                            // Check for Cmd+key combos
                            if mod_key {
                                match ctx.key_code {
                                    // A = Select All (visual refresh only, no scroll)
                                    65 => {
                                        d.select_all();
                                        cursor_changed = false;
                                        needs_visual_refresh = true;
                                    }
                                    // C = Copy
                                    67 => {
                                        if let Some(selected) = d.selected_text() {
                                            text_edit::clipboard_write(&selected);
                                        }
                                        cursor_changed = false;
                                    }
                                    // X = Cut
                                    88 => {
                                        if let Some(selected) = d.selected_text() {
                                            text_edit::clipboard_write(&selected);
                                            d.delete_selection();
                                            text_changed = true;
                                        }
                                    }
                                    // V = Paste
                                    86 => {
                                        if let Some(clip) = text_edit::clipboard_read() {
                                            d.insert(&clip);
                                            text_changed = true;
                                        }
                                    }
                                    // F = Find (toggle search bar overlay)
                                    70 if d.config.search_enabled => {
                                        if d.search_active {
                                            // Close search
                                            close_search_overlay(&mut d);
                                        } else {
                                            d.search_active = true;
                                        }
                                        needs_visual_refresh = true;
                                        cursor_changed = false;
                                    }
                                    // G = Next match / Shift+G = Previous match
                                    71 if d.config.search_enabled => {
                                        if d.search_active {
                                            if ctx.shift {
                                                d.search_prev();
                                            } else {
                                                d.search_next();
                                            }
                                        }
                                    }
                                    // H = Toggle replace bar
                                    72 if d.config.search_enabled => {
                                        if !d.search_active {
                                            d.search_active = true;
                                        }
                                        d.replace_active = !d.replace_active;
                                        needs_visual_refresh = true;
                                        cursor_changed = false;
                                    }
                                    // R = Toggle regex mode (when search active)
                                    82 if d.config.search_enabled => {
                                        if d.search_active {
                                            d.search_regex = !d.search_regex;
                                            d.execute_search();
                                            needs_visual_refresh = true;
                                            cursor_changed = false;
                                        }
                                    }
                                    // Z = Undo / Shift+Z = Redo
                                    90 => {
                                        if ctx.shift {
                                            d.redo();
                                        } else {
                                            d.undo();
                                        }
                                        text_changed = true;
                                    }
                                    _ => {
                                        cursor_changed = false;
                                    }
                                }
                            } else {
                                cursor_changed = false;
                            }
                        }
                    }

                    if cursor_changed {
                        d.reset_cursor_blink();
                        d.ensure_cursor_visible();
                    }
                    if text_changed {
                        if let Some(ref cb) = d.on_change {
                            cb(&d.value());
                        }
                    }
                    let should_open_search = d.search_active && d.search_overlay_handle.is_none();
                    let needs_editor_refresh =
                        cursor_changed || text_changed || needs_visual_refresh;
                    (needs_editor_refresh, should_open_search)
                };
                if needs_refresh.1 {
                    // Open search overlay — don't refresh editor for this
                    open_search_overlay(&data_for_key, &format!("{}:search", "code_editor"));
                } else if needs_refresh.0 {
                    refresh_stateful(&shared_for_key);
                }
            })
            .on_blur(move |_ctx| {
                // Blur handled by FSM transition
            })
            .overflow_y_scroll()
            .relative()
            .cursor_text();

        // Register the state callback that builds visual content
        {
            let data_for_callback = Arc::clone(state);
            let instance_key_for_cb = instance_key.clone();
            let mut shared = shared_state.lock().unwrap();
            shared.state_callback = Some(Arc::new(
                move |visual: &crate::stateful::TextFieldState, container: &mut Div| {
                    let instance_key = &instance_key_for_cb;
                    let mut data = data_for_callback.lock().unwrap();
                    let cw = data.char_width();
                    // Sync search query from text_input state
                    if data.search_active {
                        let new_query = data
                            .search_input_state
                            .lock()
                            .map(|d| d.value.clone())
                            .unwrap_or_default();
                        if new_query != data.search_query {
                            data.search_query = new_query;
                            data.execute_search();
                            // Jump to nearest match from cursor
                            if !data.search_matches.is_empty() {
                                data.search_match_idx = data
                                    .search_matches
                                    .iter()
                                    .position(|&(l, c, _)| {
                                        l > data.cursor.line
                                            || (l == data.cursor.line && c >= data.cursor.column)
                                    })
                                    .unwrap_or(0);
                                let (line, col, _) = data.search_matches[data.search_match_idx];
                                data.cursor = TextPosition::new(line, col);
                                data.selection_start = None;
                                data.ensure_cursor_visible();
                            }
                        }
                        // Sync replace text from text_input
                        if data.replace_active {
                            data.replace_text = data
                                .replace_input_state
                                .lock()
                                .map(|d| d.value.clone())
                                .unwrap_or_default();
                        }
                    }
                    let content = build_editor_content(
                        &mut data,
                        visual.is_focused(),
                        cw,
                        Some(&data_for_callback),
                        instance_key,
                    );

                    // Apply visual styling
                    container.set_bg(data.config.bg_color);
                    container.set_rounded(data.config.corner_radius);

                    container.set_child(content);
                },
            ));
            shared.needs_visual_update = true;
        }

        inner.ensure_state_handlers_registered();

        // Store the Stateful's scroll physics in CodeEditorData so
        // ensure_cursor_visible can programmatically scroll
        if let Some(ref physics) = inner.inner_scroll_physics() {
            state.lock().unwrap().scroll_physics = Arc::clone(physics);
        }

        Self {
            inner,
            state: Arc::clone(state),
            instance_key,
        }
    }

    // Builder methods that update shared state config
    pub fn line_numbers(self, enabled: bool) -> Self {
        self.state.lock().unwrap().config.line_numbers = enabled;
        self
    }

    pub fn syntax(self, syntax_config: SyntaxConfig) -> Self {
        let bg = syntax_config.highlighter().background_color();
        let text_col = syntax_config.highlighter().default_color();
        let ln = syntax_config.highlighter().line_number_color();
        let hl = syntax_config.into_arc();
        {
            let mut d = self.state.lock().unwrap();
            d.highlighter = Some(hl);
            d.config.bg_color = bg;
            d.config.text_color = text_col;
            d.config.line_number_color = ln;
        }
        self
    }

    pub fn font_size(self, size: f32) -> Self {
        self.state.lock().unwrap().config.font_size = size;
        self
    }

    /// Width (px) of the gutter column that hosts line numbers and
    /// fold icons. Defaults to 48 px — enough for 3-4 digit line
    /// numbers. Editors hosting short snippets (popovers, inline
    /// node-content) can drop this to 28-32 px so the line-number
    /// column doesn't look like dead space next to a 4-line
    /// script. Has no effect when `line_numbers(false)`.
    pub fn gutter_width(self, px: f32) -> Self {
        self.state.lock().unwrap().config.gutter_width = px;
        self
    }

    /// Enable the Cmd+F / Cmd+G / Cmd+H / Cmd+R find / replace
    /// keybindings + search-bar overlay. Disabled by default
    /// because the search bar is pushed via the legacy overlay
    /// manager (a sibling top-level overlay, not nested under the
    /// host's content), and clicks inside it dismiss the host
    /// popover through the click_outside registry. Opt in for
    /// hosts that own the full viewport; leave off when embedding
    /// inside a `cn::popover` or any host overlay that uses
    /// click_outside dismissal — the keybindings then no-op so
    /// host shortcuts (Cmd+F = browser find on web, etc.) pass
    /// through cleanly.
    pub fn search_enabled(self, enabled: bool) -> Self {
        self.state.lock().unwrap().config.search_enabled = enabled;
        self
    }

    pub fn line_height(self, multiplier: f32) -> Self {
        self.state.lock().unwrap().config.line_height = multiplier;
        self
    }

    pub fn padding(self, padding: f32) -> Self {
        self.state.lock().unwrap().config.padding = padding;
        self
    }

    pub fn on_change<F: Fn(&str) + Send + Sync + 'static>(self, callback: F) -> Self {
        self.state.lock().unwrap().on_change = Some(Arc::new(callback));
        self
    }

    pub fn code_bg(self, color: Color) -> Self {
        self.state.lock().unwrap().config.bg_color = color;
        self
    }

    pub fn text_color(self, color: Color) -> Self {
        self.state.lock().unwrap().config.text_color = color;
        self
    }

    /// Show minimap on the right side
    pub fn minimap(self, enabled: bool) -> Self {
        self.state.lock().unwrap().config.minimap = enabled;
        self
    }

    /// Show indentation guides
    pub fn indent_guides(self, enabled: bool) -> Self {
        self.state.lock().unwrap().config.indent_guides = enabled;
        self
    }

    /// Enable code folding (collapsible blocks)
    pub fn code_folding(self, enabled: bool) -> Self {
        self.state.lock().unwrap().config.code_folding = enabled;
        self
    }

    // Shadowed Div methods
    pub fn w(mut self, px: f32) -> Self {
        self.inner = self.inner.w(px);
        self
    }
    pub fn h(mut self, px: f32) -> Self {
        self.inner = self.inner.h(px);
        // Update viewport height for scroll calculations
        let (vh, physics) = {
            let mut d = self.state.lock().unwrap();
            let vh = (px - d.config.padding * 2.0).max(0.0);
            d.viewport_height = vh;
            (vh, Arc::clone(&d.scroll_physics))
        };
        if let Ok(mut p) = physics.lock() {
            p.viewport_height = vh;
        }
        self
    }
    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.inner = self.inner.border(width, color);
        self
    }
    pub fn rounded(self, radius: f32) -> Self {
        self.state.lock().unwrap().config.corner_radius = radius;
        self
    }
    pub fn m(mut self, value: f32) -> Self {
        self.inner = self.inner.m(value);
        self
    }
    pub fn mt(mut self, value: f32) -> Self {
        self.inner = self.inner.mt(value);
        self
    }
    pub fn mb(mut self, value: f32) -> Self {
        self.inner = self.inner.mb(value);
        self
    }
}

impl ElementBuilder for CodeEditor {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        {
            let shared_state = self.inner.shared_state();
            let mut shared = shared_state.lock().unwrap();
            shared.base_render_props = Some(self.inner.inner_render_props());
            shared.base_style = self.inner.inner_layout_style();
        }
        self.inner.build(tree)
    }
    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }
    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }
    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("code-editor")
    }
    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }
    fn scroll_physics(&self) -> Option<crate::widgets::scroll::SharedScrollPhysics> {
        self.inner.scroll_physics()
    }
    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }
}

// ============================================================================
// Shared Visual Building Helpers
// ============================================================================

/// SVG for fold chevron (right-pointing = collapsed)
const FOLD_COLLAPSED_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M6 3l5 5-5 5z" fill="currentColor"/></svg>"#;
/// SVG for fold chevron (down-pointing = expanded)
const FOLD_EXPANDED_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M3 6l5 5 5-5z" fill="currentColor"/></svg>"#;

fn build_gutter(
    visible_lines: &[usize],
    line_height_px: f32,
    config: &CodeConfig,
    fold_regions: &[FoldRegion],
    folded_starts: &[usize],
    pad: f32,
) -> Div {
    let has_fold = config.code_folding;

    let mut col = div().flex_col().justify_start().padding_y_px(pad);

    for &line_idx in visible_lines {
        let mut row = div().h(line_height_px).flex_row().items_center();

        // Line number — monospace, right-aligned, flex_grow to fill available space
        row = row.child(
            div().flex_grow().flex_row().justify_end().pr(1.0).child(
                text(format!("{}", line_idx + 1))
                    .size(config.font_size)
                    .color(config.line_number_color)
                    .monospace()
                    .no_wrap(),
            ),
        );

        // Fold icon (SVG element)
        if has_fold {
            let is_fold_point = fold_regions.iter().any(|r| r.start_line == line_idx);
            let is_folded = folded_starts.contains(&line_idx);
            if is_fold_point {
                let svg_src = if is_folded {
                    FOLD_COLLAPSED_SVG
                } else {
                    FOLD_EXPANDED_SVG
                };
                let icon_size = config.font_size;
                let icon_w = config.font_size;
                row = row.child(
                    div()
                        .w(icon_w)
                        .h(line_height_px)
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .child(
                            crate::svg::svg(svg_src)
                                .size(icon_size, icon_size)
                                .tint(config.line_number_color),
                        ),
                );
            } else {
                row = row.child(div().w(config.font_size));
            }
        }

        col = col.child(row);
    }

    div()
        .flex_col()
        .w(config.gutter_width)
        .flex_shrink_0()
        .child(col)
        .child(div().flex_grow()) // fill remaining height (no bg)
}

fn build_styled_line(
    styled_line: &crate::styled_text::StyledLine,
    config: &CodeConfig,
    line_height_px: f32,
) -> Div {
    let mut line_div = div().h(line_height_px).flex_row().items_center();

    if styled_line.spans.is_empty() {
        line_div = line_div.child(text(" ").size(config.font_size).color(config.text_color));
    } else {
        for span in &styled_line.spans {
            let span_text = &styled_line.text[span.start..span.end];
            let mut txt = text(span_text)
                .size(config.font_size)
                .color(span.color)
                .no_wrap();
            if span.bold {
                txt = txt.bold();
            }
            txt = txt.monospace();
            line_div = line_div.child(txt);
        }
    }

    line_div
}

/// Build a minimap — a scaled-down overview of all code lines
/// Build a minimap widget from code editor state.
/// This should be placed as a sibling OUTSIDE the code_editor, not inside it.
pub fn code_minimap(state: &SharedCodeEditorState) -> Div {
    let data = state.lock().unwrap();
    let line_height_px = data.config.font_size * data.config.line_height;
    build_minimap(&data, line_height_px)
}

fn build_minimap(data: &CodeEditorData, line_height_px: f32) -> Div {
    let config = &data.config;
    let minimap_line_h = 2.0_f32; // Each line is 2px tall in minimap
    let visible_lines = data.visible_lines();
    let total_h = visible_lines.len() as f32 * minimap_line_h;

    let mut minimap = div()
        .flex_col()
        .w(config.minimap_width)
        .bg(Color::rgba(0.0, 0.0, 0.0, 0.15))
        .padding_y_px(4.0)
        .padding_x_px(2.0);

    // Viewport indicator
    let viewport_h = data.viewport_height;
    let content_h = data.content_height();
    let scroll_offset = data
        .scroll_physics
        .lock()
        .map(|p| -p.offset_y)
        .unwrap_or(0.0);
    let viewport_ratio = if content_h > 0.0 {
        viewport_h / content_h
    } else {
        1.0
    };
    let indicator_h = (total_h * viewport_ratio).max(8.0);
    let indicator_top = if content_h > viewport_h {
        (scroll_offset / (content_h - viewport_h)) * (total_h - indicator_h)
    } else {
        0.0
    };

    // Build minimap content with viewport indicator
    let mut content = div().flex_col().relative().w_full();

    // Viewport indicator (absolute positioned)
    content = content.child(
        div()
            .absolute()
            .left(0.0)
            .top(indicator_top + 4.0)
            .w_full()
            .h(indicator_h)
            .bg(Color::rgba(1.0, 1.0, 1.0, 0.1))
            .rounded(1.0),
    );

    // Render each visible line as a thin colored bar
    let styled = data.get_styled_content_readonly();
    for &line_idx in &visible_lines {
        if line_idx >= styled.lines.len() {
            break;
        }
        let styled_line = &styled.lines[line_idx];
        let line_text = &styled_line.text;
        let char_count = line_text.chars().count();

        // Line width proportional to content length (capped)
        let width_ratio = (char_count as f32 / 80.0).min(1.0);
        let bar_w = (config.minimap_width - 4.0) * width_ratio;

        // Use dominant color from first span, or default
        let color = if let Some(span) = styled_line.spans.first() {
            Color::rgba(span.color.r, span.color.g, span.color.b, 0.8)
        } else {
            Color::rgba(0.6, 0.6, 0.6, 0.4)
        };

        content = content.child(div().h(minimap_line_h).w(bar_w).bg(color).rounded(0.5));
    }

    minimap.child(content)
}

/// Build indentation guides as absolutely-positioned vertical lines.
/// Each guide spans only the block of consecutive lines at that indent level.
/// Find the matching bracket pair when cursor is adjacent to a bracket.
/// Returns Some((open_pos, close_pos)) if cursor is beside a bracket.
fn find_matching_brackets(data: &CodeEditorData) -> Option<(TextPosition, TextPosition)> {
    let line = data.cursor.line;
    let col = data.cursor.column;
    if line >= data.lines.len() {
        return None;
    }
    let line_text = &data.lines[line];
    let chars: Vec<char> = line_text.chars().collect();

    // Check char at cursor and char before cursor
    let at_cursor = if col < chars.len() {
        Some(chars[col])
    } else {
        None
    };
    let before_cursor = if col > 0 { Some(chars[col - 1]) } else { None };

    // Determine which bracket to match and its position
    let (bracket_char, bracket_col) = if let Some(c) = at_cursor {
        if matches!(c, '{' | '}' | '(' | ')' | '[' | ']') {
            (c, col)
        } else {
            let c2 = before_cursor?;
            if matches!(c2, '{' | '}' | '(' | ')' | '[' | ']') {
                (c2, col - 1)
            } else {
                return None;
            }
        }
    } else {
        let c = before_cursor?;
        if matches!(c, '{' | '}' | '(' | ')' | '[' | ']') {
            (c, col - 1)
        } else {
            return None;
        }
    };

    let (open, close) = match bracket_char {
        '{' | '}' => ('{', '}'),
        '(' | ')' => ('(', ')'),
        '[' | ']' => ('[', ']'),
        _ => return None,
    };

    let is_open = bracket_char == open;
    let bracket_pos = TextPosition::new(line, bracket_col);

    if is_open {
        // Search forward for matching close
        let mut depth = 0i32;
        for (l, text) in data.lines.iter().enumerate().skip(line) {
            let start_col = if l == line { bracket_col } else { 0 };
            for (ci, ch) in text.chars().enumerate() {
                if ci < start_col {
                    continue;
                }
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some((bracket_pos, TextPosition::new(l, ci)));
                    }
                }
            }
        }
    } else {
        // Search backward for matching open
        let mut depth = 0i32;
        for l in (0..=line).rev() {
            let line_chars: Vec<char> = data.lines[l].chars().collect();
            let end_col = if l == line {
                bracket_col
            } else {
                line_chars.len().saturating_sub(1)
            };
            for ci in (0..=end_col).rev() {
                if ci >= line_chars.len() {
                    continue;
                }
                let ch = line_chars[ci];
                if ch == close {
                    depth += 1;
                } else if ch == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some((TextPosition::new(l, ci), bracket_pos));
                    }
                }
            }
        }
    }

    None
}

/// Build indent guides based on fold regions (bracket-matched blocks only).
/// Each guide runs vertically from the line after `{` to the line with `}`.
fn build_indent_guides(
    data: &CodeEditorData,
    visible_lines: &[usize],
    line_height_px: f32,
    pad: f32,
) -> Vec<Div> {
    let guide_color = data.config.indent_guide_color;
    let fold_regions = data.detect_fold_regions();
    let mut guides = Vec::new();

    // Build a map from actual line index to visible row index
    let mut line_to_vis: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (vis_idx, &line_idx) in visible_lines.iter().enumerate() {
        line_to_vis.insert(line_idx, vis_idx);
    }

    for region in &fold_regions {
        // Skip folded regions (they're not visible)
        if data.is_fold_start(region.start_line) {
            continue;
        }

        // Get the indent level of the opening line
        let open_line = &data.lines[region.start_line];
        let indent_chars = open_line.chars().take_while(|c| c.is_whitespace()).count();
        let x = data.measure_mono(&" ".repeat(indent_chars + 1)) + pad;

        // Guide runs from line after { to line with }
        let guide_start = region.start_line + 1;
        let guide_end = region.end_line;

        if let (Some(&vis_start), Some(&vis_end)) =
            (line_to_vis.get(&guide_start), line_to_vis.get(&guide_end))
        {
            let top = vis_start as f32 * line_height_px + pad;
            let h = (vis_end - vis_start + 1) as f32 * line_height_px;
            if h > 0.0 {
                guides.push(
                    div()
                        .absolute()
                        .left(x)
                        .top(top)
                        .w(1.0)
                        .h(h)
                        .bg(guide_color),
                );
            }
        }
    }

    guides
}

/// Build the visual content for the editable code editor
fn build_editor_content(
    data: &mut CodeEditorData,
    is_focused: bool,
    char_width: f32,
    shared_state: Option<&SharedCodeEditorState>,
    instance_key: &str,
) -> Div {
    let styled = data.get_styled_content();
    let config = &data.config;
    let line_height_px = config.font_size * config.line_height;
    let visible_lines = data.visible_lines();

    // The Stateful container uses overflow_y_scroll, so its direct child
    // must grow taller than the viewport for scrolling to work.
    // Use a single flex_row: [gutter | code_area | minimap]
    let mut container = div().flex_row().w_full();

    let pad = config.padding;
    let mut code_area = div().flex_col().flex_grow().relative();

    // Current line highlight (subtle background behind active line)
    if is_focused && data.selection_start.is_none() {
        let line_top = data.cursor.line as f32 * line_height_px + pad;
        code_area = code_area.child(
            div()
                .absolute()
                .left(0.0)
                .top(line_top)
                .w_full()
                .h(line_height_px)
                .bg(config.indent_guide_color),
        );
    }

    // Selection highlights (behind text)
    if let Some(sel_start) = data.selection_start {
        let (start, end) = order_positions(sel_start, data.cursor);
        if start != end {
            let sel_color = config.selection_color;
            for line_idx in start.line..=end.line {
                if line_idx >= data.lines.len() {
                    break;
                }
                let line_text = &data.lines[line_idx];
                let line_char_count = line_text.chars().count();
                let col_start = if line_idx == start.line {
                    start.column
                } else {
                    0
                };
                let col_end = if line_idx == end.line {
                    end.column
                } else {
                    line_char_count
                };

                let x_start = if col_start > 0 {
                    let before: String = line_text.chars().take(col_start).collect();
                    data.measure_mono(&before)
                } else {
                    0.0
                };
                let x_end = if col_end > 0 {
                    let before: String = line_text.chars().take(col_end).collect();
                    data.measure_mono(&before)
                } else {
                    0.0
                };
                let width = if col_end == line_char_count && line_idx != end.line {
                    (x_end - x_start) + config.font_size * 0.5
                } else {
                    x_end - x_start
                };

                if width > 0.0 {
                    let sel_top = line_idx as f32 * line_height_px + pad;
                    code_area = code_area.child(
                        div()
                            .absolute()
                            .left(x_start + pad)
                            .top(sel_top)
                            .w(width)
                            .h(line_height_px)
                            .bg(sel_color)
                            .rounded(2.0),
                    );
                }
            }
        }
    }

    // Bracket matching highlights
    if is_focused {
        let bracket_positions = find_matching_brackets(data);
        if let Some((open, close)) = bracket_positions {
            let bracket_bg = Color::rgba(1.0, 1.0, 0.3, 0.15);
            for pos in [open, close] {
                // Find visible row for this line
                if let Some(vis_row) = visible_lines.iter().position(|&l| l == pos.line) {
                    if pos.line < data.lines.len() {
                        let before: String =
                            data.lines[pos.line].chars().take(pos.column).collect();
                        let x = data.measure_mono(&before) + pad;
                        let char_w = data.measure_mono(
                            &data.lines[pos.line]
                                .chars()
                                .skip(pos.column)
                                .take(1)
                                .collect::<String>(),
                        );
                        let y = vis_row as f32 * line_height_px + pad;
                        code_area = code_area.child(
                            div()
                                .absolute()
                                .left(x)
                                .top(y)
                                .w(char_w.max(2.0))
                                .h(line_height_px)
                                .bg(bracket_bg)
                                .rounded(2.0),
                        );
                    }
                }
            }
        }
    }

    // Indentation guides (behind text)
    if config.indent_guides {
        for guide_div in build_indent_guides(data, &visible_lines, line_height_px, pad) {
            code_area = code_area.child(guide_div);
        }
    }

    // Search match highlights
    if data.search_active && !data.search_matches.is_empty() {
        let match_bg = Color::rgba(1.0, 0.8, 0.0, 0.25);
        let active_bg = Color::rgba(1.0, 0.6, 0.0, 0.5);
        for (match_idx, &(line_idx, col_start, col_end)) in data.search_matches.iter().enumerate() {
            if let Some(vis_row) = visible_lines.iter().position(|&l| l == line_idx) {
                if line_idx < data.lines.len() {
                    let before: String = data.lines[line_idx].chars().take(col_start).collect();
                    let matched: String = data.lines[line_idx]
                        .chars()
                        .skip(col_start)
                        .take(col_end - col_start)
                        .collect();
                    let x = data.measure_mono(&before) + pad;
                    let w = data.measure_mono(&matched);
                    let y = vis_row as f32 * line_height_px + pad;
                    let bg = if match_idx == data.search_match_idx {
                        active_bg
                    } else {
                        match_bg
                    };
                    code_area = code_area.child(
                        div()
                            .absolute()
                            .left(x)
                            .top(y)
                            .w(w)
                            .h(line_height_px)
                            .bg(bg)
                            .rounded(2.0),
                    );
                }
            }
        }
    }

    // Text lines in a padded wrapper (only visible lines)
    let mut text_wrapper = div()
        .flex_col()
        .justify_start()
        .padding_x_px(pad)
        .padding_y_px(pad);
    for &line_idx in &visible_lines {
        let line_div = if line_idx < styled.lines.len() {
            let mut ld = build_styled_line(&styled.lines[line_idx], config, line_height_px);
            // Show fold indicator on fold start lines
            if config.code_folding && data.is_fold_start(line_idx) {
                ld = ld.child(
                    text(" ...")
                        .size(config.font_size * 0.85)
                        .color(config.line_number_color)
                        .monospace(),
                );
            }
            ld
        } else {
            // Fallback for lines beyond styled content (shouldn't happen but safety)
            div()
                .h(line_height_px)
                .flex_row()
                .items_center()
                .child(text(" ").size(config.font_size).color(config.text_color))
        };
        text_wrapper = text_wrapper.child(line_div);
    }
    code_area = code_area.child(text_wrapper);

    // Cursor (offset by padding to align with text)
    if is_focused {
        let cursor_height = config.font_size * 1.2;
        let cursor_line = data.cursor.line;
        let cursor_col = data.cursor.column;

        let cursor_x = if cursor_col > 0 && cursor_line < data.lines.len() {
            let text_before: String = data.lines[cursor_line].chars().take(cursor_col).collect();
            data.measure_mono(&text_before) + pad
        } else {
            pad
        };

        let cursor_top =
            (cursor_line as f32 * line_height_px) + (line_height_px - cursor_height) / 2.0 + pad;

        let cursor_state_clone = Arc::clone(&data.cursor_state);
        let cursor_color = config.cursor_color;

        {
            if let Ok(mut cs) = cursor_state_clone.lock() {
                cs.visible = true;
                cs.color = cursor_color;
                cs.x = cursor_x;
                cs.animation = CursorAnimation::SmoothFade;
            }
        }

        let cursor_state_for_canvas = Arc::clone(&data.cursor_state);
        let cursor_canvas = canvas(
            move |ctx: &mut dyn blinc_core::DrawContext, bounds: crate::canvas::CanvasBounds| {
                let cs = cursor_state_for_canvas.lock().unwrap();
                if !cs.visible {
                    return;
                }
                let opacity = cs.current_opacity();
                if opacity < 0.01 {
                    return;
                }
                let color = Color::rgba(
                    cursor_color.r,
                    cursor_color.g,
                    cursor_color.b,
                    cursor_color.a * opacity,
                );
                ctx.fill_rect(
                    Rect::new(0.0, 0.0, bounds.width, bounds.height),
                    CornerRadius::default(),
                    Brush::Solid(color),
                );
            },
        )
        .absolute()
        .top(cursor_top)
        .left(cursor_x)
        .w(2.0)
        .h(cursor_height);

        code_area = code_area.child(cursor_canvas);
    }

    // Build layout: [gutter | code_area] (scrollable) + minimap (fixed overlay)
    // overflow_y_scroll on the Stateful parent handles scrolling
    if config.line_numbers || config.code_folding {
        let fold_regions = data.detect_fold_regions();
        let folded_starts: Vec<usize> = data.folded_regions.iter().map(|&(s, _)| s).collect();
        container = container.child(build_gutter(
            &visible_lines,
            line_height_px,
            config,
            &fold_regions,
            &folded_starts,
            pad,
        ));
    }
    // Search bar is rendered as an overlay (not inside scroll content)
    container = container.child(code_area);
    container
}

/// Close the search overlay and reset search state
fn close_search_overlay(data: &mut CodeEditorData) {
    data.search_active = false;
    data.replace_active = false;
    data.search_query.clear();
    data.search_matches.clear();
    if let Some(handle) = data.search_overlay_handle.take() {
        if let Some(ctx) = crate::overlay_state::OverlayContext::try_get() {
            ctx.overlay_manager().lock().unwrap().close(handle);
        }
    }
    if let Ok(mut i) = data.search_input_state.lock() {
        i.value.clear();
        i.cursor = 0;
    }
    if let Ok(mut i) = data.replace_input_state.lock() {
        i.value.clear();
        i.cursor = 0;
    }
}

/// Open the search bar as an overlay with a signal-driven Stateful inside
fn open_search_overlay(shared_state: &SharedCodeEditorState, instance_key: &str) {
    use crate::overlay_state::get_overlay_manager;
    use crate::widgets::overlay::{OverlayAnimation, OverlayManagerExt};

    let mgr = get_overlay_manager();

    // Create a signal for search state changes
    let signal = blinc_core::context_state::use_signal_keyed(
        &format!("{}:search_signal", instance_key),
        || 0u64,
    );
    let signal_id = signal.id();

    // Store signal ID in data so button callbacks can fire it
    {
        let mut d = shared_state.lock().unwrap();
        d.search_signal_id = Some(signal_id);
    }

    // Position at top-right of editor bounds
    let (ex, ey, ew, _eh) = shared_state.lock().unwrap().editor_bounds;
    let bar_w = 400.0;
    let overlay_x = (ex + ew - bar_w - 8.0).max(ex);
    let overlay_y = ey + 4.0;

    let state_for_content = Arc::clone(shared_state);
    let key = instance_key.to_string();

    let handle = mgr
        .dropdown()
        .at(overlay_x, overlay_y)
        .animation(OverlayAnimation::none())
        .dismiss_on_escape(true)
        .on_close({
            let state_for_close = Arc::clone(shared_state);
            move || {
                if let Ok(mut d) = state_for_close.lock() {
                    d.search_active = false;
                    d.replace_active = false;
                    d.search_overlay_handle = None;
                    d.search_signal_id = None;
                }
            }
        })
        .content(move || {
            // Wrap search bar in a Stateful that rebuilds when signal fires
            let state_for_stateful = Arc::clone(&state_for_content);
            let key_for_stateful = key.clone();

            let search_stateful_state =
                crate::stateful::use_fsm_keyed::<_, crate::stateful::ButtonState>(
                    &format!("{}:search_stateful", key),
                    crate::stateful::ButtonState::default(),
                );

            div().child(
                crate::stateful::Stateful::with_shared_state(search_stateful_state)
                    .deps(&[signal_id])
                    .on_state({
                        let s = Arc::clone(&state_for_content);
                        let k = key_for_stateful.clone();
                        move |_ctx, container| {
                            let data = s.lock().unwrap();
                            let config = data.config.clone();
                            let bar = build_search_bar(&data, &config, &s, &k);
                            container.set_child(bar);
                        }
                    }),
            )
        })
        .show();

    shared_state.lock().unwrap().search_overlay_handle = Some(handle);
}

/// Fire the search signal to trigger overlay content rebuild
fn fire_search_signal(shared_state: &SharedCodeEditorState) {
    if let Ok(d) = shared_state.lock() {
        if let Some(signal_id) = d.search_signal_id {
            drop(d); // Release lock before firing signal
            crate::stateful::check_stateful_deps(&[signal_id]);
        }
    }
}

// SVG icons for search bar (outline style using stroke)
const ICON_UP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 10l4-4 4 4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const ICON_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const ICON_CLOSE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 4l8 8M12 4l-8 8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>"#;
const ICON_REPLACE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M3 8h8m-3-3l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const ICON_REPLACE_ALL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M2 5h7m-3-3l3 3-3 3M2 11h7m-3-3l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

/// Ghost icon button — transparent bg, themed hover
fn search_icon_button(
    key: &str,
    icon_svg: &'static str,
    icon_size: f32,
    color: Color,
    on_click: impl Fn() + Send + Sync + 'static,
) -> crate::widgets::button::Button {
    let theme = ThemeState::get();
    let hover = theme.color(ColorToken::SurfaceElevated);
    let pressed = theme.color(ColorToken::SurfaceOverlay);
    let btn_state = crate::stateful::use_fsm_keyed::<_, crate::stateful::ButtonState>(
        key,
        crate::stateful::ButtonState::default(),
    );
    let s = icon_size + 4.0;
    crate::widgets::button::Button::with_content(btn_state, move |_| {
        div().w(s).h(s).items_center().justify_center().child(
            crate::svg::svg(icon_svg)
                .size(icon_size, icon_size)
                .color(color),
        )
    })
    .bg_color(Color::TRANSPARENT)
    .hover_color(hover)
    .pressed_color(pressed)
    .rounded(4.0)
    .px(0.0)
    .py(0.0)
    .on_click(move |_| on_click())
}

/// Outline toggle button — border when inactive, accent bg when active
fn search_toggle_button(
    key: &str,
    label: &'static str,
    active: bool,
    color: Color,
    accent: Color,
    font_size: f32,
    on_click: impl Fn() + Send + Sync + 'static,
) -> crate::widgets::button::Button {
    let theme = ThemeState::get();
    let hover = theme.color(ColorToken::SurfaceElevated);
    let border_color = theme.color(ColorToken::Border);
    let btn_state = crate::stateful::use_fsm_keyed::<_, crate::stateful::ButtonState>(
        key,
        crate::stateful::ButtonState::default(),
    );

    let (bg, border, tc) = if active {
        (accent.with_alpha(0.15), accent, accent)
    } else {
        (Color::TRANSPARENT, border_color, color)
    };

    crate::widgets::button::Button::with_content(btn_state, move |_| {
        div()
            .items_center()
            .justify_center()
            .child(text(label).size(font_size).color(tc).monospace().no_wrap())
    })
    .bg_color(bg)
    .hover_color(hover)
    .pressed_color(accent.with_alpha(0.3))
    .border(1.0, border)
    .rounded(3.0)
    .border(1.0, border)
    .px(1.0)
    .py(0.0)
    .on_click(move |_| on_click())
}

/// Build the search bar overlay (VS Code style)
fn build_search_bar(
    data: &CodeEditorData,
    config: &CodeConfig,
    shared_state: &SharedCodeEditorState,
    instance_key: &str,
) -> Div {
    let theme = ThemeState::get();
    let small_font = config.font_size * 0.85;
    let icon_size = config.font_size + 2.0;
    let label_color = config.line_number_color;
    let accent = config.cursor_color;
    let text_col = config.text_color;
    let border_color = config.gutter_separator_color;
    let input_w = 240.0;

    let match_info = if data.search_query.is_empty() {
        String::new()
    } else if data.search_matches.is_empty() {
        "No results".to_string()
    } else {
        format!(
            "{}/{}",
            data.search_match_idx + 1,
            data.search_matches.len()
        )
    };

    // --- Chevron (toggle replace) ---
    let chevron_svg = if data.replace_active {
        FOLD_EXPANDED_SVG
    } else {
        FOLD_COLLAPSED_SVG
    };
    let state_for_toggle = Arc::clone(shared_state);
    // Helper: fire signal to rebuild overlay content
    let notify = |state: &SharedCodeEditorState| {
        let s = Arc::clone(state);
        move || fire_search_signal(&s)
    };

    // --- Chevron toggle (expand/collapse replace) ---
    let notify_toggle = notify(shared_state);
    let chevron = search_icon_button(
        &format!("{}:stoggle", instance_key),
        chevron_svg,
        icon_size,
        text_col,
        move || {
            if let Ok(mut d) = state_for_toggle.lock() {
                d.replace_active = !d.replace_active;
            }
            notify_toggle();
        },
    );

    // --- Toggle buttons (Aa, ab, .*) — absolutely positioned inside find input ---
    let state_for_case = Arc::clone(shared_state);
    let notify_case = notify(shared_state);
    let case_btn = search_toggle_button(
        &format!("{}:scase", instance_key),
        "Aa",
        data.search_case_sensitive,
        label_color,
        accent,
        small_font,
        move || {
            if let Ok(mut d) = state_for_case.lock() {
                d.search_case_sensitive = !d.search_case_sensitive;
                d.execute_search();
            }
            notify_case();
        },
    );
    let state_for_word = Arc::clone(shared_state);
    let notify_word = notify(shared_state);
    let word_btn = search_toggle_button(
        &format!("{}:sword", instance_key),
        "ab",
        data.search_whole_word,
        label_color,
        accent,
        small_font,
        move || {
            if let Ok(mut d) = state_for_word.lock() {
                d.search_whole_word = !d.search_whole_word;
                d.execute_search();
            }
            notify_word();
        },
    );
    let state_for_regex = Arc::clone(shared_state);
    let notify_regex = notify(shared_state);
    let regex_btn = search_toggle_button(
        &format!("{}:sregex", instance_key),
        ".*",
        data.search_regex,
        label_color,
        accent,
        small_font,
        move || {
            if let Ok(mut d) = state_for_regex.lock() {
                d.search_regex = !d.search_regex;
                d.execute_search();
            }
            notify_regex();
        },
    );

    // Find input with toggle buttons overlaid at the right edge
    let search_state = Arc::clone(&data.search_input_state);
    let find_input_with_toggles = div()
        .relative()
        .w(input_w)
        .child(text_input(&search_state).w_full().text_size(small_font))
        .child(
            div()
                .absolute()
                .right(2.0)
                .top(0.0)
                .h_full()
                .flex_row()
                .items_center()
                .gap_px(1.0)
                .child(case_btn)
                .child(word_btn)
                .child(regex_btn),
        );

    // --- Nav buttons ---
    let state_for_prev = Arc::clone(shared_state);
    let prev_btn = search_icon_button(
        &format!("{}:sprev", instance_key),
        ICON_UP,
        icon_size,
        text_col,
        move || {
            if let Ok(mut d) = state_for_prev.lock() {
                d.search_prev();
            }
        },
    );
    let state_for_next = Arc::clone(shared_state);
    let next_btn = search_icon_button(
        &format!("{}:snext", instance_key),
        ICON_DOWN,
        icon_size,
        text_col,
        move || {
            if let Ok(mut d) = state_for_next.lock() {
                d.search_next();
            }
        },
    );
    let state_for_close = Arc::clone(shared_state);
    let close_btn = search_icon_button(
        &format!("{}:sclose", instance_key),
        ICON_CLOSE,
        icon_size,
        text_col,
        move || {
            if let Ok(mut d) = state_for_close.lock() {
                close_search_overlay(&mut d);
            }
        },
    );

    // === FIND ROW: [>] [input+toggles] [count] [↑] [↓] [×] ===
    let mut find_row = div()
        .flex_row()
        .items_center()
        .gap_px(4.0)
        .child(chevron)
        .child(find_input_with_toggles);

    if !match_info.is_empty() {
        find_row = find_row.child(
            text(&match_info)
                .size(small_font * 0.9)
                .color(label_color)
                .no_wrap(),
        );
    }
    find_row = find_row.child(prev_btn).child(next_btn).child(close_btn);

    // === CONTAINER (sized by content, not fixed) ===
    let mut search_bar = div()
        .w_fit()
        .flex_col()
        .gap_px(4.0)
        .p_px(6.0)
        .bg(config.bg_color)
        .border(1.0, border_color)
        .rounded(4.0)
        .shadow_md()
        .child(find_row);

    // === REPLACE ROW: [spacer] [input] [⇄] [⇄⇄] ===
    if data.replace_active {
        let replace_state = Arc::clone(&data.replace_input_state);
        let replace_input = div()
            .w(input_w)
            .child(text_input(&replace_state).w_full().text_size(small_font));

        let state_for_r1 = Arc::clone(shared_state);
        let notify_r1 = notify(shared_state);
        let replace_btn = search_icon_button(
            &format!("{}:sr1", instance_key),
            ICON_REPLACE,
            icon_size,
            text_col,
            move || {
                if let Ok(mut d) = state_for_r1.lock() {
                    d.replace_current();
                }
                notify_r1();
            },
        );
        let state_for_ra = Arc::clone(shared_state);
        let notify_ra = notify(shared_state);
        let replace_all_btn = search_icon_button(
            &format!("{}:sra", instance_key),
            ICON_REPLACE_ALL,
            icon_size,
            text_col,
            move || {
                if let Ok(mut d) = state_for_ra.lock() {
                    d.replace_all();
                }
                notify_ra();
            },
        );

        // Spacer matches chevron button: icon_size + 4 (content) + button padding
        let chevron_w = icon_size + 4.0;
        let replace_row = div()
            .flex_row()
            .items_center()
            .gap_px(4.0)
            .child(div().w(chevron_w))
            .child(replace_input)
            .child(replace_btn)
            .child(replace_all_btn);

        search_bar = search_bar.child(replace_row);
    }

    search_bar
}

// ============================================================================
// Convenience Constructors
// ============================================================================

/// Create a read-only code block
pub fn code(content: impl Into<String>) -> Code {
    Code::new(content)
}

/// Create a preformatted text block (alias for code)
pub fn pre(content: impl Into<String>) -> Code {
    Code::new(content)
}

/// Create an editable code editor widget
#[track_caller]
pub fn code_editor(state: &SharedCodeEditorState) -> CodeEditor {
    CodeEditor::new(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static THEME_INIT: Once = Once::new();

    fn ensure_theme_initialized() {
        THEME_INIT.call_once(ThemeState::init_default);
    }

    #[test]
    fn test_code_creation() {
        ensure_theme_initialized();
        let c = code("fn main() {}");
        assert!(!c.config.line_numbers);
    }

    #[test]
    fn test_code_builder() {
        ensure_theme_initialized();
        let c = code("let x = 42;")
            .line_numbers(true)
            .font_size(14.0)
            .rounded(12.0);

        assert!(c.config.line_numbers);
        assert_eq!(c.config.font_size, 14.0);
        assert_eq!(c.config.corner_radius, 12.0);
    }

    #[test]
    fn test_editor_state_insert() {
        ensure_theme_initialized();
        let state = code_editor_state("hello");
        {
            let mut d = state.lock().unwrap();
            d.cursor = TextPosition::new(0, 5);
            d.insert(" world");
            assert_eq!(d.value(), "hello world");
        }
    }

    #[test]
    fn test_editor_state_newline() {
        ensure_theme_initialized();
        let state = code_editor_state("hello world");
        {
            let mut d = state.lock().unwrap();
            d.cursor = TextPosition::new(0, 5);
            d.insert("\n");
            assert_eq!(d.lines.len(), 2);
            assert_eq!(d.lines[0], "hello");
            assert_eq!(d.lines[1], " world");
        }
    }

    #[test]
    fn test_editor_state_undo_redo() {
        ensure_theme_initialized();
        let state = code_editor_state("hello");
        {
            let mut d = state.lock().unwrap();
            d.cursor = TextPosition::new(0, 5);
            d.insert(" world");
            assert_eq!(d.value(), "hello world");
            d.undo();
            assert_eq!(d.value(), "hello");
            d.redo();
            assert_eq!(d.value(), "hello world");
        }
    }

    #[test]
    fn test_editor_state_select_all() {
        ensure_theme_initialized();
        let state = code_editor_state("line1\nline2");
        {
            let mut d = state.lock().unwrap();
            d.select_all();
            assert_eq!(d.selected_text(), Some("line1\nline2".to_string()));
        }
    }

    #[test]
    fn test_editor_state_word_nav() {
        ensure_theme_initialized();
        let state = code_editor_state("hello world");
        {
            let mut d = state.lock().unwrap();
            d.cursor = TextPosition::new(0, 0);
            d.move_word_right(false);
            assert_eq!(d.cursor.column, 6); // after "hello "
            d.move_word_left(false);
            assert_eq!(d.cursor.column, 0);
        }
    }
}
