//! Collecting keyboard events for the router.

#![allow(unused_variables, clippy::too_many_arguments)]

use super::collect::{Collect, PendingEvent};
use crate::windowed::WindowState;
use blinc_platform::{ControlFlow, InputEvent, Key, KeyState};
use blinc_platform_desktop::DesktopPlatform;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// A keyboard event: shortcuts, key codes and text input.
pub(super) fn collect_keyboard(
    ws: &mut WindowState,
    window: &blinc_platform_desktop::DesktopWindow,
    input_event: InputEvent,
    c: Collect<'_>,
    platform: &DesktopPlatform,
    frame_dirty: &Arc<AtomicBool>,
) -> Option<ControlFlow> {
    let Collect {
        pending_events,
        keyboard_events,
        scroll_ended,
        gesture_ended,
        scroll_info,
        scroll_cancel_hit,
    } = c;
    let (Some(windowed_ctx), Some(tree)) = (&mut ws.ctx, &ws.render_tree) else {
        return None;
    };
    let router = &mut windowed_ctx.event_router;
    let scale = windowed_ctx.scale_factor as f32;
    match input_event {
        InputEvent::Keyboard(kb_event) => {
            let mods = &kb_event.modifiers;
            // Cache the live modifier state so subsequent
            // pointer events can propagate it through to
            // their EventContext.
            //
            // The cache uses TWO inputs to stay
            // accurate across backend quirks:
            //
            // 1. Start from kb_event.modifiers (winit's
            //    snapshot at the moment the event was
            //    queued). Good when ModifiersChanged
            //    fires before the key event.
            // 2. Override explicitly for modifier-key
            //    transitions: a Pressed Shift sets
            //    cache.shift=true, a Released Shift
            //    clears it. Handles backends where
            //    ModifiersChanged queues AFTER the
            //    KeyboardInput (so the snapshot reports
            //    the pre-transition state).
            let mut next = *mods;
            let is_press = matches!(kb_event.state, blinc_platform::KeyState::Pressed);
            let is_release = matches!(kb_event.state, blinc_platform::KeyState::Released);
            match (&kb_event.key, is_press, is_release) {
                (blinc_platform::Key::Shift, true, _) => next.shift = true,
                (blinc_platform::Key::Shift, _, true) => next.shift = false,
                (blinc_platform::Key::Ctrl, true, _) => next.ctrl = true,
                (blinc_platform::Key::Ctrl, _, true) => next.ctrl = false,
                (blinc_platform::Key::Alt, true, _) => next.alt = true,
                (blinc_platform::Key::Alt, _, true) => next.alt = false,
                (blinc_platform::Key::Meta, true, _) => next.meta = true,
                (blinc_platform::Key::Meta, _, true) => next.meta = false,
                _ => {}
            }
            if next != ws.cached_modifiers {
                tracing::trace!(
                    target: "blinc_app::modifiers",
                    shift = next.shift,
                    ctrl = next.ctrl,
                    alt = next.alt,
                    meta = next.meta,
                    "cached modifiers updated"
                );
            }
            ws.cached_modifiers = next;

            let key_char = key_char(&kb_event);

            if kb_event.key == Key::Back && blinc_layout::back_handler::dispatch_back() {
                router.clear_event_callback();
                return Some(ControlFlow::Continue);
            }
            let key_code = key_code(&kb_event.key);

            match kb_event.state {
                KeyState::Pressed => {
                    // Tab traversal and Enter/Space activation
                    // take their keys from the widgets.
                    let navigated = ws
                        .render_tree
                        .as_mut()
                        .is_some_and(|t| t.handle_navigation_key(router, key_code, mods.shift));

                    // Dispatch KEY_DOWN for all keys
                    if !navigated {
                        router.on_key_down(key_code);
                    }

                    if !navigated {
                        queue_key_press(keyboard_events, key_code, key_char, mods);
                    }
                }
                KeyState::Released => {
                    router.on_key_up(key_code);

                    // Also broadcast KEY_UP through the
                    // `keyboard_events` path. Without this the
                    // focus-targeted dispatch only fires on the
                    // focused leaf node, so ancestor handlers
                    // (e.g. `blinc_input::DivInputExt::capture_input`
                    // attached to a viewport Div) never see
                    // releases and their internal
                    // `keys_down`-tracking sets never clear —
                    // which in turn makes polling consumers see
                    // every key as permanently-held after the
                    // first press. Matches the broadcast path
                    // KEY_DOWN already uses below.
                    if key_code != 0 {
                        keyboard_events.push(PendingEvent {
                            event_type: blinc_core::events::event_types::KEY_UP,
                            key_char: None,
                            key_code,
                            shift: mods.shift,
                            ctrl: mods.ctrl,
                            alt: mods.alt,
                            meta: mods.meta,
                            ..Default::default()
                        });
                    }
                }
            }
        }
        _ => {}
    }
    None
}

/// Numeric codes used by widget navigation and shortcut handlers.
fn key_code(key: &Key) -> u32 {
    match key {
        Key::Backspace => 8,
        Key::Delete => 127,
        Key::Enter => 13,
        Key::Tab => 9,
        Key::F1 => 112,
        Key::F2 => 113,
        Key::F3 => 114,
        Key::F4 => 115,
        Key::F5 => 116,
        Key::F6 => 117,
        Key::F7 => 118,
        Key::F8 => 119,
        Key::F9 => 120,
        Key::F10 => 121,
        Key::F11 => 122,
        Key::F12 => 123,
        Key::Insert => 45,
        Key::ContextMenu => 93,
        Key::Escape => 27,
        Key::Space => 32,
        Key::Left => 37,
        Key::Right => 39,
        Key::Up => 38,
        Key::Down => 40,
        Key::Home => 36,
        Key::End => 35,
        Key::PageUp => 33,
        Key::PageDown => 34,
        // Map letter keys to ASCII uppercase for Cmd+key shortcuts
        Key::A => 65,
        Key::B => 66,
        Key::C => 67,
        Key::D => 68,
        Key::E => 69,
        Key::F => 70,
        Key::G => 71,
        Key::H => 72,
        Key::I => 73,
        Key::J => 74,
        Key::K => 75,
        Key::L => 76,
        Key::M => 77,
        Key::N => 78,
        Key::O => 79,
        Key::P => 80,
        Key::Q => 81,
        Key::R => 82,
        Key::S => 83,
        Key::T => 84,
        Key::U => 85,
        Key::V => 86,
        Key::W => 87,
        Key::X => 88,
        Key::Y => 89,
        Key::Z => 90,
        // Digit row — match standard JS
        // KeyboardEvent.keyCode for parity
        // with web-convention chord tables.
        Key::Num0 => 48,
        Key::Num1 => 49,
        Key::Num2 => 50,
        Key::Num3 => 51,
        Key::Num4 => 52,
        Key::Num5 => 53,
        Key::Num6 => 54,
        Key::Num7 => 55,
        Key::Num8 => 56,
        Key::Num9 => 57,
        // Punctuation / symbol keys — JS
        // keyCode values so chord tables
        // that bind `,` / `=` / `-` etc
        // resolve cleanly.
        Key::Semicolon => 186,
        Key::Equals => 187,
        Key::Comma => 188,
        Key::Minus => 189,
        Key::Period => 190,
        Key::Slash => 191,
        Key::Grave => 192,
        Key::LeftBracket => 219,
        Key::Backslash => 220,
        Key::RightBracket => 221,
        Key::Quote => 222,
        _ => 0,
    }
}

fn queue_key_press(
    events: &mut Vec<PendingEvent>,
    key_code: u32,
    key_char: Option<char>,
    mods: &blinc_platform::Modifiers,
) {
    use blinc_core::events::event_types;
    let mut push = |event_type, key_char| {
        events.push(PendingEvent {
            event_type,
            key_char,
            key_code,
            shift: mods.shift,
            ctrl: mods.ctrl,
            alt: mods.alt,
            meta: mods.meta,
            ..Default::default()
        });
    };
    if key_code != 0 {
        push(event_types::KEY_DOWN, None);
    }
    if (!mods.ctrl || mods.alt)
        && !mods.meta
        && let Some(ch) = key_char
    {
        push(event_types::TEXT_INPUT, Some(ch));
    }
}

pub(super) fn dispatch_keyboard_events(
    tree: &mut blinc_layout::renderer::RenderTree,
    keyboard_events: Vec<PendingEvent>,
) {
    use blinc_core::events::event_types;
    let mut prevent_text = false;
    for event in keyboard_events {
        if event.event_type == event_types::TEXT_INPUT {
            if !prevent_text && let Some(c) = event.key_char {
                tree.broadcast_text_input_event(c, event.shift, event.ctrl, event.alt, event.meta);
            }
        } else {
            let prevented = tree.broadcast_key_event_cancellable(
                event.event_type,
                event.key_code,
                event.shift,
                event.ctrl,
                event.alt,
                event.meta,
            );
            prevent_text = event.event_type == event_types::KEY_DOWN && prevented;
        }
    }
}

fn key_char(kb_event: &blinc_platform::KeyboardEvent) -> Option<char> {
    let mods = &kb_event.modifiers;
    // AltGr requires layout-produced text, not a synthesized Ctrl+Alt letter.
    if mods.ctrl && mods.alt {
        return kb_event.typed_char();
    }
    kb_event.typed_char().or(match &kb_event.key {
        Key::Char(c) => Some(*c),
        Key::Space => Some(' '),
        Key::A => Some(if mods.shift { 'A' } else { 'a' }),
        Key::B => Some(if mods.shift { 'B' } else { 'b' }),
        Key::C => Some(if mods.shift { 'C' } else { 'c' }),
        Key::D => Some(if mods.shift { 'D' } else { 'd' }),
        Key::E => Some(if mods.shift { 'E' } else { 'e' }),
        Key::F => Some(if mods.shift { 'F' } else { 'f' }),
        Key::G => Some(if mods.shift { 'G' } else { 'g' }),
        Key::H => Some(if mods.shift { 'H' } else { 'h' }),
        Key::I => Some(if mods.shift { 'I' } else { 'i' }),
        Key::J => Some(if mods.shift { 'J' } else { 'j' }),
        Key::K => Some(if mods.shift { 'K' } else { 'k' }),
        Key::L => Some(if mods.shift { 'L' } else { 'l' }),
        Key::M => Some(if mods.shift { 'M' } else { 'm' }),
        Key::N => Some(if mods.shift { 'N' } else { 'n' }),
        Key::O => Some(if mods.shift { 'O' } else { 'o' }),
        Key::P => Some(if mods.shift { 'P' } else { 'p' }),
        Key::Q => Some(if mods.shift { 'Q' } else { 'q' }),
        Key::R => Some(if mods.shift { 'R' } else { 'r' }),
        Key::S => Some(if mods.shift { 'S' } else { 's' }),
        Key::T => Some(if mods.shift { 'T' } else { 't' }),
        Key::U => Some(if mods.shift { 'U' } else { 'u' }),
        Key::V => Some(if mods.shift { 'V' } else { 'v' }),
        Key::W => Some(if mods.shift { 'W' } else { 'w' }),
        Key::X => Some(if mods.shift { 'X' } else { 'x' }),
        Key::Y => Some(if mods.shift { 'Y' } else { 'y' }),
        Key::Z => Some(if mods.shift { 'Z' } else { 'z' }),
        Key::Num0 => Some(if mods.shift { ')' } else { '0' }),
        Key::Num1 => Some(if mods.shift { '!' } else { '1' }),
        Key::Num2 => Some(if mods.shift { '@' } else { '2' }),
        Key::Num3 => Some(if mods.shift { '#' } else { '3' }),
        Key::Num4 => Some(if mods.shift { '$' } else { '4' }),
        Key::Num5 => Some(if mods.shift { '%' } else { '5' }),
        Key::Num6 => Some(if mods.shift { '^' } else { '6' }),
        Key::Num7 => Some(if mods.shift { '&' } else { '7' }),
        Key::Num8 => Some(if mods.shift { '*' } else { '8' }),
        Key::Num9 => Some(if mods.shift { '(' } else { '9' }),
        Key::Minus => Some(if mods.shift { '_' } else { '-' }),
        Key::Equals => Some(if mods.shift { '+' } else { '=' }),
        Key::LeftBracket => Some(if mods.shift { '{' } else { '[' }),
        Key::RightBracket => Some(if mods.shift { '}' } else { ']' }),
        Key::Backslash => Some(if mods.shift { '|' } else { '\\' }),
        Key::Semicolon => Some(if mods.shift { ':' } else { ';' }),
        Key::Quote => Some(if mods.shift { '"' } else { '\'' }),
        Key::Comma => Some(if mods.shift { '<' } else { ',' }),
        Key::Period => Some(if mods.shift { '>' } else { '.' }),
        Key::Slash => Some(if mods.shift { '?' } else { '/' }),
        Key::Grave => Some(if mods.shift { '~' } else { '`' }),
        _ => None,
    })
}

pub(super) fn has_key_broadcast(events: &[PendingEvent], event_type: u32) -> bool {
    use blinc_core::events::event_types;
    matches!(event_type, event_types::KEY_DOWN | event_types::KEY_UP)
        && events.iter().any(|event| event.event_type == event_type)
}

#[cfg(test)]
#[path = "keyboard/tests.rs"]
mod tests;
