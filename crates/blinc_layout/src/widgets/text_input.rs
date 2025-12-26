//! Ready-to-use TextInput widget
//!
//! Single-line text input with:
//! - Input types: text, number, integer, email, password, url, tel, search
//! - Validation support with constraints
//! - Cursor and selection with blinking
//! - Visual states: idle, hovered, focused
//! - Built-in styling that just works
//! - Inherits ALL Div methods for full layout control via Deref

use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use blinc_core::Color;

/// Get elapsed time in milliseconds since app start (for cursor blinking)
///
/// This is a globally consistent time source used for cursor blinking.
/// It uses the same epoch across all text inputs.
pub fn elapsed_ms() -> u64 {
    static START_TIME: OnceLock<Instant> = OnceLock::new();
    let start = START_TIME.get_or_init(Instant::now);
    start.elapsed().as_millis() as u64
}

/// Standard cursor blink interval in milliseconds
/// 400ms provides a responsive feel (vs. 530ms which can feel sluggish)
pub const CURSOR_BLINK_INTERVAL_MS: u64 = 400;

// =============================================================================
// Global focus tracking - tracks focused text inputs independent of node IDs
// This survives tree rebuilds and ensures proper blur handling
// =============================================================================

use std::sync::Weak;

/// Global counter of focused text inputs
/// Incremented when a text input gains focus, decremented when it loses focus
static GLOBAL_FOCUS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Global flag indicating text widgets need a UI rebuild
/// Set by event handlers, checked by the windowed app
static NEEDS_REBUILD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Global flag indicating continuous redraws are needed (e.g., cursor blink)
/// Unlike NEEDS_REBUILD, this doesn't trigger tree rebuild, just visual refresh
static NEEDS_CONTINUOUS_REDRAW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Global reference to the currently focused text input state
/// Used to forcibly blur the previous input when a new one gains focus
static FOCUSED_TEXT_INPUT: Mutex<Option<Weak<Mutex<TextInputState>>>> = Mutex::new(None);

/// Global reference to the currently focused text area state
/// Stored separately since TextAreaState is a different type
static FOCUSED_TEXT_AREA: Mutex<Option<Weak<Mutex<crate::widgets::text_area::TextAreaState>>>> =
    Mutex::new(None);

/// Check if any text input is currently focused
/// This is used by the windowed app to trigger cursor blink rebuilds
pub fn has_focused_text_input() -> bool {
    GLOBAL_FOCUS_COUNT.load(Ordering::Relaxed) > 0
}

/// Check and clear the continuous redraw flag
///
/// Returns true if any text widget needs continuous redraws (e.g., cursor blink).
/// Call this from the animation scheduler's redraw logic to keep cursor blinking.
pub fn take_needs_continuous_redraw() -> bool {
    NEEDS_CONTINUOUS_REDRAW.swap(false, Ordering::SeqCst)
}

/// Request continuous redraws (for cursor blink animation)
///
/// Called when a text input gains focus to keep cursor animation running.
/// The windowed app should call `take_needs_continuous_redraw()` each frame
/// and request redraw if true.
fn request_continuous_redraw() {
    if has_focused_text_input() {
        NEEDS_CONTINUOUS_REDRAW.store(true, Ordering::SeqCst);
    }
}

/// Public version of request_continuous_redraw for windowed app to call
/// This keeps cursor blink going as long as a text input is focused
pub fn request_continuous_redraw_pub() {
    request_continuous_redraw();
}

/// Check and clear the text widget rebuild flag
/// Returns true if any text widget requested a rebuild since last check
/// This is called by the windowed app in the render loop
pub fn take_needs_rebuild() -> bool {
    NEEDS_REBUILD.swap(false, Ordering::SeqCst)
}

/// Request a UI rebuild from a text widget event handler
/// Called internally when focus/text changes require visual update
pub(crate) fn request_rebuild() {
    NEEDS_REBUILD.store(true, Ordering::SeqCst);
}

/// Called when a text input gains focus (internal use by text widgets)
pub(crate) fn increment_focus_count() {
    GLOBAL_FOCUS_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Called when a text input loses focus (internal use by text widgets)
pub(crate) fn decrement_focus_count() {
    // Saturating subtract to prevent underflow
    let _ = GLOBAL_FOCUS_COUNT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
        Some(v.saturating_sub(1))
    });
}

/// Set the currently focused text input state (called when gaining focus)
/// This will forcibly blur any previously focused text input
pub(crate) fn set_focused_text_input(state: &SharedTextInputState) {
    use crate::stateful::StateTransitions;
    use blinc_core::events::event_types;

    let mut focused = FOCUSED_TEXT_INPUT.lock().unwrap();

    // First, blur any previously focused text input
    if let Some(weak) = focused.take() {
        if let Some(prev_state) = weak.upgrade() {
            if !Arc::ptr_eq(&prev_state, state) {
                if let Ok(mut s) = prev_state.lock() {
                    if s.visual.is_focused() {
                        if let Some(new_state) = s.visual.on_event(event_types::BLUR) {
                            s.visual = new_state;
                            decrement_focus_count();
                            tracing::info!("TextInput forcibly blurred previous input");
                        }
                    }
                }
            }
        }
    }

    // Also blur any focused text area
    blur_focused_text_area();

    // Set new focused input
    *focused = Some(Arc::downgrade(state));
}

/// Clear the focused text input reference (called when losing focus)
pub(crate) fn clear_focused_text_input(state: &SharedTextInputState) {
    let mut focused = FOCUSED_TEXT_INPUT.lock().unwrap();
    if let Some(weak) = focused.as_ref() {
        if let Some(prev_state) = weak.upgrade() {
            if Arc::ptr_eq(&prev_state, state) {
                *focused = None;
            }
        }
    }
}

/// Set the currently focused text area state (called when gaining focus)
/// This will forcibly blur any previously focused text input or area
pub(crate) fn set_focused_text_area(state: &crate::widgets::text_area::SharedTextAreaState) {
    use crate::stateful::StateTransitions;
    use blinc_core::events::event_types;

    // First, blur any focused text input
    {
        let mut focused = FOCUSED_TEXT_INPUT.lock().unwrap();
        if let Some(weak) = focused.take() {
            if let Some(prev_state) = weak.upgrade() {
                if let Ok(mut s) = prev_state.lock() {
                    if s.visual.is_focused() {
                        if let Some(new_state) = s.visual.on_event(event_types::BLUR) {
                            s.visual = new_state;
                            decrement_focus_count();
                            tracing::info!("TextInput forcibly blurred for text area focus");
                        }
                    }
                }
            }
        }
    }

    // Then blur any previously focused text area
    let mut focused = FOCUSED_TEXT_AREA.lock().unwrap();
    if let Some(weak) = focused.take() {
        if let Some(prev_state) = weak.upgrade() {
            if !Arc::ptr_eq(&prev_state, state) {
                if let Ok(mut s) = prev_state.lock() {
                    if s.visual.is_focused() {
                        if let Some(new_state) = s.visual.on_event(event_types::BLUR) {
                            s.visual = new_state;
                            decrement_focus_count();
                            tracing::info!("TextArea forcibly blurred previous area");
                        }
                    }
                }
            }
        }
    }

    // Set new focused area
    *focused = Some(Arc::downgrade(state));
}

/// Clear the focused text area reference (called when losing focus)
pub(crate) fn clear_focused_text_area(state: &crate::widgets::text_area::SharedTextAreaState) {
    let mut focused = FOCUSED_TEXT_AREA.lock().unwrap();
    if let Some(weak) = focused.as_ref() {
        if let Some(prev_state) = weak.upgrade() {
            if Arc::ptr_eq(&prev_state, state) {
                *focused = None;
            }
        }
    }
}

/// Helper to blur focused text area (called from set_focused_text_input)
fn blur_focused_text_area() {
    use crate::stateful::StateTransitions;
    use blinc_core::events::event_types;

    let mut focused = FOCUSED_TEXT_AREA.lock().unwrap();
    if let Some(weak) = focused.take() {
        if let Some(prev_state) = weak.upgrade() {
            if let Ok(mut s) = prev_state.lock() {
                if s.visual.is_focused() {
                    if let Some(new_state) = s.visual.on_event(event_types::BLUR) {
                        s.visual = new_state;
                        decrement_focus_count();
                        tracing::info!("TextArea forcibly blurred for text input focus");
                    }
                }
            }
        }
    }
}

use crate::canvas::canvas;
use crate::div::{div, Div, ElementBuilder};
use crate::element::RenderProps;
use crate::stateful::TextFieldState;
use crate::text::text;
use crate::text_selection::{clear_selection, set_selection, SelectionSource};
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::cursor::{cursor_state, CursorAnimation, SharedCursorState};

/// Input type enum similar to HTML input types
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InputType {
    /// Plain text input (default)
    #[default]
    Text,
    /// Numeric input (allows digits, optional sign, decimal)
    Number,
    /// Integer input (only digits and optional sign)
    Integer,
    /// Email input (basic email validation)
    Email,
    /// Password input (masked display)
    Password,
    /// URL input
    Url,
    /// Telephone input
    Tel,
    /// Search input
    Search,
}

impl InputType {
    /// Check if a character is allowed for this input type
    pub fn allows_char(&self, c: char) -> bool {
        match self {
            InputType::Text | InputType::Password | InputType::Search => true,
            InputType::Number => c.is_ascii_digit() || c == '.' || c == '-' || c == '+',
            InputType::Integer => c.is_ascii_digit() || c == '-' || c == '+',
            InputType::Email => c.is_ascii_alphanumeric() || "@._-+".contains(c),
            InputType::Url => c.is_ascii() && !c.is_ascii_control(),
            InputType::Tel => c.is_ascii_digit() || "+-() ".contains(c),
        }
    }

    /// Validate the complete value for this input type
    pub fn validate(&self, value: &str) -> bool {
        if value.is_empty() {
            return true;
        }

        match self {
            InputType::Text | InputType::Password | InputType::Search | InputType::Tel => true,
            InputType::Number => value.parse::<f64>().is_ok(),
            InputType::Integer => value.parse::<i64>().is_ok(),
            InputType::Email => {
                let parts: Vec<&str> = value.split('@').collect();
                parts.len() == 2 && !parts[0].is_empty() && parts[1].contains('.')
            }
            InputType::Url => value.starts_with("http://") || value.starts_with("https://"),
        }
    }

    /// Should this input type be masked?
    pub fn is_masked(&self) -> bool {
        matches!(self, InputType::Password)
    }
}

/// Number constraints for numeric inputs
#[derive(Clone, Copy, Debug, Default)]
pub struct NumberConstraints {
    /// Minimum value (inclusive)
    pub min: Option<f64>,
    /// Maximum value (inclusive)
    pub max: Option<f64>,
    /// Step increment
    pub step: Option<f64>,
}

impl NumberConstraints {
    /// Create new number constraints
    pub fn new() -> Self {
        Self::default()
    }

    /// Set minimum value
    pub fn min(mut self, min: f64) -> Self {
        self.min = Some(min);
        self
    }

    /// Set maximum value
    pub fn max(mut self, max: f64) -> Self {
        self.max = Some(max);
        self
    }

    /// Set step increment
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    /// Validate a value against constraints
    pub fn validate(&self, value: f64) -> bool {
        if let Some(min) = self.min {
            if value < min {
                return false;
            }
        }
        if let Some(max) = self.max {
            if value > max {
                return false;
            }
        }
        true
    }
}

/// TextInput configuration
#[derive(Clone)]
pub struct TextInputConfig {
    /// Placeholder text shown when empty
    pub placeholder: String,
    /// Input type (text, number, email, etc.)
    pub input_type: InputType,
    /// Number constraints (min, max, step) for numeric inputs
    pub number_constraints: NumberConstraints,
    /// Width of the input
    pub width: f32,
    /// Height of the input
    pub height: f32,
    /// Font size
    pub font_size: f32,
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
    /// Error border color (for invalid input)
    pub error_border_color: Color,
    /// Border width
    pub border_width: f32,
    /// Corner radius
    pub corner_radius: f32,
    /// Horizontal padding
    pub padding_x: f32,
    /// Cursor color
    pub cursor_color: Color,
    /// Selection color
    pub selection_color: Color,
    /// Whether the input is disabled
    pub disabled: bool,
    /// Maximum length (0 = unlimited)
    pub max_length: usize,
    /// Whether the field is required
    pub required: bool,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        Self {
            placeholder: String::new(),
            input_type: InputType::Text,
            number_constraints: NumberConstraints::default(),
            width: 200.0,
            height: 40.0,
            font_size: 14.0,
            text_color: Color::rgba(0.9, 0.9, 0.9, 1.0),
            placeholder_color: Color::rgba(0.5, 0.5, 0.5, 1.0),
            bg_color: Color::rgba(0.15, 0.15, 0.2, 1.0),
            focused_bg_color: Color::rgba(0.18, 0.18, 0.25, 1.0),
            border_color: Color::rgba(0.3, 0.3, 0.35, 1.0),
            focused_border_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            error_border_color: Color::rgba(1.0, 0.3, 0.3, 1.0),
            border_width: 1.0,
            corner_radius: 8.0,
            padding_x: 12.0,
            cursor_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            selection_color: Color::rgba(0.4, 0.6, 1.0, 0.3),
            disabled: false,
            max_length: 0,
            required: false,
        }
    }
}

/// TextInput widget state with text editing capability
#[derive(Debug, Clone)]
pub struct TextInputState {
    /// Current text value
    pub value: String,
    /// Cursor position (character index)
    pub cursor: usize,
    /// Selection start position (if selecting)
    pub selection_start: Option<usize>,
    /// Visual state for styling
    pub visual: TextFieldState,
    /// Placeholder text
    pub placeholder: String,
    /// Whether input is disabled
    pub disabled: bool,
    /// Whether value is masked (password)
    pub masked: bool,
    /// Input type for validation
    pub input_type: InputType,
    /// Number constraints
    pub constraints: NumberConstraints,
    /// Whether required
    pub required: bool,
    /// Validation error message
    pub validation_error: Option<String>,
    /// Whether currently valid
    pub is_valid: bool,
    /// Time when focus was gained (for cursor blinking)
    /// Stored as milliseconds since some epoch (e.g., app start)
    pub focus_time_ms: u64,
    /// Cursor blink interval in milliseconds
    pub cursor_blink_interval_ms: u64,
    /// Canvas-based cursor state for smooth animation
    /// This is cloned into the canvas render callback
    pub cursor_state: SharedCursorState,
}

impl Default for TextInputState {
    fn default() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            selection_start: None,
            visual: TextFieldState::Idle,
            placeholder: String::new(),
            disabled: false,
            masked: false,
            input_type: InputType::Text,
            constraints: NumberConstraints::default(),
            required: false,
            validation_error: None,
            is_valid: true,
            focus_time_ms: 0,
            cursor_blink_interval_ms: 530, // Standard cursor blink rate (~530ms)
            cursor_state: cursor_state(),
        }
    }
}

impl TextInputState {
    /// Create a new text input state
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with initial value
    pub fn with_value(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self {
            value,
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

    /// Get the text before the cursor (for measuring cursor position)
    pub fn text_before_cursor(&self) -> String {
        if self.masked {
            "•".repeat(self.cursor)
        } else {
            self.value.chars().take(self.cursor).collect()
        }
    }

    /// Reset cursor blink (call when focus gained or cursor moved)
    pub fn reset_cursor_blink(&mut self, current_time_ms: u64) {
        self.focus_time_ms = current_time_ms;
        // Also reset the canvas cursor state for smooth animation
        if let Ok(mut cs) = self.cursor_state.lock() {
            cs.reset_blink();
        }
    }

    /// Validate the current value
    pub fn validate(&mut self) {
        // Check required
        if self.required && self.value.is_empty() {
            self.is_valid = false;
            self.validation_error = Some("This field is required".to_string());
            return;
        }

        // Check input type validation
        if !self.input_type.validate(&self.value) {
            self.is_valid = false;
            self.validation_error = Some(match self.input_type {
                InputType::Number => "Please enter a valid number".to_string(),
                InputType::Integer => "Please enter a valid integer".to_string(),
                InputType::Email => "Please enter a valid email address".to_string(),
                InputType::Url => "Please enter a valid URL".to_string(),
                _ => "Invalid input".to_string(),
            });
            return;
        }

        // Check number constraints
        if matches!(self.input_type, InputType::Number | InputType::Integer) {
            if let Ok(num) = self.value.parse::<f64>() {
                if !self.constraints.validate(num) {
                    self.is_valid = false;
                    let min = self
                        .constraints
                        .min
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    let max = self
                        .constraints
                        .max
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    self.validation_error =
                        Some(match (self.constraints.min, self.constraints.max) {
                            (Some(_), Some(_)) => {
                                format!("Value must be between {} and {}", min, max)
                            }
                            (Some(_), None) => format!("Value must be at least {}", min),
                            (None, Some(_)) => format!("Value must be at most {}", max),
                            (None, None) => "Invalid value".to_string(),
                        });
                    return;
                }
            }
        }

        self.is_valid = true;
        self.validation_error = None;
    }

    /// Get display text (masked if password)
    pub fn display_text(&self) -> String {
        if self.masked {
            "•".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        }
    }

    /// Insert text at cursor with input type filtering
    pub fn insert(&mut self, text: &str) {
        self.delete_selection();

        // Filter characters based on input type
        let filtered: String = text
            .chars()
            .filter(|c| self.input_type.allows_char(*c))
            .collect();

        if filtered.is_empty() {
            return;
        }

        let byte_pos = self.char_to_byte(self.cursor);
        self.value.insert_str(byte_pos, &filtered);
        self.cursor += filtered.chars().count();
        self.validate();
    }

    /// Delete character before cursor (backspace)
    pub fn delete_backward(&mut self) {
        if self.delete_selection() {
            self.validate();
            return;
        }
        if self.cursor > 0 {
            let start = self.char_to_byte(self.cursor - 1);
            let end = self.char_to_byte(self.cursor);
            self.value.replace_range(start..end, "");
            self.cursor -= 1;
            self.validate();
        }
    }

    /// Delete character after cursor
    pub fn delete_forward(&mut self) {
        if self.delete_selection() {
            self.validate();
            return;
        }
        let len = self.value.chars().count();
        if self.cursor < len {
            let start = self.char_to_byte(self.cursor);
            let end = self.char_to_byte(self.cursor + 1);
            self.value.replace_range(start..end, "");
            self.validate();
        }
    }

    /// Delete selection, returns true if there was a selection
    fn delete_selection(&mut self) -> bool {
        if let Some(start) = self.selection_start.take() {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            if from != to {
                let start_byte = self.char_to_byte(from);
                let end_byte = self.char_to_byte(to);
                self.value.replace_range(start_byte..end_byte, "");
                self.cursor = from;
                return true;
            }
        }
        false
    }

    /// Move cursor left
    pub fn move_left(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            if let Some(start) = self.selection_start {
                self.cursor = self.cursor.min(start);
                self.selection_start = None;
                return;
            }
        }
        if self.cursor > 0 {
            self.cursor -= 1;
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
                self.cursor = self.cursor.max(start);
                self.selection_start = None;
                return;
            }
        }
        let len = self.value.chars().count();
        if self.cursor < len {
            self.cursor += 1;
        }
        if !select {
            self.selection_start = None;
        }
    }

    /// Move to start
    pub fn move_to_start(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor = 0;
    }

    /// Move to end
    pub fn move_to_end(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor = self.value.chars().count();
    }

    /// Select all
    pub fn select_all(&mut self) {
        self.selection_start = Some(0);
        self.cursor = self.value.chars().count();
    }

    /// Get selected text
    pub fn selected_text(&self) -> Option<String> {
        self.selection_start.map(|start| {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            self.value.chars().skip(from).take(to - from).collect()
        })
    }

    /// Update global selection state based on current selection
    ///
    /// Call this after selection changes to keep clipboard operations in sync.
    pub fn sync_global_selection(&self) {
        if let Some(selected) = self.selected_text() {
            if !selected.is_empty() {
                // Text inputs can be cut (they're editable)
                set_selection(selected, SelectionSource::TextInput, true);
                return;
            }
        }
        // No selection - clear global state
        clear_selection();
    }

    /// Is focused?
    pub fn is_focused(&self) -> bool {
        self.visual.is_focused()
    }

    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Get numeric value
    pub fn as_number(&self) -> Option<f64> {
        self.value.parse().ok()
    }

    /// Get integer value
    pub fn as_integer(&self) -> Option<i64> {
        self.value.parse().ok()
    }

    fn char_to_byte(&self, char_pos: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }
}

/// Shared text input state handle
pub type SharedTextInputState = Arc<Mutex<TextInputState>>;

/// Create a shared text input state
pub fn text_input_state() -> SharedTextInputState {
    Arc::new(Mutex::new(TextInputState::new()))
}

/// Create a shared text input state with placeholder
pub fn text_input_state_with_placeholder(placeholder: impl Into<String>) -> SharedTextInputState {
    Arc::new(Mutex::new(TextInputState::with_placeholder(placeholder)))
}

/// Ready-to-use text input element
///
/// Inherits all Div methods via Deref, so you have full layout control.
///
/// Usage: `text_input(&state).placeholder("Enter text").w(200.0).rounded(12.0)`
pub struct TextInput {
    /// Inner div - ALL Div methods are available via Deref
    inner: Div,
    /// Text input state
    state: SharedTextInputState,
    /// Text input configuration
    config: TextInputConfig,
}

// Deref to Div gives TextInput ALL Div methods for reading
impl Deref for TextInput {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for TextInput {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl TextInput {
    /// Create a new text input with shared state
    pub fn new(state: &SharedTextInputState) -> Self {
        let config = TextInputConfig::default();

        // Build initial visual structure with default event handlers
        let inner = Self::create_inner(&config, state);

        Self {
            inner,
            state: Arc::clone(state),
            config,
        }
    }

    /// Create the inner Div with visual structure and default event handlers
    fn create_inner(config: &TextInputConfig, state: &SharedTextInputState) -> Div {
        let state_guard = state.lock().unwrap();

        // Use state's placeholder if value is empty (state.placeholder takes precedence)
        let display = if state_guard.value.is_empty() {
            if !state_guard.placeholder.is_empty() {
                state_guard.placeholder.clone()
            } else {
                config.placeholder.clone()
            }
        } else {
            state_guard.display_text()
        };

        let text_color = if state_guard.value.is_empty() {
            config.placeholder_color
        } else if state_guard.disabled {
            Color::rgba(0.4, 0.4, 0.4, 1.0)
        } else {
            config.text_color
        };

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

        // Override border if validation error
        let border = if !state_guard.is_valid && !state_guard.value.is_empty() {
            config.error_border_color
        } else {
            border
        };

        // Check if cursor should be shown (focused state)
        let is_focused = matches!(
            state_guard.visual,
            TextFieldState::Focused | TextFieldState::FocusedHovered
        );
        let cursor_color = config.cursor_color;
        let selection_color = config.selection_color;

        // Get cursor position and selection state
        let cursor_pos = state_guard.cursor;
        let selection_start = state_guard.selection_start;

        // Cursor height is based on font size (slightly taller for visibility)
        let cursor_height = config.font_size * 1.2;

        // Get selection range (sorted: from < to)
        let selection_range: Option<(usize, usize)> = selection_start.map(|start| {
            if start < cursor_pos {
                (start, cursor_pos)
            } else {
                (cursor_pos, start)
            }
        });

        // Clone the cursor state for the canvas callback
        let cursor_state_for_canvas = Arc::clone(&state_guard.cursor_state);

        drop(state_guard);

        // Calculate cursor x position using text measurement
        let cursor_x = if cursor_pos > 0 && !display.is_empty() {
            let text_before: String = display.chars().take(cursor_pos).collect();
            crate::text_measure::measure_text(&text_before, config.font_size).width
        } else {
            0.0
        };

        // Build inner content as a relative container for absolute cursor positioning
        // Use raw pixel padding (config.padding_x is already in pixels)
        let mut inner_content = div()
            .w_full()
            .h_full()
            .bg(bg)
            .rounded(config.corner_radius - 1.0)
            .padding_x_px(config.padding_x) // Use raw pixels, not 4x units
            .relative() // Enable absolute positioning for children
            .flex_row()
            .justify_start() // Text starts from left
            .items_center() // Vertically centered
            .overflow_clip();

        // Always render the full text (if any)
        if !display.is_empty() {
            if let Some((sel_start, sel_end)) = selection_range {
                // Has selection: render in three parts with highlighting
                let mut text_container = div().flex_row().items_center();

                // 1. Text before selection (normal)
                let before_sel: String = display.chars().take(sel_start).collect();
                if !before_sel.is_empty() {
                    text_container = text_container.child(
                        text(&before_sel)
                            .size(config.font_size)
                            .color(text_color)
                            .text_left()
                            .v_center(),
                    );
                }

                // 2. Selected text (highlighted)
                let selected: String = display
                    .chars()
                    .skip(sel_start)
                    .take(sel_end - sel_start)
                    .collect();
                if !selected.is_empty() {
                    text_container = text_container.child(
                        div().bg(selection_color).rounded(2.0).child(
                            text(&selected)
                                .size(config.font_size)
                                .color(text_color)
                                .text_left()
                                .v_center(),
                        ),
                    );
                }

                // 3. Text after selection (normal)
                let after_sel: String = display.chars().skip(sel_end).collect();
                if !after_sel.is_empty() {
                    text_container = text_container.child(
                        text(&after_sel)
                            .size(config.font_size)
                            .color(text_color)
                            .text_left()
                            .v_center(),
                    );
                }

                inner_content = inner_content.child(text_container);
            } else {
                // No selection: render full text as single element
                let text_element = text(&display)
                    .size(config.font_size)
                    .color(text_color)
                    .text_left()
                    .v_center();
                inner_content = inner_content.child(text_element);
            }
        }

        // Add cursor as a canvas-based overlay with smooth animation
        // The canvas handles its own opacity animation without tree rebuilds
        if is_focused && selection_range.is_none() {
            // Calculate vertical center offset for the cursor
            let cursor_top = (config.height - config.border_width * 2.0 - cursor_height) / 2.0;

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
            .left(config.padding_x + cursor_x)
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
        let config_for_click = config.clone();
        let state_for_blur = Arc::clone(state);
        let state_for_hover_enter = Arc::clone(state);
        let state_for_hover_leave = Arc::clone(state);
        let state_for_text_input = Arc::clone(state);
        let state_for_key_down = Arc::clone(state);

        div()
            .w(config.width)
            .h(config.height)
            .bg(border)
            .rounded(config.corner_radius)
            .p(config.border_width)
            .child(inner_content)
            // Wire up event handlers using FSM transitions
            .on_mouse_down(move |ctx| {
                // First, forcibly blur any previously focused text input/area
                // This must be done BEFORE we lock our own state to avoid deadlock
                set_focused_text_input(&state_for_click);

                if let Ok(mut s) = state_for_click.lock() {
                    if !s.disabled {
                        // Try POINTER_DOWN first (Hovered -> Focused)
                        // Then try FOCUS as fallback (Idle -> Focused)
                        let was_focused = s.visual.is_focused();
                        let old_state = s.visual;
                        if let Some(new_state) = s
                            .visual
                            .on_event(event_types::POINTER_DOWN)
                            .or_else(|| s.visual.on_event(event_types::FOCUS))
                        {
                            s.visual = new_state;
                            // Track global focus for cursor blink
                            if !was_focused && new_state.is_focused() {
                                increment_focus_count();
                                request_continuous_redraw();
                            }
                            tracing::info!(
                                "TextInput mouse_down: {:?} -> {:?}",
                                old_state,
                                new_state
                            );
                        } else {
                            tracing::info!("TextInput mouse_down: already focused {:?}", old_state);
                        }

                        // Always position cursor on click (even if already focused)
                        // local_x is relative to the text input element
                        // We need to account for padding and border
                        let click_x = ctx.local_x
                            - config_for_click.padding_x
                            - config_for_click.border_width;

                        if click_x <= 0.0 || s.value.is_empty() {
                            // Click before text or empty - cursor at start
                            s.cursor = 0;
                        } else {
                            // Measure text widths to find cursor position
                            let display_text = s.display_text();
                            let mut cursor_pos = display_text.chars().count();

                            // Find the character position where click_x falls
                            for (i, _) in display_text.char_indices() {
                                let text_before: String = display_text.chars().take(i).collect();
                                let text_at: String = display_text.chars().take(i + 1).collect();

                                let width_before = crate::text_measure::measure_text(
                                    &text_before,
                                    config_for_click.font_size,
                                )
                                .width;
                                let width_at = crate::text_measure::measure_text(
                                    &text_at,
                                    config_for_click.font_size,
                                )
                                .width;

                                // Check if click is between these two positions
                                let char_center = (width_before + width_at) / 2.0;
                                if click_x < char_center {
                                    cursor_pos = i;
                                    break;
                                }
                            }

                            s.cursor = cursor_pos;
                        }

                        // Clear selection when clicking
                        s.selection_start = None;
                        s.reset_cursor_blink(elapsed_ms());
                        s.sync_global_selection();
                        tracing::debug!(
                            "TextInput clicked, was_focused: {}, cursor: {}",
                            was_focused,
                            s.cursor
                        );
                    }
                }
                // Request rebuild for focus/cursor change
                request_rebuild();
            })
            .on_blur(move |_ctx| {
                if let Ok(mut s) = state_for_blur.lock() {
                    if !s.disabled {
                        // Use FSM: BLUR triggers Focused -> Idle
                        let was_focused = s.visual.is_focused();
                        let old_state = s.visual;
                        if let Some(new_state) = s.visual.on_event(event_types::BLUR) {
                            s.visual = new_state;
                            // Track global focus for cursor blink
                            if was_focused && !new_state.is_focused() {
                                decrement_focus_count();
                            }
                            tracing::info!(
                                "TextInput blur received: {:?} -> {:?}",
                                old_state,
                                new_state
                            );
                        } else {
                            tracing::info!(
                                "TextInput blur received but no transition from {:?}",
                                old_state
                            );
                        }
                    }
                }
                // Clear this as the focused input if it was
                clear_focused_text_input(&state_for_blur);
                // Request rebuild for blur change
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
                            tracing::debug!("TextInput received char: {:?}, value: {}", c, s.value);
                            request_rebuild();
                        }
                    }
                }
            })
            // Handle special keys (backspace, arrows, etc.)
            .on_key_down(move |ctx| {
                if let Ok(mut s) = state_for_key_down.lock() {
                    if !s.disabled && s.visual.is_focused() {
                        let mut cursor_changed = true;
                        match ctx.key_code {
                            8 => {
                                // Backspace
                                s.delete_backward();
                                tracing::debug!("TextInput backspace, value: {}", s.value);
                            }
                            127 => {
                                // Delete
                                s.delete_forward();
                            }
                            37 => {
                                // Left arrow
                                s.move_left(ctx.shift);
                            }
                            39 => {
                                // Right arrow
                                s.move_right(ctx.shift);
                            }
                            36 => {
                                // Home
                                s.move_to_start(ctx.shift);
                            }
                            35 => {
                                // End
                                s.move_to_end(ctx.shift);
                            }
                            65 if ctx.ctrl || ctx.meta => {
                                // Ctrl+A or Cmd+A - Select all
                                s.select_all();
                            }
                            _ => {
                                cursor_changed = false;
                            }
                        }
                        // Reset cursor blink to keep it visible during interaction
                        if cursor_changed {
                            s.reset_cursor_blink(elapsed_ms());
                            // Sync selection state with global clipboard state
                            s.sync_global_selection();
                            request_rebuild();
                        }
                    }
                }
            })
    }

    /// Rebuild the inner visual with current config and state (preserves outer structure)
    fn rebuild_inner(&mut self) {
        // Rebuild with updated config/state
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

    /// Set as password field (masked)
    pub fn password(mut self) -> Self {
        self.config.input_type = InputType::Password;
        if let Ok(mut s) = self.state.lock() {
            s.masked = true;
            s.input_type = InputType::Password;
        }
        self
    }

    /// Set as email field
    pub fn email(mut self) -> Self {
        self.config.input_type = InputType::Email;
        if let Ok(mut s) = self.state.lock() {
            s.input_type = InputType::Email;
        }
        self
    }

    /// Set as number field
    pub fn number(mut self) -> Self {
        self.config.input_type = InputType::Number;
        if let Ok(mut s) = self.state.lock() {
            s.input_type = InputType::Number;
        }
        self
    }

    /// Set as integer field
    pub fn integer(mut self) -> Self {
        self.config.input_type = InputType::Integer;
        if let Ok(mut s) = self.state.lock() {
            s.input_type = InputType::Integer;
        }
        self
    }

    /// Set as URL field
    pub fn url(mut self) -> Self {
        self.config.input_type = InputType::Url;
        if let Ok(mut s) = self.state.lock() {
            s.input_type = InputType::Url;
        }
        self
    }

    /// Set as telephone field
    pub fn tel(mut self) -> Self {
        self.config.input_type = InputType::Tel;
        if let Ok(mut s) = self.state.lock() {
            s.input_type = InputType::Tel;
        }
        self
    }

    /// Set as search field
    pub fn search(mut self) -> Self {
        self.config.input_type = InputType::Search;
        if let Ok(mut s) = self.state.lock() {
            s.input_type = InputType::Search;
        }
        self
    }

    /// Set minimum value for numeric inputs
    pub fn min(mut self, min: f64) -> Self {
        self.config.number_constraints.min = Some(min);
        if let Ok(mut s) = self.state.lock() {
            s.constraints.min = Some(min);
        }
        self
    }

    /// Set maximum value for numeric inputs
    pub fn max(mut self, max: f64) -> Self {
        self.config.number_constraints.max = Some(max);
        if let Ok(mut s) = self.state.lock() {
            s.constraints.max = Some(max);
        }
        self
    }

    /// Set as required field
    pub fn required(mut self) -> Self {
        self.config.required = true;
        if let Ok(mut s) = self.state.lock() {
            s.required = true;
        }
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

    /// Set font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.config.font_size = size;
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
        self.inner = std::mem::take(&mut self.inner).w(px);
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        self.config.height = px;
        self.inner = std::mem::take(&mut self.inner).h(px);
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
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

    /// Set padding on all sides of the outer container
    /// (This affects the visual border width since the outer div creates the border)
    pub fn p(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).p(px);
        self
    }

    /// Set horizontal padding inside the text input field
    /// (This affects the inner padding where text is displayed, not the outer border)
    pub fn px(mut self, px: f32) -> Self {
        self.config.padding_x = px;
        // Rebuild inner to apply new padding
        self.rebuild_inner();
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

/// Create a ready-to-use text input element
///
/// The text input inherits ALL Div methods, so you have full layout control.
///
/// # Example
///
/// ```ignore
/// let state = text_input_state_with_placeholder("Enter username");
/// text_input(&state)
///     .w(280.0)
///     .rounded(12.0)
///     .shadow_sm()
/// ```
pub fn text_input(state: &SharedTextInputState) -> TextInput {
    TextInput::new(state)
}

impl ElementBuilder for TextInput {
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
    fn test_text_input_state_insert() {
        let mut state = TextInputState::new();
        state.insert("hello");
        assert_eq!(state.value, "hello");
        assert_eq!(state.cursor, 5);

        state.insert(" world");
        assert_eq!(state.value, "hello world");
        assert_eq!(state.cursor, 11);
    }

    #[test]
    fn test_text_input_state_delete() {
        let mut state = TextInputState::with_value("hello");
        state.cursor = 5;

        state.delete_backward();
        assert_eq!(state.value, "hell");
        assert_eq!(state.cursor, 4);

        state.cursor = 2;
        state.delete_forward();
        assert_eq!(state.value, "hel");
    }

    #[test]
    fn test_text_input_state_selection() {
        let mut state = TextInputState::with_value("hello world");

        state.select_all();
        assert_eq!(state.selection_start, Some(0));
        assert_eq!(state.cursor, 11);
        assert_eq!(state.selected_text(), Some("hello world".to_string()));

        state.insert("new");
        assert_eq!(state.value, "new");
        assert_eq!(state.selection_start, None);
    }

    #[test]
    fn test_input_type_filtering() {
        let mut state = TextInputState::new();
        state.input_type = InputType::Number;

        state.insert("123.45");
        assert_eq!(state.value, "123.45");

        state.value.clear();
        state.cursor = 0;
        state.insert("abc123");
        assert_eq!(state.value, "123");
    }

    #[test]
    fn test_email_validation() {
        let mut state = TextInputState::new();
        state.input_type = InputType::Email;
        state.value = "test@example.com".to_string();
        state.validate();
        assert!(state.is_valid);

        state.value = "invalid".to_string();
        state.validate();
        assert!(!state.is_valid);
    }
}
