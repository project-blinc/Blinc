//! Input event types for keyboard, mouse, and touch

/// Scroll gesture phase (for trackpad/touchpad scrolling)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollPhase {
    /// Scroll gesture starting (finger touched trackpad)
    Started,
    /// Scroll is in progress
    #[default]
    Moved,
    /// Scroll gesture ended (finger lifted, momentum may continue)
    Ended,
    /// Momentum/inertia scrolling has ended
    MomentumEnded,
}

/// Input events
#[derive(Clone, Debug)]
pub enum InputEvent {
    /// Mouse event
    Mouse(MouseEvent),
    /// Keyboard event
    Keyboard(KeyboardEvent),
    /// Touch event (mobile/touchscreen)
    Touch(TouchEvent),
    /// Scroll/wheel event
    Scroll {
        /// Horizontal scroll delta
        delta_x: f32,
        /// Vertical scroll delta
        delta_y: f32,
        /// Scroll phase (for trackpad gestures)
        phase: ScrollPhase,
    },
    /// Scroll gesture ended (touchpad momentum finished)
    ScrollEnd,
}

// ============================================================================
// Mouse Events
// ============================================================================

/// Mouse events
#[derive(Clone, Debug)]
pub enum MouseEvent {
    /// Mouse moved to position
    Moved {
        /// X position in window coordinates
        x: f32,
        /// Y position in window coordinates
        y: f32,
    },
    /// Mouse button pressed
    ButtonPressed {
        /// Which button was pressed
        button: MouseButton,
        /// X position when pressed
        x: f32,
        /// Y position when pressed
        y: f32,
    },
    /// Mouse button released
    ButtonReleased {
        /// Which button was released
        button: MouseButton,
        /// X position when released
        x: f32,
        /// Y position when released
        y: f32,
    },
    /// Mouse entered the window
    Entered,
    /// Mouse left the window
    Left,
}

/// Mouse buttons
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Left mouse button
    Left,
    /// Right mouse button
    Right,
    /// Middle mouse button (scroll wheel click)
    Middle,
    /// Back button (side button)
    Back,
    /// Forward button (side button)
    Forward,
    /// Other button with index
    Other(u16),
}

// ============================================================================
// Keyboard Events
// ============================================================================

/// Keyboard event
#[derive(Clone, Debug)]
pub struct KeyboardEvent {
    /// The key that was pressed or released
    pub key: Key,
    /// Whether the key was pressed or released
    pub state: KeyState,
    /// Modifier keys held during this event
    pub modifiers: Modifiers,
}

/// Key press/release state
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyState {
    /// Key was pressed
    Pressed,
    /// Key was released
    Released,
}

/// Modifier key state
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Shift key is held
    pub shift: bool,
    /// Control key is held
    pub ctrl: bool,
    /// Alt key is held (Option on macOS)
    pub alt: bool,
    /// Meta key is held (Command on macOS, Windows key on Windows)
    pub meta: bool,
}

impl Modifiers {
    /// Check if no modifiers are held
    pub fn is_empty(&self) -> bool {
        !self.shift && !self.ctrl && !self.alt && !self.meta
    }

    /// Check if only shift is held
    pub fn shift_only(&self) -> bool {
        self.shift && !self.ctrl && !self.alt && !self.meta
    }

    /// Check if only ctrl is held
    pub fn ctrl_only(&self) -> bool {
        !self.shift && self.ctrl && !self.alt && !self.meta
    }

    /// Check if only alt is held
    pub fn alt_only(&self) -> bool {
        !self.shift && !self.ctrl && self.alt && !self.meta
    }

    /// Check if only meta is held
    pub fn meta_only(&self) -> bool {
        !self.shift && !self.ctrl && !self.alt && self.meta
    }
}

/// Key codes
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

    // Special keys
    Space,
    Enter,
    Escape,
    Backspace,
    Tab,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,

    // Arrow keys
    Left,
    Right,
    Up,
    Down,

    // Modifier keys (for tracking state)
    Shift,
    Ctrl,
    Alt,
    Meta,

    // Punctuation and symbols
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,
    Grave,

    // Character input (for text input)
    Char(char),

    // Unknown key
    Unknown,
}

// ============================================================================
// Touch Events
// ============================================================================

/// Touch events for touchscreens
#[derive(Clone, Debug)]
pub enum TouchEvent {
    /// A touch started
    Started {
        /// Unique identifier for this touch
        id: u64,
        /// X position in window coordinates
        x: f32,
        /// Y position in window coordinates
        y: f32,
        /// Touch pressure (0.0 - 1.0)
        pressure: f32,
    },
    /// A touch moved
    Moved {
        /// Unique identifier for this touch
        id: u64,
        /// X position in window coordinates
        x: f32,
        /// Y position in window coordinates
        y: f32,
        /// Touch pressure (0.0 - 1.0)
        pressure: f32,
    },
    /// A touch ended
    Ended {
        /// Unique identifier for this touch
        id: u64,
        /// X position when ended
        x: f32,
        /// Y position when ended
        y: f32,
    },
    /// A touch was cancelled (e.g., by system gesture)
    Cancelled {
        /// Unique identifier for this touch
        id: u64,
    },
}

impl TouchEvent {
    /// Get the touch ID
    pub fn id(&self) -> u64 {
        match self {
            TouchEvent::Started { id, .. } => *id,
            TouchEvent::Moved { id, .. } => *id,
            TouchEvent::Ended { id, .. } => *id,
            TouchEvent::Cancelled { id } => *id,
        }
    }

    /// Get the position (returns None for Cancelled)
    pub fn position(&self) -> Option<(f32, f32)> {
        match self {
            TouchEvent::Started { x, y, .. } => Some((*x, *y)),
            TouchEvent::Moved { x, y, .. } => Some((*x, *y)),
            TouchEvent::Ended { x, y, .. } => Some((*x, *y)),
            TouchEvent::Cancelled { .. } => None,
        }
    }
}
