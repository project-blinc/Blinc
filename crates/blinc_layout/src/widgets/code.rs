//! Code block widget with syntax highlighting
//!
//! A code display/editing widget that supports:
//! - Syntax highlighting via regex-based token matching
//! - Optional line numbers in the gutter
//! - Read-only by default, editable with `.edit(true)`
//! - All Div layout methods via Deref
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_layout::syntax::{SyntaxConfig, RustHighlighter};
//!
//! // Read-only code block
//! code(r#"fn main() { println!("Hello"); }"#)
//!     .syntax(SyntaxConfig::new(RustHighlighter::new()))
//!     .line_numbers(true)
//!     .font_size(14.0)
//!     .rounded(8.0)
//!
//! // Editable code block with change callback
//! code("let x = 42;")
//!     .edit(true)
//!     .on_change(|new_content| {
//!         println!("Content changed: {}", new_content);
//!     })
//! ```

use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use blinc_core::{Brush, Color, CornerRadius, Rect};

use crate::canvas::canvas;
use crate::div::{div, Div, ElementBuilder, ElementTypeId};
use crate::element::RenderProps;
use crate::styled_text::StyledText;
use crate::syntax::{SyntaxConfig, SyntaxHighlighter, TokenHit};
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::cursor::{cursor_state, CursorAnimation, SharedCursorState};
use crate::widgets::text_area::TextPosition;
use crate::widgets::text_input::{
    decrement_focus_count, increment_focus_count, request_continuous_redraw_pub, request_rebuild,
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
    /// Whether editing is enabled
    pub editable: bool,
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
}

impl Default for CodeConfig {
    fn default() -> Self {
        Self {
            font_size: 13.0,
            line_height: 1.5,
            line_numbers: false,
            gutter_width: 48.0,
            padding: 16.0,
            corner_radius: 8.0,
            editable: false,
            bg_color: Color::rgba(0.12, 0.12, 0.14, 1.0),
            text_color: Color::rgba(0.9, 0.9, 0.9, 1.0),
            line_number_color: Color::rgba(0.45, 0.45, 0.5, 1.0),
            cursor_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            selection_color: Color::rgba(0.4, 0.6, 1.0, 0.3),
            gutter_bg_color: Color::rgba(0.10, 0.10, 0.12, 1.0),
            gutter_separator_color: Color::rgba(0.2, 0.2, 0.22, 1.0),
        }
    }
}

// ============================================================================
// Internal State (not exposed to users)
// ============================================================================

/// Internal state for editable code blocks
#[derive(Debug, Clone)]
struct CodeState {
    /// Lines of text
    lines: Vec<String>,
    /// Cursor position
    cursor: TextPosition,
    /// Selection start (if selecting)
    selection_start: Option<TextPosition>,
    /// Whether currently focused
    focused: bool,
    /// Canvas-based cursor state
    cursor_state: SharedCursorState,
}

impl Default for CodeState {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: TextPosition::default(),
            selection_start: None,
            focused: false,
            cursor_state: cursor_state(),
        }
    }
}

impl CodeState {
    fn new(content: &str) -> Self {
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(|s| s.to_string()).collect()
        };

        Self {
            lines,
            cursor: TextPosition::default(),
            selection_start: None,
            focused: false,
            cursor_state: cursor_state(),
        }
    }

    /// Get full text content
    fn value(&self) -> String {
        self.lines.join("\n")
    }

    /// Check if empty
    fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    /// Insert text at cursor position
    fn insert(&mut self, text: &str) {
        // Delete selection first if any
        self.delete_selection();

        for ch in text.chars() {
            if ch == '\n' {
                self.insert_newline();
            } else {
                self.insert_char(ch);
            }
        }
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
            let byte_pos = char_to_byte_pos(current_line, self.cursor.column);

            let new_line = current_line[byte_pos..].to_string();
            self.lines[self.cursor.line] = current_line[..byte_pos].to_string();
            self.lines.insert(self.cursor.line + 1, new_line);

            self.cursor.line += 1;
            self.cursor.column = 0;
        }
    }

    fn delete_backward(&mut self) {
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }

        if self.cursor.column > 0 {
            let line = &mut self.lines[self.cursor.line];
            let byte_pos = char_to_byte_pos(line, self.cursor.column - 1);
            let end_byte = char_to_byte_pos(line, self.cursor.column);
            line.replace_range(byte_pos..end_byte, "");
            self.cursor.column -= 1;
        } else if self.cursor.line > 0 {
            // Merge with previous line
            let current = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
            self.lines[self.cursor.line].push_str(&current);
        }
    }

    fn delete_forward(&mut self) {
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }

        if self.cursor.line < self.lines.len() {
            let line_len = self.lines[self.cursor.line].chars().count();
            if self.cursor.column < line_len {
                let line = &mut self.lines[self.cursor.line];
                let byte_pos = char_to_byte_pos(line, self.cursor.column);
                let end_byte = char_to_byte_pos(line, self.cursor.column + 1);
                line.replace_range(byte_pos..end_byte, "");
            } else if self.cursor.line + 1 < self.lines.len() {
                // Merge with next line
                let next = self.lines.remove(self.cursor.line + 1);
                self.lines[self.cursor.line].push_str(&next);
            }
        }
    }

    fn delete_selection(&mut self) {
        let Some(sel_start) = self.selection_start.take() else {
            return;
        };

        let (start, end) = order_positions(sel_start, self.cursor);

        if start.line == end.line {
            let line = &mut self.lines[start.line];
            let start_byte = char_to_byte_pos(line, start.column);
            let end_byte = char_to_byte_pos(line, end.column);
            line.replace_range(start_byte..end_byte, "");
        } else {
            // Multi-line selection
            let start_byte = char_to_byte_pos(&self.lines[start.line], start.column);
            let end_byte = char_to_byte_pos(&self.lines[end.line], end.column);

            let new_line = self.lines[start.line][..start_byte].to_string()
                + &self.lines[end.line][end_byte..];

            // Remove lines in between
            for _ in start.line..=end.line {
                self.lines.remove(start.line);
            }
            self.lines.insert(start.line, new_line);
        }

        self.cursor = start;
    }

    fn move_left(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }

        if self.cursor.column > 0 {
            self.cursor.column -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.column = self.lines[self.cursor.line].chars().count();
        }
    }

    fn move_right(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
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

    fn move_up(&mut self, select: bool) {
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

    fn move_down(&mut self, select: bool) {
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

    fn move_to_line_start(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor.column = 0;
    }

    fn move_to_line_end(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        if self.cursor.line < self.lines.len() {
            self.cursor.column = self.lines[self.cursor.line].chars().count();
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

type SharedCodeState = Arc<Mutex<CodeState>>;

// ============================================================================
// Code Widget
// ============================================================================

/// Type alias for the on_change callback
type OnChangeCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// Type alias for the on_token_click callback (for intellisense)
type OnTokenClickCallback = Arc<dyn Fn(&TokenHit) + Send + Sync + 'static>;

/// Code block widget
///
/// Displays code with optional syntax highlighting and line numbers.
/// By default read-only; use `.edit(true)` to enable editing.
pub struct Code {
    /// The actual visual structure (Div with all children)
    /// This is rebuilt whenever config changes
    inner: Div,
    /// Static content (for read-only mode)
    content: String,
    /// Internal state (for edit mode)
    state: SharedCodeState,
    /// Configuration
    config: CodeConfig,
    /// Syntax highlighter
    highlighter: Option<Arc<dyn SyntaxHighlighter>>,
    /// Change callback
    on_change: Option<OnChangeCallback>,
    /// Token click callback (for intellisense)
    on_token_click: Option<OnTokenClickCallback>,
    /// Whether inner needs rebuilding
    needs_rebuild: bool,
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
    /// Create a new code block with the given content
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        let state = Arc::new(Mutex::new(CodeState::new(&content)));
        let config = CodeConfig::default();

        let mut code = Self {
            inner: Div::new(),
            content,
            state,
            config,
            highlighter: None,
            on_change: None,
            on_token_click: None,
            needs_rebuild: true,
        };
        code.rebuild_inner();
        code
    }

    /// Rebuild the visual structure after config changes
    fn rebuild_inner(&mut self) {
        self.inner = self.create_visual_structure();
        self.needs_rebuild = false;
    }

    /// Mark that inner needs rebuilding (called by builder methods)
    fn mark_needs_rebuild(&mut self) {
        self.needs_rebuild = true;
    }

    // ========================================================================
    // Builder Methods
    // ========================================================================

    /// Enable or disable line numbers
    pub fn line_numbers(mut self, enabled: bool) -> Self {
        self.config.line_numbers = enabled;
        self.rebuild_inner();
        self
    }

    /// Enable or disable editing
    pub fn edit(mut self, enabled: bool) -> Self {
        self.config.editable = enabled;
        self.rebuild_inner();
        self
    }

    /// Set syntax highlighting configuration
    pub fn syntax(mut self, config: SyntaxConfig) -> Self {
        // Store colors before consuming config
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

    /// Set the font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.config.font_size = size;
        self.rebuild_inner();
        self
    }

    /// Set the line height multiplier
    pub fn line_height(mut self, multiplier: f32) -> Self {
        self.config.line_height = multiplier;
        self.rebuild_inner();
        self
    }

    /// Set the padding
    pub fn padding(mut self, padding: f32) -> Self {
        self.config.padding = padding;
        self.rebuild_inner();
        self
    }

    /// Set callback for content changes (when editable)
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_change = Some(Arc::new(callback));
        self
    }

    /// Set callback for token clicks (for intellisense features)
    ///
    /// The callback receives a `TokenHit` with information about the clicked token,
    /// including the token text, type, and position.
    ///
    /// # Example
    /// ```ignore
    /// code("fn main() {}")
    ///     .syntax(SyntaxConfig::new(RustHighlighter::new()))
    ///     .on_token_click(|hit| {
    ///         println!("Clicked on {:?}: {}", hit.token_type, hit.text);
    ///         // Show documentation, go to definition, etc.
    ///     })
    /// ```
    pub fn on_token_click<F>(mut self, callback: F) -> Self
    where
        F: Fn(&TokenHit) + Send + Sync + 'static,
    {
        self.on_token_click = Some(Arc::new(callback));
        self
    }

    /// Set background color
    pub fn code_bg(mut self, color: Color) -> Self {
        self.config.bg_color = color;
        self
    }

    /// Set text color
    pub fn text_color(mut self, color: Color) -> Self {
        self.config.text_color = color;
        self
    }

    // ========================================================================
    // Internal Methods
    // ========================================================================

    /// Get the styled content with syntax highlighting applied
    fn get_styled_content(&self) -> StyledText {
        let content = if self.config.editable {
            self.state.lock().unwrap().value()
        } else {
            self.content.clone()
        };

        if let Some(ref highlighter) = self.highlighter {
            highlighter.highlight(&content)
        } else {
            StyledText::plain(&content, self.config.text_color)
        }
    }

    /// Create the visual structure (the actual code display)
    fn create_visual_structure(&self) -> Div {
        let styled = self.get_styled_content();
        let line_height_px = self.config.font_size * self.config.line_height;
        let num_lines = styled.line_count().max(1);

        // Main container
        let mut container = div()
            .flex_row()
            .bg(self.config.bg_color)
            .rounded(self.config.corner_radius)
            .overflow_clip();

        // Line numbers gutter (if enabled)
        if self.config.line_numbers {
            let mut line_numbers_col = div()
                .flex_col()
                .padding_y_px(self.config.padding)
                .padding_x_px(8.0);

            for line_num in 1..=num_lines {
                line_numbers_col = line_numbers_col.child(
                    div()
                        .h(line_height_px)
                        .flex_row()
                        .justify_end()
                        .items_center()
                        .child(
                            text(format!("{}", line_num))
                                .size(self.config.font_size)
                                .color(self.config.line_number_color)
                                .text_right(),
                        ),
                );
            }

            // Gutter with separator (separator as a 1px wide div)
            let gutter = div()
                .flex_row()
                .bg(self.config.gutter_bg_color)
                .w(self.config.gutter_width)
                .child(line_numbers_col.flex_grow())
                .child(div().w(1.0).h_full().bg(self.config.gutter_separator_color));

            container = container.child(gutter);
        }

        // Code content area
        // Don't use overflow_clip here - rely on outer container's clip
        let mut code_area = div()
            .flex_col()
            .flex_grow()
            .padding_x_px(self.config.padding)
            .padding_y_px(self.config.padding)
            .relative();

        // Render each line with styled spans
        for styled_line in &styled.lines {
            // Don't use overflow_clip on line divs - rely on outer container's clip
            let mut line_div = div().h(line_height_px).flex_row().items_center();

            if styled_line.spans.is_empty() {
                // Empty line - add a space to maintain height
                line_div = line_div.child(
                    text(" ")
                        .size(self.config.font_size)
                        .color(self.config.text_color),
                );
            } else {
                // Render each span with its color
                for span in &styled_line.spans {
                    let span_text = &styled_line.text[span.start..span.end];
                    let mut txt = text(span_text)
                        .size(self.config.font_size)
                        .color(span.color)
                        .no_wrap(); // Don't wrap individual spans

                    if span.bold {
                        txt = txt.bold();
                    }

                    txt = txt.monospace();

                    line_div = line_div.child(txt);
                }
            }

            code_area = code_area.child(line_div);
        }

        // Add cursor if editable and focused
        if self.config.editable {
            let state = self.state.lock().unwrap();
            if state.focused {
                let cursor_height = self.config.font_size * 1.2;
                let cursor_line = state.cursor.line;
                let cursor_col = state.cursor.column;

                // Calculate cursor x position
                let cursor_x = if cursor_col > 0 && cursor_line < state.lines.len() {
                    let line_text = &state.lines[cursor_line];
                    let text_before: String = line_text.chars().take(cursor_col).collect();
                    crate::text_measure::measure_text(&text_before, self.config.font_size).width
                } else {
                    0.0
                };

                let cursor_top =
                    (cursor_line as f32 * line_height_px) + (line_height_px - cursor_height) / 2.0;

                let cursor_state_clone = Arc::clone(&state.cursor_state);

                // Update cursor state
                {
                    if let Ok(mut cs) = cursor_state_clone.lock() {
                        cs.visible = true;
                        cs.color = self.config.cursor_color;
                        cs.x = cursor_x;
                        cs.animation = CursorAnimation::SmoothFade;
                    }
                }

                drop(state);

                // Add cursor canvas
                let cursor_color = self.config.cursor_color;
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

                        let color = Color::rgba(
                            cursor_color.r,
                            cursor_color.g,
                            cursor_color.b,
                            cursor_color.a * opacity,
                        );

                        // bounds only has width/height; canvas is positioned at (0,0) in local coords
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
        }

        container = container.child(code_area);

        // Add event handlers if editable
        if self.config.editable {
            let state_for_click = Arc::clone(&self.state);
            let state_for_key = Arc::clone(&self.state);
            let state_for_text = Arc::clone(&self.state);
            let state_for_blur = Arc::clone(&self.state);
            let on_change_for_key = self.on_change.clone();
            let on_change_for_text = self.on_change.clone();

            container = container
                .on_mouse_down(move |_ctx| {
                    let mut s = state_for_click.lock().unwrap();
                    if !s.focused {
                        s.focused = true;
                        increment_focus_count();
                        request_continuous_redraw_pub();
                    }
                    request_rebuild();
                })
                .on_blur(move |_ctx| {
                    let mut s = state_for_blur.lock().unwrap();
                    s.focused = false;
                    s.selection_start = None;
                    if let Ok(mut cs) = s.cursor_state.lock() {
                        cs.visible = false;
                    }
                    decrement_focus_count();
                    request_rebuild();
                })
                .on_key_down(move |ctx| {
                    let mut s = state_for_key.lock().unwrap();
                    if !s.focused {
                        return;
                    }

                    let mut changed = false;
                    let mut cursor_changed = true;

                    match ctx.key_code {
                        8 => {
                            // Backspace
                            s.delete_backward();
                            changed = true;
                        }
                        127 => {
                            // Delete
                            s.delete_forward();
                            changed = true;
                        }
                        13 => {
                            // Enter
                            s.insert("\n");
                            changed = true;
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
                        9 => {
                            // Tab - insert spaces
                            s.insert("    ");
                            changed = true;
                        }
                        _ => {
                            cursor_changed = false;
                        }
                    }

                    // Reset cursor blink on keystroke
                    if cursor_changed {
                        if let Ok(mut cs) = s.cursor_state.lock() {
                            cs.reset_blink();
                        }
                    }

                    if changed {
                        if let Some(ref callback) = on_change_for_key {
                            callback(&s.value());
                        }
                    }

                    if cursor_changed {
                        request_rebuild();
                    }
                })
                .on_text_input(move |ctx| {
                    let mut s = state_for_text.lock().unwrap();
                    if !s.focused {
                        return;
                    }

                    if let Some(c) = ctx.key_char {
                        s.insert(&c.to_string());

                        // Reset cursor blink
                        if let Ok(mut cs) = s.cursor_state.lock() {
                            cs.reset_blink();
                        }

                        if let Some(ref callback) = on_change_for_text {
                            callback(&s.value());
                        }

                        request_rebuild();
                    }
                });
        }

        container
    }

    // ========================================================================
    // Shadowed Div Methods (return Self instead of Div)
    // ========================================================================

    /// Set width
    pub fn w(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).w(px);
        self
    }

    /// Set height
    pub fn h(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).h(px);
        self
    }

    /// Set width to 100%
    pub fn w_full(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).w_full();
        self
    }

    /// Set corner radius
    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = radius;
        self
    }

    /// Set margin
    pub fn m(mut self, value: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).m(value);
        self
    }

    /// Set margin top
    pub fn mt(mut self, value: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).mt(value);
        self
    }

    /// Set margin bottom
    pub fn mb(mut self, value: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).mb(value);
        self
    }
}

impl ElementBuilder for Code {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Delegate to the inner Div which has all children
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Delegate to inner - it has all the children
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div // Code renders as a composed Div structure
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }
}

// ============================================================================
// Convenience Constructors
// ============================================================================

/// Create a code block with the given content
///
/// # Example
/// ```ignore
/// code("fn main() {}")
///     .line_numbers(true)
///     .syntax(SyntaxConfig::new(RustHighlighter::new()))
/// ```
pub fn code(content: impl Into<String>) -> Code {
    Code::new(content)
}

/// Create a preformatted text block (alias for code)
///
/// Same as `code()` but semantically for preformatted text.
pub fn pre(content: impl Into<String>) -> Code {
    Code::new(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_creation() {
        let c = code("fn main() {}");
        assert!(!c.config.editable);
        assert!(!c.config.line_numbers);
    }

    #[test]
    fn test_code_builder() {
        let c = code("let x = 42;")
            .line_numbers(true)
            .edit(true)
            .font_size(14.0)
            .rounded(12.0);

        assert!(c.config.line_numbers);
        assert!(c.config.editable);
        assert_eq!(c.config.font_size, 14.0);
        assert_eq!(c.config.corner_radius, 12.0);
    }

    #[test]
    fn test_code_state_insert() {
        let mut state = CodeState::new("hello");
        state.cursor = TextPosition::new(0, 5);
        state.insert(" world");
        assert_eq!(state.value(), "hello world");
    }

    #[test]
    fn test_code_state_newline() {
        let mut state = CodeState::new("hello world");
        state.cursor = TextPosition::new(0, 5);
        state.insert_newline();
        assert_eq!(state.lines.len(), 2);
        assert_eq!(state.lines[0], "hello");
        assert_eq!(state.lines[1], " world");
    }

    #[test]
    fn test_code_state_delete() {
        let mut state = CodeState::new("hello");
        state.cursor = TextPosition::new(0, 5);
        state.delete_backward();
        assert_eq!(state.value(), "hell");
    }
}
