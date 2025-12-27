//! Desktop input conversion (winit -> blinc_platform)

use blinc_platform::{
    InputEvent, Key, KeyState, KeyboardEvent, Modifiers, MouseButton, MouseEvent, ScrollPhase,
    TouchEvent,
};
use winit::event::{ElementState, MouseButton as WinitMouseButton, Touch, TouchPhase};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};

/// Convert winit mouse button to blinc MouseButton
pub fn convert_mouse_button(button: WinitMouseButton) -> MouseButton {
    match button {
        WinitMouseButton::Left => MouseButton::Left,
        WinitMouseButton::Right => MouseButton::Right,
        WinitMouseButton::Middle => MouseButton::Middle,
        WinitMouseButton::Back => MouseButton::Back,
        WinitMouseButton::Forward => MouseButton::Forward,
        WinitMouseButton::Other(n) => MouseButton::Other(n),
    }
}

/// Convert winit element state to blinc KeyState
pub fn convert_key_state(state: ElementState) -> KeyState {
    match state {
        ElementState::Pressed => KeyState::Pressed,
        ElementState::Released => KeyState::Released,
    }
}

/// Convert winit modifiers to blinc Modifiers
pub fn convert_modifiers(modifiers: ModifiersState) -> Modifiers {
    Modifiers {
        shift: modifiers.shift_key(),
        ctrl: modifiers.control_key(),
        alt: modifiers.alt_key(),
        meta: modifiers.super_key(),
    }
}

/// Convert winit key to blinc Key
pub fn convert_key(key: &WinitKey) -> Key {
    match key {
        WinitKey::Named(named) => match named {
            // Special keys
            NamedKey::Space => Key::Space,
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Escape,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Tab => Key::Tab,
            NamedKey::Delete => Key::Delete,
            NamedKey::Insert => Key::Insert,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,

            // Arrow keys
            NamedKey::ArrowLeft => Key::Left,
            NamedKey::ArrowRight => Key::Right,
            NamedKey::ArrowUp => Key::Up,
            NamedKey::ArrowDown => Key::Down,

            // Modifier keys
            NamedKey::Shift => Key::Shift,
            NamedKey::Control => Key::Ctrl,
            NamedKey::Alt => Key::Alt,
            NamedKey::Super => Key::Meta,

            // Function keys
            NamedKey::F1 => Key::F1,
            NamedKey::F2 => Key::F2,
            NamedKey::F3 => Key::F3,
            NamedKey::F4 => Key::F4,
            NamedKey::F5 => Key::F5,
            NamedKey::F6 => Key::F6,
            NamedKey::F7 => Key::F7,
            NamedKey::F8 => Key::F8,
            NamedKey::F9 => Key::F9,
            NamedKey::F10 => Key::F10,
            NamedKey::F11 => Key::F11,
            NamedKey::F12 => Key::F12,

            _ => Key::Unknown,
        },
        WinitKey::Character(c) => {
            let ch = c.chars().next().unwrap_or('\0');
            match ch.to_ascii_uppercase() {
                'A' => Key::A,
                'B' => Key::B,
                'C' => Key::C,
                'D' => Key::D,
                'E' => Key::E,
                'F' => Key::F,
                'G' => Key::G,
                'H' => Key::H,
                'I' => Key::I,
                'J' => Key::J,
                'K' => Key::K,
                'L' => Key::L,
                'M' => Key::M,
                'N' => Key::N,
                'O' => Key::O,
                'P' => Key::P,
                'Q' => Key::Q,
                'R' => Key::R,
                'S' => Key::S,
                'T' => Key::T,
                'U' => Key::U,
                'V' => Key::V,
                'W' => Key::W,
                'X' => Key::X,
                'Y' => Key::Y,
                'Z' => Key::Z,
                '0' => Key::Num0,
                '1' => Key::Num1,
                '2' => Key::Num2,
                '3' => Key::Num3,
                '4' => Key::Num4,
                '5' => Key::Num5,
                '6' => Key::Num6,
                '7' => Key::Num7,
                '8' => Key::Num8,
                '9' => Key::Num9,
                '-' => Key::Minus,
                '=' => Key::Equals,
                '[' => Key::LeftBracket,
                ']' => Key::RightBracket,
                '\\' => Key::Backslash,
                ';' => Key::Semicolon,
                '\'' => Key::Quote,
                ',' => Key::Comma,
                '.' => Key::Period,
                '/' => Key::Slash,
                '`' => Key::Grave,
                _ => Key::Char(ch),
            }
        }
        _ => Key::Unknown,
    }
}

/// Convert winit keyboard event to blinc InputEvent
pub fn convert_keyboard_event(
    key: &WinitKey,
    state: ElementState,
    modifiers: ModifiersState,
) -> InputEvent {
    InputEvent::Keyboard(KeyboardEvent {
        key: convert_key(key),
        state: convert_key_state(state),
        modifiers: convert_modifiers(modifiers),
    })
}

/// Convert winit touch event to blinc InputEvent
pub fn convert_touch_event(touch: &Touch) -> InputEvent {
    let id = touch.id;
    let x = touch.location.x as f32;
    let y = touch.location.y as f32;
    // winit doesn't provide pressure directly, use force if available
    let pressure = match touch.force {
        Some(winit::event::Force::Normalized(p)) => p as f32,
        Some(winit::event::Force::Calibrated {
            force,
            max_possible_force,
            ..
        }) => (force / max_possible_force) as f32,
        None => 1.0,
    };

    let touch_event = match touch.phase {
        TouchPhase::Started => TouchEvent::Started { id, x, y, pressure },
        TouchPhase::Moved => TouchEvent::Moved { id, x, y, pressure },
        TouchPhase::Ended => TouchEvent::Ended { id, x, y },
        TouchPhase::Cancelled => TouchEvent::Cancelled { id },
    };

    InputEvent::Touch(touch_event)
}

/// Convert mouse move to blinc InputEvent
pub fn mouse_moved(x: f32, y: f32) -> InputEvent {
    InputEvent::Mouse(MouseEvent::Moved { x, y })
}

/// Convert mouse button press to blinc InputEvent
pub fn mouse_pressed(button: WinitMouseButton, x: f32, y: f32) -> InputEvent {
    InputEvent::Mouse(MouseEvent::ButtonPressed {
        button: convert_mouse_button(button),
        x,
        y,
    })
}

/// Convert mouse button release to blinc InputEvent
pub fn mouse_released(button: WinitMouseButton, x: f32, y: f32) -> InputEvent {
    InputEvent::Mouse(MouseEvent::ButtonReleased {
        button: convert_mouse_button(button),
        x,
        y,
    })
}

/// Convert scroll event to blinc InputEvent
pub fn scroll_event(delta_x: f32, delta_y: f32, phase: TouchPhase) -> InputEvent {
    let scroll_phase = match phase {
        TouchPhase::Started => ScrollPhase::Started,
        TouchPhase::Moved => ScrollPhase::Moved,
        TouchPhase::Ended => ScrollPhase::Ended,
        TouchPhase::Cancelled => ScrollPhase::MomentumEnded,
    };
    InputEvent::Scroll {
        delta_x,
        delta_y,
        phase: scroll_phase,
    }
}

/// Create a scroll end event (momentum finished)
pub fn scroll_end_event() -> InputEvent {
    InputEvent::ScrollEnd
}
