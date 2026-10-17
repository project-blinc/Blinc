//! Ready-to-use TextArea widget
//!
//! Multi-line text area with:
//! - Multi-line text editing
//! - Row/column sizing (like HTML textarea)
//! - Cursor and selection
//! - Visual states: idle, hovered, focused
//! - Built-in styling that just works
//! - Inherits ALL Div methods for full layout control via Deref

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use blinc_core::Color;
use blinc_core::reactive::SignalId;
use blinc_theme::{ColorToken, ThemeState};

use crate::canvas::canvas;
use crate::css_parser::{ElementState, Stylesheet, active_stylesheet};
use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::stateful::{StateTransitions, TextFieldState};
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::cursor::{CursorAnimation, SharedCursorState, cursor_state};
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

// =============================================================================
// CSS Override Resolution
// =============================================================================

/// Apply CSS stylesheet overrides to a TextAreaConfig.
fn apply_css_overrides_textarea(
    cfg: &mut TextAreaConfig,
    stylesheet: &Stylesheet,
    element_id: Option<&str>,
    css_classes: &[std::sync::Arc<str>],
    visual: &TextFieldState,
) {
    let state = match visual {
        TextFieldState::Hovered | TextFieldState::FocusedHovered => Some(ElementState::Hover),
        TextFieldState::Focused => Some(ElementState::Focus),
        TextFieldState::Disabled => Some(ElementState::Disabled),
        TextFieldState::Idle => None,
    };

    // 1. Apply class-based styles (lowest priority — overridden by ID).
    // FocusedHovered order: base → :hover → :focus, so focus colour wins
    // over hover while the user is typing. See text_input::apply_css_overrides
    // for the full rationale.
    for class in css_classes {
        if let Some(base) = stylesheet.get_class(class) {
            apply_style_to_textarea_config(cfg, base, visual);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_class_with_state(class, s) {
                apply_style_to_textarea_config(cfg, state_style, visual);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(s) = stylesheet.get_class_with_state(class, ElementState::Focus) {
                apply_style_to_textarea_config(cfg, s, visual);
            }
        }
    }

    // 2. Apply ID-based styles (higher priority — overrides class)
    if let Some(element_id) = element_id {
        if let Some(base) = stylesheet.get(element_id) {
            apply_style_to_textarea_config(cfg, base, visual);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_with_state(element_id, s) {
                apply_style_to_textarea_config(cfg, state_style, visual);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(focus_style) = stylesheet.get_with_state(element_id, ElementState::Focus) {
                apply_style_to_textarea_config(cfg, focus_style, visual);
            }
        }

        // 3. Apply ::placeholder style (ID-only)
        if let Some(placeholder_style) = stylesheet.get_placeholder_style(element_id) {
            if let Some(color) = placeholder_style.text_color {
                cfg.placeholder_color = color;
            }
            if let Some(color) = placeholder_style.placeholder_color {
                cfg.placeholder_color = color;
            }
        }
    }
}

fn apply_style_to_textarea_config(
    cfg: &mut TextAreaConfig,
    style: &crate::element_style::ElementStyle,
    visual: &TextFieldState,
) {
    if let Some(blinc_core::Brush::Solid(color)) = style.background.as_ref() {
        match visual {
            TextFieldState::Idle => cfg.bg_color = *color,
            TextFieldState::Hovered => cfg.hover_bg_color = *color,
            TextFieldState::Focused | TextFieldState::FocusedHovered => {
                cfg.focused_bg_color = *color;
            }
            TextFieldState::Disabled => {}
        }
    }
    if let Some(color) = style.border_color {
        match visual {
            TextFieldState::Idle => cfg.border_color = color,
            TextFieldState::Hovered => cfg.hover_border_color = color,
            TextFieldState::Focused | TextFieldState::FocusedHovered => {
                cfg.focused_border_color = color;
            }
            TextFieldState::Disabled => {}
        }
    }
    if let Some(w) = style.border_width {
        cfg.border_width = w;
    }
    if let Some(cr) = style.corner_radius {
        cfg.corner_radius = cr.top_left;
    }
    if let Some(color) = style.text_color {
        cfg.text_color = color;
    }
    if let Some(size) = style.font_size {
        cfg.font_size = size;
    }
    if let Some(color) = style.caret_color {
        cfg.cursor_color = color;
    }
    if let Some(color) = style.selection_color {
        cfg.selection_color = color;
    }
    if let Some(color) = style.placeholder_color {
        cfg.placeholder_color = color;
    }
}

/// Extract outline properties from stylesheet for the current state.
///
/// Walks class selectors first (lowest priority) and then `#id`
/// selectors (overrides). Without the class branch, focus-ring rules
/// like `.cn-textarea:focus { outline: 2px solid var(--border-focus); }`
/// from the cn stylesheet silently no-op because cn::textarea
/// attaches via class, not id. Mirrors the same fix on
/// `text_input::extract_outline_from_stylesheet`.
fn extract_outline_from_textarea_stylesheet(
    stylesheet: &Stylesheet,
    element_id: Option<&str>,
    css_classes: &[std::sync::Arc<str>],
    visual: &TextFieldState,
) -> Option<(f32, Color, f32)> {
    let mut width = None;
    let mut color = None;
    let mut offset = None;

    let state = match visual {
        TextFieldState::Hovered | TextFieldState::FocusedHovered => Some(ElementState::Hover),
        TextFieldState::Focused => Some(ElementState::Focus),
        TextFieldState::Disabled => Some(ElementState::Disabled),
        TextFieldState::Idle => None,
    };

    let mut absorb = |s: &crate::element_style::ElementStyle| {
        if let Some(w) = s.outline_width {
            width = Some(w);
        }
        if let Some(c) = s.outline_color {
            color = Some(c);
        }
        if let Some(o) = s.outline_offset {
            offset = Some(o);
        }
    };

    // 1. Class-based styles (lowest priority — overridden by ID).
    // FocusedHovered order: base → :hover → :focus, so the focus ring
    // wins over hover.
    for class in css_classes {
        if let Some(base) = stylesheet.get_class(class) {
            absorb(base);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_class_with_state(class, s) {
                absorb(state_style);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(s) = stylesheet.get_class_with_state(class, ElementState::Focus) {
                absorb(s);
            }
        }
    }

    // 2. ID-based styles (overrides class)
    if let Some(element_id) = element_id {
        if let Some(base) = stylesheet.get(element_id) {
            absorb(base);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_with_state(element_id, s) {
                absorb(state_style);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(focus_style) = stylesheet.get_with_state(element_id, ElementState::Focus) {
                absorb(focus_style);
            }
        }
    }

    width.map(|w| {
        (
            w,
            color.unwrap_or(Color::rgba(0.23, 0.51, 0.97, 0.5)),
            offset.unwrap_or(0.0),
        )
    })
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
    /// Whether `width` was asked for by the caller (`w`, `cols`, `size`,
    /// `text_size`) rather than measured after layout.
    ///
    /// `width` doubles as the text-wrapping width, so the layout bounds
    /// callback keeps it in sync with the box the widget actually got.
    /// Emitting that measured value as a fixed style width is what must
    /// not happen: it equals the parent's available width by
    /// construction, and a child exactly filling its parent tips taffy
    /// into sizing the ancestor by min-content instead of its children.
    /// Unset, the wrapper stays auto-width and stretches, which is both
    /// correct on the first frame and immune to that edge.
    pub width_is_explicit: bool,
}

impl Default for TextAreaConfig {
    fn default() -> Self {
        let theme = ThemeState::get();
        Self {
            placeholder: String::new(),
            width: 300.0,
            width_is_explicit: false,
            height: 120.0,
            rows: None,
            cols: None,
            font_size: 14.0,
            line_height: 1.4,
            char_width_ratio: 0.6,
            text_color: theme.color(ColorToken::TextPrimary),
            placeholder_color: theme.color(ColorToken::TextTertiary),
            bg_color: theme.color(ColorToken::InputBg),
            hover_bg_color: theme.color(ColorToken::InputBgHover),
            focused_bg_color: theme.color(ColorToken::InputBgFocus),
            border_color: theme.color(ColorToken::BorderSecondary),
            hover_border_color: theme.color(ColorToken::BorderHover),
            focused_border_color: theme.color(ColorToken::BorderFocus),
            border_width: 1.5,
            corner_radius: 8.0,
            padding_x: 12.0,
            padding_y: 10.0,
            cursor_color: theme.color(ColorToken::Accent),
            selection_color: theme.color(ColorToken::Selection),
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
    /// Anchor position for drag-to-select. Set in on_mouse_down when
    /// the user clicks (mouse, not touch) and read by the on_drag
    /// handler to extend `selection_start` as the cursor moves while
    /// pressed. `None` outside an active drag-select gesture.
    /// Mirrors `TextInputData.drag_select_anchor` but uses a 2D
    /// `TextPosition` because text_area positions are
    /// (line, column) rather than the single `usize` byte offset
    /// text_input uses.
    pub(crate) drag_select_anchor: Option<TextPosition>,
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
    /// Bumped whenever something the field shows changes; its content is
    /// bound to it. `None` until a `TextArea` is made over the state.
    pub(crate) revision: Option<blinc_core::reactive::Signal<u64>>,
    /// The field's node, once built.
    pub(crate) node_id: Option<LayoutNodeId>,
    /// The config of the field made over this state, which lays out its lines.
    pub(crate) config: Option<Arc<Mutex<TextAreaConfig>>>,
    /// [`Self::layout_key`] as of the last [`Self::compute_visual_lines`].
    pub(crate) laid_out: Option<u64>,
    /// Last clicked visual line index (set by line click handlers, read by main handler)
    /// This is used to accurately determine which line was clicked when local_y
    /// is relative to the clicked line element rather than the whole text area.
    pub(crate) clicked_visual_line: Option<usize>,
    /// Change version counter - increments on each text change
    /// Use this with `signal_id()` to depend on text changes from external components
    pub(crate) change_version: Arc<AtomicU64>,
    /// Signal ID for the change version (set externally when binding to reactive system)
    pub(crate) change_signal_id: Option<SignalId>,
    /// Invoked with the new text after an edit. Runs OUTSIDE the state
    /// lock -- a callback that writes a signal can end up back in this
    /// widget, and holding the lock across that deadlocks.
    pub(crate) on_change: Option<crate::widgets::TextCallback>,
    /// Layout bounds storage - updated after layout to get actual rendered dimensions
    pub layout_bounds_storage: crate::renderer::LayoutBoundsStorage,
    /// CSS element ID for stylesheet matching (set via TextArea::id())
    pub(crate) css_element_id: Option<String>,
    /// CSS class names for stylesheet matching (set via TextArea::class())
    pub(crate) css_classes: Vec<std::sync::Arc<str>>,
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
            .finish()
    }
}

impl Default for TextAreaState {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: TextPosition::default(),
            selection_start: None,
            drag_select_anchor: None,
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
            revision: None,
            node_id: None,
            config: None,
            laid_out: None,
            clicked_visual_line: None, // Set by line click handlers
            change_version: Arc::new(AtomicU64::new(0)),
            change_signal_id: None,
            on_change: None,
            layout_bounds_storage: Arc::new(Mutex::new(None)),
            css_element_id: None,
            css_classes: Vec::new(),
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

    /// Get the signal ID for text change notifications
    ///
    /// Use this to depend on text changes from external components.
    /// Returns None if no signal has been set.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let text_state = ctx.use_state_keyed("editor", || Arc::new(Mutex::new(TextAreaState::new())));
    /// let state = text_state.get();
    ///
    /// // Set up a signal for the text area
    /// let change_signal = ctx.use_signal(0u64);
    /// state.lock().unwrap().set_change_signal(change_signal.id());
    ///
    /// // Use the signal as a dependency for live preview
    /// stateful(preview_state)
    ///     .deps(&[state.lock().unwrap().signal_id().unwrap()])
    ///     .child(markdown_light(&state.lock().unwrap().value()))
    /// ```
    pub fn signal_id(&self) -> Option<SignalId> {
        self.change_signal_id
    }

    /// Set the signal ID for text change notifications
    ///
    /// When text changes, this signal will be notified (via check_stateful_deps).
    pub fn set_change_signal(&mut self, signal_id: SignalId) {
        self.change_signal_id = Some(signal_id);
    }

    /// Get the current change version
    ///
    /// This increments each time text is modified.
    pub fn change_version(&self) -> u64 {
        self.change_version.load(Ordering::SeqCst)
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
                    self.insert_newline_internal();
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
        self.insert_newline_internal();
    }

    /// Internal newline insertion (no notify)
    fn insert_newline_internal(&mut self) {
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

    /// Move cursor one word left
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
            self.cursor.column = crate::widgets::text_edit::word_boundary_left(
                &self.lines[self.cursor.line],
                self.cursor.column,
            );
        }
    }

    /// Move cursor one word right
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
                self.cursor.column = crate::widgets::text_edit::word_boundary_right(
                    &self.lines[self.cursor.line],
                    self.cursor.column,
                );
            }
        }
    }

    /// Delete from cursor to the previous word boundary
    pub fn delete_word_backward(&mut self) {
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }
        let saved = self.cursor;
        self.move_word_left(false);
        if self.cursor != saved {
            self.selection_start = Some(saved);
            self.delete_selection();
        }
    }

    /// Delete from cursor to the next word boundary
    pub fn delete_word_forward(&mut self) {
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }
        let saved = self.cursor;
        self.move_word_right(false);
        if self.cursor != saved {
            self.selection_start = Some(saved);
            self.cursor = saved;
            // Swap: we want to delete forward, so select from cursor to word end
            self.selection_start = Some(self.cursor);
            self.move_word_right(false);
            let end = self.cursor;
            self.cursor = saved;
            self.selection_start = Some(end);
            self.delete_selection();
        }
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

    /// What the visual lines are laid out from: the text and the sizes.
    fn layout_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        self.lines.hash(&mut h);
        self.font_size.to_bits().hash(&mut h);
        self.available_width.to_bits().hash(&mut h);
        self.wrap_enabled.hash(&mut h);
        h.finish()
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
        self.laid_out = Some(self.layout_key());
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

/// Enqueue a text_area for focus on the NEXT frame, after its widget
/// has been built into the tree.
///
/// Companion to [`crate::widgets::text_input::focus_text_input_deferred`].
/// See that function for the timing rationale: focusing BEFORE the
/// matching overlay mounts triggers continuous_redraw against an
/// unstable composition; deferring until after mount avoids that.
pub fn focus_text_area_deferred(state: &SharedTextAreaState) {
    crate::widgets::text_input::enqueue_pending_focus_area(Arc::downgrade(state));
}

/// Programmatically focus a text_area by its shared state handle.
///
/// Mirrors [`crate::widgets::text_input::focus_text_input`]: sets the
/// widget's visual state to `Focused`, marks it as the active focus
/// target for keyboard / IME / cursor-blink machinery, blurs any
/// previously-focused text_input or text_area, and increments the
/// soft-keyboard refcount. Use from a parent widget's open-handler
/// when an embedded text area should grab focus on appearance (e.g.
/// an inline editor popover that should land cursor-ready).
///
/// Safe to call before the matching [`TextArea`] is built: it is built
/// showing the state's visual state, so it starts out focused.
pub fn focus_text_area(state: &SharedTextAreaState) {
    use crate::widgets::text_input::{increment_focus_count, set_focused_text_area};
    use blinc_core::events::event_types;
    let focused = if let Ok(mut s) = state.lock() {
        if !s.visual.is_focused() {
            if let Some(new_state) = s.visual.on_event(event_types::FOCUS) {
                s.visual = new_state;
            } else {
                s.visual = TextFieldState::Focused;
            }
            s.focus_time_ms = crate::widgets::text_input::elapsed_ms();
            s.reset_cursor_blink();
            increment_focus_count();
            true
        } else {
            false
        }
    } else {
        false
    };
    set_focused_text_area(state);
    if focused {
        notify_text_area(state);
    }
}

/// Lay the lines out again and repaint the field: everything it shows is
/// bound to the revision. Called after any change to the state.
pub(crate) fn notify_text_area(state: &SharedTextAreaState) {
    // The config is copied out first: a layout callback locks the config
    // and then the state.
    let base = state
        .lock()
        .ok()
        .and_then(|d| d.config.clone())
        .and_then(|c| c.lock().ok().map(|c| c.clone()));
    let revision = match state.lock() {
        Ok(mut d) => {
            if let Some(base) = &base {
                let (cfg, _) = resolved_config(base, &d);
                take_metrics(&mut d, &cfg);
            }
            if d.laid_out != Some(d.layout_key()) {
                d.compute_visual_lines();
            }
            d.revision
        }
        Err(_) => None,
    };
    if let Some(revision) = revision {
        revision.update(|n| n.wrapping_add(1));
    }
    crate::stateful::request_redraw();
}

/// Repaint a text area after its state was changed from outside the widget.
pub fn refresh_text_area(state: &SharedTextAreaState) {
    notify_text_area(state);
}

/// Copy the sizes the state lays its lines out with from `cfg`.
fn take_metrics(d: &mut TextAreaState, cfg: &TextAreaConfig) {
    d.line_height = cfg.font_size * cfg.line_height;
    d.viewport_height = cfg.effective_height() - cfg.padding_y * 2.0 - cfg.border_width * 2.0;
    d.font_size = cfg.font_size;
    d.available_width = cfg.effective_width() - cfg.padding_x * 2.0 - cfg.border_width * 2.0;
    d.wrap_enabled = cfg.wrap;
}

/// The config with the stylesheet's rules for the field's state applied,
/// and the outline those rules give it.
fn resolved_config(
    base: &TextAreaConfig,
    d: &TextAreaState,
) -> (TextAreaConfig, Option<(f32, Color, f32)>) {
    let mut cfg = base.clone();
    if d.css_element_id.is_none() && d.css_classes.is_empty() {
        return (cfg, None);
    }
    let Some(stylesheet) = active_stylesheet() else {
        return (cfg, None);
    };
    apply_css_overrides_textarea(
        &mut cfg,
        &stylesheet,
        d.css_element_id.as_deref(),
        &d.css_classes,
        &d.visual,
    );
    let outline = extract_outline_from_textarea_stylesheet(
        &stylesheet,
        d.css_element_id.as_deref(),
        &d.css_classes,
        &d.visual,
    );
    (cfg, outline)
}

/// Ready-to-use text area element
///
/// Built once: what it shows is bound to its state's revision, so typing,
/// focus, hover, selection and scrolling patch it in place.
///
/// Usage: `text_area(&state).rows(4).w(400.0).rounded(12.0)`
pub struct TextArea {
    /// The element as configured so far: layout calls and the handlers land
    /// on it as they are made. The content is added when it is first built.
    inner: std::cell::RefCell<Div>,
    built: std::cell::OnceCell<Div>,
    /// Text area state (content, cursor, etc.)
    state: SharedTextAreaState,
    /// Text area configuration
    config: Arc<Mutex<TextAreaConfig>>,
    /// Caller's handle onto this field, if bound.
    element_ref: Option<crate::selector::DivRef>,
}

impl TextArea {
    /// Bind a [`crate::selector::TextareaRef`] to this field.
    ///
    /// The state half is handed over now — the field is built from
    /// state its caller already holds — and the element half is
    /// resolved by the renderer when the field is built.
    pub fn bind(mut self, textarea_ref: &crate::selector::TextareaRef) -> Self {
        textarea_ref.bind_state(&self.state);
        self.element_ref = Some(textarea_ref.element().clone());
        self
    }

    /// Create a new text area with shared state
    pub fn new(state: &SharedTextAreaState) -> Self {
        let config = Arc::new(Mutex::new(TextAreaConfig::default()));
        let (default_width, default_height) = {
            let cfg = config.lock().unwrap();
            (cfg.effective_width(), cfg.effective_height())
        };
        // A field with a revision is one a deferred focus can land on.
        if let Ok(mut d) = state.lock() {
            d.revision
                .get_or_insert_with(|| blinc_core::reactive::signal(0u64));
            d.config = Some(Arc::clone(&config));
        }

        // HTML-like flex behaviour: `min_w(0)` lets the field shrink below
        // its content in a flex container; the height is always explicit.
        // The inner scroll clips, so this does not: clipping here would cut
        // into the rounded border.
        let inner = Self::create_inner_with_handlers(Arc::clone(state), Arc::clone(&config))
            .w(default_width)
            .h(default_height)
            .min_w(0.0);

        let textarea = Self {
            inner: std::cell::RefCell::new(inner),
            built: std::cell::OnceCell::new(),
            state: Arc::clone(state),
            config,
            element_ref: None,
        };
        textarea.update_scroll_dimensions();
        textarea
    }

    /// The element, finished on first use: the content and the bound looks
    /// added to what the builder calls made.
    fn element(&self) -> &Div {
        crate::build_once::build_once(&self.built, || {
            let outer = self.inner.take();
            let cfg = self.config.lock().unwrap().clone();
            assemble(outer, &self.state, &cfg)
        })
    }

    /// Create the element with all the event handlers registered
    fn create_inner_with_handlers(
        data: SharedTextAreaState,
        config: Arc<Mutex<TextAreaConfig>>,
    ) -> Div {
        use blinc_core::events::event_types;

        let data_for_click = Arc::clone(&data);
        let data_for_text = Arc::clone(&data);
        let data_for_key = Arc::clone(&data);
        let data_for_drag = Arc::clone(&data);
        let config_for_click = Arc::clone(&config);
        let config_for_drag = Arc::clone(&config);
        let data_for_enter = Arc::clone(&data);
        let data_for_leave = Arc::clone(&data);

        div()
            // The pointer over the field changes its look.
            .on_hover_enter(move |_| pointer_crossed(&data_for_enter, event_types::POINTER_ENTER))
            .on_hover_leave(move |_| pointer_crossed(&data_for_leave, event_types::POINTER_LEAVE))
            // Handle mouse down to focus and position cursor
            .on_mouse_down(move |ctx| {
                // First, forcibly blur any previously focused text input/area
                set_focused_text_area(&data_for_click);

                // Get click position from context for cursor positioning.
                // `ctx.local_x` / `local_y` are relative to THIS handler's
                // node — the field's box — not the inner text
                // content. The wrapper renders a `padding_x`-wide left
                // spacer + a `border_width` border before the scroll
                // container, and a `padding_y` top spacer before the
                // middle row. Subtract both to get text-content-relative
                // coordinates; otherwise every click lands `padding_x +
                // border_width` ≈ 13.5px (≈ 2 chars at the default font)
                // to the right of the intended character. Same fix shape
                // text_input uses at its own click handler.
                let cfg = config_for_click.lock().unwrap();
                let font_size = cfg.font_size;
                let line_height = cfg.font_size * cfg.line_height;
                let available_width =
                    cfg.effective_width() - cfg.padding_x * 2.0 - cfg.border_width * 2.0;
                let wrap_enabled = cfg.wrap;
                let text_origin_x = cfg.padding_x + cfg.border_width;
                let text_origin_y = cfg.padding_y + cfg.border_width;
                drop(cfg);
                let click_x = ctx.local_x - text_origin_x;
                let click_y = ctx.local_y - text_origin_y;

                let needs_refresh = {
                    let mut d = match data_for_click.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled {
                        return;
                    }

                    // Bump the focus-tap generation counter so the
                    // mobile runner sees this as a "user tapped a text
                    // area" event even if it was already focused. See
                    // `text_input::focus_tap_generation` docs.
                    crate::widgets::text_input::bump_focus_tap_generation();

                    // Register the layout node id with the generic
                    // focused-editable atomic so the scroll-into-view
                    // helper has a single lookup that covers every
                    // text-editable widget regardless of which typed
                    // tracker (text_input vs text_area) holds the data.
                    //
                    // No blur callback — text_area has its own
                    // dedicated `FOCUSED_TEXT_AREA` tracker that the
                    // typed blur path walks.
                    crate::widgets::text_input::set_focused_editable_node(ctx.node_id, None);

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
                    // Seed the drag-select anchor for desktop mouse
                    // clicks (touch leaves anchor = None so a drag
                    // becomes cursor reposition, mirroring
                    // text_input's behaviour). The .on_drag handler
                    // below reads this and extends `selection_start`
                    // as the pointer moves while pressed.
                    if crate::widgets::text_input::is_touch_input() {
                        d.drag_select_anchor = None;
                    } else {
                        d.drag_select_anchor = Some(new_pos);
                    }
                    d.reset_cursor_blink();

                    // Mobile UX touches: light haptic on every tap +
                    // hide any leftover edit menu from a previous
                    // double-tap. The edit menu itself fires from a
                    // double-tap detector below. The double-tap
                    // tracking lives on the data lock so we don't
                    // need extra fields here.
                    //
                    // Also arm the long-press timer so a press-and-
                    // hold of 500 ms shows the edit menu with
                    // PASTE available — matches the iOS
                    // UITextField / Android EditText long-press UX.
                    if crate::widgets::text_input::is_touch_input() {
                        crate::widgets::text_edit::haptic_selection();
                        crate::widgets::text_edit::hide_edit_menu();
                        // No word-selection callback here: text_area
                        // doesn't yet implement double-tap word
                        // selection either, so passing `None` keeps
                        // both gestures consistent within the widget
                        // (long press just shows the menu without
                        // selecting anything). When text_area gains
                        // double-tap word selection, mirror the
                        // text_input/code pattern and pass a closure
                        // that calls `text_edit::word_at_position`
                        // against the cursor's line.
                        crate::widgets::text_input::arm_long_press_timer(
                            ctx.bounds_x + click_x,
                            ctx.bounds_y + click_y,
                            ctx.bounds_height.clamp(24.0, 48.0),
                            None,
                        );
                    }

                    true // needs refresh
                }; // Lock released here

                // Trigger incremental refresh AFTER releasing the data lock
                if needs_refresh {
                    notify_text_area(&data_for_click);
                }
            })
            // Mouse drag to extend selection.
            //
            // Mirrors text_input's on_drag at text_input.rs ~2491-2560:
            // while pointer is pressed, recompute cursor position from
            // the current local_x / local_y, then if drag_select_anchor
            // is Some (seeded by on_mouse_down for desktop mouse), set
            // selection_start = Some(anchor) and cursor = new_pos so
            // the selection-rendering pass at
            // text_area.rs:2217-2277 draws the highlight rects.
            // Touch leaves anchor = None (set in on_mouse_down) so
            // dragging on touch repositions the cursor instead of
            // selecting — matches text_input's touch UX.
            .on_drag(move |ctx| {
                let cfg = config_for_drag.lock().unwrap();
                let wrap_enabled = cfg.wrap;
                let text_origin_x = cfg.padding_x + cfg.border_width;
                let text_origin_y = cfg.padding_y + cfg.border_width;
                drop(cfg);

                let needs_refresh = {
                    let mut d = match data_for_drag.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    if d.disabled || !d.visual.is_focused() {
                        return;
                    }
                    if d.visual_lines.is_empty() || wrap_enabled {
                        d.compute_visual_lines();
                    }
                    // Recompute the cursor position from the live drag
                    // coordinates. clicked_visual_line is set by the
                    // per-visual-line click handlers but only on
                    // POINTER_DOWN — during drag we fall through to the
                    // xy lookup. Subtract the left padding + border so
                    // text_x lands in text-content space; same correction
                    // the on_mouse_down handler applies.
                    let text_x = (ctx.local_x - text_origin_x).max(0.0);
                    let text_y = (ctx.local_y - text_origin_y).max(0.0);
                    let new_pos = d.cursor_position_from_xy(text_x, text_y);

                    if crate::widgets::text_input::is_touch_input() {
                        // Touch drag = cursor reposition, no select.
                        if new_pos != d.cursor {
                            d.cursor = new_pos;
                            d.selection_start = None;
                            true
                        } else {
                            false
                        }
                    } else if let Some(anchor) = d.drag_select_anchor {
                        if new_pos != anchor {
                            d.selection_start = Some(anchor);
                            d.cursor = new_pos;
                            true
                        } else {
                            // Drag still on the anchor — clear any
                            // residual selection so a zero-distance
                            // drag matches a plain click.
                            if d.selection_start.is_some() {
                                d.selection_start = None;
                                true
                            } else {
                                false
                            }
                        }
                    } else {
                        false
                    }
                };
                if needs_refresh {
                    notify_text_area(&data_for_drag);
                }
            })
            // Handle text input
            .on_event(event_types::TEXT_INPUT, move |ctx| {
                let (needs_refresh, change_signal, edited) = {
                    let mut d = match data_for_text.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled || !d.visual.is_focused() {
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
                        d.insert(&c.to_string());
                        d.reset_cursor_blink();
                        // Recompute visual lines after text change
                        d.compute_visual_lines();
                        // Ensure cursor is visible after text insertion (use cached values)
                        let line_height = d.line_height;
                        let viewport_height = d.viewport_height;
                        d.ensure_cursor_visible(line_height, viewport_height);
                        tracing::debug!("TextArea received char: {:?}, value: {}", c, d.value());
                        (true, d.change_signal_id, changed_text(&d))
                    } else {
                        (false, None, None)
                    }
                }; // Lock released here

                // Trigger incremental refresh AFTER releasing the data lock
                if needs_refresh {
                    notify_text_area(&data_for_text);
                }

                // Notify external listeners of text change
                if let Some(signal_id) = change_signal {
                    crate::stateful::check_stateful_deps(&[signal_id]);
                }
                fire_on_change(edited);
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
                    let mod_key = ctx.meta || ctx.ctrl;

                    match ctx.key_code {
                        8 if mod_key => {
                            // Cmd+Backspace: delete word backward
                            d.delete_word_backward();
                            text_changed = true;
                        }
                        8 => {
                            // Backspace
                            d.delete_backward();
                            text_changed = true;
                        }
                        127 if mod_key => {
                            // Cmd+Delete: delete word forward
                            d.delete_word_forward();
                            text_changed = true;
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
                        }
                        37 => {
                            // Left arrow
                            if mod_key {
                                d.move_word_left(ctx.shift);
                            } else {
                                d.move_left(ctx.shift);
                            }
                        }
                        39 => {
                            // Right arrow
                            if mod_key {
                                d.move_word_right(ctx.shift);
                            } else {
                                d.move_right(ctx.shift);
                            }
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
                            if mod_key {
                                match ctx.key_code {
                                    65 => d.select_all(), // Cmd+A
                                    67 => {
                                        // Cmd+C — Copy
                                        if let Some(sel) = d.selected_text() {
                                            crate::widgets::text_edit::clipboard_write(&sel);
                                        }
                                        cursor_changed = false;
                                    }
                                    88 => {
                                        // Cmd+X — Cut
                                        if let Some(sel) = d.selected_text() {
                                            crate::widgets::text_edit::clipboard_write(&sel);
                                            d.delete_selection();
                                            text_changed = true;
                                        }
                                    }
                                    86 => {
                                        // Cmd+V — Paste
                                        if let Some(clip) =
                                            crate::widgets::text_edit::clipboard_read()
                                        {
                                            d.insert(&clip);
                                            text_changed = true;
                                        }
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

                    // Get change signal ID if text changed
                    let change_signal = if text_changed {
                        d.change_signal_id
                    } else {
                        None
                    };
                    let edited = if text_changed { changed_text(&d) } else { None };

                    (cursor_changed, should_blur, change_signal, edited)
                }; // Lock released here

                // Handle blur (Escape key)
                if needs_refresh.1 {
                    crate::widgets::text_input::blur_all_text_inputs();
                } else if needs_refresh.0 {
                    // Trigger incremental refresh AFTER releasing the data lock
                    notify_text_area(&data_for_key);
                }

                // Notify external listeners of text change
                if let Some(signal_id) = needs_refresh.2 {
                    crate::stateful::check_stateful_deps(&[signal_id]);
                }
                fire_on_change(needs_refresh.3);
            })
            // Set text cursor (I-beam) for text area
            .cursor_text()
        // Note: Scroll events are handled by the scroll() widget inside build_content
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
        self.inner = std::cell::RefCell::new(self.inner.take().h(height));
        self.update_scroll_dimensions();
        self
    }

    /// Set number of visible columns (like HTML textarea cols attribute)
    pub fn cols(mut self, cols: usize) -> Self {
        let width = {
            let mut cfg = self.config.lock().unwrap();
            cfg.cols = Some(cols);
            cfg.width_is_explicit = true;
            cfg.effective_width()
        };
        self.inner = std::cell::RefCell::new(self.inner.take().w(width));
        self
    }

    /// Set both rows and cols
    pub fn text_size(mut self, rows: usize, cols: usize) -> Self {
        let (width, height) = {
            let mut cfg = self.config.lock().unwrap();
            cfg.rows = Some(rows);
            cfg.cols = Some(cols);
            cfg.width_is_explicit = true;
            (cfg.effective_width(), cfg.effective_height())
        };
        self.inner = std::cell::RefCell::new(self.inner.take().w(width).h(height));
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
            cfg.width_is_explicit = true;
        }
        self.inner = std::cell::RefCell::new(self.inner.take().w(px));
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.height = px;
            cfg.rows = None;
        }
        self.inner = std::cell::RefCell::new(self.inner.take().h(px));
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
            cfg.width_is_explicit = true;
        }
        self.inner = std::cell::RefCell::new(self.inner.take().size(w, h));
        self.update_scroll_dimensions();
        self
    }

    pub fn square(mut self, size: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().square(size));
        self
    }

    pub fn w_full(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().w_full());
        self
    }

    pub fn h_full(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().h_full());
        self
    }

    pub fn w_fit(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().w_fit());
        self
    }

    pub fn h_fit(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().h_fit());
        self
    }

    pub fn min_w(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().min_w(px));
        self
    }

    pub fn p(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().p(px));
        self
    }

    pub fn px(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().px(px));
        self
    }

    pub fn py(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().py(px));
        self
    }

    pub fn m(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().m(px));
        self
    }

    pub fn mx(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().mx(px));
        self
    }

    pub fn my(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().my(px));
        self
    }

    pub fn gap(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().gap(px));
        self
    }

    pub fn flex_row(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().flex_row());
        self
    }

    pub fn flex_col(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().flex_col());
        self
    }

    pub fn flex_grow(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().flex_grow());
        self
    }

    pub fn items_center(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().items_center());
        self
    }

    pub fn items_start(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().items_start());
        self
    }

    pub fn items_end(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().items_end());
        self
    }

    pub fn justify_center(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().justify_center());
        self
    }

    pub fn justify_start(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().justify_start());
        self
    }

    pub fn justify_end(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().justify_end());
        self
    }

    pub fn justify_between(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().justify_between());
        self
    }

    pub fn bg(mut self, color: impl Into<blinc_core::Brush>) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().background(color));
        self
    }

    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.lock().unwrap().corner_radius = radius;
        self.inner = std::cell::RefCell::new(self.inner.take().rounded(radius));
        self
    }

    /// Set the CSS element ID for stylesheet matching
    ///
    /// When set, the TextArea will query the active stylesheet for
    /// styles matching `#id`, `#id:hover`, `#id:focus`, `#id:disabled`,
    /// and `#id::placeholder`.
    pub fn id(mut self, id: &str) -> Self {
        if let Ok(mut d) = self.state.lock() {
            d.css_element_id = Some(id.to_string());
        }
        self.inner = std::cell::RefCell::new(self.inner.take().id(id));
        self
    }

    /// Add a CSS class name for selector matching
    pub fn class(mut self, name: &str) -> Self {
        if let Ok(mut d) = self.state.lock() {
            d.css_classes.push(blinc_core::intern::intern(name));
        }
        self.inner = std::cell::RefCell::new(self.inner.take().class(name));
        self
    }

    pub fn border(mut self, width: f32, color: blinc_core::Color) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().border(width, color));
        self
    }

    pub fn border_color(mut self, color: blinc_core::Color) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().border_color(color));
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().border_width(width));
        self
    }

    pub fn shadow(mut self, shadow: blinc_core::Shadow) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().shadow(shadow));
        self
    }

    pub fn shadow_sm(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().shadow_sm());
        self
    }

    pub fn shadow_md(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().shadow_md());
        self
    }

    pub fn shadow_lg(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().shadow_lg());
        self
    }

    pub fn transform(mut self, transform: blinc_core::Transform) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().transform(transform));
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().opacity(opacity));
        self
    }

    pub fn overflow_clip(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().overflow_clip());
        self
    }

    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().child(child));
        self
    }

    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: ElementBuilder + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().children(children));
        self
    }

    // Event handlers
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_click(handler));
        self
    }

    pub fn on_hover_enter<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_hover_enter(handler));
        self
    }

    pub fn on_hover_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_hover_leave(handler));
        self
    }

    pub fn on_mouse_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_mouse_down(handler));
        self
    }

    pub fn on_mouse_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_mouse_up(handler));
        self
    }

    pub fn on_focus<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_focus(handler));
        self
    }

    pub fn on_blur<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_blur(handler));
        self
    }

    pub fn on_key_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_key_down(handler));
        self
    }

    pub fn on_key_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_key_up(handler));
        self
    }

    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_scroll(handler));
        self
    }

    /// Called after every edit with the new text.
    ///
    /// Complements [`Self::on_change_signal`], which only pokes the
    /// stateful dep graph: this one carries the text, which is what a
    /// caller keeping an external value (a DSL signal, a form model) in
    /// sync actually needs.
    pub fn on_change<F>(self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        if let Ok(mut state) = self.state.lock() {
            state.on_change = Some(Arc::new(callback));
        }
        self
    }

    /// Set a signal ID to be notified when text content changes
    ///
    /// When the text area content is modified (typing, deletion, paste, etc.),
    /// this signal will be triggered via `check_stateful_deps`, causing any
    /// stateful elements that depend on this signal to rebuild.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let markdown_state = ctx.use_state_keyed("editor", || {
    ///     Arc::new(Mutex::new(TextAreaState::new()))
    /// });
    ///
    /// // Editor panel
    /// text_area(&markdown_state.get())
    ///     .on_change_signal(markdown_state.signal_id())
    ///
    /// // Preview panel - will rebuild when text changes
    /// stateful(preview_state)
    ///     .deps(&[markdown_state.signal_id()])
    ///     .on_state(|_, container| { ... })
    /// ```
    pub fn on_change_signal(self, signal_id: SignalId) -> Self {
        // Store the signal ID in the TextAreaState
        if let Ok(mut state) = self.state.lock() {
            state.set_change_signal(signal_id);
        }
        self
    }
}

/// The callback and the text to hand it, read while the state lock is
/// already held.
///
/// Split from firing it on purpose: the callback commonly writes a
/// signal, which can refresh a stateful that locks this same state, so
/// it must run only after the lock is released.
fn changed_text(d: &TextAreaState) -> Option<(crate::widgets::TextCallback, String)> {
    d.on_change.as_ref().map(|cb| (Arc::clone(cb), d.value()))
}

/// Fire what [`changed_text`] captured, now that the lock is gone.
fn fire_on_change(edited: Option<(crate::widgets::TextCallback, String)>) {
    if let Some((cb, text)) = edited {
        cb(&text);
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
    fn bound_element_ref(&self) -> Option<&crate::selector::DivRef> {
        self.element_ref.as_ref()
    }

    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let node = self.element().build(tree);
        if let Ok(mut d) = self.state.lock() {
            d.node_id = Some(node);
        }
        node
    }

    fn render_props(&self) -> RenderProps {
        self.element().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.element().children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        crate::div::ElementTypeId::Div
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("textarea")
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.element())
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(self.element())
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.element().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(self.element())
    }

    fn layout_bounds_storage(&self) -> Option<crate::renderer::LayoutBoundsStorage> {
        // Return the layout bounds storage from the state so it gets updated after layout
        if let Ok(data) = self.state.lock() {
            Some(Arc::clone(&data.layout_bounds_storage))
        } else {
            None
        }
    }

    fn layout_bounds_callback(&self) -> Option<crate::renderer::LayoutBoundsCallback> {
        // A new size wraps the lines differently and moves the viewport.
        let config = Arc::clone(&self.config);
        let state = Arc::clone(&self.state);
        Some(Arc::new(move |bounds| {
            let mut changed = false;
            if let Ok(mut cfg) = config.lock() {
                let width_changed = (cfg.width - bounds.width).abs() > 1.0;
                let height_changed = (cfg.height - bounds.height).abs() > 1.0;
                if width_changed {
                    cfg.width = bounds.width;
                }
                if height_changed {
                    cfg.height = bounds.height;
                    let viewport_height =
                        bounds.height - cfg.padding_y * 2.0 - cfg.border_width * 2.0;
                    if let Ok(data) = state.lock() {
                        if let Ok(mut p) = data.scroll_physics.lock() {
                            p.viewport_height = viewport_height;
                        }
                    }
                }
                changed = width_changed || height_changed;
            }
            if changed {
                notify_text_area(&state);
            }
        }))
    }
}

/// The pointer entered or left a field: move its visual state.
fn pointer_crossed(state: &SharedTextAreaState, event: u32) {
    let changed = match state.lock() {
        Ok(mut d) if !d.disabled => match d.visual.on_event(event) {
            Some(next) => {
                d.visual = next;
                true
            }
            None => false,
        },
        _ => false,
    };
    if changed {
        notify_text_area(state);
    }
}

/// How the field's box looks in its state.
#[derive(Clone)]
struct BoxLook {
    bg: Color,
    border: Color,
    border_width: f32,
    radius: f32,
    /// Width, colour and offset.
    outline: (f32, Color, f32),
}

fn box_look(base: &TextAreaConfig, d: &TextAreaState) -> BoxLook {
    let (cfg, outline) = resolved_config(base, d);
    let (bg, border) = match d.visual {
        TextFieldState::Focused | TextFieldState::FocusedHovered => {
            (cfg.focused_bg_color, cfg.focused_border_color)
        }
        TextFieldState::Hovered => (cfg.hover_bg_color, cfg.hover_border_color),
        TextFieldState::Disabled => (
            Color::rgba(0.12, 0.12, 0.15, 0.5),
            Color::rgba(0.25, 0.25, 0.3, 0.5),
        ),
        TextFieldState::Idle => (cfg.bg_color, cfg.border_color),
    };
    BoxLook {
        bg,
        border,
        border_width: cfg.border_width,
        radius: cfg.corner_radius,
        outline: outline.unwrap_or((0.0, Color::TRANSPARENT, 0.0)),
    }
}

/// The text a visual line shows: its own text, or the placeholder on the
/// first line of an empty field.
fn row_text(d: &TextAreaState, base: &TextAreaConfig, row: usize) -> String {
    if d.is_empty() {
        if row > 0 {
            String::new()
        } else if !d.placeholder.is_empty() {
            d.placeholder.clone()
        } else {
            base.placeholder.clone()
        }
    } else {
        d.visual_lines
            .get(row)
            .map(|vl| vl.text.clone())
            .unwrap_or_default()
    }
}

/// Where the selection's highlight sits on a visual line: its left edge
/// and width, `(0, 0)` where the line has none. Shown while focused.
fn selection_span(d: &TextAreaState, row: usize) -> (f32, f32) {
    let none = (0.0, 0.0);
    if !d.visual.is_focused() {
        return none;
    }
    let Some(sel_start) = d.selection_start else {
        return none;
    };
    let (start, end) = d.order_positions(sel_start, d.cursor);
    let Some(vl) = d.visual_lines.get(row) else {
        return none;
    };
    if start == end || vl.logical_line < start.line || vl.logical_line > end.line {
        return none;
    }
    let line_chars = d
        .lines
        .get(vl.logical_line)
        .map(|l| l.chars().count())
        .unwrap_or(0);
    let from = if vl.logical_line == start.line {
        start.column
    } else {
        0
    };
    let to = if vl.logical_line == end.line {
        end.column
    } else {
        line_chars
    };
    // The part of the selection on this visual line.
    let from = from.max(vl.start_char).min(vl.end_char);
    let to = to.max(vl.start_char).min(vl.end_char);
    if to <= from {
        return none;
    }
    let width_to = |col: usize| {
        let prefix: String = vl.text.chars().take(col - vl.start_char).collect();
        crate::text_measure::measure_text(&prefix, d.font_size).width
    };
    let (x0, x1) = (width_to(from), width_to(to));
    // A selection running on past this line ends in a sliver for the break.
    let runs_on =
        to >= vl.end_char.min(line_chars) && !(vl.logical_line == end.line && to == end.column);
    let sliver = if runs_on { d.font_size * 0.5 } else { 0.0 };
    (x0, x1 - x0 + sliver)
}

/// The field as built: the box's look bound to its state, around the content.
fn assemble(mut outer: Div, state: &SharedTextAreaState, base: &TextAreaConfig) -> Div {
    use crate::binding::TypedPendingBinding;
    use crate::property::PropertyId;
    use blinc_core::reactive::{ReactiveGraph, computed, signal};

    let revision = {
        let mut d = state.lock().unwrap();
        // Laid out for the state the field starts in.
        let (cfg, _) = resolved_config(base, &d);
        take_metrics(&mut d, &cfg);
        d.compute_visual_lines();
        *d.revision.get_or_insert_with(|| signal(0u64))
    };

    let look = {
        let (state, base) = (Arc::clone(state), base.clone());
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            box_look(&base, &state.lock().unwrap())
        })
    };
    // Read once, so the look is subscribed to the revision.
    let first = look
        .try_get()
        .unwrap_or_else(|| box_look(base, &state.lock().unwrap()));
    outer = outer
        .bg(first.bg)
        .border(first.border_width, first.border)
        .rounded(first.radius)
        .outline_width(first.outline.0)
        .outline_color(first.outline.1)
        .outline_offset(first.outline.2);
    outer
        .pending_bindings
        .push(Box::new(TypedPendingBinding::from_computed(
            look.clone(),
            PropertyId::Background,
            |props, look: BoxLook| props.background = Some(blinc_core::Brush::Solid(look.bg)),
        )));
    outer
        .pending_bindings
        .push(Box::new(TypedPendingBinding::from_computed(
            look.clone(),
            PropertyId::BorderColor,
            |props, look: BoxLook| props.border_color = Some(look.border),
        )));
    outer
        .pending_bindings
        .push(Box::new(TypedPendingBinding::from_computed(
            look.clone(),
            PropertyId::CornerRadius,
            |props, look: BoxLook| {
                props.border_radius = blinc_core::CornerRadius::uniform(look.radius)
            },
        )));
    outer
        .pending_bindings
        .push(Box::new(TypedPendingBinding::from_computed(
            look,
            PropertyId::Outline,
            |props, look: BoxLook| {
                props.outline_width = look.outline.0;
                props.outline_color = Some(look.outline.1);
                props.outline_offset = look.outline.2;
            },
        )));
    // A border width a state's rule changes moves the content, so it is
    // bound as layout.
    let border_width = {
        let (state, base) = (Arc::clone(state), base.clone());
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            box_look(&base, &state.lock().unwrap()).border_width
        })
    };
    outer = outer.border_width(&border_width);

    let (cfg, _) = resolved_config(base, &state.lock().unwrap());
    outer.child(content(state, &cfg, base, revision))
}

/// The inside of the field: one row per visual line in a vertical scroll,
/// and the caret, all bound to the revision.
fn content(
    state: &SharedTextAreaState,
    cfg: &TextAreaConfig,
    base: &TextAreaConfig,
    revision: blinc_core::reactive::Signal<u64>,
) -> Div {
    use blinc_core::reactive::{ReactiveGraph, computed};

    let line_height = cfg.font_size * cfg.line_height;
    let cursor_height = cfg.font_size * 1.2;

    let text_color = {
        let (state, base) = (Arc::clone(state), base.clone());
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            let d = state.lock().unwrap();
            let (cfg, _) = resolved_config(&base, &d);
            if d.is_empty() {
                cfg.placeholder_color
            } else if d.disabled {
                Color::rgba(0.4, 0.4, 0.4, 1.0)
            } else {
                cfg.text_color
            }
        })
    };
    let rows = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            let count = state.lock().unwrap().visual_lines.len().max(1);
            (0..count).collect::<Vec<usize>>()
        })
    };
    // Wrapped rows are as wide as the text area; unwrapped ones as their text.
    let row_width = cfg.wrap.then(|| {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            state.lock().unwrap().available_width
        })
    });

    let row = {
        let (state, base) = (Arc::clone(state), base.clone());
        let (font_size, selection_color) = (cfg.font_size, cfg.selection_color);
        move |index: usize| {
            text_row(
                &state,
                &base,
                revision,
                index,
                line_height,
                font_size,
                selection_color,
                row_width.as_ref(),
                &text_color,
            )
        }
    };

    // The caret, moved to the cursor's line and column. Drawn while focused.
    if let Ok(mut cs) = state.lock().unwrap().cursor_state.lock() {
        cs.color = cfg.cursor_color;
        cs.animation = CursorAnimation::SmoothFade;
    }
    let at = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            let d = state.lock().unwrap();
            let (x, y) = d.cursor_position_from_visual_lines();
            // Fonts keep descender space at the bottom of the line, which
            // sits the glyphs high: the caret is raised to match.
            let top = y + (d.line_height - cursor_height) / 2.0 - d.font_size * 0.1;
            blinc_core::Transform::translate(x, top)
        })
    };
    let state_for_caret = Arc::clone(state);
    let caret = canvas(
        move |ctx: &mut dyn blinc_core::DrawContext, bounds: crate::canvas::CanvasBounds| {
            let (shown, cursor_state) = {
                let d = state_for_caret.lock().unwrap();
                (d.visual.is_focused(), Arc::clone(&d.cursor_state))
            };
            if !shown {
                return;
            }
            let cs = cursor_state.lock().unwrap();
            let opacity = cs.blink_opacity();
            if opacity < 0.01 {
                return;
            }
            let color = Color::rgba(cs.color.r, cs.color.g, cs.color.b, cs.color.a * opacity);
            ctx.fill_rect(
                blinc_core::Rect::new(0.0, 0.0, cs.width, bounds.height),
                blinc_core::CornerRadius::default(),
                blinc_core::Brush::Solid(color),
            );
        },
    )
    .w(2.0)
    .h(cursor_height);

    let text_content = div()
        .flex_col()
        .justify_start()
        .items_start()
        .relative()
        .overflow_visible()
        .for_each(&rows, |index| *index, row)
        .child(
            div()
                .absolute()
                .left(0.0)
                .top(0.0)
                .w(2.0)
                .h(cursor_height)
                .transform(&at)
                .child(caret),
        );

    // The scroll clips the rows and carries the vertical offset; the outer
    // box does the rounding.
    let scrollable_content =
        Scroll::with_physics(Arc::clone(&state.lock().unwrap().scroll_physics))
            .direction(ScrollDirection::Vertical)
            .no_bounce()
            .flex_grow()
            .child(text_content);

    // Heights are explicit so the box has proper intrinsic dimensions; the
    // width is only pinned when the caller asked for one. A measured width
    // must never be emitted here -- see `TextAreaConfig::width_is_explicit`.
    // Left auto, the wrapper is stretched by its parent, which is what a
    // full-width field wants and what a `w_fit()` ancestor can shrink.
    let (padding_x, padding_y) = (cfg.padding_x, cfg.padding_y);
    let pin_width = cfg.width_is_explicit;
    let content_width = cfg.effective_width();
    let content_height = cfg.effective_height();
    let inner_height = content_height - padding_y * 2.0;
    let pinned = |d: Div| if pin_width { d.w(content_width) } else { d };

    pinned(div().flex_col())
        .h(content_height)
        .child(pinned(div().h(padding_y)))
        .child(
            pinned(div().flex_row().h(inner_height))
                .child(div().w(padding_x).h(inner_height))
                .child(scrollable_content)
                .child(div().w(padding_x).h(inner_height)),
        )
        .child(pinned(div().h(padding_y)))
}

/// One visual line: its text, and the selection's highlight behind it.
#[allow(clippy::too_many_arguments)]
fn text_row(
    state: &SharedTextAreaState,
    base: &TextAreaConfig,
    revision: blinc_core::reactive::Signal<u64>,
    index: usize,
    line_height: f32,
    font_size: f32,
    selection_color: Color,
    width: Option<&blinc_core::reactive::Computed<f32>>,
    color: &blinc_core::reactive::Computed<Color>,
) -> Div {
    use crate::binding::IntoReactive;
    use blinc_core::reactive::{ReactiveGraph, computed};

    let text = {
        let (state, base) = (Arc::clone(state), base.clone());
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            row_text(&state.lock().unwrap(), &base, index)
        })
    };
    let highlight_at = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            let (x, _) = selection_span(&state.lock().unwrap(), index);
            blinc_core::Transform::translate(x, 0.0)
        })
    };
    let highlight_width = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            selection_span(&state.lock().unwrap(), index).1
        })
    };
    // Tells the field's own handler which line a press landed on.
    let state_for_press = Arc::clone(state);
    let row = div()
        .h(line_height)
        .flex_row()
        .items_center()
        .relative()
        .on_mouse_down(move |_| {
            if let Ok(mut d) = state_for_press.lock() {
                d.clicked_visual_line = Some(index);
            }
        })
        .child(
            div()
                .absolute()
                .left(0.0)
                .top(0.0)
                .h(line_height)
                .w(&highlight_width)
                .transform(&highlight_at)
                .bg(selection_color)
                .rounded(2.0),
        )
        .child(
            crate::text::Text::bound(text.into_reactive())
                .size(font_size)
                .color(color)
                .text_left()
                .no_wrap(),
        );
    match width {
        Some(width) => row.w(width),
        None => row,
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
