//! A text input's state and the types it is configured with.

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputType {
    #[default]
    Text,
    Number,
    Integer,
    Email,
    Password,
    Url,
    Tel,
    Search,
}

#[derive(Clone, Debug, Default)]
pub struct InputConstraints {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub pattern: Option<String>,
    pub required: bool,
}

impl InputConstraints {
    pub fn max_length(max: usize) -> Self {
        Self {
            max_length: Some(max),
            ..Default::default()
        }
    }

    pub fn required() -> Self {
        Self {
            required: true,
            ..Default::default()
        }
    }

    pub fn number_range(min: f64, max: f64) -> Self {
        Self {
            min_value: Some(min),
            max_value: Some(max),
            ..Default::default()
        }
    }
}

// =============================================================================
// TextInputData - the external state that persists across rebuilds
// =============================================================================

/// Shared text input data handle
pub type SharedTextInputData = Arc<Mutex<TextInputData>>;

/// Text input data (content, cursor, validation)
///
/// The state the field shows, its visual state (hover/focus) included.
#[derive(Clone)]
pub struct TextInputData {
    pub value: String,
    pub cursor: usize,
    pub selection_start: Option<usize>,
    pub placeholder: String,
    pub input_type: InputType,
    pub constraints: InputConstraints,
    pub disabled: bool,
    pub masked: bool,
    pub is_valid: bool,
    pub visual: TextFieldState,
    pub focus_time_ms: u64,
    pub cursor_state: SharedCursorState,
    /// Horizontal scroll offset for text that exceeds the input width
    pub scroll_offset_x: f32,
    /// Computed width of the text input (set after layout, used for scroll calculations)
    /// This is updated when the layout is computed and allows proper scroll behavior
    /// even when `use_full_width` is true.
    pub computed_width: Option<f32>,
    /// Layout bounds storage - updated after each layout computation
    /// Used to get the actual computed width for proper scroll behavior
    pub layout_bounds_storage: crate::renderer::LayoutBoundsStorage,
    /// Bumped whenever something the field shows changes; its content is
    /// bound to it. `None` until a `TextInput` is made over the data.
    pub(crate) revision: Option<blinc_core::reactive::Signal<u64>>,
    /// The field's node, once built.
    pub(crate) node_id: Option<LayoutNodeId>,
    /// Callback invoked when text value changes
    pub(crate) on_change_callback: Option<OnChangeCallback>,
    /// Optional stepper hook fired when the user presses ↑ / ↓ / + / −
    /// while focused. Argument is `+1` for increment, `-1` for
    /// decrement. When set, the keys are *consumed* (the default
    /// behaviour — character insertion for `+` / `−`, no-op for
    /// arrows on a single-line field — is suppressed). Used by
    /// `cn::number_input` to wire keyboard stepping to the bound
    /// `State<f64>`. Unset by default so a plain text input still
    /// accepts a typed `-` as the leading sign of a negative number.
    pub(crate) on_step_callback: Option<Arc<dyn Fn(i32) + Send + Sync>>,
    /// Forces the next render sync to apply even while focused (used by
    /// `cn::number_input`'s stepper hook). Consumed on read.
    pub force_sync_once: bool,
    /// Hook for composite inputs that need empty-Backspace behavior.
    pub(crate) on_backspace_empty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Hook for composite inputs that need to own paste handling.
    pub(crate) on_paste_override_callback: Option<crate::widgets::TextPredicate>,
    /// Hook for composite inputs that need to redirect focus before the
    /// field becomes active.
    pub(crate) on_focus_request_callback: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    /// CSS element ID for stylesheet matching (set via TextInput::id())
    pub(crate) css_element_id: Option<String>,
    /// CSS class names for stylesheet matching (set via TextInput::class())
    pub(crate) css_classes: Vec<std::sync::Arc<str>>,
    /// Last click timestamp for double-click detection
    pub(crate) last_click_time: Option<web_time::Instant>,
    /// Anchor position for drag-to-select
    pub(crate) drag_select_anchor: Option<usize>,
    /// Undo history. Each entry snapshots `(value, cursor, selection_start)`
    /// captured immediately BEFORE a text-mutating operation runs (insert,
    /// delete_*). Cmd+Z pops from this stack onto the redo stack and
    /// restores the popped entry. Capped at [`UNDO_HISTORY_MAX`] entries
    /// — older entries are dropped from the front when the cap is hit.
    pub(crate) undo_stack: Vec<UndoEntry>,
    /// Redo history. Populated by [`Self::undo`] (and cleared by any new
    /// edit since a fresh edit starts a new branch in the history). Cmd+Shift+Z
    /// (or Cmd+Y) pops from this stack onto the undo stack.
    pub(crate) redo_stack: Vec<UndoEntry>,
}

/// One snapshot in the undo / redo history. Stores the full pre-edit
/// state of the text input. We snapshot the entire `value` rather than
/// a diff because the typical input field is short enough that the
/// memory cost is negligible (a 100-entry stack of 80-char strings is
/// ~8 KB).
#[derive(Clone, Debug)]
pub struct UndoEntry {
    pub value: String,
    pub cursor: usize,
    pub selection_start: Option<usize>,
}

/// Maximum number of entries kept in the undo / redo stacks. Once
/// exceeded, the oldest entry is dropped from the front to make room.
pub(super) const UNDO_HISTORY_MAX: usize = 100;

impl std::fmt::Debug for TextInputData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInputData")
            .field("value", &self.value)
            .field("cursor", &self.cursor)
            .field("selection_start", &self.selection_start)
            .field("placeholder", &self.placeholder)
            .field("input_type", &self.input_type)
            .field("constraints", &self.constraints)
            .field("disabled", &self.disabled)
            .field("masked", &self.masked)
            .field("is_valid", &self.is_valid)
            .field("visual", &self.visual)
            .field("focus_time_ms", &self.focus_time_ms)
            .finish()
    }
}

impl Default for TextInputData {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a shared text input data
pub fn text_input_data() -> SharedTextInputData {
    Arc::new(Mutex::new(TextInputData::new()))
}

/// Create a shared text input data with placeholder
pub fn text_input_data_with_placeholder(placeholder: impl Into<String>) -> SharedTextInputData {
    Arc::new(Mutex::new(TextInputData::with_placeholder(placeholder)))
}

// Backwards compatibility aliases
pub type TextInputState = TextInputData;

pub type SharedTextInputState = SharedTextInputData;

pub fn text_input_state() -> SharedTextInputData {
    text_input_data()
}

pub fn text_input_state_with_placeholder(placeholder: impl Into<String>) -> SharedTextInputData {
    text_input_data_with_placeholder(placeholder)
}

// =============================================================================
// CSS Override Resolution
// =============================================================================
