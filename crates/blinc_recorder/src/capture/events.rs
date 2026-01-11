//! Event types for recording user interactions.

use super::primitives::{Point, Timestamp};
use serde::{Deserialize, Serialize};

/// A recorded event with timestamp.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimestampedEvent {
    /// When the event occurred (relative to session start).
    pub timestamp: Timestamp,
    /// The event data.
    pub event: RecordedEvent,
}

impl TimestampedEvent {
    pub fn new(timestamp: Timestamp, event: RecordedEvent) -> Self {
        Self { timestamp, event }
    }
}

/// All recordable event types.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecordedEvent {
    // Mouse events
    MouseDown(MouseEvent),
    MouseUp(MouseEvent),
    MouseMove(MouseMoveEvent),
    Click(MouseEvent),
    DoubleClick(MouseEvent),

    // Keyboard events
    KeyDown(KeyEvent),
    KeyUp(KeyEvent),
    TextInput(TextInputEvent),

    // Scroll events
    Scroll(ScrollEvent),

    // Focus events
    FocusChange(FocusChangeEvent),

    // Hover events
    HoverEnter(HoverEvent),
    HoverLeave(HoverEvent),

    // Window events
    WindowResize(WindowResizeEvent),
    WindowFocus(bool),

    // Custom/application events
    Custom(CustomEvent),
}

/// Mouse button identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

impl Default for MouseButton {
    fn default() -> Self {
        MouseButton::Left
    }
}

/// Mouse event data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MouseEvent {
    /// Position in window coordinates.
    pub position: Point,
    /// Which button was pressed/released.
    pub button: MouseButton,
    /// Modifier keys held during the event.
    pub modifiers: Modifiers,
    /// Target element ID (if any).
    pub target_element: Option<String>,
}

/// Mouse move event data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MouseMoveEvent {
    /// Current position in window coordinates.
    pub position: Point,
    /// Element currently under cursor (if any).
    pub hover_element: Option<String>,
}

/// Keyboard key identifiers.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    // Letters
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // Numbers
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Navigation
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,

    // Editing
    Backspace,
    Delete,
    Insert,
    Enter,
    Tab,
    Escape,
    Space,

    // Modifiers (as keys)
    Shift,
    Control,
    Alt,
    Meta,

    // Other
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,

    // Unknown/other key with scan code
    Other(u32),
}

/// Modifier key state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn any(&self) -> bool {
        self.shift || self.ctrl || self.alt || self.meta
    }
}

/// Keyboard event data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyEvent {
    /// The key that was pressed/released.
    pub key: Key,
    /// Modifier keys held during the event.
    pub modifiers: Modifiers,
    /// Whether this is a repeat event.
    pub is_repeat: bool,
    /// Focused element at time of event.
    pub focused_element: Option<String>,
}

/// Text input event data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextInputEvent {
    /// The text that was input.
    pub text: String,
    /// Focused element receiving the input.
    pub focused_element: Option<String>,
}

/// Scroll event data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScrollEvent {
    /// Position where scroll occurred.
    pub position: Point,
    /// Scroll delta (x, y).
    pub delta_x: f32,
    pub delta_y: f32,
    /// Element being scrolled (if any).
    pub target_element: Option<String>,
}

/// Focus change event data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FocusChangeEvent {
    /// Element that lost focus (if any).
    pub from: Option<String>,
    /// Element that gained focus (if any).
    pub to: Option<String>,
}

/// Hover event data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HoverEvent {
    /// Element ID that was entered/left.
    pub element_id: String,
    /// Mouse position at time of event.
    pub position: Point,
}

/// Window resize event data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowResizeEvent {
    /// New window width.
    pub width: u32,
    /// New window height.
    pub height: u32,
    /// New scale factor (if changed).
    pub scale_factor: Option<f64>,
}

/// Custom application event.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomEvent {
    /// Event name/type.
    pub name: String,
    /// Optional payload (JSON string).
    pub payload: Option<String>,
}

impl RecordedEvent {
    /// Get a descriptive name for the event type.
    pub fn event_type(&self) -> &'static str {
        match self {
            RecordedEvent::MouseDown(_) => "mouse_down",
            RecordedEvent::MouseUp(_) => "mouse_up",
            RecordedEvent::MouseMove(_) => "mouse_move",
            RecordedEvent::Click(_) => "click",
            RecordedEvent::DoubleClick(_) => "double_click",
            RecordedEvent::KeyDown(_) => "key_down",
            RecordedEvent::KeyUp(_) => "key_up",
            RecordedEvent::TextInput(_) => "text_input",
            RecordedEvent::Scroll(_) => "scroll",
            RecordedEvent::FocusChange(_) => "focus_change",
            RecordedEvent::HoverEnter(_) => "hover_enter",
            RecordedEvent::HoverLeave(_) => "hover_leave",
            RecordedEvent::WindowResize(_) => "window_resize",
            RecordedEvent::WindowFocus(_) => "window_focus",
            RecordedEvent::Custom(_) => "custom",
        }
    }

    /// Get the target element ID if applicable.
    pub fn target_element(&self) -> Option<&str> {
        match self {
            RecordedEvent::MouseDown(e) => e.target_element.as_deref(),
            RecordedEvent::MouseUp(e) => e.target_element.as_deref(),
            RecordedEvent::Click(e) => e.target_element.as_deref(),
            RecordedEvent::DoubleClick(e) => e.target_element.as_deref(),
            RecordedEvent::MouseMove(e) => e.hover_element.as_deref(),
            RecordedEvent::KeyDown(e) => e.focused_element.as_deref(),
            RecordedEvent::KeyUp(e) => e.focused_element.as_deref(),
            RecordedEvent::TextInput(e) => e.focused_element.as_deref(),
            RecordedEvent::Scroll(e) => e.target_element.as_deref(),
            RecordedEvent::HoverEnter(e) => Some(&e.element_id),
            RecordedEvent::HoverLeave(e) => Some(&e.element_id),
            RecordedEvent::FocusChange(e) => e.to.as_deref(),
            RecordedEvent::WindowResize(_) => None,
            RecordedEvent::WindowFocus(_) => None,
            RecordedEvent::Custom(_) => None,
        }
    }
}
