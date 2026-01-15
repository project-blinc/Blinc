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

#[cfg(target_os = "fuchsia")]
use fidl_fuchsia_ui_pointer::{
    EventPhase, MouseEvent as FidlMouseEvent, TouchEvent as FidlTouchEvent,
};
#[cfg(target_os = "fuchsia")]
use fidl_fuchsia_ui_input3::{KeyEvent as FidlKeyEvent, KeyEventType, Modifiers as FidlModifiers};

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
#[cfg(target_os = "fuchsia")]
pub fn from_fuchsia_touch_event(
    event: &FidlTouchEvent,
    scale_factor: f64,
) -> Option<Touch> {
    let pointer_sample = event.pointer_sample.as_ref()?;

    // Get phase
    let phase = match pointer_sample.phase? {
        EventPhase::Add => TouchPhase::Began,
        EventPhase::Change => TouchPhase::Moved,
        EventPhase::Remove => TouchPhase::Ended,
        EventPhase::Cancel => TouchPhase::Cancelled,
    };

    // Get position in viewport coordinates
    let position = pointer_sample.position_in_viewport.as_ref()?;
    let x = position[0] as f32 / scale_factor as f32;
    let y = position[1] as f32 / scale_factor as f32;

    // Get pointer ID from interaction ID
    let pointer_id = event.interaction_id.as_ref()
        .map(|id| id.pointer_id as u64)
        .unwrap_or(0);

    Some(Touch::new(pointer_id, x, y, phase))
}

/// Convert fuchsia.ui.pointer.MouseEvent to MouseInteraction
#[cfg(target_os = "fuchsia")]
pub fn from_fuchsia_mouse_event(
    event: &FidlMouseEvent,
    scale_factor: f64,
) -> Option<MouseInteraction> {
    let pointer_sample = event.pointer_sample.as_ref()?;

    // Get position
    let position = pointer_sample.position_in_viewport.as_ref()?;
    let x = position[0] as f32 / scale_factor as f32;
    let y = position[1] as f32 / scale_factor as f32;

    // Get device ID
    let device_id = event.device_info.as_ref()
        .and_then(|info| info.id)
        .unwrap_or(0);

    // Get button states
    let pressed_buttons = pointer_sample.pressed_buttons.as_ref()
        .map(|btns| btns.iter().fold(0u32, |acc, &b| acc | (1 << b)))
        .unwrap_or(0);

    // TODO: Track button state changes for newly_pressed/newly_released

    // Get scroll
    let scroll_v = pointer_sample.scroll_v.map(|v| (0.0, v as f64));

    Some(MouseInteraction {
        device_id,
        position: (x, y),
        pressed_buttons,
        newly_pressed: 0,
        newly_released: 0,
        scroll: None,
        scroll_v,
        timestamp_ns: event.timestamp.unwrap_or(0),
    })
}

/// Convert fuchsia.ui.input3.KeyEvent to KeyEvent
#[cfg(target_os = "fuchsia")]
pub fn from_fuchsia_key_event(event: &FidlKeyEvent) -> Option<KeyEvent> {
    let key = event.key.map(|k| k as u32)?;

    let state = match event.type_? {
        KeyEventType::Pressed => KeyState::Pressed,
        KeyEventType::Released => KeyState::Released,
        KeyEventType::Sync => KeyState::Pressed, // Sync = key was pressed before focus
        KeyEventType::Cancel => KeyState::Released, // Cancel = release due to focus loss
    };

    // Convert modifiers
    let mods = event.modifiers.map(convert_fuchsia_modifiers).unwrap_or_default();

    // Get character from key_meaning
    let character = event.key_meaning.as_ref().and_then(|meaning| {
        match meaning {
            fidl_fuchsia_ui_input3::KeyMeaning::Codepoint(cp) => char::from_u32(*cp),
            _ => None,
        }
    });

    Some(match state {
        KeyState::Pressed => KeyEvent::pressed(key, mods, character),
        KeyState::Released => KeyEvent::released(key, mods),
    })
}

/// Convert Fuchsia modifiers to our KeyModifiers
#[cfg(target_os = "fuchsia")]
fn convert_fuchsia_modifiers(mods: FidlModifiers) -> KeyModifiers {
    KeyModifiers {
        shift: mods.contains(FidlModifiers::SHIFT),
        ctrl: mods.contains(FidlModifiers::CTRL),
        alt: mods.contains(FidlModifiers::ALT),
        meta: mods.contains(FidlModifiers::META),
        caps_lock: mods.contains(FidlModifiers::CAPS_LOCK),
        num_lock: mods.contains(FidlModifiers::NUM_LOCK),
    }
}

// ============================================================================
// Mouse Input
// ============================================================================

/// Mouse button from fuchsia.ui.pointer
///
/// Wraps the button ID from Fuchsia (0 = primary, 1 = secondary, 2 = tertiary)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuchsiaMouseButton(pub u8);

impl FuchsiaMouseButton {
    /// Primary (left) button
    pub const PRIMARY: Self = Self(0);
    /// Secondary (right) button
    pub const SECONDARY: Self = Self(1);
    /// Tertiary (middle) button
    pub const TERTIARY: Self = Self(2);

    /// Convert to Blinc MouseButton
    pub fn to_blinc(self) -> MouseButton {
        match self.0 {
            0 => MouseButton::Left,
            1 => MouseButton::Right,
            2 => MouseButton::Middle,
            _ => MouseButton::Other(self.0 as u16),
        }
    }

    /// Check if this is the primary button
    pub fn is_primary(&self) -> bool {
        self.0 == 0
    }

    /// Check if this is the secondary button
    pub fn is_secondary(&self) -> bool {
        self.0 == 1
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

    /// Get the button if present
    pub fn get_button(&self) -> Option<MouseButton> {
        self.button.map(|b| b.to_blinc())
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

// ============================================================================
// Pointer Protocol Types (fuchsia.ui.pointer)
// ============================================================================

/// Pointer interaction ID (unique per touch/mouse interaction)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InteractionId {
    /// Device ID
    pub device_id: u32,
    /// Pointer ID within device
    pub pointer_id: u32,
    /// Interaction ID (sequence number)
    pub interaction_id: u32,
}

impl InteractionId {
    /// Create a new interaction ID
    pub fn new(device_id: u32, pointer_id: u32, interaction_id: u32) -> Self {
        Self {
            device_id,
            pointer_id,
            interaction_id,
        }
    }
}

/// Pointer event status (for responding to TouchSource.Watch)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchResponseType {
    /// App wants to receive events for this interaction
    Yes,
    /// App doesn't want this interaction
    No,
    /// App will decide based on gesture
    Maybe,
    /// App is holding decision (for gesture disambiguation)
    Hold,
}

/// Touch interaction from fuchsia.ui.pointer.TouchSource
#[derive(Clone, Debug)]
pub struct TouchInteraction {
    /// Interaction ID
    pub id: InteractionId,
    /// Touch phase
    pub phase: TouchPhase,
    /// X position in view-local logical pixels
    pub x: f32,
    /// Y position in view-local logical pixels
    pub y: f32,
    /// Force/pressure (if available)
    pub force: Option<f32>,
}

impl Default for TouchInteraction {
    fn default() -> Self {
        Self {
            id: InteractionId::default(),
            phase: TouchPhase::Cancelled,
            x: 0.0,
            y: 0.0,
            force: None,
        }
    }
}

impl TouchInteraction {
    /// Create a new touch interaction
    pub fn new(id: InteractionId, phase: TouchPhase, x: f32, y: f32) -> Self {
        Self { id, phase, x, y, force: None }
    }

    /// Get position as tuple
    pub fn position(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    /// Convert to Blinc Touch
    pub fn to_touch(&self) -> Touch {
        Touch::new(
            self.id.pointer_id as u64,
            self.x,
            self.y,
            self.phase,
        )
    }
}

/// Mouse interaction from fuchsia.ui.pointer.MouseSource
#[derive(Clone, Debug)]
pub struct MouseInteraction {
    /// Mouse phase
    pub phase: MousePhase,
    /// X position in view-local logical pixels
    pub x: f32,
    /// Y position in view-local logical pixels
    pub y: f32,
    /// Currently pressed buttons
    pub buttons: Vec<FuchsiaMouseButton>,
    /// Scroll delta (dx, dy)
    pub scroll_delta: Option<(f32, f32)>,
}

impl Default for MouseInteraction {
    fn default() -> Self {
        Self {
            phase: MousePhase::Move,
            x: 0.0,
            y: 0.0,
            buttons: vec![],
            scroll_delta: None,
        }
    }
}

impl MouseInteraction {
    /// Create a new mouse interaction
    pub fn new(phase: MousePhase, x: f32, y: f32) -> Self {
        Self { phase, x, y, buttons: vec![], scroll_delta: None }
    }

    /// Get position as tuple
    pub fn position(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    /// Check if primary button is pressed
    pub fn is_primary_pressed(&self) -> bool {
        self.buttons.iter().any(|b| b.is_primary())
    }

    /// Check if secondary button is pressed
    pub fn is_secondary_pressed(&self) -> bool {
        self.buttons.iter().any(|b| b.is_secondary())
    }

    /// Convert to Blinc Mouse event
    pub fn to_mouse(&self) -> Mouse {
        let button = self.buttons.first().copied();

        match self.phase {
            MousePhase::Enter => Mouse {
                x: self.x,
                y: self.y,
                phase: MousePhase::Enter,
                button: None,
                scroll_delta: (0.0, 0.0),
            },
            MousePhase::Leave => Mouse {
                x: self.x,
                y: self.y,
                phase: MousePhase::Leave,
                button: None,
                scroll_delta: (0.0, 0.0),
            },
            MousePhase::Move => Mouse::move_event(self.x, self.y),
            MousePhase::Down => Mouse {
                x: self.x,
                y: self.y,
                phase: MousePhase::Down,
                button,
                scroll_delta: (0.0, 0.0),
            },
            MousePhase::Up => Mouse {
                x: self.x,
                y: self.y,
                phase: MousePhase::Up,
                button,
                scroll_delta: (0.0, 0.0),
            },
            MousePhase::Scroll => Mouse {
                x: self.x,
                y: self.y,
                phase: MousePhase::Scroll,
                button: None,
                scroll_delta: self.scroll_delta.unwrap_or((0.0, 0.0)),
            },
        }
    }
}

/// Keyboard listener for fuchsia.ui.input3.Keyboard
#[derive(Clone, Debug)]
pub struct KeyboardListenerRequest {
    /// Key that was pressed/released
    pub key: u32,
    /// Key event type
    pub event_type: KeyState,
    /// Current modifiers
    pub modifiers: KeyModifiers,
    /// Semantic meaning (character produced)
    pub meaning: Option<KeyMeaning>,
    /// Timestamp in nanoseconds
    pub timestamp_ns: i64,
}

/// Semantic key meaning from fuchsia.ui.input3
#[derive(Clone, Debug)]
pub enum KeyMeaning {
    /// Character produced
    Codepoint(u32),
    /// Non-printing key meaning
    NonPrintable(NonPrintableKey),
}

/// Non-printable key meanings
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonPrintableKey {
    Enter,
    Tab,
    Backspace,
    Delete,
    Escape,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,
}

impl KeyboardListenerRequest {
    /// Convert to Blinc KeyEvent
    pub fn to_key_event(&self) -> KeyEvent {
        let character = match &self.meaning {
            Some(KeyMeaning::Codepoint(cp)) => char::from_u32(*cp),
            _ => None,
        };

        match self.event_type {
            KeyState::Pressed => KeyEvent::pressed(self.key, self.modifiers, character),
            KeyState::Released => KeyEvent::released(self.key, self.modifiers),
        }
    }
}

// ============================================================================
// Pointer Source Management
// ============================================================================

/// Touch source state manager
///
/// Manages the fuchsia.ui.pointer.TouchSource protocol state.
/// Handles the hanging-get Watch pattern and response tracking.
pub struct TouchSourceState {
    /// Active interactions (pending response)
    active_interactions: std::collections::HashMap<InteractionId, TouchResponseType>,
    /// Whether we've connected to the touch source
    connected: bool,
}

impl TouchSourceState {
    /// Create new touch source state
    pub fn new() -> Self {
        Self {
            active_interactions: std::collections::HashMap::new(),
            connected: false,
        }
    }

    /// Mark as connected
    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Process a touch event and return the response
    ///
    /// For gesture disambiguation, we may need to hold our response
    /// until we know if this is a scroll vs tap.
    pub fn process_touch(&mut self, interaction: &TouchInteraction) -> TouchResponseType {
        match interaction.phase {
            TouchPhase::Began => {
                // New interaction - accept by default
                self.active_interactions
                    .insert(interaction.id, TouchResponseType::Yes);
                TouchResponseType::Yes
            }
            TouchPhase::Moved | TouchPhase::Ended | TouchPhase::Cancelled => {
                // Continue with existing decision
                self.active_interactions
                    .get(&interaction.id)
                    .copied()
                    .unwrap_or(TouchResponseType::Yes)
            }
        }
    }

    /// Set response for an interaction (for gesture disambiguation)
    pub fn set_response(&mut self, id: InteractionId, response: TouchResponseType) {
        self.active_interactions.insert(id, response);
    }

    /// Clear completed interaction
    pub fn clear_interaction(&mut self, id: &InteractionId) {
        self.active_interactions.remove(id);
    }

    /// Get all pending responses for Watch call
    pub fn pending_responses(&self) -> Vec<(InteractionId, TouchResponseType)> {
        self.active_interactions
            .iter()
            .map(|(id, resp)| (*id, *resp))
            .collect()
    }
}

impl Default for TouchSourceState {
    fn default() -> Self {
        Self::new()
    }
}

/// Mouse source state manager
///
/// Manages the fuchsia.ui.pointer.MouseSource protocol state.
pub struct MouseSourceState {
    /// Last known position
    last_position: Option<(f32, f32)>,
    /// Currently pressed buttons (bitfield)
    pressed_buttons: u32,
    /// Whether we've connected to the mouse source
    connected: bool,
}

impl MouseSourceState {
    /// Create new mouse source state
    pub fn new() -> Self {
        Self {
            last_position: None,
            pressed_buttons: 0,
            connected: false,
        }
    }

    /// Mark as connected
    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Update state from mouse interaction
    pub fn update(&mut self, interaction: &MouseInteraction) {
        self.last_position = Some(interaction.position());
        // Convert buttons to bitfield
        self.pressed_buttons = interaction.buttons.iter().fold(0u32, |acc, b| acc | (1 << b.0));
    }

    /// Get last known position
    pub fn position(&self) -> Option<(f32, f32)> {
        self.last_position
    }

    /// Check if any button is pressed
    pub fn any_button_pressed(&self) -> bool {
        self.pressed_buttons != 0
    }

    /// Check if primary button is pressed
    pub fn is_primary_pressed(&self) -> bool {
        self.pressed_buttons & 0x01 != 0
    }
}

impl Default for MouseSourceState {
    fn default() -> Self {
        Self::new()
    }
}

/// Combined pointer state manager
///
/// Manages both touch and mouse input sources.
pub struct PointerState {
    /// Touch source state
    pub touch: TouchSourceState,
    /// Mouse source state
    pub mouse: MouseSourceState,
}

impl PointerState {
    /// Create new pointer state
    pub fn new() -> Self {
        Self {
            touch: TouchSourceState::new(),
            mouse: MouseSourceState::new(),
        }
    }

    /// Check if any pointer source is connected
    pub fn any_connected(&self) -> bool {
        self.touch.is_connected() || self.mouse.is_connected()
    }
}

impl Default for PointerState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Focus State Management
// ============================================================================

/// Focus state manager for fuchsia.ui.views.ViewRefFocused
///
/// Tracks focus changes via the hanging-get Watch pattern.
pub struct FocusWatcher {
    /// Current focus state
    focused: bool,
    /// Whether we've received initial focus state
    has_state: bool,
}

impl FocusWatcher {
    /// Create new focus watcher
    pub fn new() -> Self {
        Self {
            focused: false,
            has_state: false,
        }
    }

    /// Update focus state
    ///
    /// Returns true if focus changed.
    pub fn update(&mut self, focused: bool) -> bool {
        let changed = self.focused != focused;
        self.focused = focused;
        self.has_state = true;
        changed
    }

    /// Check if view is focused
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Check if we've received initial state
    pub fn has_state(&self) -> bool {
        self.has_state
    }
}

impl Default for FocusWatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Keyboard Listener State
// ============================================================================

/// Keyboard listener state for fuchsia.ui.input3.Keyboard
///
/// Manages keyboard input via SetListener protocol.
pub struct KeyboardListenerState {
    /// Currently held keys (for key repeat detection)
    held_keys: std::collections::HashSet<u32>,
    /// Current modifier state
    modifiers: KeyModifiers,
    /// Whether listener is registered
    registered: bool,
}

impl KeyboardListenerState {
    /// Create new keyboard listener state
    pub fn new() -> Self {
        Self {
            held_keys: std::collections::HashSet::new(),
            modifiers: KeyModifiers::default(),
            registered: false,
        }
    }

    /// Mark as registered
    pub fn set_registered(&mut self, registered: bool) {
        self.registered = registered;
    }

    /// Check if registered
    pub fn is_registered(&self) -> bool {
        self.registered
    }

    /// Process a key event
    ///
    /// Returns true if this is a new key press (not repeat).
    pub fn process_key(&mut self, request: &KeyboardListenerRequest) -> bool {
        // Update modifiers
        self.modifiers = request.modifiers;

        match request.event_type {
            KeyState::Pressed => {
                // Check if this is a new press or repeat
                let is_new = !self.held_keys.contains(&request.key);
                self.held_keys.insert(request.key);
                is_new
            }
            KeyState::Released => {
                self.held_keys.remove(&request.key);
                true // Releases are always processed
            }
        }
    }

    /// Check if a key is currently held
    pub fn is_key_held(&self, key: u32) -> bool {
        self.held_keys.contains(&key)
    }

    /// Get current modifiers
    pub fn modifiers(&self) -> KeyModifiers {
        self.modifiers
    }

    /// Clear all held keys (e.g., on focus loss)
    pub fn clear_held_keys(&mut self) {
        self.held_keys.clear();
    }
}

impl Default for KeyboardListenerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Combined input state for all Fuchsia input sources
pub struct InputState {
    /// Pointer (touch/mouse) state
    pub pointer: PointerState,
    /// Focus state
    pub focus: FocusWatcher,
    /// Keyboard state
    pub keyboard: KeyboardListenerState,
}

impl InputState {
    /// Create new input state
    pub fn new() -> Self {
        Self {
            pointer: PointerState::new(),
            focus: FocusWatcher::new(),
            keyboard: KeyboardListenerState::new(),
        }
    }

    /// Handle focus loss - clears keyboard held keys
    pub fn on_focus_lost(&mut self) {
        self.focus.update(false);
        self.keyboard.clear_held_keys();
    }

    /// Handle focus gain
    pub fn on_focus_gained(&mut self) {
        self.focus.update(true);
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}
