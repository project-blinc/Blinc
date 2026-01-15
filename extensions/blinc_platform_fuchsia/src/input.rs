//! Fuchsia input handling
//!
//! Converts fuchsia.ui.pointer and fuchsia.ui.input3 events to Blinc input events.
//!
//! # Input Sources
//!
//! Fuchsia provides input through several protocols:
//!
//! - **fuchsia.ui.pointer.TouchSource** - Touch input
//! - **fuchsia.ui.pointer.MouseSource** - Mouse input
//! - **fuchsia.ui.input3.Keyboard** - Keyboard input
//!
//! # Coordinate Systems
//!
//! Input coordinates are in the view's local coordinate space:
//! - Origin (0,0) is top-left
//! - Units are in logical pixels (DIP)

use blinc_platform::{
    InputEvent, Key, KeyState as PlatformKeyState, KeyboardEvent, Modifiers, MouseButton,
    MouseEvent, ScrollPhase, TouchEvent,
};

// ============================================================================
// Touch Input
// ============================================================================

/// Touch phase from fuchsia.ui.pointer
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    /// Touch began (ADD in Fuchsia)
    Began,
    /// Touch moved (CHANGE in Fuchsia)
    Moved,
    /// Touch ended (REMOVE in Fuchsia)
    Ended,
    /// Touch cancelled (CANCEL in Fuchsia)
    Cancelled,
}

/// A single touch point
#[derive(Clone, Debug)]
pub struct Touch {
    /// Unique identifier for this touch (pointer_id in Fuchsia)
    pub id: u64,
    /// X position in logical pixels
    pub x: f32,
    /// Y position in logical pixels
    pub y: f32,
    /// Touch phase
    pub phase: TouchPhase,
    /// Pressure (0.0 to 1.0, if available)
    pub pressure: f32,
}

impl Touch {
    /// Create a new touch event
    pub fn new(id: u64, x: f32, y: f32, phase: TouchPhase) -> Self {
        Self {
            id,
            x,
            y,
            phase,
            pressure: 1.0,
        }
    }

    /// Create with explicit pressure
    pub fn with_pressure(id: u64, x: f32, y: f32, phase: TouchPhase, pressure: f32) -> Self {
        Self {
            id,
            x,
            y,
            phase,
            pressure,
        }
    }
}

/// Convert a Fuchsia touch to a Blinc input event
pub fn convert_touch(touch: &Touch) -> InputEvent {
    match touch.phase {
        TouchPhase::Began => InputEvent::Touch(TouchEvent::Started {
            id: touch.id,
            x: touch.x,
            y: touch.y,
            pressure: touch.pressure,
        }),
        TouchPhase::Moved => InputEvent::Touch(TouchEvent::Moved {
            id: touch.id,
            x: touch.x,
            y: touch.y,
            pressure: touch.pressure,
        }),
        TouchPhase::Ended => InputEvent::Touch(TouchEvent::Ended {
            id: touch.id,
            x: touch.x,
            y: touch.y,
        }),
        TouchPhase::Cancelled => InputEvent::Touch(TouchEvent::Cancelled { id: touch.id }),
    }
}

/// Convert fuchsia.ui.pointer.TouchEvent to Blinc Touch
///
/// Note: This function would be implemented when building with Fuchsia SDK
#[cfg(target_os = "fuchsia")]
pub fn from_fuchsia_touch_event(
    _event: (), // fidl_fuchsia_ui_pointer::TouchEvent
    _scale_factor: f64,
) -> Vec<Touch> {
    // TODO: Implement conversion from fidl_fuchsia_ui_pointer::TouchEvent
    // - Extract pointer_id, position, phase
    // - Convert coordinates using scale_factor
    vec![]
}

// ============================================================================
// Mouse Input
// ============================================================================

/// Mouse button from fuchsia.ui.pointer
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuchsiaMouseButton {
    /// Primary (left) button
    Primary,
    /// Secondary (right) button
    Secondary,
    /// Tertiary (middle) button
    Tertiary,
}

impl FuchsiaMouseButton {
    /// Convert to Blinc MouseButton
    pub fn to_blinc(self) -> MouseButton {
        match self {
            Self::Primary => MouseButton::Left,
            Self::Secondary => MouseButton::Right,
            Self::Tertiary => MouseButton::Middle,
        }
    }
}

/// Mouse event phase
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MousePhase {
    /// Mouse entered the view
    Enter,
    /// Mouse moved within the view
    Move,
    /// Mouse button pressed
    Down,
    /// Mouse button released
    Up,
    /// Mouse left the view
    Leave,
    /// Scroll wheel
    Scroll,
}

/// Mouse event data
#[derive(Clone, Debug)]
pub struct Mouse {
    /// X position in logical pixels
    pub x: f32,
    /// Y position in logical pixels
    pub y: f32,
    /// Event phase
    pub phase: MousePhase,
    /// Button (for Down/Up events)
    pub button: Option<FuchsiaMouseButton>,
    /// Scroll delta (for Scroll events)
    pub scroll_delta: (f32, f32),
}

impl Mouse {
    /// Create a new mouse move event
    pub fn move_event(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            phase: MousePhase::Move,
            button: None,
            scroll_delta: (0.0, 0.0),
        }
    }

    /// Create a mouse button down event
    pub fn button_down(x: f32, y: f32, button: FuchsiaMouseButton) -> Self {
        Self {
            x,
            y,
            phase: MousePhase::Down,
            button: Some(button),
            scroll_delta: (0.0, 0.0),
        }
    }

    /// Create a mouse button up event
    pub fn button_up(x: f32, y: f32, button: FuchsiaMouseButton) -> Self {
        Self {
            x,
            y,
            phase: MousePhase::Up,
            button: Some(button),
            scroll_delta: (0.0, 0.0),
        }
    }

    /// Create a scroll event
    pub fn scroll(x: f32, y: f32, delta_x: f32, delta_y: f32) -> Self {
        Self {
            x,
            y,
            phase: MousePhase::Scroll,
            button: None,
            scroll_delta: (delta_x, delta_y),
        }
    }
}

/// Convert a Fuchsia mouse event to a Blinc input event
pub fn convert_mouse(mouse: &Mouse) -> Option<InputEvent> {
    match mouse.phase {
        MousePhase::Enter => Some(InputEvent::Mouse(MouseEvent::Entered)),
        MousePhase::Leave => Some(InputEvent::Mouse(MouseEvent::Left)),
        MousePhase::Move => Some(InputEvent::Mouse(MouseEvent::Moved {
            x: mouse.x,
            y: mouse.y,
        })),
        MousePhase::Down => mouse.button.map(|btn| {
            InputEvent::Mouse(MouseEvent::ButtonPressed {
                button: btn.to_blinc(),
                x: mouse.x,
                y: mouse.y,
            })
        }),
        MousePhase::Up => mouse.button.map(|btn| {
            InputEvent::Mouse(MouseEvent::ButtonReleased {
                button: btn.to_blinc(),
                x: mouse.x,
                y: mouse.y,
            })
        }),
        MousePhase::Scroll => Some(InputEvent::Scroll {
            delta_x: mouse.scroll_delta.0,
            delta_y: mouse.scroll_delta.1,
            phase: ScrollPhase::Moved,
        }),
    }
}

// ============================================================================
// Keyboard Input
// ============================================================================

/// Key state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyState {
    /// Key was pressed
    Pressed,
    /// Key was released
    Released,
}

/// Keyboard event data
#[derive(Clone, Debug)]
pub struct KeyEvent {
    /// Key code (fuchsia.ui.input3.Key)
    pub key: u32,
    /// Key state
    pub state: KeyState,
    /// Modifiers active during this event
    pub modifiers: KeyModifiers,
    /// Character produced by this key (if any)
    pub character: Option<char>,
}

impl KeyEvent {
    /// Create a key press event
    pub fn pressed(key: u32, modifiers: KeyModifiers, character: Option<char>) -> Self {
        Self {
            key,
            state: KeyState::Pressed,
            modifiers,
            character,
        }
    }

    /// Create a key release event
    pub fn released(key: u32, modifiers: KeyModifiers) -> Self {
        Self {
            key,
            state: KeyState::Released,
            modifiers,
            character: None,
        }
    }
}

/// Keyboard modifiers
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    /// Shift key is pressed
    pub shift: bool,
    /// Control key is pressed
    pub ctrl: bool,
    /// Alt/Option key is pressed
    pub alt: bool,
    /// Meta/Super/Cmd key is pressed
    pub meta: bool,
    /// Caps lock is active
    pub caps_lock: bool,
    /// Num lock is active
    pub num_lock: bool,
}

impl KeyModifiers {
    /// Create empty modifiers
    pub fn none() -> Self {
        Self::default()
    }

    /// Create with shift
    pub fn with_shift() -> Self {
        Self {
            shift: true,
            ..Default::default()
        }
    }

    /// Create with ctrl
    pub fn with_ctrl() -> Self {
        Self {
            ctrl: true,
            ..Default::default()
        }
    }

    /// Check if any modifier is active
    pub fn any(&self) -> bool {
        self.shift || self.ctrl || self.alt || self.meta
    }
}

/// Convert a Fuchsia key event to a Blinc input event
pub fn convert_key(key_event: &KeyEvent) -> InputEvent {
    let platform_state = match key_event.state {
        KeyState::Pressed => PlatformKeyState::Pressed,
        KeyState::Released => PlatformKeyState::Released,
    };

    InputEvent::Keyboard(KeyboardEvent {
        key: fuchsia_key_to_blinc(key_event.key),
        state: platform_state,
        modifiers: convert_modifiers(&key_event.modifiers),
    })
}

fn convert_modifiers(mods: &KeyModifiers) -> Modifiers {
    Modifiers {
        shift: mods.shift,
        ctrl: mods.ctrl,
        alt: mods.alt,
        meta: mods.meta,
    }
}

/// Convert Fuchsia key code to Blinc Key
fn fuchsia_key_to_blinc(key: u32) -> Key {
    use key_codes::*;

    match key {
        // Letters
        KEY_A => Key::A,
        KEY_B => Key::B,
        KEY_C => Key::C,
        KEY_D => Key::D,
        KEY_E => Key::E,
        KEY_F => Key::F,
        KEY_G => Key::G,
        KEY_H => Key::H,
        KEY_I => Key::I,
        KEY_J => Key::J,
        KEY_K => Key::K,
        KEY_L => Key::L,
        KEY_M => Key::M,
        KEY_N => Key::N,
        KEY_O => Key::O,
        KEY_P => Key::P,
        KEY_Q => Key::Q,
        KEY_R => Key::R,
        KEY_S => Key::S,
        KEY_T => Key::T,
        KEY_U => Key::U,
        KEY_V => Key::V,
        KEY_W => Key::W,
        KEY_X => Key::X,
        KEY_Y => Key::Y,
        KEY_Z => Key::Z,
        // Numbers
        KEY_0 => Key::Num0,
        KEY_1 => Key::Num1,
        KEY_2 => Key::Num2,
        KEY_3 => Key::Num3,
        KEY_4 => Key::Num4,
        KEY_5 => Key::Num5,
        KEY_6 => Key::Num6,
        KEY_7 => Key::Num7,
        KEY_8 => Key::Num8,
        KEY_9 => Key::Num9,
        // Special
        KEY_ENTER => Key::Enter,
        KEY_ESCAPE => Key::Escape,
        KEY_BACKSPACE => Key::Backspace,
        KEY_TAB => Key::Tab,
        KEY_SPACE => Key::Space,
        // Arrows
        KEY_LEFT => Key::Left,
        KEY_RIGHT => Key::Right,
        KEY_UP => Key::Up,
        KEY_DOWN => Key::Down,
        // Function keys
        KEY_F1 => Key::F1,
        KEY_F2 => Key::F2,
        KEY_F3 => Key::F3,
        KEY_F4 => Key::F4,
        KEY_F5 => Key::F5,
        KEY_F6 => Key::F6,
        KEY_F7 => Key::F7,
        KEY_F8 => Key::F8,
        KEY_F9 => Key::F9,
        KEY_F10 => Key::F10,
        KEY_F11 => Key::F11,
        KEY_F12 => Key::F12,
        // Modifiers
        KEY_LEFT_SHIFT | KEY_RIGHT_SHIFT => Key::Shift,
        KEY_LEFT_CTRL | KEY_RIGHT_CTRL => Key::Ctrl,
        KEY_LEFT_ALT | KEY_RIGHT_ALT => Key::Alt,
        KEY_LEFT_META | KEY_RIGHT_META => Key::Meta,
        // Unknown
        _ => Key::Unknown,
    }
}

// ============================================================================
// Common Key Codes (matching fuchsia.ui.input3.Key)
// ============================================================================

/// Common Fuchsia key codes
pub mod key_codes {
    // Letters
    pub const KEY_A: u32 = 0x00070004;
    pub const KEY_B: u32 = 0x00070005;
    pub const KEY_C: u32 = 0x00070006;
    pub const KEY_D: u32 = 0x00070007;
    pub const KEY_E: u32 = 0x00070008;
    pub const KEY_F: u32 = 0x00070009;
    pub const KEY_G: u32 = 0x0007000A;
    pub const KEY_H: u32 = 0x0007000B;
    pub const KEY_I: u32 = 0x0007000C;
    pub const KEY_J: u32 = 0x0007000D;
    pub const KEY_K: u32 = 0x0007000E;
    pub const KEY_L: u32 = 0x0007000F;
    pub const KEY_M: u32 = 0x00070010;
    pub const KEY_N: u32 = 0x00070011;
    pub const KEY_O: u32 = 0x00070012;
    pub const KEY_P: u32 = 0x00070013;
    pub const KEY_Q: u32 = 0x00070014;
    pub const KEY_R: u32 = 0x00070015;
    pub const KEY_S: u32 = 0x00070016;
    pub const KEY_T: u32 = 0x00070017;
    pub const KEY_U: u32 = 0x00070018;
    pub const KEY_V: u32 = 0x00070019;
    pub const KEY_W: u32 = 0x0007001A;
    pub const KEY_X: u32 = 0x0007001B;
    pub const KEY_Y: u32 = 0x0007001C;
    pub const KEY_Z: u32 = 0x0007001D;

    // Numbers
    pub const KEY_1: u32 = 0x0007001E;
    pub const KEY_2: u32 = 0x0007001F;
    pub const KEY_3: u32 = 0x00070020;
    pub const KEY_4: u32 = 0x00070021;
    pub const KEY_5: u32 = 0x00070022;
    pub const KEY_6: u32 = 0x00070023;
    pub const KEY_7: u32 = 0x00070024;
    pub const KEY_8: u32 = 0x00070025;
    pub const KEY_9: u32 = 0x00070026;
    pub const KEY_0: u32 = 0x00070027;

    // Special keys
    pub const KEY_ENTER: u32 = 0x00070028;
    pub const KEY_ESCAPE: u32 = 0x00070029;
    pub const KEY_BACKSPACE: u32 = 0x0007002A;
    pub const KEY_TAB: u32 = 0x0007002B;
    pub const KEY_SPACE: u32 = 0x0007002C;

    // Arrow keys
    pub const KEY_LEFT: u32 = 0x00070050;
    pub const KEY_RIGHT: u32 = 0x0007004F;
    pub const KEY_UP: u32 = 0x00070052;
    pub const KEY_DOWN: u32 = 0x00070051;

    // Function keys
    pub const KEY_F1: u32 = 0x0007003A;
    pub const KEY_F2: u32 = 0x0007003B;
    pub const KEY_F3: u32 = 0x0007003C;
    pub const KEY_F4: u32 = 0x0007003D;
    pub const KEY_F5: u32 = 0x0007003E;
    pub const KEY_F6: u32 = 0x0007003F;
    pub const KEY_F7: u32 = 0x00070040;
    pub const KEY_F8: u32 = 0x00070041;
    pub const KEY_F9: u32 = 0x00070042;
    pub const KEY_F10: u32 = 0x00070043;
    pub const KEY_F11: u32 = 0x00070044;
    pub const KEY_F12: u32 = 0x00070045;

    // Modifiers
    pub const KEY_LEFT_CTRL: u32 = 0x000700E0;
    pub const KEY_LEFT_SHIFT: u32 = 0x000700E1;
    pub const KEY_LEFT_ALT: u32 = 0x000700E2;
    pub const KEY_LEFT_META: u32 = 0x000700E3;
    pub const KEY_RIGHT_CTRL: u32 = 0x000700E4;
    pub const KEY_RIGHT_SHIFT: u32 = 0x000700E5;
    pub const KEY_RIGHT_ALT: u32 = 0x000700E6;
    pub const KEY_RIGHT_META: u32 = 0x000700E7;
}
