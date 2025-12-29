//! Ready-to-use TextArea widget
//!
//! Multi-line text area with:
//! - Multi-line text editing
//! - Row/column sizing (like HTML textarea)
//! - Cursor and selection
//! - Visual states: idle, hovered, focused
//! - Built-in styling that just works
//! - Inherits ALL Div methods for full layout control via Deref

use std::sync::{Arc, Mutex};

use blinc_core::Color;

use crate::canvas::canvas;
use crate::div::{div, Div, ElementBuilder};
use crate::element::RenderProps;
use crate::stateful::{
    refresh_stateful, SharedState, StateTransitions, Stateful, StatefulInner, TextFieldState,
};
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::cursor::{cursor_state, CursorAnimation, SharedCursorState};
use crate::widgets::scroll::{Scroll, ScrollDirection, ScrollPhysics, SharedScrollPhysics};
use crate::widgets::text_input::{
    elapsed_ms, increment_focus_count, request_continuous_redraw_pub, set_focused_text_area,
};

/// Position in a multi-line text (line and column)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TextPosition {
    /// Line index (0-based)
    pub line: usize,
    /// Column index (character offset within line, 0-based)
    pub column: usize,
}

impl TextPosition {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// TextArea configuration
#[derive(Clone)]
pub struct TextAreaConfig {
    /// Placeholder text shown when empty
    pub placeholder: String,
    /// Width of the text area (can be overridden by cols)
    pub width: f32,
    /// Height of the text area (can be overridden by rows)
    pub height: f32,
    /// Number of visible rows (overrides height if set)
    pub rows: Option<usize>,
    /// Number of visible columns/character width (overrides width if set)
    pub cols: Option<usize>,
    /// Font size
    pub font_size: f32,
    /// Line height multiplier
    pub line_height: f32,
    /// Approximate character width in ems (for cols calculation)
    pub char_width_ratio: f32,
    /// Text color
    pub text_color: Color,
    /// Placeholder text color
    pub placeholder_color: Color,
    /// Background color
    pub bg_color: Color,
    /// Hovered background color
    pub hover_bg_color: Color,
    /// Focused background color
    pub focused_bg_color: Color,
    /// Border color
    pub border_color: Color,
    /// Hovered border color
    pub hover_border_color: Color,
    /// Focused border color
    pub focused_border_color: Color,
    /// Border width
    pub border_width: f32,
    /// Corner radius
    pub corner_radius: f32,
    /// Horizontal padding
    pub padding_x: f32,
    /// Vertical padding
    pub padding_y: f32,
    /// Cursor color
    pub cursor_color: Color,
    /// Selection color
    pub selection_color: Color,
    /// Whether the text area is disabled
    pub disabled: bool,
    /// Maximum character count (0 = unlimited)
    pub max_length: usize,
    /// Whether text wraps at container bounds (default: true)
    /// When true, long lines wrap to the next visual line.
    /// When false, content scrolls horizontally.
    pub wrap: bool,
}

impl Default for TextAreaConfig {
    fn default() -> Self {
        Self {
            placeholder: String::new(),
            width: 300.0,
            height: 120.0,
            rows: None,
            cols: None,
            font_size: 14.0,
            line_height: 1.4,
            char_width_ratio: 0.6,
            text_color: Color::rgba(0.9, 0.9, 0.9, 1.0),
            placeholder_color: Color::rgba(0.5, 0.5, 0.5, 1.0),
            bg_color: Color::rgba(0.15, 0.15, 0.2, 1.0),
            hover_bg_color: Color::rgba(0.18, 0.18, 0.23, 1.0),
            focused_bg_color: Color::rgba(0.18, 0.18, 0.25, 1.0),
            border_color: Color::rgba(0.3, 0.3, 0.35, 1.0),
            hover_border_color: Color::rgba(0.4, 0.4, 0.45, 1.0),
            focused_border_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            border_width: 1.5,
            corner_radius: 8.0,
            padding_x: 12.0,
            padding_y: 10.0,
            cursor_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            selection_color: Color::rgba(0.4, 0.6, 1.0, 0.3),
            disabled: false,
            max_length: 0,
            // Default to wrapping since we now have visual lines computed for proper
            // cursor tracking. Visual lines are computed in the callback and used for
            // both rendering and cursor positioning to ensure they match.
            wrap: true,
        }
    }
}

impl TextAreaConfig {
    /// Calculate the effective width based on cols or explicit width
    pub fn effective_width(&self) -> f32 {
        if let Some(cols) = self.cols {
            let char_width = self.font_size * self.char_width_ratio;
            cols as f32 * char_width + self.padding_x * 2.0 + self.border_width * 2.0
        } else {
            self.width
        }
    }

    /// Calculate the effective height based on rows or explicit height
    pub fn effective_height(&self) -> f32 {
        if let Some(rows) = self.rows {
            let single_line_height = self.font_size * self.line_height;
            rows as f32 * single_line_height + self.padding_y * 2.0 + self.border_width * 2.0
        } else {
            self.height
        }
    }
}

/// A visual line segment - represents a portion of a logical line that fits on one visual line
#[derive(Clone, Debug)]
pub struct VisualLine {
    /// Index of the logical line this visual line belongs to
    pub logical_line: usize,
    /// Start character index within the logical line
    pub start_char: usize,
    /// End character index within the logical line (exclusive)
    pub end_char: usize,
    /// The text content of this visual line (cached for rendering)
    pub text: String,
    /// Width of this visual line in pixels
    pub width: f32,
}

/// TextArea widget state
#[derive(Clone)]
pub struct TextAreaState {
    /// Lines of text
    pub lines: Vec<String>,
    /// Cursor position
    pub cursor: TextPosition,
    /// Selection start position (if selecting)
    pub selection_start: Option<TextPosition>,
    /// Visual state for styling
    pub visual: TextFieldState,
    /// Placeholder text
    pub placeholder: String,
    /// Whether disabled
    pub disabled: bool,
    /// Time when focus was gained (for cursor blinking)
    /// Stored as milliseconds since some epoch (e.g., app start)
    pub focus_time_ms: u64,
    /// Cursor blink interval in milliseconds
    pub cursor_blink_interval_ms: u64,
    /// Canvas-based cursor state for smooth animation
    pub cursor_state: SharedCursorState,
    /// Shared scroll physics for vertical scrolling
    pub(crate) scroll_physics: SharedScrollPhysics,
    /// Cached viewport height for scroll calculations
    pub(crate) viewport_height: f32,
    /// Cached line height for scroll calculations
    pub(crate) line_height: f32,
    /// Cached font size for scroll calculations with wrapping
    pub(crate) font_size: f32,
    /// Cached available width for text (for wrapping calculations)
    pub(crate) available_width: f32,
    /// Cached wrap enabled flag
    pub(crate) wrap_enabled: bool,
    /// Computed visual lines (recomputed when text or width changes)
    /// Each VisualLine represents one rendered row of text
    pub(crate) visual_lines: Vec<VisualLine>,
    /// Reference to the Stateful's shared state for triggering incremental updates
    pub(crate) stateful_state: Option<SharedState<TextFieldState>>,
    /// Last clicked visual line index (set by line click handlers, read by main handler)
    /// This is used to accurately determine which line was clicked when local_y
    /// is relative to the clicked line element rather than the whole text area.
    pub(crate) clicked_visual_line: Option<usize>,
}

impl std::fmt::Debug for TextAreaState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextAreaState")
            .field("lines", &self.lines)
            .field("cursor", &self.cursor)
            .field("selection_start", &self.selection_start)
            .field("visual", &self.visual)
            .field("placeholder", &self.placeholder)
            .field("disabled", &self.disabled)
            .field("focus_time_ms", &self.focus_time_ms)
            .field("cursor_blink_interval_ms", &self.cursor_blink_interval_ms)
            // Skip stateful_state since StatefulInner doesn't implement Debug
            .finish()
    }
}

impl Default for TextAreaState {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: TextPosition::default(),
            selection_start: None,
            visual: TextFieldState::Idle,
            placeholder: String::new(),
            disabled: false,
            focus_time_ms: 0,
            cursor_blink_interval_ms: 530, // Standard cursor blink rate (~530ms)
            cursor_state: cursor_state(),
            scroll_physics: Arc::new(Mutex::new(ScrollPhysics::default())),
            viewport_height: 120.0,   // Default height from TextAreaConfig
            line_height: 14.0 * 1.4,  // Default font_size * line_height
            font_size: 14.0,          // Default font size
            available_width: 276.0,   // Default width minus padding/borders
            wrap_enabled: true,       // Default to wrapping (visual lines handle cursor tracking)
            visual_lines: Vec::new(), // Computed on first layout
            stateful_state: None,
            clicked_visual_line: None, // Set by line click handlers
        }
    }
}

impl TextAreaState {
    /// Create new text area state
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with initial value
    pub fn with_value(value: impl Into<String>) -> Self {
        let value = value.into();
        let lines: Vec<String> = if value.is_empty() {
            vec![String::new()]
        } else {
            value.lines().map(|s| s.to_string()).collect()
        };
        let cursor = TextPosition::new(
            lines.len().saturating_sub(1),
            lines.last().map(|l| l.chars().count()).unwrap_or(0),
        );
        Self {
            lines,
            cursor,
            ..Default::default()
        }
    }

    /// Create with placeholder
    pub fn with_placeholder(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            ..Default::default()
        }
    }

    /// Get the full text value
    pub fn value(&self) -> String {
        self.lines.join("\n")
    }

    /// Set the text value
    pub fn set_value(&mut self, value: &str) {
        self.lines = if value.is_empty() {
            vec![String::new()]
        } else {
            value.lines().map(|s| s.to_string()).collect()
        };
        self.cursor = TextPosition::new(
            self.lines.len().saturating_sub(1),
            self.lines.last().map(|l| l.chars().count()).unwrap_or(0),
        );
        self.selection_start = None;
    }

    /// Get number of lines
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Get a specific line
    pub fn get_line(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(|s| s.as_str())
    }

    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    /// Is focused?
    pub fn is_focused(&self) -> bool {
        self.visual.is_focused()
    }

    /// Check if cursor should be visible based on current time
    /// Returns true if cursor is in the "on" phase of blinking
    pub fn is_cursor_visible(&self, current_time_ms: u64) -> bool {
        if self.cursor_blink_interval_ms == 0 {
            return true; // No blinking, always visible
        }
        let elapsed = current_time_ms.saturating_sub(self.focus_time_ms);
        let phase = (elapsed / self.cursor_blink_interval_ms) % 2;
        phase == 0
    }

    /// Reset cursor blink (call when focus gained or cursor moved)
    pub fn reset_cursor_blink(&mut self) {
        self.focus_time_ms = elapsed_ms();
        // Also reset the canvas cursor state for smooth animation
        if let Ok(mut cs) = self.cursor_state.lock() {
            cs.reset_blink();
        }
    }

    /// Insert text at cursor
    pub fn insert(&mut self, text: &str) {
        self.delete_selection();

        if text.contains('\n') {
            for (i, part) in text.split('\n').enumerate() {
                if i > 0 {
                    self.insert_newline();
                }
                self.insert_text(part);
            }
        } else {
            self.insert_text(text);
        }
    }

    fn insert_text(&mut self, text: &str) {
        let line_idx = self.cursor.line.min(self.lines.len().saturating_sub(1));
        let byte_pos = char_to_byte_pos(&self.lines[line_idx], self.cursor.column);
        self.lines[line_idx].insert_str(byte_pos, text);
        self.cursor.column += text.chars().count();
    }

    /// Insert a newline at cursor
    pub fn insert_newline(&mut self) {
        self.delete_selection();

        let line_idx = self.cursor.line.min(self.lines.len().saturating_sub(1));
        let byte_pos = char_to_byte_pos(&self.lines[line_idx], self.cursor.column);

        let after = self.lines[line_idx].split_off(byte_pos);
        self.lines.insert(line_idx + 1, after);

        self.cursor.line += 1;
        self.cursor.column = 0;
    }

    /// Delete character before cursor (backspace)
    pub fn delete_backward(&mut self) {
        if self.delete_selection() {
            return;
        }

        if self.cursor.column > 0 {
            let start_byte =
                char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column - 1);
            let end_byte = char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column);
            self.lines[self.cursor.line].replace_range(start_byte..end_byte, "");
            self.cursor.column -= 1;
        } else if self.cursor.line > 0 {
            let current_line = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
            self.lines[self.cursor.line].push_str(&current_line);
        }
    }

    /// Delete character after cursor (delete)
    pub fn delete_forward(&mut self) {
        if self.delete_selection() {
            return;
        }

        let line_len = self.lines[self.cursor.line].chars().count();
        if self.cursor.column < line_len {
            let start_byte = char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column);
            let end_byte = char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column + 1);
            self.lines[self.cursor.line].replace_range(start_byte..end_byte, "");
        } else if self.cursor.line < self.lines.len() - 1 {
            let next_line = self.lines.remove(self.cursor.line + 1);
            self.lines[self.cursor.line].push_str(&next_line);
        }
    }

    /// Delete selected text
    fn delete_selection(&mut self) -> bool {
        if let Some(start) = self.selection_start {
            let (from, to) = self.order_positions(start, self.cursor);

            if from != to {
                if from.line == to.line {
                    let start_byte = char_to_byte_pos(&self.lines[from.line], from.column);
                    let end_byte = char_to_byte_pos(&self.lines[from.line], to.column);
                    self.lines[from.line].replace_range(start_byte..end_byte, "");
                } else {
                    let from_byte = char_to_byte_pos(&self.lines[from.line], from.column);
                    self.lines[from.line].truncate(from_byte);

                    let to_byte = char_to_byte_pos(&self.lines[to.line], to.column);
                    let after_text = self.lines[to.line][to_byte..].to_string();
                    self.lines[from.line].push_str(&after_text);

                    for _ in from.line + 1..=to.line {
                        if from.line + 1 < self.lines.len() {
                            self.lines.remove(from.line + 1);
                        }
                    }
                }

                self.cursor = from;
                self.selection_start = None;
                return true;
            }
        }
        self.selection_start = None;
        false
    }

    /// Order two positions (returns (earlier, later))
    fn order_positions(&self, a: TextPosition, b: TextPosition) -> (TextPosition, TextPosition) {
        if a.line < b.line || (a.line == b.line && a.column <= b.column) {
            (a, b)
        } else {
            (b, a)
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            if let Some(start) = self.selection_start {
                let (from, _) = self.order_positions(start, self.cursor);
                self.cursor = from;
                self.selection_start = None;
                return;
            }
        }

        if self.cursor.column > 0 {
            self.cursor.column -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
        }

        if !select {
            self.selection_start = None;
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            if let Some(start) = self.selection_start {
                let (_, to) = self.order_positions(start, self.cursor);
                self.cursor = to;
                self.selection_start = None;
                return;
            }
        }

        let line_len = self.lines[self.cursor.line].chars().count();
        if self.cursor.column < line_len {
            self.cursor.column += 1;
        } else if self.cursor.line < self.lines.len() - 1 {
            self.cursor.line += 1;
            self.cursor.column = 0;
        }

        if !select {
            self.selection_start = None;
        }
    }

    /// Move cursor up (handles visual lines for wrapped text)
    pub fn move_up(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }

        // If we have visual lines, use them for navigation
        if !self.visual_lines.is_empty() && self.wrap_enabled {
            let current_visual_idx = self.visual_line_for_cursor();
            if current_visual_idx > 0 {
                // Move to the visual line above
                let prev_visual_idx = current_visual_idx - 1;
                let prev_vl = &self.visual_lines[prev_visual_idx];

                // Calculate cursor x position to maintain horizontal position
                let cursor_x = self.cursor_x_in_visual_line();

                // Find the column in the previous visual line that best matches our x position
                let column = self.find_column_at_x(prev_visual_idx, cursor_x);

                self.cursor.line = prev_vl.logical_line;
                self.cursor.column = column;
            }
        } else {
            // Fallback: simple logical line navigation
            if self.cursor.line > 0 {
                self.cursor.line -= 1;
                let line_len = self.lines[self.cursor.line].chars().count();
                self.cursor.column = self.cursor.column.min(line_len);
            }
        }
    }

    /// Move cursor down (handles visual lines for wrapped text)
    pub fn move_down(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }

        // If we have visual lines, use them for navigation
        if !self.visual_lines.is_empty() && self.wrap_enabled {
            let current_visual_idx = self.visual_line_for_cursor();
            if current_visual_idx < self.visual_lines.len() - 1 {
                // Move to the visual line below
                let next_visual_idx = current_visual_idx + 1;
                let next_vl = &self.visual_lines[next_visual_idx];

                // Calculate cursor x position to maintain horizontal position
                let cursor_x = self.cursor_x_in_visual_line();

                // Find the column in the next visual line that best matches our x position
                let column = self.find_column_at_x(next_visual_idx, cursor_x);

                self.cursor.line = next_vl.logical_line;
                self.cursor.column = column;
            }
        } else {
            // Fallback: simple logical line navigation
            if self.cursor.line < self.lines.len() - 1 {
                self.cursor.line += 1;
                let line_len = self.lines[self.cursor.line].chars().count();
                self.cursor.column = self.cursor.column.min(line_len);
            }
        }
    }

    /// Find the column in a visual line that best matches a given x position
    fn find_column_at_x(&self, visual_line_idx: usize, target_x: f32) -> usize {
        if visual_line_idx >= self.visual_lines.len() {
            return 0;
        }

        let vl = &self.visual_lines[visual_line_idx];
        if vl.text.is_empty() {
            return vl.start_char;
        }

        let char_count = vl.text.chars().count();
        let mut best_pos = 0;
        let mut min_dist = f32::MAX;

        // Find character position that best matches target_x
        for i in 0..=char_count {
            let prefix: String = vl.text.chars().take(i).collect();
            let prefix_width = crate::text_measure::measure_text(&prefix, self.font_size).width;

            let dist = (prefix_width - target_x).abs();
            if dist < min_dist {
                min_dist = dist;
                best_pos = i;
            }
        }

        vl.start_char + best_pos
    }

    /// Move to start of line
    pub fn move_to_line_start(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor.column = 0;
    }

    /// Move to end of line
    pub fn move_to_line_end(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor.column = self.lines[self.cursor.line].chars().count();
    }

    /// Move to start of text
    pub fn move_to_start(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor = TextPosition::new(0, 0);
    }

    /// Move to end of text
    pub fn move_to_end(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        let last_line = self.lines.len().saturating_sub(1);
        self.cursor = TextPosition::new(last_line, self.lines[last_line].chars().count());
    }

    /// Select all text
    pub fn select_all(&mut self) {
        self.selection_start = Some(TextPosition::new(0, 0));
        let last_line = self.lines.len().saturating_sub(1);
        self.cursor = TextPosition::new(last_line, self.lines[last_line].chars().count());
    }

    /// Get selected text
    pub fn selected_text(&self) -> Option<String> {
        self.selection_start.map(|start| {
            let (from, to) = self.order_positions(start, self.cursor);

            if from.line == to.line {
                self.lines[from.line]
                    .chars()
                    .skip(from.column)
                    .take(to.column - from.column)
                    .collect()
            } else {
                let mut result = String::new();
                result.extend(self.lines[from.line].chars().skip(from.column));

                for line in &self.lines[from.line + 1..to.line] {
                    result.push('\n');
                    result.push_str(line);
                }

                if to.line > from.line {
                    result.push('\n');
                    result.extend(self.lines[to.line].chars().take(to.column));
                }

                result
            }
        })
    }

    /// Calculate the number of visual lines a text line takes when wrapped
    ///
    /// Returns 1 for short lines, more for lines that wrap.
    fn visual_lines_for_text(text: &str, font_size: f32, available_width: f32) -> usize {
        if text.is_empty() || available_width <= 0.0 {
            return 1;
        }

        let metrics = crate::text_measure::measure_text(text, font_size);
        if metrics.width <= available_width {
            return 1;
        }

        // Estimate number of lines by dividing total width by available width
        // Add 1 because integer division rounds down
        ((metrics.width / available_width).ceil() as usize).max(1)
    }

    /// Compute visual lines for all text content
    ///
    /// This creates a list of VisualLine entries that map logical lines to
    /// visual lines, accounting for word wrapping. Each VisualLine contains:
    /// - The logical line index it belongs to
    /// - Start and end character indices within that logical line
    /// - The actual text content and its measured width
    ///
    /// Call this whenever:
    /// - Text content changes
    /// - Available width changes (e.g., container resize)
    /// - Font size changes
    pub fn compute_visual_lines(&mut self) {
        self.visual_lines.clear();

        let font_size = self.font_size;
        let available_width = self.available_width;
        let wrap_enabled = self.wrap_enabled;

        for (logical_line_idx, line_text) in self.lines.iter().enumerate() {
            if line_text.is_empty() {
                // Empty line still takes up one visual line
                self.visual_lines.push(VisualLine {
                    logical_line: logical_line_idx,
                    start_char: 0,
                    end_char: 0,
                    text: String::new(),
                    width: 0.0,
                });
                continue;
            }

            if !wrap_enabled || available_width <= 0.0 {
                // No wrapping - entire logical line is one visual line
                let width = crate::text_measure::measure_text(line_text, font_size).width;
                self.visual_lines.push(VisualLine {
                    logical_line: logical_line_idx,
                    start_char: 0,
                    end_char: line_text.chars().count(),
                    text: line_text.clone(),
                    width,
                });
                continue;
            }

            // Wrapping enabled - split line into visual lines
            // Use word-based wrapping: try to break at word boundaries
            let chars: Vec<char> = line_text.chars().collect();
            let char_count = chars.len();
            let mut start_char = 0;

            while start_char < char_count {
                // Find how many characters fit in available_width
                let mut end_char = start_char;
                let mut last_word_break = start_char;
                let mut current_width = 0.0;

                // Scan forward to find break point
                while end_char < char_count {
                    // Check width up to this character (inclusive)
                    let test_end = end_char + 1;
                    let test_text: String = chars[start_char..test_end].iter().collect();
                    let test_width = crate::text_measure::measure_text(&test_text, font_size).width;

                    if test_width > available_width && end_char > start_char {
                        // This character would exceed width, break here
                        break;
                    }

                    current_width = test_width;
                    end_char = test_end;

                    // Track word boundary (space character)
                    if end_char < char_count && chars[end_char - 1].is_whitespace() {
                        last_word_break = end_char;
                    }
                }

                // If we found a word break, use it (unless it's at the start)
                if last_word_break > start_char && end_char < char_count {
                    end_char = last_word_break;
                    let text: String = chars[start_char..end_char].iter().collect();
                    current_width = crate::text_measure::measure_text(&text, font_size).width;
                }

                // Create visual line
                let text: String = chars[start_char..end_char].iter().collect();
                self.visual_lines.push(VisualLine {
                    logical_line: logical_line_idx,
                    start_char,
                    end_char,
                    text,
                    width: current_width,
                });

                start_char = end_char;
            }

            // If line ended exactly at boundary, we should have at least one visual line
            if self.visual_lines.last().map(|vl| vl.logical_line) != Some(logical_line_idx) {
                self.visual_lines.push(VisualLine {
                    logical_line: logical_line_idx,
                    start_char: char_count,
                    end_char: char_count,
                    text: String::new(),
                    width: 0.0,
                });
            }
        }
    }

    /// Calculate cursor position from a known visual line index and x coordinate
    ///
    /// This is used when a line element's click handler has already determined
    /// which visual line was clicked. The x coordinate is relative to the line element.
    pub fn cursor_position_from_visual_line(&self, visual_line_idx: usize, x: f32) -> TextPosition {
        if visual_line_idx >= self.visual_lines.len() {
            return TextPosition::default();
        }

        let vl = &self.visual_lines[visual_line_idx];
        let column = self.char_position_in_visual_line(visual_line_idx, x, self.font_size);

        TextPosition::new(vl.logical_line, column)
    }

    /// Get the visual line index for a given cursor position
    ///
    /// Returns the index into visual_lines where the cursor is located.
    pub fn visual_line_for_cursor(&self) -> usize {
        let cursor_line = self.cursor.line;
        let cursor_col = self.cursor.column;

        for (idx, vl) in self.visual_lines.iter().enumerate() {
            if vl.logical_line == cursor_line {
                // Check if cursor is within this visual line's range
                if cursor_col >= vl.start_char && cursor_col <= vl.end_char {
                    return idx;
                }
                // If cursor is at the end of a wrapped line, it might be at start of next visual line
                if cursor_col == vl.end_char {
                    // Check if there's another visual line for same logical line
                    if idx + 1 < self.visual_lines.len()
                        && self.visual_lines[idx + 1].logical_line == cursor_line
                        && self.visual_lines[idx + 1].start_char == cursor_col
                    {
                        return idx + 1;
                    }
                    return idx;
                }
            }
        }

        // Fallback: return last visual line
        self.visual_lines.len().saturating_sub(1)
    }

    /// Get cursor X position within its visual line
    ///
    /// Returns the pixel offset from the left edge of the visual line to the cursor.
    pub fn cursor_x_in_visual_line(&self) -> f32 {
        let cursor_line = self.cursor.line;
        let cursor_col = self.cursor.column;

        // Find the visual line containing the cursor
        for vl in &self.visual_lines {
            if vl.logical_line == cursor_line
                && cursor_col >= vl.start_char
                && cursor_col <= vl.end_char
            {
                // Measure text from start of visual line to cursor
                let local_col = cursor_col - vl.start_char;
                if local_col == 0 {
                    return 0.0;
                }
                let text_before: String = vl.text.chars().take(local_col).collect();
                return crate::text_measure::measure_text(&text_before, self.font_size).width;
            }
        }

        0.0
    }

    /// Get cursor position (x, visual_y) using computed visual lines
    ///
    /// Returns (cursor_x, cursor_visual_y) for positioning the cursor element.
    pub fn cursor_position_from_visual_lines(&self) -> (f32, f32) {
        let visual_line_idx = self.visual_line_for_cursor();
        let cursor_x = self.cursor_x_in_visual_line();
        let cursor_visual_y = visual_line_idx as f32 * self.line_height;
        (cursor_x, cursor_visual_y)
    }

    /// Get total visual line count
    pub fn visual_line_count(&self) -> usize {
        if self.visual_lines.is_empty() {
            // Fallback when visual lines not computed yet
            self.lines.len()
        } else {
            self.visual_lines.len()
        }
    }

    /// Get content height based on visual lines
    pub fn content_height_from_visual_lines(&self) -> f32 {
        self.visual_line_count() as f32 * self.line_height
    }

    /// Calculate cursor Y position accounting for text wrapping
    ///
    /// Returns (cursor_y, content_height) where cursor_y is the visual Y position
    /// of the cursor line, and content_height is the total visual height.
    fn calculate_wrapped_positions(
        &self,
        font_size: f32,
        line_height: f32,
        available_width: f32,
        wrap_enabled: bool,
    ) -> (f32, f32) {
        if !wrap_enabled || available_width <= 0.0 {
            // No wrapping - simple calculation
            let cursor_y = self.cursor.line as f32 * line_height;
            let content_height = self.lines.len() as f32 * line_height;
            return (cursor_y, content_height);
        }

        // With wrapping, count visual lines up to cursor line
        let mut visual_line_count = 0usize;
        let mut cursor_visual_y = 0.0f32;

        for (idx, line) in self.lines.iter().enumerate() {
            let line_visual_count = Self::visual_lines_for_text(line, font_size, available_width);

            if idx < self.cursor.line {
                visual_line_count += line_visual_count;
            } else if idx == self.cursor.line {
                cursor_visual_y = visual_line_count as f32 * line_height;
                // For the cursor's line, we need to account for wrapped position
                // within that line based on cursor column position
                if line_visual_count > 1 && !line.is_empty() {
                    let text_before: String = line.chars().take(self.cursor.column).collect();
                    let prefix_width =
                        crate::text_measure::measure_text(&text_before, font_size).width;
                    let lines_into = (prefix_width / available_width).floor() as usize;
                    cursor_visual_y += lines_into as f32 * line_height;
                }
                visual_line_count += line_visual_count;
            } else {
                visual_line_count += line_visual_count;
            }
        }

        let content_height = visual_line_count as f32 * line_height;
        (cursor_visual_y, content_height)
    }

    /// Ensure the cursor is visible by adjusting scroll offset if needed
    ///
    /// This should be called after any cursor movement to auto-scroll
    /// when the cursor moves outside the visible area.
    /// Uses cached wrap settings from the state.
    pub fn ensure_cursor_visible(&mut self, line_height: f32, viewport_height: f32) {
        // Use visual lines for accurate cursor position if available
        let (cursor_y, content_height) = if !self.visual_lines.is_empty() {
            let (_, cursor_y) = self.cursor_position_from_visual_lines();
            let content_height = self.content_height_from_visual_lines();
            (cursor_y, content_height)
        } else {
            self.calculate_wrapped_positions(
                self.font_size,
                line_height,
                self.available_width,
                self.wrap_enabled,
            )
        };
        let cursor_bottom = cursor_y + line_height;

        // Get current scroll offset from physics (offset_y is negative when scrolled down)
        let mut physics = self.scroll_physics.lock().unwrap();
        let current_offset = -physics.offset_y; // Convert to positive scroll offset

        // If cursor is above visible area, scroll up
        let mut new_offset = current_offset;
        if cursor_y < current_offset {
            new_offset = cursor_y;
        }

        // If cursor is below visible area, scroll down
        if cursor_bottom > current_offset + viewport_height {
            new_offset = cursor_bottom - viewport_height;
        }

        // Clamp scroll offset to valid range
        let max_scroll = (content_height - viewport_height).max(0.0);
        new_offset = new_offset.clamp(0.0, max_scroll);

        // Update physics offset (negative for scroll physics convention)
        physics.offset_y = -new_offset;
    }

    /// Get current scroll offset (positive value, 0 = top)
    pub fn scroll_offset(&self) -> f32 {
        -self.scroll_physics.lock().unwrap().offset_y
    }

    /// Calculate cursor position from click coordinates
    ///
    /// Takes x/y coordinates relative to the text content area (after padding).
    /// Returns the TextPosition (line, column) for the clicked location.
    pub fn cursor_position_from_xy(&self, x: f32, y: f32) -> TextPosition {
        if self.lines.is_empty() {
            return TextPosition::default();
        }

        let line_height = self.line_height;
        let font_size = self.font_size;
        let scroll_offset = self.scroll_offset();

        // Account for scroll offset - y is in viewport space
        let text_y = y + scroll_offset;

        // Find the visual line index that was clicked
        let visual_line_idx = (text_y / line_height).floor().max(0.0) as usize;

        // Use computed visual lines if available for accurate positioning
        if !self.visual_lines.is_empty() {
            // Clamp to valid range
            let visual_line_idx = visual_line_idx.min(self.visual_lines.len().saturating_sub(1));
            let vl = &self.visual_lines[visual_line_idx];

            // Find character position within this visual line
            let column = self.char_position_in_visual_line(visual_line_idx, x, font_size);

            return TextPosition::new(vl.logical_line, column);
        }

        // Fallback: no visual lines computed
        let logical_line = visual_line_idx.min(self.lines.len().saturating_sub(1));
        let column = self.char_position_from_x(logical_line, x, font_size);
        TextPosition::new(logical_line, column)
    }

    /// Find character position from x coordinate within a visual line
    fn char_position_in_visual_line(
        &self,
        visual_line_idx: usize,
        x: f32,
        font_size: f32,
    ) -> usize {
        if visual_line_idx >= self.visual_lines.len() {
            return 0;
        }

        let vl = &self.visual_lines[visual_line_idx];
        if vl.text.is_empty() {
            return vl.start_char;
        }

        let char_count = vl.text.chars().count();
        let mut best_pos = 0;
        let mut min_dist = f32::MAX;

        // Check position before each character and after the last within this visual line
        for i in 0..=char_count {
            let prefix: String = vl.text.chars().take(i).collect();
            let prefix_width = crate::text_measure::measure_text(&prefix, font_size).width;

            let dist = (prefix_width - x).abs();
            if dist < min_dist {
                min_dist = dist;
                best_pos = i;
            }
        }

        // Convert local position to absolute position within logical line
        vl.start_char + best_pos
    }

    /// Find character position from x coordinate within a line
    fn char_position_from_x(&self, line_index: usize, x: f32, font_size: f32) -> usize {
        if line_index >= self.lines.len() {
            return 0;
        }

        let line = &self.lines[line_index];
        if line.is_empty() {
            return 0;
        }

        let char_count = line.chars().count();
        let mut best_pos = 0;
        let mut min_dist = f32::MAX;

        // Check position before each character and after the last
        for i in 0..=char_count {
            let prefix: String = line.chars().take(i).collect();
            let prefix_width = crate::text_measure::measure_text(&prefix, font_size).width;

            let dist = (prefix_width - x).abs();
            if dist < min_dist {
                min_dist = dist;
                best_pos = i;
            }
        }

        best_pos
    }
}

/// Convert character index to byte index
fn char_to_byte_pos(line: &str, char_pos: usize) -> usize {
    line.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}

/// Shared text area state handle
pub type SharedTextAreaState = Arc<Mutex<TextAreaState>>;

/// Create a shared text area state
pub fn text_area_state() -> SharedTextAreaState {
    Arc::new(Mutex::new(TextAreaState::new()))
}

/// Create a shared text area state with placeholder
pub fn text_area_state_with_placeholder(placeholder: impl Into<String>) -> SharedTextAreaState {
    Arc::new(Mutex::new(TextAreaState::with_placeholder(placeholder)))
}

/// Ready-to-use text area element
///
/// Uses FSM-driven state management via `Stateful<TextFieldState>` for visual states
/// while maintaining separate text content state for editing.
///
/// Usage: `text_area(&state).rows(4).w(400.0).rounded(12.0)`
pub struct TextArea {
    /// Inner Stateful element for FSM-driven visual states
    inner: Stateful<TextFieldState>,
    /// Text area state (content, cursor, etc.)
    state: SharedTextAreaState,
    /// Text area configuration
    config: Arc<Mutex<TextAreaConfig>>,
}

impl TextArea {
    /// Create a new text area with shared state
    pub fn new(state: &SharedTextAreaState) -> Self {
        let config = Arc::new(Mutex::new(TextAreaConfig::default()));
        let cfg = config.lock().unwrap();
        let default_width = cfg.effective_width();
        let default_height = cfg.effective_height();
        drop(cfg);

        // Get initial visual state and existing stateful_state from data
        let (initial_visual, existing_stateful_state) = {
            let d = state.lock().unwrap();
            (d.visual, d.stateful_state.clone())
        };

        // Reuse existing stateful_state if available, otherwise create new one
        // This ensures state persists across rebuilds (e.g., window resize)
        let shared_state: SharedState<TextFieldState> =
            existing_stateful_state.unwrap_or_else(|| {
                let new_state = Arc::new(Mutex::new(StatefulInner::new(initial_visual)));
                // Store reference in TextAreaState for triggering refreshes
                if let Ok(mut d) = state.lock() {
                    d.stateful_state = Some(Arc::clone(&new_state));
                }
                new_state
            });

        // Clear stale node_id from previous tree builds
        // During full rebuild (e.g., window resize), the old node_id points to
        // nodes in the old tree. Clearing it ensures the new node_id gets assigned
        // when this element is added to the new tree.
        {
            let mut shared = shared_state.lock().unwrap();
            shared.node_id = None;
        }

        // Create inner Stateful with text area event handlers
        let mut inner = Self::create_inner_with_handlers(
            Arc::clone(&shared_state),
            Arc::clone(state),
            Arc::clone(&config),
        );

        // Set default dimensions from config
        // HTML-like flex behavior:
        // 1. min-width: 0 - allows shrinking below content size in flex containers
        // 2. flex-shrink: 1 (default) - allows shrinking when container is constrained
        // 3. The height is always set explicitly (based on rows or config.height)
        // Note: Don't use overflow_clip() here - the inner Scroll handles clipping.
        // Using overflow_clip on the outer container with rounded corners causes
        // the clip to interfere with border rendering at the corners.
        inner = inner.w(default_width).h(default_height).min_w(0.0);

        // Register callback immediately so it's available for incremental diff
        // The diff system calls children_builders() before build(), so the callback
        // must be registered here, not in build()
        {
            let config_for_callback = Arc::clone(&config);
            let data_for_callback = Arc::clone(state);
            let shared_state_for_callback = Arc::clone(state);
            let mut shared = shared_state.lock().unwrap();

            shared.state_callback = Some(Arc::new(
                move |visual: &TextFieldState, container: &mut Div| {
                    let cfg = config_for_callback.lock().unwrap().clone();
                    let mut data_guard = data_for_callback.lock().unwrap();

                    // Sync visual state to data so it matches the FSM
                    data_guard.visual = *visual;

                    // Update cached scroll dimensions from config
                    let line_height = cfg.font_size * cfg.line_height;
                    let viewport_height =
                        cfg.effective_height() - cfg.padding_y * 2.0 - cfg.border_width * 2.0;
                    let available_width =
                        cfg.effective_width() - cfg.padding_x * 2.0 - cfg.border_width * 2.0;
                    data_guard.line_height = line_height;
                    data_guard.viewport_height = viewport_height;
                    data_guard.font_size = cfg.font_size;
                    data_guard.available_width = available_width;
                    data_guard.wrap_enabled = cfg.wrap;

                    // Recompute visual lines for proper cursor tracking with wrapped text
                    data_guard.compute_visual_lines();

                    // Determine colors based on visual state
                    let (bg, border) = match visual {
                        TextFieldState::Focused | TextFieldState::FocusedHovered => {
                            (cfg.focused_bg_color, cfg.focused_border_color)
                        }
                        TextFieldState::Hovered => (cfg.hover_bg_color, cfg.hover_border_color),
                        TextFieldState::Disabled => (
                            Color::rgba(0.12, 0.12, 0.15, 0.5),
                            Color::rgba(0.25, 0.25, 0.3, 0.5),
                        ),
                        _ => (cfg.bg_color, cfg.border_color),
                    };

                    // Apply visual styling directly to the container (preserves fixed dimensions)
                    // Note: Don't use set_padding_x/set_padding_y - use explicit spacers like TextInput
                    // This ensures proper visual separation from rounded corners
                    // Note: Don't use set_overflow_clip() here - let the Scroll handle clipping
                    // The outer container just provides visual styling (bg, border, rounded)
                    container.set_bg(bg);
                    container.set_border(cfg.border_width, border);
                    container.set_rounded(cfg.corner_radius);

                    // Build content wrapper with explicit padding spacers (like TextInput)
                    let content = TextArea::build_content(
                        *visual,
                        &data_guard,
                        &cfg,
                        Arc::clone(&shared_state_for_callback),
                    );
                    container.set_child(content);
                },
            ));

            shared.needs_visual_update = true;
        }

        // Ensure state handlers (hover/press) are registered immediately
        // so they're available for incremental diff
        inner.ensure_state_handlers_registered();

        let textarea = Self {
            inner,
            state: Arc::clone(state),
            config,
        };

        // Initialize scroll dimensions from default config
        textarea.update_scroll_dimensions();

        textarea
    }

    /// Create the inner Stateful element with all event handlers registered
    fn create_inner_with_handlers(
        shared_state: SharedState<TextFieldState>,
        data: SharedTextAreaState,
        config: Arc<Mutex<TextAreaConfig>>,
    ) -> Stateful<TextFieldState> {
        use blinc_core::events::event_types;

        let data_for_click = Arc::clone(&data);
        let data_for_text = Arc::clone(&data);
        let data_for_key = Arc::clone(&data);
        let config_for_click = Arc::clone(&config);
        let shared_for_click = Arc::clone(&shared_state);
        let shared_for_text = Arc::clone(&shared_state);
        let shared_for_key = Arc::clone(&shared_state);

        Stateful::with_shared_state(shared_state)
            // Handle mouse down to focus and position cursor
            .on_mouse_down(move |ctx| {
                // First, forcibly blur any previously focused text input/area
                set_focused_text_area(&data_for_click);

                // Get click position from context for cursor positioning
                let click_x = ctx.local_x;
                let click_y = ctx.local_y;

                // Get config values for visual line computation
                // Note: We DON'T use padding offsets here because local_x/local_y from the event
                // are relative to the innermost hit element (the text content), not the outer container
                let cfg = config_for_click.lock().unwrap();
                let font_size = cfg.font_size;
                let line_height = cfg.font_size * cfg.line_height;
                let available_width =
                    cfg.effective_width() - cfg.padding_x * 2.0 - cfg.border_width * 2.0;
                let wrap_enabled = cfg.wrap;
                drop(cfg);

                let needs_refresh = {
                    let mut d = match data_for_click.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled {
                        return;
                    }

                    // Set focus via FSM transition
                    {
                        let mut shared = shared_for_click.lock().unwrap();
                        if !shared.state.is_focused() {
                            // Transition to focused state
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

                    // Update data state
                    let was_focused = d.visual.is_focused();
                    if !was_focused {
                        d.visual = TextFieldState::Focused;
                        increment_focus_count();
                        request_continuous_redraw_pub();
                    }

                    // Update cached config values for visual line computation
                    d.font_size = font_size;
                    d.line_height = line_height;
                    d.available_width = available_width;
                    d.wrap_enabled = wrap_enabled;

                    // Ensure visual lines are computed before positioning cursor
                    // This is needed because click handler may run before the callback
                    // that computes visual lines during rebuild
                    if d.visual_lines.is_empty() || d.wrap_enabled {
                        d.compute_visual_lines();
                    }

                    // Position cursor at click location
                    // Use clicked_visual_line if set by a line element's click handler,
                    // otherwise fall back to y-coordinate calculation
                    let text_x = click_x.max(0.0);

                    let new_pos = if let Some(visual_line_idx) = d.clicked_visual_line.take() {
                        // Line element told us which visual line was clicked
                        // Use that for accurate positioning
                        d.cursor_position_from_visual_line(visual_line_idx, text_x)
                    } else {
                        // Fallback: try to compute from y coordinate
                        let text_y = click_y.max(0.0);
                        d.cursor_position_from_xy(text_x, text_y)
                    };
                    d.cursor = new_pos;
                    d.selection_start = None; // Clear any selection
                    d.reset_cursor_blink();

                    true // needs refresh
                }; // Lock released here

                // Trigger incremental refresh AFTER releasing the data lock
                if needs_refresh {
                    refresh_stateful(&shared_for_click);
                }
            })
            // Handle text input
            .on_event(event_types::TEXT_INPUT, move |ctx| {
                let needs_refresh = {
                    let mut d = match data_for_text.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled || !d.visual.is_focused() {
                        return;
                    }

                    if let Some(c) = ctx.key_char {
                        d.insert(&c.to_string());
                        d.reset_cursor_blink();
                        // Recompute visual lines after text change
                        d.compute_visual_lines();
                        // Ensure cursor is visible after text insertion (use cached values)
                        let line_height = d.line_height;
                        let viewport_height = d.viewport_height;
                        d.ensure_cursor_visible(line_height, viewport_height);
                        tracing::debug!("TextArea received char: {:?}, value: {}", c, d.value());
                        true
                    } else {
                        false
                    }
                }; // Lock released here

                // Trigger incremental refresh AFTER releasing the data lock
                if needs_refresh {
                    refresh_stateful(&shared_for_text);
                }
            })
            // Handle key down for navigation and deletion
            .on_key_down(move |ctx| {
                let needs_refresh = {
                    let mut d = match data_for_key.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled || !d.visual.is_focused() {
                        return;
                    }

                    let mut cursor_changed = true;
                    let mut should_blur = false;
                    let mut text_changed = false;
                    match ctx.key_code {
                        8 => {
                            // Backspace
                            d.delete_backward();
                            text_changed = true;
                            tracing::debug!("TextArea backspace, value: {}", d.value());
                        }
                        127 => {
                            // Delete
                            d.delete_forward();
                            text_changed = true;
                        }
                        13 => {
                            // Enter - insert newline
                            d.insert_newline();
                            text_changed = true;
                            tracing::debug!("TextArea newline, lines: {}", d.line_count());
                        }
                        37 => {
                            // Left arrow
                            d.move_left(ctx.shift);
                        }
                        39 => {
                            // Right arrow
                            d.move_right(ctx.shift);
                        }
                        38 => {
                            // Up arrow
                            d.move_up(ctx.shift);
                        }
                        40 => {
                            // Down arrow
                            d.move_down(ctx.shift);
                        }
                        36 => {
                            // Home
                            d.move_to_line_start(ctx.shift);
                        }
                        35 => {
                            // End
                            d.move_to_line_end(ctx.shift);
                        }
                        27 => {
                            // Escape - blur the textarea
                            should_blur = true;
                            cursor_changed = true;
                        }
                        _ => {
                            cursor_changed = false;
                        }
                    }

                    // Recompute visual lines after text changes
                    if text_changed {
                        d.compute_visual_lines();
                    }

                    if cursor_changed && !should_blur {
                        d.reset_cursor_blink();
                        // Ensure cursor is visible (auto-scroll if needed, use cached values)
                        let line_height = d.line_height;
                        let viewport_height = d.viewport_height;
                        d.ensure_cursor_visible(line_height, viewport_height);
                    }

                    (cursor_changed, should_blur)
                }; // Lock released here

                // Handle blur (Escape key)
                if needs_refresh.1 {
                    crate::widgets::text_input::blur_all_text_inputs();
                } else if needs_refresh.0 {
                    // Trigger incremental refresh AFTER releasing the data lock
                    refresh_stateful(&shared_for_key);
                }
            })
        // Note: Scroll events are handled by the scroll() widget inside build_content
    }

    /// Build the content div based on current visual state and data
    /// Returns a Div with explicit padding spacers (like TextInput) for proper
    /// visual separation from rounded corners.
    fn build_content(
        visual: TextFieldState,
        data: &TextAreaState,
        config: &TextAreaConfig,
        shared_state: SharedTextAreaState,
    ) -> Div {
        // Note: Visual styling (bg, border, rounded) is now applied directly to the
        // container in the callback via set_* methods, not here.

        let text_color = if data.is_empty() {
            config.placeholder_color
        } else if data.disabled {
            Color::rgba(0.4, 0.4, 0.4, 1.0)
        } else {
            config.text_color
        };

        // Check if cursor should be shown (focused state)
        let is_focused = visual.is_focused();
        let cursor_color = config.cursor_color;

        // Cursor dimensions
        let cursor_height = config.font_size * 1.2;
        let line_height = config.font_size * config.line_height;

        // Calculate available width for text (for wrap calculations)
        let text_area_width =
            config.effective_width() - config.padding_x * 2.0 - config.border_width * 2.0;

        // Use visual lines for cursor positioning (computed in callback before build_content)
        // This provides accurate cursor tracking for wrapped text
        let (cursor_x, cursor_visual_y) = if !data.visual_lines.is_empty() {
            data.cursor_position_from_visual_lines()
        } else {
            // Fallback: simple calculation when visual lines not yet computed
            let cursor_line = data.cursor.line;
            let cursor_col = data.cursor.column;
            let cursor_x = if cursor_col > 0 && cursor_line < data.lines.len() {
                let line_text = &data.lines[cursor_line];
                let text_before: String = line_text.chars().take(cursor_col).collect();
                crate::text_measure::measure_text(&text_before, config.font_size).width
            } else {
                0.0
            };
            let cursor_y = cursor_line as f32 * line_height;
            (cursor_x, cursor_y)
        };

        // Clone the cursor state for the canvas callback
        let cursor_state_for_canvas = Arc::clone(&data.cursor_state);

        // Build cursor canvas element (if focused)
        // The cursor is positioned inside the scroll content so it scrolls with text
        let cursor_canvas_opt = if is_focused {
            // Cursor top is based on visual line position plus vertical centering within line
            // Shift cursor UP - fonts have descender space at bottom which pushes visible text upward
            let descender_offset = config.font_size * 0.1;
            let cursor_top =
                cursor_visual_y + (line_height - cursor_height) / 2.0 - descender_offset;
            let cursor_left = cursor_x;

            {
                if let Ok(mut cs) = cursor_state_for_canvas.lock() {
                    cs.visible = true;
                    cs.color = cursor_color;
                    cs.x = cursor_x;
                    cs.animation = CursorAnimation::SmoothFade;
                }
            }

            let cursor_state_clone = Arc::clone(&cursor_state_for_canvas);
            let cursor_canvas = canvas(
                move |ctx: &mut dyn blinc_core::DrawContext,
                      bounds: crate::canvas::CanvasBounds| {
                    let cs = cursor_state_clone.lock().unwrap();

                    if !cs.visible {
                        return;
                    }

                    let opacity = cs.current_opacity();
                    if opacity < 0.01 {
                        return;
                    }

                    let color = blinc_core::Color::rgba(
                        cs.color.r,
                        cs.color.g,
                        cs.color.b,
                        cs.color.a * opacity,
                    );

                    ctx.fill_rect(
                        blinc_core::Rect::new(0.0, 0.0, cs.width, bounds.height),
                        blinc_core::CornerRadius::default(),
                        blinc_core::Brush::Solid(color),
                    );
                },
            )
            .absolute()
            .left(cursor_left)
            .top(cursor_top)
            .w(2.0)
            .h(cursor_height);

            Some(cursor_canvas)
        } else {
            if let Ok(mut cs) = cursor_state_for_canvas.lock() {
                cs.visible = false;
            }
            None
        };

        // Build text content - left-aligned column of text lines
        // Use relative positioning to allow cursor absolute positioning within
        // Note: Don't use w_full() here - each line has explicit width, and w_full would
        // cause content to extend into the rounded corner areas of the outer container
        // Use overflow_visible to prevent clipping cursor when it shifts up
        let mut text_content = div()
            .flex_col()
            .justify_start()
            .items_start()
            .relative()
            .overflow_visible();

        if data.is_empty() {
            // Use state's placeholder if available, otherwise fall back to config
            let placeholder = if !data.placeholder.is_empty() {
                &data.placeholder
            } else {
                &config.placeholder
            };

            // Placeholder always uses no_wrap for consistent appearance
            text_content = text_content.child(
                div()
                    .h(line_height)
                    .flex_row()
                    .items_center()
                    .w(text_area_width)
                    .child(
                        text(placeholder)
                            .size(config.font_size)
                            .color(text_color)
                            .text_left()
                            .no_wrap(),
                    ),
            );
        } else if config.wrap && !data.visual_lines.is_empty() {
            // Wrapping mode with computed visual lines
            // Render each visual line segment for precise cursor alignment
            // Each line has a click handler that stores its visual line index
            for (visual_line_idx, vl) in data.visual_lines.iter().enumerate() {
                let line_text = if vl.text.is_empty() {
                    " "
                } else {
                    vl.text.as_str()
                };
                let state_for_line = Arc::clone(&shared_state);

                // Each visual line has fixed height and a click handler
                text_content = text_content.child(
                    div()
                        .h(line_height)
                        .w(text_area_width)
                        .flex_row()
                        .items_center()
                        .on_mouse_down(move |_ctx| {
                            // Store which visual line was clicked so the main handler knows
                            if let Ok(mut state) = state_for_line.lock() {
                                state.clicked_visual_line = Some(visual_line_idx);
                            }
                        })
                        .child(
                            text(line_text)
                                .size(config.font_size)
                                .color(text_color)
                                .text_left()
                                .no_wrap(), // Visual lines are pre-wrapped, don't wrap again
                        ),
                );
            }
        } else if config.wrap {
            // Wrapping mode fallback: use natural text wrapping
            // This path is used when visual lines not yet computed
            // In this mode, line_idx corresponds to logical line (visual line not computed)
            for (line_idx, line) in data.lines.iter().enumerate() {
                let line_text = if line.is_empty() { " " } else { line.as_str() };
                let state_for_line = Arc::clone(&shared_state);

                // Don't use fixed height - let it grow based on wrapped content
                // Use min_h to ensure empty/short lines still have proper height
                text_content = text_content.child(
                    div()
                        .min_h(line_height)
                        .w(text_area_width)
                        .on_mouse_down(move |_ctx| {
                            // Store line index (logical line in fallback mode)
                            if let Ok(mut state) = state_for_line.lock() {
                                state.clicked_visual_line = Some(line_idx);
                            }
                        })
                        .child(
                            text(line_text)
                                .size(config.font_size)
                                .color(text_color)
                                .text_left(),
                            // No .no_wrap() - text wraps at container width
                        ),
                );
            }
        } else {
            // No-wrap mode: each line stays on single line, horizontally scrollable
            // In this mode, line_idx corresponds to both logical and visual line
            for (line_idx, line) in data.lines.iter().enumerate() {
                let line_text = if line.is_empty() { " " } else { line.as_str() };
                let state_for_line = Arc::clone(&shared_state);

                text_content = text_content.child(
                    div()
                        .h(line_height)
                        .flex_row()
                        .items_center()
                        .on_mouse_down(move |_ctx| {
                            // Store line index
                            if let Ok(mut state) = state_for_line.lock() {
                                state.clicked_visual_line = Some(line_idx);
                            }
                        })
                        .child(
                            text(line_text)
                                .size(config.font_size)
                                .color(text_color)
                                .text_left()
                                .no_wrap(),
                        ),
                );
            }
        }

        // Add cursor inside text_content so it scrolls with the text
        if let Some(cursor) = cursor_canvas_opt {
            text_content = text_content.child(cursor);
        }

        // Build wrapper with explicit padding spacers (like TextInput)
        // This ensures proper visual separation from rounded corners
        let padding_x = config.padding_x;
        let padding_y = config.padding_y;

        // Wrap text content in scroll container with shared physics
        // This provides proper scroll handling and clipping
        // TextArea scroll doesn't use bounce animation - just hard stops at edges
        // Note: Don't add rounded() to scroll - the outer container handles visual rounding
        let scrollable_content = Scroll::with_physics(Arc::clone(&data.scroll_physics))
            .direction(ScrollDirection::Vertical)
            .no_bounce()
            .flex_grow() // Take remaining space
            .child(text_content);

        // Main content wrapper - uses explicit sizing to ensure proper intrinsic dimensions
        // Using flex_grow/w_full causes issues with w_fit() ancestors because
        // Percent(1.0) doesn't resolve correctly when ancestors don't have fixed sizes.
        // Structure matches TextInput: outer styled container -> padding spacers -> clip container
        let content_width = config.effective_width();
        let content_height = config.effective_height();
        let inner_height = content_height - padding_y * 2.0;

        div()
            .flex_col()
            .w(content_width)
            .h(content_height)
            // Top padding spacer
            .child(div().h(padding_y).w(content_width))
            // Middle row with left/right padding and scroll content
            .child(
                div()
                    .flex_row()
                    .h(inner_height)
                    .w(content_width)
                    // Left padding spacer
                    .child(div().w(padding_x).h(inner_height))
                    // Scroll content in the middle (no rounded corners on scroll itself)
                    .child(scrollable_content)
                    // Right padding spacer
                    .child(div().w(padding_x).h(inner_height)),
            )
            // Bottom padding spacer
            .child(div().h(padding_y).w(content_width))
    }

    /// Set placeholder text
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        let placeholder = text.into();
        self.config.lock().unwrap().placeholder = placeholder.clone();
        if let Ok(mut s) = self.state.lock() {
            s.placeholder = placeholder;
        }
        self
    }

    /// Update cached scroll dimensions from config
    /// This must be called whenever config values that affect scroll calculation change
    fn update_scroll_dimensions(&self) {
        let cfg = self.config.lock().unwrap();
        let line_height = cfg.font_size * cfg.line_height;
        let viewport_height = cfg.effective_height() - cfg.padding_y * 2.0 - cfg.border_width * 2.0;
        let viewport_width = cfg.effective_width() - cfg.padding_x * 2.0 - cfg.border_width * 2.0;
        drop(cfg);

        if let Ok(mut s) = self.state.lock() {
            s.line_height = line_height;
            s.viewport_height = viewport_height;
            // Update scroll physics viewport dimensions
            if let Ok(mut physics) = s.scroll_physics.lock() {
                physics.viewport_height = viewport_height;
                physics.viewport_width = viewport_width;
            }
        }
    }

    /// Set number of visible rows (like HTML textarea rows attribute)
    pub fn rows(mut self, rows: usize) -> Self {
        let height = {
            let mut cfg = self.config.lock().unwrap();
            cfg.rows = Some(rows);
            cfg.effective_height()
        };
        self.inner = std::mem::take(&mut self.inner).h(height);
        self.update_scroll_dimensions();
        self
    }

    /// Set number of visible columns (like HTML textarea cols attribute)
    pub fn cols(mut self, cols: usize) -> Self {
        let width = {
            let mut cfg = self.config.lock().unwrap();
            cfg.cols = Some(cols);
            cfg.effective_width()
        };
        self.inner = std::mem::take(&mut self.inner).w(width);
        self
    }

    /// Set both rows and cols
    pub fn text_size(mut self, rows: usize, cols: usize) -> Self {
        let (width, height) = {
            let mut cfg = self.config.lock().unwrap();
            cfg.rows = Some(rows);
            cfg.cols = Some(cols);
            (cfg.effective_width(), cfg.effective_height())
        };
        self.inner = std::mem::take(&mut self.inner).w(width).h(height);
        self.update_scroll_dimensions();
        self
    }

    /// Set font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.config.lock().unwrap().font_size = size;
        self.update_scroll_dimensions();
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.lock().unwrap().disabled = disabled;
        if let Ok(mut s) = self.state.lock() {
            s.disabled = disabled;
            if disabled {
                s.visual = TextFieldState::Disabled;
            }
        }
        self
    }

    /// Set maximum length
    pub fn max_length(mut self, max: usize) -> Self {
        self.config.lock().unwrap().max_length = max;
        self
    }

    /// Enable or disable text wrapping
    ///
    /// When wrapping is enabled (default), long lines wrap to the next visual line.
    /// When disabled, text scrolls horizontally instead.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.config.lock().unwrap().wrap = wrap;
        self
    }

    /// Disable text wrapping (alias for `.wrap(false)`)
    pub fn no_wrap(self) -> Self {
        self.wrap(false)
    }

    // =========================================================================
    // Builder methods that return Self (shadow Div methods for fluent API)
    // =========================================================================

    pub fn w(mut self, px: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.width = px;
            cfg.cols = None;
        }
        self.inner = std::mem::take(&mut self.inner).w(px);
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.height = px;
            cfg.rows = None;
        }
        self.inner = std::mem::take(&mut self.inner).h(px);
        self.update_scroll_dimensions();
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.width = w;
            cfg.height = h;
            cfg.cols = None;
            cfg.rows = None;
        }
        self.inner = std::mem::take(&mut self.inner).size(w, h);
        self.update_scroll_dimensions();
        self
    }

    pub fn square(mut self, size: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).square(size);
        self
    }

    pub fn w_full(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).w_full();
        self
    }

    pub fn h_full(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).h_full();
        self
    }

    pub fn w_fit(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).w_fit();
        self
    }

    pub fn h_fit(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).h_fit();
        self
    }

    pub fn min_w(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).min_w(px);
        self
    }

    pub fn p(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).p(px);
        self
    }

    pub fn px(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).px(px);
        self
    }

    pub fn py(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).py(px);
        self
    }

    pub fn m(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).m(px);
        self
    }

    pub fn mx(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).mx(px);
        self
    }

    pub fn my(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).my(px);
        self
    }

    pub fn gap(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).gap(px);
        self
    }

    pub fn flex_row(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).flex_row();
        self
    }

    pub fn flex_col(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).flex_col();
        self
    }

    pub fn flex_grow(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).flex_grow();
        self
    }

    pub fn items_center(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).items_center();
        self
    }

    pub fn items_start(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).items_start();
        self
    }

    pub fn items_end(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).items_end();
        self
    }

    pub fn justify_center(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_center();
        self
    }

    pub fn justify_start(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_start();
        self
    }

    pub fn justify_end(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_end();
        self
    }

    pub fn justify_between(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_between();
        self
    }

    pub fn bg(mut self, color: impl Into<blinc_core::Brush>) -> Self {
        self.inner = std::mem::take(&mut self.inner).bg(color);
        self
    }

    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.lock().unwrap().corner_radius = radius;
        self.inner = std::mem::take(&mut self.inner).rounded(radius);
        self
    }

    pub fn border(mut self, width: f32, color: blinc_core::Color) -> Self {
        self.inner = std::mem::take(&mut self.inner).border(width, color);
        self
    }

    pub fn border_color(mut self, color: blinc_core::Color) -> Self {
        self.inner = std::mem::take(&mut self.inner).border_color(color);
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).border_width(width);
        self
    }

    pub fn shadow(mut self, shadow: blinc_core::Shadow) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow(shadow);
        self
    }

    pub fn shadow_sm(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow_sm();
        self
    }

    pub fn shadow_md(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow_md();
        self
    }

    pub fn shadow_lg(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow_lg();
        self
    }

    pub fn transform(mut self, transform: blinc_core::Transform) -> Self {
        self.inner = std::mem::take(&mut self.inner).transform(transform);
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).opacity(opacity);
        self
    }

    pub fn overflow_clip(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).overflow_clip();
        self
    }

    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.inner = std::mem::take(&mut self.inner).child(child);
        self
    }

    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: ElementBuilder + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).children(children);
        self
    }

    // Event handlers
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_click(handler);
        self
    }

    pub fn on_hover_enter<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_hover_enter(handler);
        self
    }

    pub fn on_hover_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_hover_leave(handler);
        self
    }

    pub fn on_mouse_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_mouse_down(handler);
        self
    }

    pub fn on_mouse_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_mouse_up(handler);
        self
    }

    pub fn on_focus<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_focus(handler);
        self
    }

    pub fn on_blur<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_blur(handler);
        self
    }

    pub fn on_key_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_key_down(handler);
        self
    }

    pub fn on_key_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_key_up(handler);
        self
    }

    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_scroll(handler);
        self
    }
}

/// Create a ready-to-use multi-line text area
///
/// The text area inherits ALL Div methods, so you have full layout control.
///
/// # Example
///
/// ```ignore
/// let state = text_area_state_with_placeholder("Enter message...");
/// text_area(&state)
///     .rows(4)
///     .w(400.0)
///     .rounded(12.0)
///     .shadow_sm()
/// ```
/// Create a text area widget
/// By default, width inherits from parent (w_full). Use .w() to set explicit width.
pub fn text_area(state: &SharedTextAreaState) -> TextArea {
    TextArea::new(state).w_full()
}

impl ElementBuilder for TextArea {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Set base render props for incremental updates
        // Note: callback and handlers are registered in new() so they're available for incremental diff
        {
            let shared_state = self.inner.shared_state();
            let mut shared = shared_state.lock().unwrap();
            shared.base_render_props = Some(self.inner.inner_render_props());
        }

        // Build the inner Stateful
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        crate::div::ElementTypeId::Div
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_area_state_insert() {
        let mut state = TextAreaState::new();
        state.insert("hello");
        assert_eq!(state.value(), "hello");

        state.insert_newline();
        state.insert("world");
        assert_eq!(state.value(), "hello\nworld");
        assert_eq!(state.line_count(), 2);
    }

    #[test]
    fn test_text_area_state_delete() {
        let mut state = TextAreaState::with_value("hello\nworld");
        state.cursor = TextPosition::new(1, 5);

        state.delete_backward();
        assert_eq!(state.value(), "hello\nworl");

        state.cursor = TextPosition::new(1, 0);
        state.delete_backward();
        assert_eq!(state.value(), "helloworl");
        assert_eq!(state.line_count(), 1);
    }

    #[test]
    fn test_text_area_state_navigation() {
        let mut state = TextAreaState::with_value("line1\nline2\nline3");
        state.cursor = TextPosition::new(1, 3);

        state.move_up(false);
        assert_eq!(state.cursor, TextPosition::new(0, 3));

        state.move_down(false);
        assert_eq!(state.cursor, TextPosition::new(1, 3));

        state.move_to_line_start(false);
        assert_eq!(state.cursor, TextPosition::new(1, 0));

        state.move_to_line_end(false);
        assert_eq!(state.cursor, TextPosition::new(1, 5));
    }

    #[test]
    fn test_text_area_state_selection() {
        let mut state = TextAreaState::with_value("hello\nworld");

        state.select_all();
        assert_eq!(state.selected_text(), Some("hello\nworld".to_string()));

        state.insert("new");
        assert_eq!(state.value(), "new");
        assert_eq!(state.line_count(), 1);
    }
}
