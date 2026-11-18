//! Shared text editing utilities for code and text_area widgets
//!
//! Contains word boundary detection, clipboard integration, and
//! other helpers shared between multi-line text editing widgets.

// =============================================================================
// Mobile haptic feedback + native edit menu helpers
// =============================================================================
//
// These wrap `blinc_core::native_bridge::native_call` and are no-ops on
// platforms / apps that don't have a native bridge installed (desktop /
// web). The actual implementations live in `BlincNativeBridge.swift`
// (`UIImpactFeedbackGenerator` / `UIMenuController` / `UIEditMenu`) and
// `BlincNativeBridge.kt` (`Vibrator` / `ActionMode`). Both bridge
// templates declare two namespaces backing this module: `haptics`
// for the per-character feedback and `edit_menu` for the
// double-tap context menu.
//
// IMPORTANT — `blinc_core::native_bridge::native_call` panics if
// `NativeBridgeState::init()` was never called. The Android runner
// initializes the bridge automatically via the user-app's
// `init_android_native_bridge` call, but the iOS runner currently
// doesn't, and on desktop / web there's no bridge at all. We guard
// every call site with `NativeBridgeState::is_initialized()` so the
// helpers are *true* no-ops on uninitialized platforms instead of
// panicking the touch handler. The widgets call these from inside
// `on_mouse_down` / `on_drag` closures, which run on the platform
// runner's main thread — a panic there crashes the entire app
// (`panic in a function that cannot unwind` because the C FFI
// boundary doesn't allow Rust unwinding).
fn bridge_ready() -> bool {
    blinc_core::native_bridge::NativeBridgeState::is_initialized()
}

/// Trigger a light haptic "selection changed" feedback on mobile.
///
/// Maps to `UISelectionFeedbackGenerator.selectionChanged()` on iOS and
/// a short low-amplitude vibration on Android. Used when the user drags
/// the cursor with their finger so each character boundary crossed
/// gives a subtle tactile click — matches the native iOS UITextField /
/// Android EditText cursor-drag UX.
///
/// No-op on desktop / web and on mobile builds where the
/// `BlincNativeBridge` isn't initialized.
///
/// **Currently disabled** while we debug an interaction between the
/// haptic dispatch and the edit menu presentation on iOS — the
/// haptic call goes through the same `native_call` bridge as
/// `show_edit_menu`, and we need to rule out any cross-talk before
/// re-enabling. Re-enable by deleting the early `return`.
pub fn haptic_selection() {
    return;
    #[allow(unreachable_code)]
    {
        if !bridge_ready() {
            return;
        }
        let _ = blinc_core::native_bridge::native_call::<(), _>("haptics", "selection", ());
    }
}

/// Trigger a single short impact haptic — heavier than
/// `haptic_selection`. Used for "I just selected the word under your
/// finger" feedback on double-tap.
///
/// **Currently disabled** — see [`haptic_selection`] for the
/// rationale.
pub fn haptic_impact_light() {
    return;
    #[allow(unreachable_code)]
    {
        if !bridge_ready() {
            return;
        }
        use blinc_core::native_bridge::NativeValue;
        // The bridge templates use `impact` with a `style` arg:
        // 0 = light, 1 = medium, 2 = heavy.
        let _ = blinc_core::native_bridge::native_call::<(), _>(
            "haptics",
            "impact",
            vec![NativeValue::Int32(0)],
        );
    }
}

/// Available actions in a text-input edit menu, encoded as a bitmask
/// the native side decides how to render.
///
///   - bit 0 (0x01): Cut
///   - bit 1 (0x02): Copy
///   - bit 2 (0x04): Paste
///   - bit 3 (0x08): Select All
///
/// On iOS this becomes a `UIEditMenuInteraction` (iOS 16+) or a
/// `UIMenuController` with `UIMenuItem`s for the listed commands. On
/// Android it becomes an `ActionMode.Callback2` with the matching
/// menu items.
pub mod edit_menu_actions {
    pub const CUT: u32 = 0x01;
    pub const COPY: u32 = 0x02;
    pub const PASTE: u32 = 0x04;
    pub const SELECT_ALL: u32 = 0x08;
}

/// Show the native text-edit context menu (iOS UIEditMenuInteraction
/// / Android ActionMode) at the given screen-space position, with the
/// given selection bounds and supported actions.
///
/// The menu's actions route back to Rust through the same
/// `BlincNativeBridge` callback path: when the user picks Copy, the
/// native side calls `native_call("edit_menu", "on_action", (action,))`
/// which the Blinc app dispatches to whichever editable widget owns
/// the focus.
///
/// `actions` is a bitmask of `edit_menu_actions::*` constants —
/// callers should OR together the actions appropriate for the current
/// state (e.g. omit CUT/COPY when there's no selection, omit PASTE
/// when the clipboard is empty).
///
/// Returns `Ok(())` if the bridge is wired up. No-op on desktop / web
/// and on mobile builds without an initialized native bridge.
pub fn show_edit_menu(
    anchor_x: f32,
    anchor_y: f32,
    selection_x: f32,
    selection_y: f32,
    selection_width: f32,
    selection_height: f32,
    actions: u32,
) {
    if !bridge_ready() {
        return;
    }
    use blinc_core::native_bridge::NativeValue;
    let _ = blinc_core::native_bridge::native_call::<(), _>(
        "edit_menu",
        "show",
        vec![
            NativeValue::Float32(anchor_x),
            NativeValue::Float32(anchor_y),
            NativeValue::Float32(selection_x),
            NativeValue::Float32(selection_y),
            NativeValue::Float32(selection_width),
            NativeValue::Float32(selection_height),
            NativeValue::Int32(actions as i32),
        ],
    );
}

/// Hide the native text-edit context menu, if any is currently
/// showing. Called when focus changes, the user taps elsewhere, or
/// the editor's content shifts so the anchor would be wrong.
pub fn hide_edit_menu() {
    if !bridge_ready() {
        return;
    }
    let _ = blinc_core::native_bridge::native_call::<(), _>("edit_menu", "hide", ());
}

/// Find the start of the previous word from a character position in a line.
/// Words are separated by whitespace and punctuation boundaries.
pub fn word_boundary_left(text: &str, char_pos: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    if char_pos == 0 || chars.is_empty() {
        return 0;
    }

    let mut pos = char_pos.min(chars.len());

    // Skip whitespace to the left
    while pos > 0 && chars[pos - 1].is_whitespace() {
        pos -= 1;
    }

    if pos == 0 {
        return 0;
    }

    // Determine the category of the character we landed on
    let is_word = chars[pos - 1].is_alphanumeric() || chars[pos - 1] == '_';

    // Skip characters of the same category
    if is_word {
        while pos > 0 && (chars[pos - 1].is_alphanumeric() || chars[pos - 1] == '_') {
            pos -= 1;
        }
    } else {
        // Punctuation group
        while pos > 0
            && !chars[pos - 1].is_alphanumeric()
            && chars[pos - 1] != '_'
            && !chars[pos - 1].is_whitespace()
        {
            pos -= 1;
        }
    }

    pos
}

/// Find the end of the next word from a character position in a line.
/// Words are separated by whitespace and punctuation boundaries.
pub fn word_boundary_right(text: &str, char_pos: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if char_pos >= len || chars.is_empty() {
        return len;
    }

    let mut pos = char_pos;

    // Determine the category of the character at pos
    let is_word = chars[pos].is_alphanumeric() || chars[pos] == '_';
    let is_ws = chars[pos].is_whitespace();

    if is_ws {
        // Skip whitespace
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
    } else if is_word {
        // Skip word characters
        while pos < len && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
            pos += 1;
        }
    } else {
        // Skip punctuation
        while pos < len
            && !chars[pos].is_alphanumeric()
            && chars[pos] != '_'
            && !chars[pos].is_whitespace()
        {
            pos += 1;
        }
    }

    // Also skip trailing whitespace after a word/punct group
    while pos < len && chars[pos].is_whitespace() {
        pos += 1;
    }

    pos
}

/// Find word boundaries around a position (for double-click word selection).
/// Returns (start, end) character positions of the word at `char_pos`.
/// Unicode word boundaries keep combining marks and emoji sequences together.
pub fn word_at_position(text: &str, char_pos: usize) -> (usize, usize) {
    use unicode_segmentation::UnicodeSegmentation;

    let mut start = 0;
    let mut end = 0;
    let mut previous_kind = None;
    for segment in text.split_word_bounds() {
        let kind = if segment.chars().all(char::is_whitespace) {
            0
        } else if segment.unicode_words().next().is_some() || segment.contains('_') {
            1
        } else {
            2
        };
        // Preserve whitespace and punctuation runs used by the editors.
        if previous_kind != Some(kind) || kind == 1 {
            if char_pos < end {
                return (start, end);
            }
            start = end;
        }
        end += segment.chars().count();
        previous_kind = Some(kind);
    }
    if char_pos < end {
        (start, end)
    } else {
        (char_pos, char_pos)
    }
}

// =============================================================================
// Clipboard adapters
// =============================================================================
//
// `arboard` covers macOS, Windows, and Linux. It does NOT cover:
//
//   * `wasm32-unknown-unknown` — no platform backend at all.
//   * `target_os = "android"` — no X11/Wayland and arboard's UIKit
//     path doesn't apply.
//   * `target_os = "ios"` — arboard nominally builds for iOS but
//     its UIPasteboard path doesn't actually round-trip strings in
//     the iOS Simulator (Cut appears to work because the visual
//     effect of removing the selection happens regardless of the
//     write succeeding, but Paste comes back empty). The
//     `BlincNativeBridge` Swift side already exposes
//     `clipboard.copy` / `clipboard.paste` namespace handlers that
//     call `UIPasteboard.general` directly, so we route through
//     those instead — same pattern as Android.
//
// All three excluded targets get their own `cfg` block below.
// Everything else (desktop) uses arboard directly.

/// Read text from the system clipboard.
/// Desktop: cross-platform via arboard (macOS, Windows, Linux).
#[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
pub fn clipboard_read() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
        .filter(|t| !t.is_empty())
}

/// Android / iOS: route through the native bridge `clipboard.paste`
/// namespace, which `BlincNativeBridge.kt` (Android) and
/// `BlincNativeBridge.swift` (iOS) implement against the system
/// `ClipboardManager` / `UIPasteboard.general`. This is what the
/// soft-keyboard / edit-menu paste button needs to actually paste.
///
/// Falls back to `None` when the native bridge isn't initialized
/// (e.g. during tests, or in apps that don't link the
/// `BlincNativeBridge` glue).
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn clipboard_read() -> Option<String> {
    if !bridge_ready() {
        return None;
    }
    let result: blinc_core::native_bridge::NativeResult<String> =
        blinc_core::native_bridge::native_call("clipboard", "paste", ());
    match result {
        Ok(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Stub for wasm32. The browser clipboard API is async-only, so a
/// synchronous helper can't reach it; Cmd+V keybinds no-op without
/// crashing. Web apps that need paste should use the `Clipboard`
/// API in their own JS bindings.
#[cfg(target_arch = "wasm32")]
pub fn clipboard_read() -> Option<String> {
    None
}

/// Write text to the system clipboard.
/// Desktop: cross-platform via arboard (macOS, Windows, Linux).
#[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
pub fn clipboard_write(text: &str) -> bool {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.set_text(text.to_string()).ok())
        .is_some()
}

/// Android / iOS: route through the native bridge `clipboard.copy`
/// namespace, which `BlincNativeBridge.kt` (Android) and
/// `BlincNativeBridge.swift` (iOS) implement against the system
/// `ClipboardManager` / `UIPasteboard.general`. Mirrors
/// [`clipboard_read`].
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn clipboard_write(text: &str) -> bool {
    if !bridge_ready() {
        return false;
    }
    let result: blinc_core::native_bridge::NativeResult<()> =
        blinc_core::native_bridge::native_call("clipboard", "copy", (text,));
    result.is_ok()
}

/// Web: write to the system clipboard via
/// `navigator.clipboard.writeText(text)`. The browser's clipboard
/// write API is async and returns a Promise — we fire it and forget
/// the result, returning `true` immediately so the caller (the
/// widget's Cmd+C / Cmd+X handler) can move on with the visual
/// "selection deleted" half of cut.
///
/// Returns `false` only if there's no `window` or `navigator`
/// (e.g. running in a worker context with no DOM).
///
/// Note: writing to the clipboard requires a user-initiated event
/// gesture in most browsers. Cmd+C / Cmd+X / Cmd+X via the menu
/// all qualify because they originate from a `keydown` /
/// `pointerup` / `click` handler. Programmatic writes from
/// `setTimeout` or async tasks WITHOUT a user gesture will be
/// silently rejected by the browser.
#[cfg(target_arch = "wasm32")]
pub fn clipboard_write(text: &str) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let clipboard = window.navigator().clipboard();
    // Fire and forget — the returned Promise resolves
    // asynchronously, but we don't have a sync way to await it
    // here and the widget's UX doesn't actually need to wait for
    // the write to complete. The browser handles the rest.
    let _ = clipboard.write_text(text);
    true
}

/// Read image from the system clipboard as RGBA pixels.
/// Returns (rgba_data, width, height) or None.
/// Desktop: cross-platform via arboard (macOS, Windows, Linux).
/// Requires the `clipboard_image` crate feature; otherwise stubs to None.
#[cfg(all(
    feature = "clipboard_image",
    not(any(target_arch = "wasm32", target_os = "android", target_os = "ios"))
))]
pub fn clipboard_read_image() -> Option<(Vec<u8>, u32, u32)> {
    let mut cb = arboard::Clipboard::new().ok()?;
    let img = cb.get_image().ok()?;
    Some((img.bytes.into_owned(), img.width as u32, img.height as u32))
}

/// Stub when image clipboard is unavailable: feature off, or wasm32 /
/// Android / iOS where the native bridge currently only routes strings.
/// Mobile image clipboard would need separate
/// `clipboard.copy_image` / `clipboard.paste_image` handlers on the
/// Swift / Kotlin sides.
#[cfg(any(
    not(feature = "clipboard_image"),
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios"
))]
pub fn clipboard_read_image() -> Option<(Vec<u8>, u32, u32)> {
    None
}

/// Write image to the system clipboard from RGBA pixels.
/// Desktop: cross-platform via arboard (macOS, Windows, Linux).
/// Requires the `clipboard_image` crate feature; otherwise stubs to false.
#[cfg(all(
    feature = "clipboard_image",
    not(any(target_arch = "wasm32", target_os = "android", target_os = "ios"))
))]
pub fn clipboard_write_image(rgba: &[u8], width: u32, height: u32) -> bool {
    let img = arboard::ImageData {
        width: width as usize,
        height: height as usize,
        bytes: std::borrow::Cow::Borrowed(rgba),
    };
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.set_image(img).ok())
        .is_some()
}

/// Stub when image clipboard is unavailable. See [`clipboard_read_image`].
#[cfg(any(
    not(feature = "clipboard_image"),
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios"
))]
pub fn clipboard_write_image(_rgba: &[u8], _width: u32, _height: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_boundary_left() {
        assert_eq!(word_boundary_left("hello world", 5), 0);
        assert_eq!(word_boundary_left("hello world", 6), 0);
        assert_eq!(word_boundary_left("hello world", 11), 6);
        assert_eq!(word_boundary_left("fn main() {", 3), 0);
        assert_eq!(word_boundary_left("fn main() {", 8), 7); // at ')', skips '(' punct
    }

    #[test]
    fn test_word_boundary_right() {
        assert_eq!(word_boundary_right("hello world", 0), 6);
        assert_eq!(word_boundary_right("hello world", 6), 11);
        assert_eq!(word_boundary_right("fn main() {", 0), 3);
    }

    #[test]
    fn test_word_at_position() {
        assert_eq!(word_at_position("hello world", 2), (0, 5));
        assert_eq!(word_at_position("hello world", 7), (6, 11));
        assert_eq!(word_at_position("hello world", 5), (5, 6)); // space
    }

    #[test]
    fn unicode_word_selection_uses_character_offsets() {
        for (text, position, expected) in [
            ("cafe\u{301} noir", 4, (0, 5)),
            ("can't stop", 3, (0, 5)),
            ("привет мир", 2, (0, 6)),
            ("go 👩‍💻 now", 4, (3, 6)),
            ("hello_world", 6, (0, 11)),
            ("__::", 1, (0, 2)),
            ("__::", 2, (2, 4)),
            ("hello  world", 6, (5, 7)),
            ("word!", 4, (4, 5)),
            ("::+-", 1, (0, 4)),
            ("中文", 1, (1, 2)),
            ("", 0, (0, 0)),
            ("end", 3, (3, 3)),
        ] {
            assert_eq!(word_at_position(text, position), expected, "{text}");
        }
    }
}
