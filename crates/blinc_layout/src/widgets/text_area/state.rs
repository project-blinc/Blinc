//! The text area's state: its lines, cursor, selection and what it is drawn with.

use super::*;

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
