//! Ready-to-use TextArea widget
//!
//! Multi-line text area with:
//! - Multi-line text editing
//! - Row/column sizing (like HTML textarea)
//! - Cursor and selection
//! - Visual states: idle, hovered, focused
//! - Built-in styling that just works
//! - Inherits ALL Div methods for full layout control via Deref

use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use blinc_core::Color;

use crate::canvas::canvas;
use crate::div::{div, Div, ElementBuilder};
use crate::element::RenderProps;
use crate::stateful::TextFieldState;
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::cursor::{cursor_state, CursorAnimation, SharedCursorState};
use crate::widgets::text_input::{
    clear_focused_text_area, decrement_focus_count, elapsed_ms, increment_focus_count,
    request_continuous_redraw_pub, request_rebuild, set_focused_text_area,
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
    /// Maximum character count (0 = unlimited)
    pub max_length: usize,
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
            focused_bg_color: Color::rgba(0.18, 0.18, 0.25, 1.0),
            border_color: Color::rgba(0.3, 0.3, 0.35, 1.0),
            focused_border_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            border_width: 1.0,
            corner_radius: 8.0,
            padding_x: 12.0,
            padding_y: 10.0,
            cursor_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            selection_color: Color::rgba(0.4, 0.6, 1.0, 0.3),
            disabled: false,
            max_length: 0,
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

/// TextArea widget state
#[derive(Debug, Clone)]
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
    pub fn reset_cursor_blink(&mut self, current_time_ms: u64) {
        self.focus_time_ms = current_time_ms;
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

    /// Move cursor up
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

    /// Move cursor down
    pub fn move_down(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }

        if self.cursor.line < self.lines.len() - 1 {
            self.cursor.line += 1;
            let line_len = self.lines[self.cursor.line].chars().count();
            self.cursor.column = self.cursor.column.min(line_len);
        }
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
/// Inherits all Div methods via Deref, so you have full layout control.
///
/// Usage: `text_area(&state).rows(4).w(400.0).rounded(12.0)`
pub struct TextArea {
    /// Inner div - ALL Div methods are available via Deref
    inner: Div,
    /// Text area state
    state: SharedTextAreaState,
    /// Text area configuration
    config: TextAreaConfig,
}

// Deref to Div gives TextArea ALL Div methods for reading
impl Deref for TextArea {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for TextArea {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl TextArea {
    /// Create a new text area with shared state
    pub fn new(state: &SharedTextAreaState) -> Self {
        let config = TextAreaConfig::default();

        // Build initial visual structure with default event handlers
        let inner = Self::create_inner(&config, state);

        Self {
            inner,
            state: Arc::clone(state),
            config,
        }
    }

    /// Create the inner Div with visual structure and default event handlers
    fn create_inner(config: &TextAreaConfig, state: &SharedTextAreaState) -> Div {
        let state_guard = state.lock().unwrap();

        // Visual state-based styling
        let (bg, border) = match state_guard.visual {
            TextFieldState::Idle => (config.bg_color, config.border_color),
            TextFieldState::Hovered => (
                Color::rgba(0.18, 0.18, 0.23, 1.0),
                Color::rgba(0.4, 0.4, 0.45, 1.0),
            ),
            TextFieldState::Focused | TextFieldState::FocusedHovered => {
                (config.focused_bg_color, config.focused_border_color)
            }
            TextFieldState::Disabled => (
                Color::rgba(0.12, 0.12, 0.15, 0.5),
                Color::rgba(0.25, 0.25, 0.3, 0.5),
            ),
        };

        let text_color = if state_guard.is_empty() {
            config.placeholder_color
        } else if state_guard.disabled {
            Color::rgba(0.4, 0.4, 0.4, 1.0)
        } else {
            config.text_color
        };

        // Check if cursor should be shown (focused state)
        let is_focused = matches!(
            state_guard.visual,
            TextFieldState::Focused | TextFieldState::FocusedHovered
        );
        let cursor_color = config.cursor_color;

        // Get cursor position
        let cursor_line = state_guard.cursor.line;
        let cursor_col = state_guard.cursor.column;

        // Cursor dimensions
        let cursor_height = config.font_size * 1.2;
        let line_height = config.font_size * config.line_height;

        // Calculate cursor x position using text measurement
        let cursor_x = if cursor_col > 0 && cursor_line < state_guard.lines.len() {
            let line_text = &state_guard.lines[cursor_line];
            let text_before: String = line_text.chars().take(cursor_col).collect();
            crate::text_measure::measure_text(&text_before, config.font_size).width
        } else {
            0.0
        };

        // Clone the cursor state for the canvas callback
        let cursor_state_for_canvas = Arc::clone(&state_guard.cursor_state);

        // Build content - left-aligned column of text lines (cursor added separately)
        let mut content = div().flex_col().justify_start().items_start();

        if state_guard.is_empty() {
            // Use state's placeholder if available, otherwise fall back to config
            let placeholder = if !state_guard.placeholder.is_empty() {
                &state_guard.placeholder
            } else {
                &config.placeholder
            };

            content = content.child(
                div().h(line_height).flex_row().items_center().child(
                    text(placeholder)
                        .size(config.font_size)
                        .color(text_color)
                        .text_left(),
                ),
            );
        } else {
            for (_line_idx, line) in state_guard.lines.iter().enumerate() {
                let line_text = if line.is_empty() { " " } else { line.as_str() };
                // Render all lines normally (cursor is added as overlay)
                content = content.child(
                    div().h(line_height).flex_row().items_center().child(
                        text(line_text)
                            .size(config.font_size)
                            .color(text_color)
                            .text_left(),
                    ),
                );
            }
        }

        drop(state_guard);

        let mut inner_content = div()
            .w_full()
            .h_full()
            .bg(bg)
            .rounded(config.corner_radius - 1.0)
            .padding_y_px(config.padding_y) // Use raw pixels, not 4x units
            .padding_x_px(config.padding_x) // Use raw pixels, not 4x units
            .relative() // Enable absolute positioning for cursor overlay
            .flex_col()
            .justify_start() // Text starts from top
            .items_start() // Text starts from left
            .overflow_clip()
            .child(content);

        // Add cursor as a canvas-based overlay with smooth animation
        // The canvas handles its own opacity animation without tree rebuilds
        if is_focused {
            // Calculate cursor position
            let cursor_top = config.padding_y
                + (cursor_line as f32 * line_height)
                + (line_height - cursor_height) / 2.0;
            let cursor_left = config.padding_x + cursor_x;

            // Update cursor state for the canvas to read
            {
                if let Ok(mut cs) = cursor_state_for_canvas.lock() {
                    cs.visible = true;
                    cs.color = cursor_color;
                    cs.x = cursor_x;
                    cs.animation = CursorAnimation::SmoothFade;
                }
            }

            // Create canvas-based cursor with smooth fade animation
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

            inner_content = inner_content.child(cursor_canvas);
        } else {
            // Cursor not visible - update state
            if let Ok(mut cs) = cursor_state_for_canvas.lock() {
                cs.visible = false;
            }
        }

        // Build the outer container with size from config
        // Use FSM transitions via StateTransitions::on_event
        use crate::stateful::StateTransitions;
        use blinc_core::events::event_types;

        let state_for_click = Arc::clone(state);
        let state_for_blur = Arc::clone(state);
        let state_for_hover_enter = Arc::clone(state);
        let state_for_hover_leave = Arc::clone(state);
        let state_for_text_input = Arc::clone(state);
        let state_for_key_down = Arc::clone(state);

        div()
            .w(config.effective_width())
            .h(config.effective_height())
            .bg(border)
            .rounded(config.corner_radius)
            .p(config.border_width)
            .child(inner_content)
            // Wire up event handlers using FSM transitions
            .on_mouse_down(move |_ctx| {
                // First, forcibly blur any previously focused text input/area
                // This must be done BEFORE we lock our own state to avoid deadlock
                set_focused_text_area(&state_for_click);

                if let Ok(mut s) = state_for_click.lock() {
                    if !s.disabled {
                        // Try POINTER_DOWN first (Hovered -> Focused)
                        // Then try FOCUS as fallback (Idle -> Focused)
                        let was_focused = s.visual.is_focused();
                        let new_state = s
                            .visual
                            .on_event(event_types::POINTER_DOWN)
                            .or_else(|| s.visual.on_event(event_types::FOCUS));
                        if let Some(new_state) = new_state {
                            s.visual = new_state;
                            // Reset cursor blink on focus
                            s.reset_cursor_blink(elapsed_ms());
                            // Track focus globally (node-ID-independent)
                            if !was_focused && new_state.is_focused() {
                                increment_focus_count();
                                request_continuous_redraw_pub();
                            }
                        }
                    }
                }
                request_rebuild();
            })
            .on_blur(move |_ctx| {
                if let Ok(mut s) = state_for_blur.lock() {
                    if !s.disabled {
                        // Use FSM: BLUR triggers Focused -> Idle
                        let was_focused = s.visual.is_focused();
                        if let Some(new_state) = s.visual.on_event(event_types::BLUR) {
                            s.visual = new_state;
                            // Track focus globally (node-ID-independent)
                            if was_focused && !new_state.is_focused() {
                                decrement_focus_count();
                            }
                        }
                    }
                }
                // Clear this as the focused area if it was
                clear_focused_text_area(&state_for_blur);
                request_rebuild();
            })
            .on_hover_enter(move |_ctx| {
                if let Ok(mut s) = state_for_hover_enter.lock() {
                    if !s.disabled {
                        // Use FSM: POINTER_ENTER transitions hover states
                        if let Some(new_state) = s.visual.on_event(event_types::POINTER_ENTER) {
                            s.visual = new_state;
                            request_rebuild();
                        }
                    }
                }
            })
            .on_hover_leave(move |_ctx| {
                if let Ok(mut s) = state_for_hover_leave.lock() {
                    if !s.disabled {
                        // Use FSM: POINTER_LEAVE transitions hover states
                        if let Some(new_state) = s.visual.on_event(event_types::POINTER_LEAVE) {
                            s.visual = new_state;
                            request_rebuild();
                        }
                    }
                }
            })
            // Handle text input (character entry)
            .on_text_input(move |ctx| {
                if let Ok(mut s) = state_for_text_input.lock() {
                    if !s.disabled && s.visual.is_focused() {
                        if let Some(c) = ctx.key_char {
                            // Insert the character
                            s.insert(&c.to_string());
                            // Reset cursor blink to keep it visible while typing
                            s.reset_cursor_blink(elapsed_ms());
                            tracing::debug!(
                                "TextArea received char: {:?}, value: {}",
                                c,
                                s.value()
                            );
                            request_rebuild();
                        }
                    }
                }
            })
            // Handle special keys (backspace, arrows, enter, etc.)
            .on_key_down(move |ctx| {
                if let Ok(mut s) = state_for_key_down.lock() {
                    if !s.disabled && s.visual.is_focused() {
                        let mut cursor_changed = true;
                        match ctx.key_code {
                            8 => {
                                // Backspace
                                s.delete_backward();
                                tracing::debug!("TextArea backspace, value: {}", s.value());
                            }
                            127 => {
                                // Delete
                                s.delete_forward();
                            }
                            13 => {
                                // Enter - insert newline
                                s.insert_newline();
                                tracing::debug!("TextArea newline, lines: {}", s.line_count());
                            }
                            37 => {
                                // Left arrow
                                s.move_left(ctx.shift);
                            }
                            39 => {
                                // Right arrow
                                s.move_right(ctx.shift);
                            }
                            38 => {
                                // Up arrow
                                s.move_up(ctx.shift);
                            }
                            40 => {
                                // Down arrow
                                s.move_down(ctx.shift);
                            }
                            36 => {
                                // Home
                                s.move_to_line_start(ctx.shift);
                            }
                            35 => {
                                // End
                                s.move_to_line_end(ctx.shift);
                            }
                            _ => {
                                cursor_changed = false;
                            }
                        }
                        // Reset cursor blink to keep it visible during interaction
                        if cursor_changed {
                            s.reset_cursor_blink(elapsed_ms());
                            request_rebuild();
                        }
                    }
                }
            })
    }

    /// Rebuild the inner visual with current config and state
    fn rebuild_inner(&mut self) {
        self.inner = Self::create_inner(&self.config, &self.state);
    }

    /// Set placeholder text
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.config.placeholder = text.into();
        if let Ok(mut s) = self.state.lock() {
            s.placeholder = self.config.placeholder.clone();
        }
        self
    }

    /// Set number of visible rows (like HTML textarea rows attribute)
    pub fn rows(mut self, rows: usize) -> Self {
        self.config.rows = Some(rows);
        // Update inner height based on rows
        let height = self.config.effective_height();
        self.inner = std::mem::take(&mut self.inner).h(height);
        self
    }

    /// Set number of visible columns (like HTML textarea cols attribute)
    pub fn cols(mut self, cols: usize) -> Self {
        self.config.cols = Some(cols);
        // Update inner width based on cols
        let width = self.config.effective_width();
        self.inner = std::mem::take(&mut self.inner).w(width);
        self
    }

    /// Set both rows and cols
    pub fn text_size(mut self, rows: usize, cols: usize) -> Self {
        self.config.rows = Some(rows);
        self.config.cols = Some(cols);
        let width = self.config.effective_width();
        let height = self.config.effective_height();
        self.inner = std::mem::take(&mut self.inner).w(width).h(height);
        self
    }

    /// Set font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.config.font_size = size;
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
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
        self.config.max_length = max;
        self
    }

    // =========================================================================
    // Builder methods that return Self (shadow Div methods for fluent API)
    // =========================================================================

    pub fn w(mut self, px: f32) -> Self {
        self.config.width = px;
        self.config.cols = None;
        self.inner = std::mem::take(&mut self.inner).w(px);
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        self.config.height = px;
        self.config.rows = None;
        self.inner = std::mem::take(&mut self.inner).h(px);
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.config.width = w;
        self.config.height = h;
        self.config.cols = None;
        self.config.rows = None;
        self.inner = std::mem::take(&mut self.inner).size(w, h);
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
        self.inner = std::mem::take(&mut self.inner).background(color);
        self
    }

    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = radius;
        self.inner = std::mem::take(&mut self.inner).rounded(radius);
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

    pub fn overflow_visible(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).overflow_visible();
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
pub fn text_area(state: &SharedTextAreaState) -> TextArea {
    TextArea::new(state)
}

impl ElementBuilder for TextArea {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Build inner div - preserves event handlers
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
