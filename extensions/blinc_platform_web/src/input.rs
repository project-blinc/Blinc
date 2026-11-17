//! Browser input → `blinc_platform::InputEvent` conversion
//!
//! Every function in this module is pure (no `web-sys` dep) so it
//! compiles and unit-tests on every host. The wasm-bindgen runner module
//! reads `web_sys::MouseEvent` / `KeyboardEvent` / `WheelEvent` /
//! `TouchEvent` fields, then hands the primitive values
//! (button index, key string, dx/dy, …) to the helpers here.
//!
//! Splitting the conversion this way mirrors how
//! `blinc_platform_desktop::input` keeps winit out of its
//! `convert_*` signatures by passing in primitives where possible.
//!
//! # Key string mapping
//!
//! The W3C `KeyboardEvent.key` value is a string like `"ArrowLeft"`,
//! `"Enter"`, `"a"`, or `"F1"`. This module's [`convert_key_from_dom`]
//! consumes that string directly. The full set of named keys is
//! specified at <https://www.w3.org/TR/uievents-key/#named-key-attribute-values>.

use blinc_platform::{
    InputEvent, Key, KeyState, KeyboardEvent, Modifiers, MouseButton, MouseEvent,
};

// ===========================================================================
// Pointer / mouse buttons
// ===========================================================================

/// Convert a W3C [`MouseEvent.button`](https://developer.mozilla.org/en-US/docs/Web/API/MouseEvent/button)
/// value to a Blinc [`MouseButton`].
///
/// The W3C button index is:
/// - `0` — primary (usually left)
/// - `1` — auxiliary (usually middle / wheel click)
/// - `2` — secondary (usually right)
/// - `3` — fourth (often "back")
/// - `4` — fifth (often "forward")
pub fn convert_mouse_button(button: i16) -> MouseButton {
    match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        3 => MouseButton::Back,
        4 => MouseButton::Forward,
        n if n >= 0 => MouseButton::Other(n as u16),
        // Negative buttons indicate "no button" in some browsers; treat
        // as left-click for the rare cases where this leaks into our
        // event path.
        _ => MouseButton::Left,
    }
}

/// Pointer events expose `pointer.button` as `i32` rather than `i16`.
/// Convenience wrapper around [`convert_mouse_button`].
pub fn convert_pointer_button(button: i32) -> MouseButton {
    convert_mouse_button(button.clamp(i16::MIN as i32, i16::MAX as i32) as i16)
}

// ===========================================================================
// Modifiers
// ===========================================================================

/// Build a [`Modifiers`] struct from the four boolean fields a
/// `web_sys::MouseEvent` exposes (`shiftKey`, `ctrlKey`, `altKey`,
/// `metaKey`). Kept primitive so it's testable without `web-sys`.
pub fn modifiers_from_mouse(shift: bool, ctrl: bool, alt: bool, meta: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl,
        alt,
        meta,
    }
}

/// Same as [`modifiers_from_mouse`] but spelled differently for
/// keyboard events. Browsers expose the identical four booleans on
/// both event types — we keep separate names purely for call-site
/// readability.
pub fn modifiers_from_keyboard(shift: bool, ctrl: bool, alt: bool, meta: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl,
        alt,
        meta,
    }
}

// ===========================================================================
// Key string → Key enum
// ===========================================================================

/// Convert a W3C [`KeyboardEvent.key`](https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/key)
/// string to a Blinc [`Key`].
///
/// The W3C `key` attribute returns either a "named key" (like `"Enter"`)
/// or a printable character (like `"a"`, `"A"`, `"!"`). Both forms are
/// handled. Unrecognised single characters fall through to
/// [`Key::Char`] so editor widgets can still observe them; unknown
/// named keys map to [`Key::Unknown`].
pub fn convert_key_from_dom(dom_key: &str) -> Key {
    // Fast path: empty string => Unknown.
    if dom_key.is_empty() {
        return Key::Unknown;
    }

    // Named keys come first because the named-key check is O(1) on
    // string comparison and we don't want "F1" misclassified as
    // `Key::Char('F')`.
    match dom_key {
        // Whitespace / control
        " " | "Spacebar" => Key::Space,
        "Enter" => Key::Enter,
        "Escape" | "Esc" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Tab" => Key::Tab,
        "Delete" | "Del" => Key::Delete,
        "Insert" => Key::Insert,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,

        // Arrows
        "ArrowLeft" | "Left" => Key::Left,
        "ArrowRight" | "Right" => Key::Right,
        "ArrowUp" | "Up" => Key::Up,
        "ArrowDown" | "Down" => Key::Down,

        // Modifiers
        "Shift" => Key::Shift,
        "Control" => Key::Ctrl,
        "Alt" => Key::Alt,
        "Meta" | "OS" | "Hyper" | "Super" => Key::Meta,

        // Function keys
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,

        // System
        "ContextMenu" => Key::Menu,
        "BrowserBack" | "GoBack" => Key::Back,

        // Single-character printable keys
        s => convert_printable_char(s),
    }
}

fn convert_printable_char(s: &str) -> Key {
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return Key::Unknown,
    };
    // If there's a second character, this isn't a single-key event.
    // It could be:
    //   - a dead-key composition like "´a" — surface as Char(first)
    //     so editors can still observe something.
    //   - an unrecognised W3C named-key value like "Unidentified" or
    //     "AltGraph" — these are alphanumeric strings, NOT printable
    //     characters, and should map to Unknown rather than to
    //     Char of their leading letter.
    //
    // Heuristic: if the leading char is ASCII alphabetic, treat the
    // multi-char string as a missed named key. Anything else
    // (combining marks, IME compositions) we'll surface as the first
    // scalar.
    let extra = chars.next();
    if let Some(_extra_char) = extra {
        if first.is_ascii_alphabetic() {
            return Key::Unknown;
        }
        return Key::Char(first);
    }
    match first.to_ascii_uppercase() {
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
        _ => Key::Char(first),
    }
}

// ===========================================================================
// Builders for InputEvent — these are also pure
// ===========================================================================

/// Build a `Mouse(Moved)` event from canvas-local coordinates.
pub fn mouse_moved(x: f32, y: f32) -> InputEvent {
    InputEvent::Mouse(MouseEvent::Moved { x, y })
}

/// Build a `Mouse(ButtonPressed)` event from canvas-local coordinates.
pub fn mouse_pressed(button: MouseButton, x: f32, y: f32) -> InputEvent {
    InputEvent::Mouse(MouseEvent::ButtonPressed { button, x, y })
}

/// Build a `Mouse(ButtonReleased)` event from canvas-local coordinates.
pub fn mouse_released(button: MouseButton, x: f32, y: f32) -> InputEvent {
    InputEvent::Mouse(MouseEvent::ButtonReleased { button, x, y })
}

/// Build a keyboard event with state + modifiers.
pub fn keyboard_event(key: Key, state: KeyState, modifiers: Modifiers) -> InputEvent {
    InputEvent::Keyboard(KeyboardEvent {
        key,
        state,
        modifiers,
        text: None,
    })
}

// ===========================================================================
// Tests — every assertion in this file runs on the host platform.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_button_indices_match_w3c() {
        assert_eq!(convert_mouse_button(0), MouseButton::Left);
        assert_eq!(convert_mouse_button(1), MouseButton::Middle);
        assert_eq!(convert_mouse_button(2), MouseButton::Right);
        assert_eq!(convert_mouse_button(3), MouseButton::Back);
        assert_eq!(convert_mouse_button(4), MouseButton::Forward);
        assert_eq!(convert_mouse_button(5), MouseButton::Other(5));
    }

    #[test]
    fn mouse_button_negative_falls_back_to_left() {
        // Some browsers report -1 when no button is associated with
        // the event (e.g. mousemove). We never expect that to reach
        // `mousedown` / `mouseup` handling, but be defensive.
        assert_eq!(convert_mouse_button(-1), MouseButton::Left);
    }

    #[test]
    fn pointer_button_clamps() {
        assert_eq!(convert_pointer_button(0), MouseButton::Left);
        assert_eq!(convert_pointer_button(99_999), MouseButton::Other(32767));
    }

    #[test]
    fn modifiers_round_trip() {
        let m = modifiers_from_mouse(true, false, true, false);
        assert!(m.shift && !m.ctrl && m.alt && !m.meta);
        assert!(!m.is_empty());
    }

    #[test]
    fn arrows_map_to_arrow_keys() {
        assert_eq!(convert_key_from_dom("ArrowLeft"), Key::Left);
        assert_eq!(convert_key_from_dom("ArrowRight"), Key::Right);
        assert_eq!(convert_key_from_dom("ArrowUp"), Key::Up);
        assert_eq!(convert_key_from_dom("ArrowDown"), Key::Down);
        // Legacy short names still supported
        assert_eq!(convert_key_from_dom("Left"), Key::Left);
        assert_eq!(convert_key_from_dom("Up"), Key::Up);
    }

    #[test]
    fn function_keys_map_individually() {
        assert_eq!(convert_key_from_dom("F1"), Key::F1);
        assert_eq!(convert_key_from_dom("F12"), Key::F12);
        // F1 must NOT be misclassified as Key::F + Key::Num1.
        assert_ne!(convert_key_from_dom("F1"), Key::F);
    }

    #[test]
    fn whitespace_and_named_keys() {
        assert_eq!(convert_key_from_dom(" "), Key::Space);
        assert_eq!(convert_key_from_dom("Enter"), Key::Enter);
        assert_eq!(convert_key_from_dom("Escape"), Key::Escape);
        assert_eq!(convert_key_from_dom("Esc"), Key::Escape);
        assert_eq!(convert_key_from_dom("Tab"), Key::Tab);
        assert_eq!(convert_key_from_dom("Backspace"), Key::Backspace);
        assert_eq!(convert_key_from_dom("Delete"), Key::Delete);
        assert_eq!(convert_key_from_dom("Del"), Key::Delete);
        assert_eq!(convert_key_from_dom("Home"), Key::Home);
        assert_eq!(convert_key_from_dom("End"), Key::End);
        assert_eq!(convert_key_from_dom("PageUp"), Key::PageUp);
        assert_eq!(convert_key_from_dom("PageDown"), Key::PageDown);
    }

    #[test]
    fn modifier_aliases() {
        assert_eq!(convert_key_from_dom("Meta"), Key::Meta);
        assert_eq!(convert_key_from_dom("OS"), Key::Meta);
        assert_eq!(convert_key_from_dom("Super"), Key::Meta);
        assert_eq!(convert_key_from_dom("Control"), Key::Ctrl);
    }

    #[test]
    fn printable_lowercase_and_uppercase_collapse() {
        assert_eq!(convert_key_from_dom("a"), Key::A);
        assert_eq!(convert_key_from_dom("A"), Key::A);
        assert_eq!(convert_key_from_dom("z"), Key::Z);
        assert_eq!(convert_key_from_dom("Z"), Key::Z);
    }

    #[test]
    fn printable_digits() {
        for (s, k) in [("0", Key::Num0), ("5", Key::Num5), ("9", Key::Num9)] {
            assert_eq!(convert_key_from_dom(s), k);
        }
    }

    #[test]
    fn printable_punctuation() {
        assert_eq!(convert_key_from_dom("-"), Key::Minus);
        assert_eq!(convert_key_from_dom("="), Key::Equals);
        assert_eq!(convert_key_from_dom("["), Key::LeftBracket);
        assert_eq!(convert_key_from_dom("]"), Key::RightBracket);
        assert_eq!(convert_key_from_dom("\\"), Key::Backslash);
        assert_eq!(convert_key_from_dom(";"), Key::Semicolon);
        assert_eq!(convert_key_from_dom("'"), Key::Quote);
        assert_eq!(convert_key_from_dom(","), Key::Comma);
        assert_eq!(convert_key_from_dom("."), Key::Period);
        assert_eq!(convert_key_from_dom("/"), Key::Slash);
        assert_eq!(convert_key_from_dom("`"), Key::Grave);
    }

    #[test]
    fn unknown_named_key_falls_back_to_unknown() {
        assert_eq!(convert_key_from_dom("Unidentified"), Key::Unknown);
        assert_eq!(convert_key_from_dom("Compose"), Key::Unknown);
    }

    #[test]
    fn unknown_single_char_falls_back_to_char() {
        // Latin-1 characters that aren't in our explicit table should
        // still surface as Char so editor widgets can observe them.
        assert_eq!(convert_key_from_dom("é"), Key::Char('é'));
        assert_eq!(convert_key_from_dom("ñ"), Key::Char('ñ'));
    }

    #[test]
    fn dead_key_composition_falls_back_to_first_char() {
        // Some browsers report multi-codepoint combos for dead keys —
        // surface the first scalar rather than dropping the event.
        assert_eq!(convert_key_from_dom("´a"), Key::Char('´'));
    }

    #[test]
    fn empty_string_is_unknown() {
        assert_eq!(convert_key_from_dom(""), Key::Unknown);
    }

    #[test]
    fn input_event_builders_round_trip() {
        let p = mouse_pressed(MouseButton::Left, 12.5, 7.0);
        match p {
            InputEvent::Mouse(MouseEvent::ButtonPressed { button, x, y }) => {
                assert_eq!(button, MouseButton::Left);
                assert_eq!(x, 12.5);
                assert_eq!(y, 7.0);
            }
            _ => panic!("expected ButtonPressed"),
        }

        let r = mouse_released(MouseButton::Right, 0.0, 0.0);
        matches!(r, InputEvent::Mouse(MouseEvent::ButtonReleased { .. }));

        let m = mouse_moved(1.0, 2.0);
        matches!(m, InputEvent::Mouse(MouseEvent::Moved { .. }));

        let k = keyboard_event(
            Key::Enter,
            KeyState::Pressed,
            modifiers_from_keyboard(false, false, false, true),
        );
        match k {
            InputEvent::Keyboard(KeyboardEvent {
                key,
                state,
                modifiers,
                ..
            }) => {
                assert_eq!(key, Key::Enter);
                assert_eq!(state, KeyState::Pressed);
                assert!(modifiers.meta_only());
            }
            _ => panic!("expected Keyboard"),
        }
    }
}
