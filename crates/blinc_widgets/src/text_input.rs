//! TextInput widget with FSM-driven interactions
//!
//! The TextInput widget provides:
//! - Single-line text input with cursor
//! - Multiple input types: text, number, email, password, url, tel, search
//! - Visual states: idle, hovered, focused, disabled
//! - FSM-driven state transitions
//! - Selection support with shift+arrow/click-drag
//! - Keyboard shortcuts: Cmd+A (select all), Cmd+C/X/V (clipboard)
//! - Cursor blinking animation
//! - Customizable appearance
//! - Validation (min/max for numbers, pattern matching)

use blinc_animation::spring::{Spring, SpringConfig};
use blinc_core::events::{event_types, Event, EventData, KeyCode, Modifiers};
use blinc_core::fsm::StateMachine;
use blinc_core::{Brush, Color, DrawContext, Rect};
use blinc_layout::prelude::*;

use crate::context::WidgetContext;
use crate::widget::WidgetId;

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
            return true; // Empty is valid (use required for mandatory)
        }

        match self {
            InputType::Text | InputType::Password | InputType::Search | InputType::Tel => true,
            InputType::Number => value.parse::<f64>().is_ok(),
            InputType::Integer => value.parse::<i64>().is_ok(),
            InputType::Email => {
                // Basic email validation: has @ and at least one char before and after
                let parts: Vec<&str> = value.split('@').collect();
                parts.len() == 2 && !parts[0].is_empty() && parts[1].contains('.')
            }
            InputType::Url => {
                // Basic URL validation: starts with http:// or https://
                value.starts_with("http://") || value.starts_with("https://")
            }
        }
    }

    /// Should this input type be masked (like password)?
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

    /// Clamp a value to the constraints
    pub fn clamp(&self, value: f64) -> f64 {
        let mut result = value;
        if let Some(min) = self.min {
            result = result.max(min);
        }
        if let Some(max) = self.max {
            result = result.min(max);
        }
        result
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

/// TextInput FSM states
pub mod states {
    /// Idle state (unfocused, not hovered)
    pub const IDLE: u32 = 0;
    /// Hovered state (unfocused, cursor over input)
    pub const HOVERED: u32 = 1;
    /// Focused state (accepting input)
    pub const FOCUSED: u32 = 2;
    /// Focused and hovered
    pub const FOCUSED_HOVERED: u32 = 3;
    /// Selecting text (mouse down + drag)
    pub const SELECTING: u32 = 4;
    /// Disabled state
    pub const DISABLED: u32 = 5;
}

/// TextInput configuration
#[derive(Clone)]
pub struct TextInputConfig {
    /// Placeholder text shown when empty
    pub placeholder: String,
    /// Initial value
    pub value: String,
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
            value: String::new(),
            input_type: InputType::Text,
            number_constraints: NumberConstraints::default(),
            width: 200.0,
            height: 36.0,
            font_size: 14.0,
            text_color: Color::rgba(0.9, 0.9, 0.9, 1.0),
            placeholder_color: Color::rgba(0.5, 0.5, 0.5, 1.0),
            bg_color: Color::rgba(0.15, 0.15, 0.2, 1.0),
            focused_bg_color: Color::rgba(0.18, 0.18, 0.25, 1.0),
            border_color: Color::rgba(0.3, 0.3, 0.35, 1.0),
            focused_border_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            error_border_color: Color::rgba(1.0, 0.3, 0.3, 1.0),
            border_width: 1.0,
            corner_radius: 6.0,
            padding_x: 12.0,
            cursor_color: Color::rgba(0.4, 0.6, 1.0, 1.0),
            selection_color: Color::rgba(0.4, 0.6, 1.0, 0.3),
            disabled: false,
            max_length: 0,
            required: false,
        }
    }
}

impl TextInputConfig {
    /// Create a new text input config
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

    /// Set the input type
    pub fn input_type(mut self, input_type: InputType) -> Self {
        self.input_type = input_type;
        self
    }

    /// Set this as a number input with optional constraints
    pub fn number(mut self) -> Self {
        self.input_type = InputType::Number;
        self
    }

    /// Set this as an integer input
    pub fn integer(mut self) -> Self {
        self.input_type = InputType::Integer;
        self
    }

    /// Set this as an email input
    pub fn email(mut self) -> Self {
        self.input_type = InputType::Email;
        self
    }

    /// Set this as a password input
    pub fn password(mut self) -> Self {
        self.input_type = InputType::Password;
        self
    }

    /// Set this as a URL input
    pub fn url(mut self) -> Self {
        self.input_type = InputType::Url;
        self
    }

    /// Set this as a telephone input
    pub fn tel(mut self) -> Self {
        self.input_type = InputType::Tel;
        self
    }

    /// Set this as a search input
    pub fn search(mut self) -> Self {
        self.input_type = InputType::Search;
        self
    }

    /// Set minimum value for numeric inputs
    pub fn min(mut self, min: f64) -> Self {
        self.number_constraints.min = Some(min);
        self
    }

    /// Set maximum value for numeric inputs
    pub fn max(mut self, max: f64) -> Self {
        self.number_constraints.max = Some(max);
        self
    }

    /// Set step value for numeric inputs
    pub fn step(mut self, step: f64) -> Self {
        self.number_constraints.step = Some(step);
        self
    }

    /// Set the width
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Set the height
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Set the font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Set whether the input is disabled
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set whether the field is required
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
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
}

/// TextInput widget state
pub struct TextInputState {
    /// Current text value
    pub value: String,
    /// Cursor position (character index)
    pub cursor_pos: usize,
    /// Selection start position (if selecting)
    pub selection_start: Option<usize>,
    /// Cursor blink timer (0.0 - 1.0)
    cursor_blink: f32,
    /// Whether cursor is visible in current blink cycle
    cursor_visible: bool,
    /// Spring for focus animation
    focus_spring: Spring,
    /// Focus animation value (0 = unfocused, 1 = focused)
    pub focus_value: f32,
    /// Whether the value changed (cleared after reading)
    changed: bool,
    /// Whether enter was pressed (cleared after reading)
    submitted: bool,
    /// Whether the current value is valid
    pub is_valid: bool,
    /// Validation error message
    pub validation_error: Option<String>,
}

impl Clone for TextInputState {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            cursor_pos: self.cursor_pos,
            selection_start: self.selection_start,
            cursor_blink: self.cursor_blink,
            cursor_visible: self.cursor_visible,
            focus_spring: Spring::new(SpringConfig::snappy(), self.focus_value),
            focus_value: self.focus_value,
            changed: self.changed,
            submitted: self.submitted,
            is_valid: self.is_valid,
            validation_error: self.validation_error.clone(),
        }
    }
}

impl TextInputState {
    /// Create new text input state
    pub fn new(initial_value: String) -> Self {
        let cursor_pos = initial_value.chars().count();
        Self {
            value: initial_value,
            cursor_pos,
            selection_start: None,
            cursor_blink: 0.0,
            cursor_visible: true,
            focus_spring: Spring::new(SpringConfig::snappy(), 0.0),
            focus_value: 0.0,
            changed: false,
            submitted: false,
            is_valid: true,
            validation_error: None,
        }
    }

    /// Validate the value against input type and constraints
    pub fn validate(
        &mut self,
        input_type: InputType,
        constraints: &NumberConstraints,
        required: bool,
    ) {
        // Check required
        if required && self.value.is_empty() {
            self.is_valid = false;
            self.validation_error = Some("This field is required".to_string());
            return;
        }

        // Check input type validation
        if !input_type.validate(&self.value) {
            self.is_valid = false;
            self.validation_error = Some(match input_type {
                InputType::Number => "Please enter a valid number".to_string(),
                InputType::Integer => "Please enter a valid integer".to_string(),
                InputType::Email => "Please enter a valid email address".to_string(),
                InputType::Url => {
                    "Please enter a valid URL (starting with http:// or https://)".to_string()
                }
                _ => "Invalid input".to_string(),
            });
            return;
        }

        // Check number constraints
        if matches!(input_type, InputType::Number | InputType::Integer) {
            if let Ok(num) = self.value.parse::<f64>() {
                if !constraints.validate(num) {
                    self.is_valid = false;
                    let min = constraints.min.map(|v| v.to_string()).unwrap_or_default();
                    let max = constraints.max.map(|v| v.to_string()).unwrap_or_default();
                    self.validation_error = Some(match (constraints.min, constraints.max) {
                        (Some(_), Some(_)) => format!("Value must be between {} and {}", min, max),
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

    /// Get the numeric value if this is a number input
    pub fn as_number(&self) -> Option<f64> {
        self.value.parse().ok()
    }

    /// Get the integer value if this is an integer input
    pub fn as_integer(&self) -> Option<i64> {
        self.value.parse().ok()
    }

    /// Update animations (call each frame)
    pub fn update(&mut self, dt: f32, is_focused: bool) {
        // Update focus spring
        self.focus_spring
            .set_target(if is_focused { 1.0 } else { 0.0 });
        self.focus_spring.step(dt);
        self.focus_value = self.focus_spring.value();

        // Update cursor blink (only when focused)
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

    /// Reset cursor blink (call on any input)
    pub fn reset_blink(&mut self) {
        self.cursor_blink = 0.0;
        self.cursor_visible = true;
    }

    /// Insert text at cursor position with input type filtering
    pub fn insert(&mut self, text: &str, max_length: usize) {
        self.insert_with_filter(text, max_length, InputType::Text);
    }

    /// Insert text at cursor position with input type filtering
    pub fn insert_with_filter(&mut self, text: &str, max_length: usize, input_type: InputType) {
        // Delete selection first if any
        self.delete_selection();

        // Filter characters based on input type
        let filtered: String = text
            .chars()
            .filter(|c| input_type.allows_char(*c))
            .collect();

        if filtered.is_empty() {
            return;
        }

        let char_count = self.value.chars().count();
        let insert_len = filtered.chars().count();

        // Check max length
        if max_length > 0 && char_count + insert_len > max_length {
            let allowed = max_length.saturating_sub(char_count);
            if allowed == 0 {
                return;
            }
            let truncated: String = filtered.chars().take(allowed).collect();
            self.insert_at_cursor(&truncated);
        } else {
            self.insert_at_cursor(&filtered);
        }

        self.reset_blink();
        self.changed = true;
    }

    fn insert_at_cursor(&mut self, text: &str) {
        let byte_pos = self.cursor_byte_pos();
        self.value.insert_str(byte_pos, text);
        self.cursor_pos += text.chars().count();
    }

    /// Delete character before cursor (backspace)
    pub fn delete_backward(&mut self) {
        if self.delete_selection() {
            self.changed = true;
            return;
        }

        if self.cursor_pos > 0 {
            let char_count = self.value.chars().count();
            if self.cursor_pos <= char_count {
                // Find byte position of character before cursor
                let byte_start = self.char_to_byte_pos(self.cursor_pos - 1);
                let byte_end = self.char_to_byte_pos(self.cursor_pos);
                self.value.replace_range(byte_start..byte_end, "");
                self.cursor_pos -= 1;
                self.changed = true;
            }
        }
        self.reset_blink();
    }

    /// Delete character after cursor (delete key)
    pub fn delete_forward(&mut self) {
        if self.delete_selection() {
            self.changed = true;
            return;
        }

        let char_count = self.value.chars().count();
        if self.cursor_pos < char_count {
            let byte_start = self.char_to_byte_pos(self.cursor_pos);
            let byte_end = self.char_to_byte_pos(self.cursor_pos + 1);
            self.value.replace_range(byte_start..byte_end, "");
            self.changed = true;
        }
        self.reset_blink();
    }

    /// Delete selected text, returns true if there was a selection
    fn delete_selection(&mut self) -> bool {
        if let Some(start) = self.selection_start {
            let (from, to) = if start < self.cursor_pos {
                (start, self.cursor_pos)
            } else {
                (self.cursor_pos, start)
            };

            if from != to {
                let byte_start = self.char_to_byte_pos(from);
                let byte_end = self.char_to_byte_pos(to);
                self.value.replace_range(byte_start..byte_end, "");
                self.cursor_pos = from;
                self.selection_start = None;
                return true;
            }
        }
        self.selection_start = None;
        false
    }

    /// Move cursor left
    pub fn move_left(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        } else if !select {
            // If there's a selection, move to start of selection
            if let Some(start) = self.selection_start {
                self.cursor_pos = self.cursor_pos.min(start);
                self.selection_start = None;
                return;
            }
        }

        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
        if !select {
            self.selection_start = None;
        }
        self.reset_blink();
    }

    /// Move cursor right
    pub fn move_right(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        } else if !select {
            // If there's a selection, move to end of selection
            if let Some(start) = self.selection_start {
                self.cursor_pos = self.cursor_pos.max(start);
                self.selection_start = None;
                return;
            }
        }

        let char_count = self.value.chars().count();
        if self.cursor_pos < char_count {
            self.cursor_pos += 1;
        }
        if !select {
            self.selection_start = None;
        }
        self.reset_blink();
    }

    /// Move cursor to start
    pub fn move_to_start(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor_pos = 0;
        self.reset_blink();
    }

    /// Move cursor to end
    pub fn move_to_end(&mut self, select: bool) {
        if select && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        } else if !select {
            self.selection_start = None;
        }
        self.cursor_pos = self.value.chars().count();
        self.reset_blink();
    }

    /// Select all text
    pub fn select_all(&mut self) {
        self.selection_start = Some(0);
        self.cursor_pos = self.value.chars().count();
        self.reset_blink();
    }

    /// Get selected text
    pub fn selected_text(&self) -> Option<String> {
        self.selection_start.map(|start| {
            let (from, to) = if start < self.cursor_pos {
                (start, self.cursor_pos)
            } else {
                (self.cursor_pos, start)
            };
            self.value.chars().skip(from).take(to - from).collect()
        })
    }

    /// Check if value changed and clear the flag
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Check if submitted (enter pressed) and clear the flag
    pub fn take_submitted(&mut self) -> bool {
        std::mem::take(&mut self.submitted)
    }

    /// Get byte position from cursor position
    fn cursor_byte_pos(&self) -> usize {
        self.char_to_byte_pos(self.cursor_pos)
    }

    /// Convert character index to byte index
    fn char_to_byte_pos(&self, char_pos: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }

    /// Get display text (with password masking)
    pub fn display_text(&self, input_type: InputType) -> String {
        if input_type.is_masked() {
            "•".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        }
    }
}

/// TextInput widget
pub struct TextInput {
    /// Widget ID
    id: WidgetId,
    /// Configuration
    config: TextInputConfig,
    /// Change callback
    on_change: Option<Box<dyn FnMut(&str) + Send>>,
    /// Submit callback (enter pressed)
    on_submit: Option<Box<dyn FnMut(&str) + Send>>,
}

impl TextInput {
    /// Create a new text input
    pub fn new(ctx: &mut WidgetContext) -> Self {
        Self::with_config(ctx, TextInputConfig::default())
    }

    /// Create a text input with custom config
    pub fn with_config(ctx: &mut WidgetContext, config: TextInputConfig) -> Self {
        let fsm = Self::create_fsm(&config);
        let id = ctx.register_widget_with_fsm(fsm);

        // Initialize text input state
        let state = TextInputState::new(config.value.clone());
        ctx.set_widget_state(id, state);

        Self {
            id,
            config,
            on_change: None,
            on_submit: None,
        }
    }

    /// Create the text input FSM
    fn create_fsm(config: &TextInputConfig) -> StateMachine {
        if config.disabled {
            StateMachine::builder(states::DISABLED).build()
        } else {
            StateMachine::builder(states::IDLE)
                // Idle transitions
                .on(states::IDLE, event_types::POINTER_ENTER, states::HOVERED)
                .on(states::IDLE, event_types::FOCUS, states::FOCUSED)
                // Hovered transitions
                .on(states::HOVERED, event_types::POINTER_LEAVE, states::IDLE)
                .on(states::HOVERED, event_types::POINTER_DOWN, states::FOCUSED)
                .on(states::HOVERED, event_types::FOCUS, states::FOCUSED_HOVERED)
                // Focused transitions
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
                // Focused+Hovered transitions
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
                // Selecting transitions (back to focused on release)
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

    /// Check if the input is focused
    pub fn is_focused(&self, ctx: &WidgetContext) -> bool {
        let state = ctx.get_fsm_state(self.id).unwrap_or(states::IDLE);
        matches!(
            state,
            states::FOCUSED | states::FOCUSED_HOVERED | states::SELECTING
        )
    }

    /// Get the current value
    pub fn value(&self, ctx: &WidgetContext) -> String {
        ctx.get_widget_state::<TextInputState>(self.id)
            .map(|s| s.value.clone())
            .unwrap_or_default()
    }

    /// Set the value programmatically
    pub fn set_value(&self, ctx: &mut WidgetContext, value: impl Into<String>) {
        if let Some(state) = ctx.get_widget_state_mut::<TextInputState>(self.id) {
            state.value = value.into();
            state.cursor_pos = state.value.chars().count();
            state.selection_start = None;
        }
    }

    /// Set the change callback
    pub fn on_change<F: FnMut(&str) + Send + 'static>(mut self, callback: F) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }

    /// Set the submit callback
    pub fn on_submit<F: FnMut(&str) + Send + 'static>(mut self, callback: F) -> Self {
        self.on_submit = Some(Box::new(callback));
        self
    }

    /// Handle an event
    pub fn handle_event(&mut self, ctx: &mut WidgetContext, event: &Event) {
        if self.config.disabled {
            return;
        }

        let was_focused = self.is_focused(ctx);

        // Dispatch to FSM for state transitions
        ctx.dispatch_event(self.id, event);

        let is_focused = self.is_focused(ctx);

        // Handle text input
        if is_focused {
            match &event.data {
                EventData::TextInput { text } => {
                    if let Some(state) = ctx.get_widget_state_mut::<TextInputState>(self.id) {
                        state.insert_with_filter(
                            text,
                            self.config.max_length,
                            self.config.input_type,
                        );
                        // Validate after change
                        state.validate(
                            self.config.input_type,
                            &self.config.number_constraints,
                            self.config.required,
                        );
                        if state.take_changed() {
                            if let Some(ref mut callback) = self.on_change {
                                callback(&state.value);
                            }
                        }
                    }
                }
                EventData::Key { key, modifiers, .. } => {
                    self.handle_key(ctx, *key, *modifiers);
                }
                EventData::Clipboard { text } => {
                    if let Some(state) = ctx.get_widget_state_mut::<TextInputState>(self.id) {
                        state.insert_with_filter(
                            text,
                            self.config.max_length,
                            self.config.input_type,
                        );
                        // Validate after change
                        state.validate(
                            self.config.input_type,
                            &self.config.number_constraints,
                            self.config.required,
                        );
                        if state.take_changed() {
                            if let Some(ref mut callback) = self.on_change {
                                callback(&state.value);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Handle focus change
        if !was_focused && is_focused {
            // Just gained focus - select all text
            if let Some(state) = ctx.get_widget_state_mut::<TextInputState>(self.id) {
                state.select_all();
            }
        }
    }

    /// Handle keyboard input
    fn handle_key(&mut self, ctx: &mut WidgetContext, key: KeyCode, modifiers: Modifiers) {
        let state = match ctx.get_widget_state_mut::<TextInputState>(self.id) {
            Some(s) => s,
            None => return,
        };

        let select = modifiers.shift();
        let command = modifiers.command();

        match key {
            KeyCode::BACKSPACE => {
                if command {
                    // Delete to start of line
                    state.move_to_start(true);
                    state.delete_selection();
                    state.changed = true;
                } else {
                    state.delete_backward();
                }
            }
            KeyCode::DELETE => {
                if command {
                    // Delete to end of line
                    state.move_to_end(true);
                    state.delete_selection();
                    state.changed = true;
                } else {
                    state.delete_forward();
                }
            }
            KeyCode::LEFT => {
                if command {
                    state.move_to_start(select);
                } else {
                    state.move_left(select);
                }
            }
            KeyCode::RIGHT => {
                if command {
                    state.move_to_end(select);
                } else {
                    state.move_right(select);
                }
            }
            KeyCode::HOME => {
                state.move_to_start(select);
            }
            KeyCode::END => {
                state.move_to_end(select);
            }
            KeyCode::A if command => {
                state.select_all();
            }
            KeyCode::ENTER => {
                state.submitted = true;
            }
            KeyCode::ESCAPE => {
                // Could blur here if we had focus management
            }
            _ => {}
        }

        // Trigger callbacks
        if state.take_changed() {
            let value = state.value.clone();
            if let Some(ref mut callback) = self.on_change {
                callback(&value);
            }
        }
        if state.take_submitted() {
            let value = state.value.clone();
            if let Some(ref mut callback) = self.on_submit {
                callback(&value);
            }
        }
    }

    /// Update animations (call each frame)
    pub fn update(&self, ctx: &mut WidgetContext, dt: f32) {
        let is_focused = self.is_focused(ctx);

        if let Some(state) = ctx.get_widget_state_mut::<TextInputState>(self.id) {
            let old_focus = state.focus_value;
            let old_visible = state.cursor_visible;

            state.update(dt, is_focused);

            // Mark dirty if visual state changed
            if (state.focus_value - old_focus).abs() > 0.001 || state.cursor_visible != old_visible
            {
                ctx.mark_dirty(self.id);
            }
        }
    }

    /// Build the text input's UI element
    pub fn build(&self, ctx: &WidgetContext) -> Div {
        let state = ctx
            .get_widget_state::<TextInputState>(self.id)
            .cloned()
            .unwrap_or_else(|| TextInputState::new(String::new()));

        let fsm_state = ctx.get_fsm_state(self.id).unwrap_or(states::IDLE);
        let is_focused = matches!(
            fsm_state,
            states::FOCUSED | states::FOCUSED_HOVERED | states::SELECTING
        );
        let _is_hovered = matches!(fsm_state, states::HOVERED | states::FOCUSED_HOVERED);

        // Interpolate colors based on focus and validation state
        let bg_color = Color::lerp(
            &self.config.bg_color,
            &self.config.focused_bg_color,
            state.focus_value,
        );

        // Use error color if invalid and not empty
        let border_color = if !state.is_valid && !state.value.is_empty() {
            self.config.error_border_color
        } else {
            Color::lerp(
                &self.config.border_color,
                &self.config.focused_border_color,
                state.focus_value,
            )
        };

        // Build text content
        let display_text = state.display_text(self.config.input_type);
        let is_empty = state.value.is_empty();

        // Build the inner content container
        let mut inner = div()
            .w_full()
            .h_full()
            .bg(bg_color)
            .rounded(self.config.corner_radius.max(1.0) - 1.0)
            .px(self.config.padding_x)
            .flex_row()
            .items_center()
            .overflow_clip();

        // Add text content
        let text_content = if is_empty {
            text(&self.config.placeholder)
                .size(self.config.font_size)
                .color(self.config.placeholder_color)
        } else {
            text(&display_text)
                .size(self.config.font_size)
                .color(self.config.text_color)
        };
        inner = inner.child(text_content);

        // Cursor canvas (absolute positioned, doesn't affect text layout)
        // Canvas always exists, but cursor rect is conditionally drawn based on focus/blink state
        let cursor_visible = is_focused && state.cursor_visible;
        let cursor_color = self.config.cursor_color;
        let cursor_height = self.config.font_size + 4.0;

        // Calculate cursor x position based on text width
        // TODO: Use proper text measurement for accurate positioning
        let char_width = self.config.font_size * 0.6; // Approximate monospace width
        let cursor_x = self.config.padding_x + (state.cursor_pos as f32 * char_width);

        inner = inner.child(
            canvas(move |ctx: &mut dyn DrawContext, bounds| {
                // Only draw cursor rect when visible (blink on + focused)
                if cursor_visible {
                    let cursor_y = (bounds.height - cursor_height) / 2.0;
                    ctx.fill_rect(
                        Rect::new(0.0, cursor_y, 2.0, cursor_height),
                        0.0.into(),
                        Brush::Solid(cursor_color),
                    );
                }
            })
            .absolute()
            .left(cursor_x)
            .top(0.0)
            .w(2.0)
            .h_full(),
        );

        // Build outer container (provides border)
        div()
            .w(self.config.width)
            .h(self.config.height)
            .bg(border_color)
            .rounded(self.config.corner_radius)
            .p(self.config.border_width)
            .child(inner)
    }
}

/// Create a text input
pub fn text_input() -> TextInputBuilder {
    TextInputBuilder {
        config: TextInputConfig::default(),
        on_change: None,
        on_submit: None,
    }
}

/// Builder for creating text inputs
pub struct TextInputBuilder {
    config: TextInputConfig,
    on_change: Option<Box<dyn FnMut(&str) + Send>>,
    on_submit: Option<Box<dyn FnMut(&str) + Send>>,
}

impl TextInputBuilder {
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

    /// Set the input type
    pub fn input_type(mut self, input_type: InputType) -> Self {
        self.config.input_type = input_type;
        self
    }

    /// Set this as a number input
    pub fn number(mut self) -> Self {
        self.config.input_type = InputType::Number;
        self
    }

    /// Set this as an integer input
    pub fn integer(mut self) -> Self {
        self.config.input_type = InputType::Integer;
        self
    }

    /// Set this as an email input
    pub fn email(mut self) -> Self {
        self.config.input_type = InputType::Email;
        self
    }

    /// Set this as a password input
    pub fn password(mut self) -> Self {
        self.config.input_type = InputType::Password;
        self
    }

    /// Set this as a URL input
    pub fn url(mut self) -> Self {
        self.config.input_type = InputType::Url;
        self
    }

    /// Set this as a telephone input
    pub fn tel(mut self) -> Self {
        self.config.input_type = InputType::Tel;
        self
    }

    /// Set this as a search input
    pub fn search(mut self) -> Self {
        self.config.input_type = InputType::Search;
        self
    }

    /// Set minimum value for numeric inputs
    pub fn min(mut self, min: f64) -> Self {
        self.config.number_constraints.min = Some(min);
        self
    }

    /// Set maximum value for numeric inputs
    pub fn max(mut self, max: f64) -> Self {
        self.config.number_constraints.max = Some(max);
        self
    }

    /// Set step value for numeric inputs
    pub fn step(mut self, step: f64) -> Self {
        self.config.number_constraints.step = Some(step);
        self
    }

    /// Set whether the field is required
    pub fn required(mut self, required: bool) -> Self {
        self.config.required = required;
        self
    }

    /// Set the width
    pub fn width(mut self, width: f32) -> Self {
        self.config.width = width;
        self
    }

    /// Set the height
    pub fn height(mut self, height: f32) -> Self {
        self.config.height = height;
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

    /// Set whether the input is disabled
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Set the maximum length
    pub fn max_length(mut self, max: usize) -> Self {
        self.config.max_length = max;
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

    /// Set the submit callback (enter pressed)
    pub fn on_submit<F: FnMut(&str) + Send + 'static>(mut self, callback: F) -> Self {
        self.on_submit = Some(Box::new(callback));
        self
    }

    /// Build the text input widget
    pub fn build(self, ctx: &mut WidgetContext) -> TextInput {
        let mut input = TextInput::with_config(ctx, self.config);
        input.on_change = self.on_change;
        input.on_submit = self.on_submit;
        input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_input_state_insert() {
        let mut state = TextInputState::new(String::new());

        state.insert("hello", 0);
        assert_eq!(state.value, "hello");
        assert_eq!(state.cursor_pos, 5);

        state.insert(" world", 0);
        assert_eq!(state.value, "hello world");
        assert_eq!(state.cursor_pos, 11);
    }

    #[test]
    fn test_text_input_state_delete() {
        let mut state = TextInputState::new("hello".to_string());
        state.cursor_pos = 5;

        state.delete_backward();
        assert_eq!(state.value, "hell");
        assert_eq!(state.cursor_pos, 4);

        state.cursor_pos = 2;
        state.delete_forward();
        assert_eq!(state.value, "hel");
    }

    #[test]
    fn test_text_input_state_selection() {
        let mut state = TextInputState::new("hello world".to_string());

        state.select_all();
        assert_eq!(state.selection_start, Some(0));
        assert_eq!(state.cursor_pos, 11);
        assert_eq!(state.selected_text(), Some("hello world".to_string()));

        state.insert("new", 0);
        assert_eq!(state.value, "new");
        assert_eq!(state.selection_start, None);
    }

    #[test]
    fn test_text_input_state_max_length() {
        let mut state = TextInputState::new(String::new());

        state.insert("hello", 3);
        assert_eq!(state.value, "hel");
        assert_eq!(state.cursor_pos, 3);
    }

    #[test]
    fn test_text_input_state_navigation() {
        let mut state = TextInputState::new("hello".to_string());
        state.cursor_pos = 5;

        state.move_left(false);
        assert_eq!(state.cursor_pos, 4);

        state.move_to_start(false);
        assert_eq!(state.cursor_pos, 0);

        state.move_to_end(false);
        assert_eq!(state.cursor_pos, 5);

        state.move_right(false);
        assert_eq!(state.cursor_pos, 5); // Already at end
    }

    #[test]
    fn test_text_input_creation() {
        let mut ctx = WidgetContext::new();
        let input = TextInput::new(&mut ctx);

        assert!(ctx.is_registered(input.id()));
        assert_eq!(ctx.get_fsm_state(input.id()), Some(states::IDLE));
    }
}
