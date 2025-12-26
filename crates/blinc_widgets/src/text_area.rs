//! TextArea widget with FSM-driven interactions
//!
//! The TextArea widget provides:
//! - Multi-line text input with cursor and scrolling
//! - Visual states: idle, hovered, focused, disabled
//! - FSM-driven state transitions
//! - Selection support with shift+arrow/click-drag
//! - Line navigation with up/down arrows
//! - Keyboard shortcuts: Cmd+A (select all), Cmd+C/X/V (clipboard)
//! - Cursor blinking animation
//! - Customizable appearance

use blinc_animation::spring::{Spring, SpringConfig};
use blinc_core::events::{event_types, Event, EventData, KeyCode, Modifiers};
use blinc_core::fsm::StateMachine;
use blinc_core::Color;
use blinc_layout::prelude::*;

use crate::context::WidgetContext;
use crate::widget::WidgetId;

/// TextArea FSM states (same as TextInput)
pub mod states {
    pub const IDLE: u32 = 0;
    pub const HOVERED: u32 = 1;
    pub const FOCUSED: u32 = 2;
    pub const FOCUSED_HOVERED: u32 = 3;
    pub const SELECTING: u32 = 4;
    pub const DISABLED: u32 = 5;
}

/// TextArea configuration
#[derive(Clone)]
pub struct TextAreaConfig {
    /// Placeholder text shown when empty
    pub placeholder: String,
    /// Initial value
    pub value: String,
    /// Width of the text area (can be overridden by cols)
    pub width: f32,
    /// Height of the text area (can be overridden by rows)
    pub height: f32,
    /// Number of visible rows (overrides height if set)
    pub rows: Option<usize>,
    /// Number of visible columns/character width (overrides width if set)
    pub cols: Option<usize>,
    /// Minimum number of rows (for auto-resize)
    pub min_rows: usize,
    /// Maximum number of rows (0 = unlimited, for auto-resize)
    pub max_rows: usize,
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
    /// Focused background color
    pub focused_bg_color: Color,
    /// Border color
    pub border_color: Color,
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
    /// Whether to resize height based on content
    pub auto_resize: bool,
    /// Maximum character count (0 = unlimited)
    pub max_length: usize,
}

impl Default for TextAreaConfig {
    fn default() -> Self {
        Self {
            placeholder: String::new(),
            value: String::new(),
            width: 300.0,
            height: 120.0,
            rows: None,
            cols: None,
            min_rows: 3,
            max_rows: 0,
            font_size: 14.0,
            line_height: 1.4,
            char_width_ratio: 0.6, // Approximate character width as ratio of font size
            text_color: Color::rgba(0.9, 0.9, 0.9, 1.0),
            placeholder_color: Color::rgba(0.5, 0.5, 0.5, 1.0),
            bg_color: Color::rgba(0.15, 0.15, 0.2, 1.0),
            focused_bg_color: Color::rgba(0.18, 0.18, 0.25, 1.0),
            border_color: Color::rgba(0.3, 0.3, 0.35, 1.0),
            focused_border_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            border_width: 1.0,
            corner_radius: 6.0,
            padding_x: 12.0,
            padding_y: 10.0,
            cursor_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            selection_color: Color::rgba(0.4, 0.6, 1.0, 0.3),
            disabled: false,
            auto_resize: false,
            max_length: 0,
        }
    }
}

impl TextAreaConfig {
    /// Calculate the effective width based on cols or explicit width
    pub fn effective_width(&self) -> f32 {
        if let Some(cols) = self.cols {
            // Calculate width from columns: cols * char_width + padding
            let char_width = self.font_size * self.char_width_ratio;
            cols as f32 * char_width + self.padding_x * 2.0 + self.border_width * 2.0
        } else {
            self.width
        }
    }

    /// Calculate the effective height based on rows or explicit height
    pub fn effective_height(&self) -> f32 {
        if let Some(rows) = self.rows {
            // Calculate height from rows: rows * line_height + padding
            let single_line_height = self.font_size * self.line_height;
            rows as f32 * single_line_height + self.padding_y * 2.0 + self.border_width * 2.0
        } else {
            self.height
        }
    }
}

impl TextAreaConfig {
    /// Create a new text area config
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the placeholder text
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Set the initial value
    pub fn value(mut self, text: impl Into<String>) -> Self {
        self.value = text.into();
        self
    }

    /// Set the width (overridden if cols is set)
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Set the height (overridden if rows is set)
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Set the number of visible rows (like HTML textarea rows attribute)
    /// This overrides the height setting
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = Some(rows);
        self
    }

    /// Set the number of visible columns/characters (like HTML textarea cols attribute)
    /// This overrides the width setting
    pub fn cols(mut self, cols: usize) -> Self {
        self.cols = Some(cols);
        self
    }

    /// Set both rows and cols at once
    pub fn size(mut self, rows: usize, cols: usize) -> Self {
        self.rows = Some(rows);
        self.cols = Some(cols);
        self
    }

    /// Set the font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Set the minimum number of rows (for auto-resize)
    pub fn min_rows(mut self, rows: usize) -> Self {
        self.min_rows = rows;
        self
    }

    /// Set the maximum number of rows (for auto-resize)
    pub fn max_rows(mut self, rows: usize) -> Self {
        self.max_rows = rows;
        self
    }

    /// Set whether the text area is disabled
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set the maximum length
    pub fn max_length(mut self, max: usize) -> Self {
        self.max_length = max;
        self
    }

    /// Set the corner radius
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }

    /// Enable auto-resize based on content
    pub fn auto_resize(mut self, enabled: bool) -> Self {
        self.auto_resize = enabled;
        self
    }
}

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

/// TextArea widget state
pub struct TextAreaState {
    /// Current text value (stored as lines)
    lines: Vec<String>,
    /// Cursor position
    pub cursor: TextPosition,
    /// Selection start position (if selecting)
    pub selection_start: Option<TextPosition>,
    /// Cursor blink timer
    cursor_blink: f32,
    /// Whether cursor is visible
    cursor_visible: bool,
    /// Focus animation spring
    focus_spring: Spring,
    /// Focus animation value
    pub focus_value: f32,
    /// Scroll offset (vertical)
    scroll_y: f32,
    /// Whether value changed
    changed: bool,
}

impl Clone for TextAreaState {
    fn clone(&self) -> Self {
        Self {
            lines: self.lines.clone(),
            cursor: self.cursor,
            selection_start: self.selection_start,
            cursor_blink: self.cursor_blink,
            cursor_visible: self.cursor_visible,
            focus_spring: Spring::new(SpringConfig::snappy(), self.focus_value),
            focus_value: self.focus_value,
            scroll_y: self.scroll_y,
            changed: self.changed,
        }
    }
}

impl TextAreaState {
    /// Create new text area state
    pub fn new(initial_value: String) -> Self {
        let lines: Vec<String> = if initial_value.is_empty() {
            vec![String::new()]
        } else {
            initial_value.lines().map(|s| s.to_string()).collect()
        };

        let cursor = TextPosition::new(
            lines.len().saturating_sub(1),
            lines.last().map(|l| l.chars().count()).unwrap_or(0),
        );

        Self {
            lines,
            cursor,
            selection_start: None,
            cursor_blink: 0.0,
            cursor_visible: true,
            focus_spring: Spring::new(SpringConfig::snappy(), 0.0),
            focus_value: 0.0,
            scroll_y: 0.0,
            changed: false,
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

    /// Update animations
    pub fn update(&mut self, dt: f32, is_focused: bool) {
        // Update focus spring
        self.focus_spring
            .set_target(if is_focused { 1.0 } else { 0.0 });
        self.focus_spring.step(dt);
        self.focus_value = self.focus_spring.value();

        // Update cursor blink
        if is_focused {
            self.cursor_blink += dt;
            if self.cursor_blink >= 0.53 {
                self.cursor_blink = 0.0;
                self.cursor_visible = !self.cursor_visible;
            }
        } else {
            self.cursor_visible = true;
            self.cursor_blink = 0.0;
        }
    }

    /// Reset cursor blink
    pub fn reset_blink(&mut self) {
        self.cursor_blink = 0.0;
        self.cursor_visible = true;
    }

    /// Insert text at cursor
    pub fn insert(&mut self, text: &str, max_length: usize) {
        // Delete selection first
        self.delete_selection();

        // Check max length
        let current_len: usize = self.lines.iter().map(|l| l.chars().count()).sum::<usize>()
            + self.lines.len().saturating_sub(1);

        if max_length > 0 && current_len + text.chars().count() > max_length {
            let allowed = max_length.saturating_sub(current_len);
            if allowed == 0 {
                return;
            }
            let truncated: String = text.chars().take(allowed).collect();
            self.insert_at_cursor(&truncated);
        } else {
            self.insert_at_cursor(text);
        }

        self.reset_blink();
        self.changed = true;
    }

    fn insert_at_cursor(&mut self, text: &str) {
        let line_idx = self.cursor.line.min(self.lines.len().saturating_sub(1));

        // Handle multi-line insert
        if text.contains('\n') {
            let insert_lines: Vec<&str> = text.lines().collect();
            let col_byte = char_to_byte_pos(&self.lines[line_idx], self.cursor.column);
            let line = &mut self.lines[line_idx];

            // Split current line at cursor
            let after_cursor = line.split_off(col_byte);

            // Append first part of inserted text to current line
            if let Some(first) = insert_lines.first() {
                line.push_str(first);
            }

            // Insert middle lines
            for (i, insert_line) in insert_lines.iter().skip(1).enumerate() {
                let is_last = i == insert_lines.len() - 2;
                if is_last {
                    // Append remaining text to last line
                    let mut new_line = insert_line.to_string();
                    new_line.push_str(&after_cursor);
                    self.lines.insert(line_idx + 1 + i, new_line);
                    self.cursor.line = line_idx + 1 + i;
                    self.cursor.column = insert_line.chars().count();
                } else {
                    self.lines.insert(line_idx + 1 + i, insert_line.to_string());
                }
            }
        } else {
            // Single line insert
            let col_byte = char_to_byte_pos(&self.lines[line_idx], self.cursor.column);
            self.lines[line_idx].insert_str(col_byte, text);
            self.cursor.column += text.chars().count();
        }
    }

    /// Insert a newline at cursor
    pub fn insert_newline(&mut self) {
        self.delete_selection();

        let line_idx = self.cursor.line.min(self.lines.len().saturating_sub(1));
        let col_byte = char_to_byte_pos(&self.lines[line_idx], self.cursor.column);

        // Split line at cursor
        let after = self.lines[line_idx].split_off(col_byte);
        self.lines.insert(line_idx + 1, after);

        self.cursor.line += 1;
        self.cursor.column = 0;
        self.reset_blink();
        self.changed = true;
    }

    /// Delete character before cursor (backspace)
    pub fn delete_backward(&mut self) {
        if self.delete_selection() {
            self.changed = true;
            return;
        }

        if self.cursor.column > 0 {
            let start_byte =
                char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column - 1);
            let end_byte = char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column);
            self.lines[self.cursor.line].replace_range(start_byte..end_byte, "");
            self.cursor.column -= 1;
            self.changed = true;
        } else if self.cursor.line > 0 {
            // Join with previous line
            let current_line = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
            self.lines[self.cursor.line].push_str(&current_line);
            self.changed = true;
        }
        self.reset_blink();
    }

    /// Delete character after cursor (delete)
    pub fn delete_forward(&mut self) {
        if self.delete_selection() {
            self.changed = true;
            return;
        }

        let line_len = self.lines[self.cursor.line].chars().count();
        if self.cursor.column < line_len {
            let start_byte = char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column);
            let end_byte = char_to_byte_pos(&self.lines[self.cursor.line], self.cursor.column + 1);
            self.lines[self.cursor.line].replace_range(start_byte..end_byte, "");
            self.changed = true;
        } else if self.cursor.line < self.lines.len() - 1 {
            // Join with next line
            let next_line = self.lines.remove(self.cursor.line + 1);
            self.lines[self.cursor.line].push_str(&next_line);
            self.changed = true;
        }
        self.reset_blink();
    }

    /// Delete selected text
    fn delete_selection(&mut self) -> bool {
        if let Some(start) = self.selection_start {
            let (from, to) = self.order_positions(start, self.cursor);

            if from != to {
                // Delete text between positions
                if from.line == to.line {
                    // Same line
                    let start_byte = char_to_byte_pos(&self.lines[from.line], from.column);
                    let end_byte = char_to_byte_pos(&self.lines[from.line], to.column);
                    self.lines[from.line].replace_range(start_byte..end_byte, "");
                } else {
                    // Multiple lines
                    // Keep text before 'from' on first line
                    let from_byte = char_to_byte_pos(&self.lines[from.line], from.column);
                    self.lines[from.line].truncate(from_byte);

                    // Keep text after 'to' from last line and append to first
                    let to_byte = char_to_byte_pos(&self.lines[to.line], to.column);
                    let after_text = self.lines[to.line][to_byte..].to_string();
                    self.lines[from.line].push_str(&after_text);

                    // Remove lines in between (including 'to' line)
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
        self.reset_blink();
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
        self.reset_blink();
    }

    /// Move cursor up
    pub fn move_up(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }

        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            // Clamp column to line length
            let line_len = self.lines[self.cursor.line].chars().count();
            self.cursor.column = self.cursor.column.min(line_len);
        }
        self.reset_blink();
    }

    /// Move cursor down
    pub fn move_down(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }

        if self.cursor.line < self.lines.len() - 1 {
            self.cursor.line += 1;
            // Clamp column to line length
            let line_len = self.lines[self.cursor.line].chars().count();
            self.cursor.column = self.cursor.column.min(line_len);
        }
        self.reset_blink();
    }

    /// Move to start of line
    pub fn move_to_line_start(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor.column = 0;
        self.reset_blink();
    }

    /// Move to end of line
    pub fn move_to_line_end(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor.column = self.lines[self.cursor.line].chars().count();
        self.reset_blink();
    }

    /// Move to start of text
    pub fn move_to_start(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor = TextPosition::new(0, 0);
        self.reset_blink();
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
        self.reset_blink();
    }

    /// Select all text
    pub fn select_all(&mut self) {
        self.selection_start = Some(TextPosition::new(0, 0));
        let last_line = self.lines.len().saturating_sub(1);
        self.cursor = TextPosition::new(last_line, self.lines[last_line].chars().count());
        self.reset_blink();
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
                // First line (from cursor to end)
                result.extend(self.lines[from.line].chars().skip(from.column));

                // Middle lines
                for line in &self.lines[from.line + 1..to.line] {
                    result.push('\n');
                    result.push_str(line);
                }

                // Last line (start to cursor)
                if to.line > from.line {
                    result.push('\n');
                    result.extend(self.lines[to.line].chars().take(to.column));
                }

                result
            }
        })
    }

    /// Check if value changed and clear flag
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

/// Convert character index to byte index (helper function)
fn char_to_byte_pos(line: &str, char_pos: usize) -> usize {
    line.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}

/// TextArea widget
pub struct TextArea {
    /// Widget ID
    id: WidgetId,
    /// Configuration
    config: TextAreaConfig,
    /// Change callback
    on_change: Option<Box<dyn FnMut(&str) + Send>>,
}

impl TextArea {
    /// Create a new text area
    pub fn new(ctx: &mut WidgetContext) -> Self {
        Self::with_config(ctx, TextAreaConfig::default())
    }

    /// Create a text area with custom config
    pub fn with_config(ctx: &mut WidgetContext, config: TextAreaConfig) -> Self {
        let fsm = Self::create_fsm(&config);
        let id = ctx.register_widget_with_fsm(fsm);

        let state = TextAreaState::new(config.value.clone());
        ctx.set_widget_state(id, state);

        Self {
            id,
            config,
            on_change: None,
        }
    }

    /// Create the text area FSM
    fn create_fsm(config: &TextAreaConfig) -> StateMachine {
        if config.disabled {
            StateMachine::builder(states::DISABLED).build()
        } else {
            StateMachine::builder(states::IDLE)
                .on(states::IDLE, event_types::POINTER_ENTER, states::HOVERED)
                .on(states::IDLE, event_types::FOCUS, states::FOCUSED)
                .on(states::HOVERED, event_types::POINTER_LEAVE, states::IDLE)
                .on(states::HOVERED, event_types::POINTER_DOWN, states::FOCUSED)
                .on(states::HOVERED, event_types::FOCUS, states::FOCUSED_HOVERED)
                .on(states::FOCUSED, event_types::BLUR, states::IDLE)
                .on(
                    states::FOCUSED,
                    event_types::POINTER_ENTER,
                    states::FOCUSED_HOVERED,
                )
                .on(
                    states::FOCUSED,
                    event_types::POINTER_DOWN,
                    states::SELECTING,
                )
                .on(
                    states::FOCUSED_HOVERED,
                    event_types::POINTER_LEAVE,
                    states::FOCUSED,
                )
                .on(states::FOCUSED_HOVERED, event_types::BLUR, states::HOVERED)
                .on(
                    states::FOCUSED_HOVERED,
                    event_types::POINTER_DOWN,
                    states::SELECTING,
                )
                .on(
                    states::SELECTING,
                    event_types::POINTER_UP,
                    states::FOCUSED_HOVERED,
                )
                .on(states::SELECTING, event_types::BLUR, states::IDLE)
                .build()
        }
    }

    /// Get the widget ID
    pub fn id(&self) -> WidgetId {
        self.id
    }

    /// Check if the text area is focused
    pub fn is_focused(&self, ctx: &WidgetContext) -> bool {
        let state = ctx.get_fsm_state(self.id).unwrap_or(states::IDLE);
        matches!(
            state,
            states::FOCUSED | states::FOCUSED_HOVERED | states::SELECTING
        )
    }

    /// Get the current value
    pub fn value(&self, ctx: &WidgetContext) -> String {
        ctx.get_widget_state::<TextAreaState>(self.id)
            .map(|s| s.value())
            .unwrap_or_default()
    }

    /// Set the value programmatically
    pub fn set_value(&self, ctx: &mut WidgetContext, value: impl AsRef<str>) {
        if let Some(state) = ctx.get_widget_state_mut::<TextAreaState>(self.id) {
            state.set_value(value.as_ref());
        }
    }

    /// Set the change callback
    pub fn on_change<F: FnMut(&str) + Send + 'static>(mut self, callback: F) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }

    /// Handle an event
    pub fn handle_event(&mut self, ctx: &mut WidgetContext, event: &Event) {
        if self.config.disabled {
            return;
        }

        let was_focused = self.is_focused(ctx);
        ctx.dispatch_event(self.id, event);
        let is_focused = self.is_focused(ctx);

        if is_focused {
            match &event.data {
                EventData::TextInput { text } => {
                    if let Some(state) = ctx.get_widget_state_mut::<TextAreaState>(self.id) {
                        state.insert(text, self.config.max_length);
                        if state.take_changed() {
                            let value = state.value();
                            if let Some(ref mut callback) = self.on_change {
                                callback(&value);
                            }
                        }
                    }
                }
                EventData::Key { key, modifiers, .. } => {
                    self.handle_key(ctx, *key, *modifiers);
                }
                EventData::Clipboard { text } => {
                    if let Some(state) = ctx.get_widget_state_mut::<TextAreaState>(self.id) {
                        state.insert(text, self.config.max_length);
                        if state.take_changed() {
                            let value = state.value();
                            if let Some(ref mut callback) = self.on_change {
                                callback(&value);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Select all on focus
        if !was_focused && is_focused {
            if let Some(state) = ctx.get_widget_state_mut::<TextAreaState>(self.id) {
                state.select_all();
            }
        }
    }

    /// Handle keyboard input
    fn handle_key(&mut self, ctx: &mut WidgetContext, key: KeyCode, modifiers: Modifiers) {
        let state = match ctx.get_widget_state_mut::<TextAreaState>(self.id) {
            Some(s) => s,
            None => return,
        };

        let select = modifiers.shift();
        let command = modifiers.command();

        match key {
            KeyCode::BACKSPACE => {
                if command {
                    state.move_to_line_start(true);
                    state.delete_selection();
                    state.changed = true;
                } else {
                    state.delete_backward();
                }
            }
            KeyCode::DELETE => {
                if command {
                    state.move_to_line_end(true);
                    state.delete_selection();
                    state.changed = true;
                } else {
                    state.delete_forward();
                }
            }
            KeyCode::LEFT => {
                if command {
                    state.move_to_line_start(select);
                } else {
                    state.move_left(select);
                }
            }
            KeyCode::RIGHT => {
                if command {
                    state.move_to_line_end(select);
                } else {
                    state.move_right(select);
                }
            }
            KeyCode::UP => {
                if command {
                    state.move_to_start(select);
                } else {
                    state.move_up(select);
                }
            }
            KeyCode::DOWN => {
                if command {
                    state.move_to_end(select);
                } else {
                    state.move_down(select);
                }
            }
            KeyCode::HOME => state.move_to_start(select),
            KeyCode::END => state.move_to_end(select),
            KeyCode::A if command => state.select_all(),
            KeyCode::ENTER => state.insert_newline(),
            _ => {}
        }

        if state.take_changed() {
            let value = state.value();
            if let Some(ref mut callback) = self.on_change {
                callback(&value);
            }
        }
    }

    /// Update animations
    pub fn update(&self, ctx: &mut WidgetContext, dt: f32) {
        let is_focused = self.is_focused(ctx);

        if let Some(state) = ctx.get_widget_state_mut::<TextAreaState>(self.id) {
            let old_focus = state.focus_value;
            let old_visible = state.cursor_visible;

            state.update(dt, is_focused);

            if (state.focus_value - old_focus).abs() > 0.001 || state.cursor_visible != old_visible
            {
                ctx.mark_dirty(self.id);
            }
        }
    }

    /// Build the text area's UI element
    pub fn build(&self, ctx: &WidgetContext) -> Div {
        let state = ctx
            .get_widget_state::<TextAreaState>(self.id)
            .cloned()
            .unwrap_or_else(|| TextAreaState::new(String::new()));

        let fsm_state = ctx.get_fsm_state(self.id).unwrap_or(states::IDLE);
        let is_focused = matches!(
            fsm_state,
            states::FOCUSED | states::FOCUSED_HOVERED | states::SELECTING
        );

        let bg_color = Color::lerp(
            &self.config.bg_color,
            &self.config.focused_bg_color,
            state.focus_value,
        );
        let border_color = Color::lerp(
            &self.config.border_color,
            &self.config.focused_border_color,
            state.focus_value,
        );

        // Build text lines
        let line_height = self.config.font_size * self.config.line_height;
        let is_empty = state.lines.len() == 1 && state.lines[0].is_empty();

        let mut content = div().w_full().flex_col().gap(0.0);

        if is_empty {
            // Show placeholder
            content = content.child(
                text(&self.config.placeholder)
                    .size(self.config.font_size)
                    .color(self.config.placeholder_color),
            );
        } else {
            // Show each line
            for (i, line_text) in state.lines.iter().enumerate() {
                let mut line_div = div().h(line_height).flex_row().items_center();

                if line_text.is_empty() {
                    // Empty line - still show cursor if on this line
                    if is_focused && state.cursor_visible && state.cursor.line == i {
                        line_div = line_div.child(
                            div()
                                .w(2.0)
                                .h(self.config.font_size)
                                .bg(self.config.cursor_color)
                                .rounded(1.0),
                        );
                    }
                } else {
                    line_div = line_div.child(
                        text(line_text)
                            .size(self.config.font_size)
                            .color(self.config.text_color),
                    );

                    // Add cursor at end of line if focused
                    if is_focused && state.cursor_visible && state.cursor.line == i {
                        line_div = line_div.child(
                            div()
                                .w(2.0)
                                .h(self.config.font_size)
                                .bg(self.config.cursor_color)
                                .rounded(1.0),
                        );
                    }
                }

                content = content.child(line_div);
            }
        }

        // Inner container
        let inner = div()
            .w_full()
            .h_full()
            .bg(bg_color)
            .rounded(self.config.corner_radius.max(1.0) - 1.0)
            .p(self.config.padding_y)
            .px(self.config.padding_x)
            .overflow_clip()
            .child(content);

        // Outer container (border) - use effective dimensions
        div()
            .w(self.config.effective_width())
            .h(self.config.effective_height())
            .bg(border_color)
            .rounded(self.config.corner_radius)
            .p(self.config.border_width)
            .child(inner)
    }
}

/// Create a text area
pub fn text_area() -> TextAreaBuilder {
    TextAreaBuilder {
        config: TextAreaConfig::default(),
        on_change: None,
    }
}

/// Builder for creating text areas
pub struct TextAreaBuilder {
    config: TextAreaConfig,
    on_change: Option<Box<dyn FnMut(&str) + Send>>,
}

impl TextAreaBuilder {
    /// Set the placeholder text
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.config.placeholder = text.into();
        self
    }

    /// Set the initial value
    pub fn value(mut self, text: impl Into<String>) -> Self {
        self.config.value = text.into();
        self
    }

    /// Set the width (overridden if cols is set)
    pub fn width(mut self, width: f32) -> Self {
        self.config.width = width;
        self
    }

    /// Set the height (overridden if rows is set)
    pub fn height(mut self, height: f32) -> Self {
        self.config.height = height;
        self
    }

    /// Set the number of visible rows (like HTML textarea rows attribute)
    pub fn rows(mut self, rows: usize) -> Self {
        self.config.rows = Some(rows);
        self
    }

    /// Set the number of visible columns/characters (like HTML textarea cols attribute)
    pub fn cols(mut self, cols: usize) -> Self {
        self.config.cols = Some(cols);
        self
    }

    /// Set both rows and cols at once
    pub fn size(mut self, rows: usize, cols: usize) -> Self {
        self.config.rows = Some(rows);
        self.config.cols = Some(cols);
        self
    }

    /// Set the font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.config.font_size = size;
        self
    }

    /// Set the corner radius
    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = radius;
        self
    }

    /// Set whether the text area is disabled
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Set the maximum length
    pub fn max_length(mut self, max: usize) -> Self {
        self.config.max_length = max;
        self
    }

    /// Set the minimum number of rows (for auto-resize)
    pub fn min_rows(mut self, rows: usize) -> Self {
        self.config.min_rows = rows;
        self
    }

    /// Set the maximum number of rows (for auto-resize)
    pub fn max_rows(mut self, rows: usize) -> Self {
        self.config.max_rows = rows;
        self
    }

    /// Enable auto-resize based on content
    pub fn auto_resize(mut self, enabled: bool) -> Self {
        self.config.auto_resize = enabled;
        self
    }

    /// Set the background color
    pub fn bg(mut self, color: impl Into<Color>) -> Self {
        self.config.bg_color = color.into();
        self
    }

    /// Set the text color
    pub fn text_color(mut self, color: impl Into<Color>) -> Self {
        self.config.text_color = color.into();
        self
    }

    /// Set the change callback
    pub fn on_change<F: FnMut(&str) + Send + 'static>(mut self, callback: F) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }

    /// Build the text area widget
    pub fn build(self, ctx: &mut WidgetContext) -> TextArea {
        let mut area = TextArea::with_config(ctx, self.config);
        area.on_change = self.on_change;
        area
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_area_state_insert() {
        let mut state = TextAreaState::new(String::new());

        state.insert("hello", 0);
        assert_eq!(state.value(), "hello");

        state.insert_newline();
        state.insert("world", 0);
        assert_eq!(state.value(), "hello\nworld");
        assert_eq!(state.line_count(), 2);
    }

    #[test]
    fn test_text_area_state_delete() {
        let mut state = TextAreaState::new("hello\nworld".to_string());
        state.cursor = TextPosition::new(1, 5);

        state.delete_backward();
        assert_eq!(state.value(), "hello\nworl");

        // Move to start of second line and backspace to join lines
        state.cursor = TextPosition::new(1, 0);
        state.delete_backward();
        assert_eq!(state.value(), "helloworl");
        assert_eq!(state.line_count(), 1);
    }

    #[test]
    fn test_text_area_state_navigation() {
        let mut state = TextAreaState::new("line1\nline2\nline3".to_string());
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
        let mut state = TextAreaState::new("hello\nworld".to_string());

        state.select_all();
        assert_eq!(state.selected_text(), Some("hello\nworld".to_string()));

        state.insert("new", 0);
        assert_eq!(state.value(), "new");
        assert_eq!(state.line_count(), 1);
    }

    #[test]
    fn test_text_area_creation() {
        let mut ctx = WidgetContext::new();
        let area = TextArea::new(&mut ctx);

        assert!(ctx.is_registered(area.id()));
        assert_eq!(ctx.get_fsm_state(area.id()), Some(states::IDLE));
    }
}
