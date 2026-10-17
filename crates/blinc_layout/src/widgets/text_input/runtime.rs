//! Process-wide state the text fields share with the runner: focus counts,
//! redraw and keyboard requests, the focused editable's node.

use super::*;

/// Get elapsed time in milliseconds since app start (for cursor blinking)
pub fn elapsed_ms() -> u64 {
    elapsed_micros() / 1000
}

/// Get elapsed time in MICROSECONDS since app start.
///
/// Shares one monotonic clock with [`elapsed_ms`]. Millisecond
/// resolution quantises a 16.67 ms vsync interval to alternating 16/17 ms
/// deltas — a ~6% per-frame jitter that reads as micro-judder in any
/// dt-integrated animation (scroll momentum, timelines). Consumers that
/// integrate motion over `dt` should derive it from this instead.
pub fn elapsed_micros() -> u64 {
    static START_TIME: OnceLock<web_time::Instant> = OnceLock::new();
    let start = START_TIME.get_or_init(web_time::Instant::now);
    start.elapsed().as_micros() as u64
}

/// Standard cursor blink interval in milliseconds
pub const CURSOR_BLINK_INTERVAL_MS: u64 = 400;

// =============================================================================
// Global focus tracking
// =============================================================================

pub(super) static GLOBAL_FOCUS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Generation counter that increments on every text-input tap, regardless
/// of whether the tap actually transitions focus state. Polled by mobile
/// runners (`blinc_app::android` / `blinc_app::ios`) to detect "user tapped
/// a text input again" events that `take_keyboard_state_change` misses,
/// because that flag only fires on `0 → 1` / `1 → 0` focus-count
/// transitions.
///
/// Re-tapping the same input (or a different input while the keyboard is
/// already up) does NOT cross those transitions, so the runner has no
/// other way to know the user wanted to re-engage the keyboard. This
/// counter gives it that signal: bump on every tap that lands on a text
/// input handler, runner stores the last value it saw, and runs
/// scroll-into-view whenever the value advances.
///
/// Used by [`focus_tap_generation`] / bumped by the `text_input`,
/// `text_area`, `code_editor`, and `rich_text_editor` widgets'
/// `on_mouse_down` handlers.
pub(super) static FOCUS_TAP_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Layout node ID of the currently focused text-editable widget, encoded as
/// the raw `u64` from `LayoutNodeId::to_raw()`. `0` is the sentinel for
/// "nothing focused" since taffy never assigns id `0`.
///
/// `text_input` and `text_area` track their focus through dedicated
/// `FOCUSED_TEXT_INPUT` / `FOCUSED_TEXT_AREA` mutexes (which carry the
/// full widget data), but other text-editable widgets (`code_editor`,
/// `rich_text_editor`) don't share that data layout — they have their
/// own state types that the global trackers can't store. This atomic is
/// the lowest-common-denominator pointer-back: every editable widget can
/// register its own LayoutNodeId here on focus, and the
/// `scroll_focused_text_input_above_keyboard` helper consults this
/// generic ID when the typed lookups (`focused_text_input_node_id` /
/// `focused_text_area_node_id`) come up empty.
///
/// Bumped by [`set_focused_editable_node`] / cleared by
/// [`clear_focused_editable_node`]. Read by [`focused_editable_node_id`].
pub(super) static FOCUSED_EDITABLE_NODE_ID: AtomicU64 = AtomicU64::new(0);

/// Companion to `FOCUSED_EDITABLE_NODE_ID` — an opaque blur callback
/// that the focused widget registers alongside its node id, so
/// [`blur_all_text_inputs`] can dismiss the editor when the user taps
/// outside it. The dedicated trackers (`FOCUSED_TEXT_INPUT` /
/// `FOCUSED_TEXT_AREA`) carry typed widget data and can call into the
/// widget's blur path directly; widgets that don't fit those types
/// (`code_editor`, `rich_text_editor`) instead pass a closure here so
/// the global blur path remains a single call.
///
/// `Box<dyn Fn() + Send + Sync>` rather than `FnOnce` because the
/// closure is invoked at most once but `take()` and replace would race
/// with the writer that just registered it. The closure typically
/// captures an `Arc<Mutex<...>>` to the widget state so it can flip
/// the local `focused` flag, decrement the global focus count, and
/// release any cursor-tick callbacks the widget held.
#[allow(clippy::type_complexity)]
pub(super) static FOCUSED_EDITABLE_BLUR_CALLBACK: Mutex<Option<Box<dyn Fn() + Send + Sync>>> =
    Mutex::new(None);

/// Whether the most recent pointer event came from a touchscreen rather
/// than a mouse / trackpad.
///
/// Set to `true` from the iOS / Android runners on every
/// `TouchPhase::Began` (and reset to `false` from the desktop / web
/// runners on `mouse_down`). Editable widgets read this on
/// `on_mouse_down` / `on_drag` to switch between desktop semantics
/// (drag = extend selection) and mobile semantics (drag = move cursor
/// with haptic feedback, double-tap = native context menu).
///
/// Polled by [`is_touch_input`]. Updated by
/// [`set_touch_input`]. The flag is *sticky* — it stays set until a
/// non-touch input event flips it back, so re-rendering between
/// touch frames doesn't lose the bit.
pub(super) static INPUT_SOURCE_IS_TOUCH: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(super) static NEEDS_REBUILD: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(super) static NEEDS_RELAYOUT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(super) static NEEDS_CSS_REPARSE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(super) static NEEDS_CONTINUOUS_REDRAW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Callback for setting continuous redraw on the animation scheduler
/// This is set by the windowed app to bridge text widgets with the animation system
#[allow(clippy::type_complexity)]
pub(super) static CONTINUOUS_REDRAW_CALLBACK: Mutex<Option<Box<dyn Fn(bool) + Send + Sync>>> =
    Mutex::new(None);

/// Tracks whether the soft keyboard should be visible on mobile platforms.
/// Set to `true` when the first text widget gains focus, `false` when all lose focus.
/// Polled by the platform runner (android.rs / ios.rs) in the frame loop.
pub(super) static KEYBOARD_SHOULD_SHOW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Set when keyboard visibility state changes and needs platform action.
pub(super) static KEYBOARD_STATE_CHANGED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Set the callback for continuous redraw requests
///
/// This should be called once during app initialization to connect
/// text widget focus tracking with the animation scheduler.
pub fn set_continuous_redraw_callback<F>(callback: F)
where
    F: Fn(bool) + Send + Sync + 'static,
{
    let mut guard = CONTINUOUS_REDRAW_CALLBACK.lock().unwrap();
    *guard = Some(Box::new(callback));
}

/// Internal function to notify animation scheduler about cursor animation needs
pub(super) fn notify_continuous_redraw(enabled: bool) {
    if let Ok(guard) = CONTINUOUS_REDRAW_CALLBACK.lock() {
        if let Some(ref callback) = *guard {
            callback(enabled);
        }
    }
}

/// Internal function to flag that the soft keyboard visibility should change.
/// The platform runner polls this via `take_keyboard_state_change()`.
pub(super) fn notify_keyboard_visibility(show: bool) {
    KEYBOARD_SHOULD_SHOW.store(show, Ordering::SeqCst);
    KEYBOARD_STATE_CHANGED.store(true, Ordering::SeqCst);
}

/// Check if the keyboard visibility state changed and needs platform action.
/// Returns `Some(true)` = show keyboard, `Some(false)` = hide keyboard, `None` = no change.
/// The flag is consumed (cleared) on read.
pub fn take_keyboard_state_change() -> Option<bool> {
    if KEYBOARD_STATE_CHANGED.swap(false, Ordering::SeqCst) {
        Some(KEYBOARD_SHOULD_SHOW.load(Ordering::SeqCst))
    } else {
        None
    }
}

pub fn has_focused_text_input() -> bool {
    GLOBAL_FOCUS_COUNT.load(Ordering::Relaxed) > 0
}

/// Get the current text-input tap generation counter.
///
/// Increments on every tap that lands on a text input or text area
/// `on_mouse_down` handler, regardless of whether the focus state
/// actually transitioned. Mobile platform runners use this as a more
/// reliable "user just tapped an input" signal than
/// [`take_keyboard_state_change`], which only fires on transitions
/// of the global focus count and misses re-taps of an already-focused
/// input.
///
/// The runner pattern is:
/// ```ignore
/// let gen = focus_tap_generation();
/// if gen != last_seen_gen {
///     last_seen_gen = gen;
///     tree.scroll_focused_text_input_above_keyboard(viewport_h, inset);
/// }
/// ```
pub fn focus_tap_generation() -> u64 {
    FOCUS_TAP_GENERATION.load(Ordering::Relaxed)
}

/// Bump the tap generation counter.
///
/// Called by text-editable widgets (`text_input`, `text_area`,
/// `code_editor`, `rich_text_editor`) from their `on_mouse_down`
/// handlers, after they've confirmed the tap landed on the widget
/// (passes the disabled / pointer_events check). Mobile runners poll
/// this via [`focus_tap_generation`] to drive scroll-into-view on
/// re-taps and cross-input focus swaps.
pub fn bump_focus_tap_generation() {
    FOCUS_TAP_GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Register a text-editable widget's `LayoutNodeId` as the currently
/// focused editable, optionally with a blur callback.
///
/// Called by widgets that don't fit the dedicated `text_input` /
/// `text_area` focus trackers (`code_editor`, `rich_text_editor`).
/// Pass `node_id` from the widget's `on_mouse_down` handler so the
/// scroll-into-view helper knows which node to keep above the
/// soft keyboard.
///
/// `blur_callback`, if `Some`, is invoked from
/// [`blur_all_text_inputs`] when the user taps outside any editable
/// widget. It should clear the widget's local `focused` flag, call
/// [`decrement_focus_count`] (so the soft keyboard hides), and
/// release any cursor-tick callbacks the widget held. Widgets that
/// have their own `on_event(BLUR)` handling can pass `None` and
/// rely on that path instead.
pub fn set_focused_editable_node(
    node_id: LayoutNodeId,
    blur_callback: Option<Box<dyn Fn() + Send + Sync>>,
) {
    FOCUSED_EDITABLE_NODE_ID.store(node_id.to_raw(), Ordering::Relaxed);
    if let Ok(mut slot) = FOCUSED_EDITABLE_BLUR_CALLBACK.lock() {
        *slot = blur_callback;
    }
}

/// Clear the focused-editable node id and drop any registered blur
/// callback. See [`set_focused_editable_node`].
///
/// Also dismisses any open native edit menu (Cut / Copy / Paste /
/// Select All) and cancels any armed long-press timer. The edit
/// menu is anchored to the focused widget — leaving it visible
/// after focus moves away would let the user pick Cut / Copy / etc.
/// against the wrong (now-unfocused) input. Cancelling the
/// long-press is the same logic: a timer that fires while no
/// editable is focused would pop a menu against a stale anchor.
pub fn clear_focused_editable_node() {
    FOCUSED_EDITABLE_NODE_ID.store(0, Ordering::Relaxed);
    if let Ok(mut slot) = FOCUSED_EDITABLE_BLUR_CALLBACK.lock() {
        *slot = None;
    }
    crate::widgets::text_edit::hide_edit_menu();
    cancel_long_press_timer();
}

/// Get the LayoutNodeId of the currently focused generic editable widget,
/// if any. Used by the mobile-runner scroll-into-view helper as a fallback
/// when the typed `focused_text_input_node_id` / `focused_text_area_node_id`
/// lookups return `None` (e.g. for `code_editor` and `rich_text_editor`).
pub fn focused_editable_node_id() -> Option<LayoutNodeId> {
    let raw = FOCUSED_EDITABLE_NODE_ID.load(Ordering::Relaxed);
    if raw == 0 {
        None
    } else {
        Some(LayoutNodeId::from_raw(raw))
    }
}

/// Set whether the most recent pointer input came from a touchscreen.
///
/// Called by platform runners on every input event so editable widgets
/// can branch on input source: mouse drags extend selections, touch
/// drags move the cursor with haptic feedback. Pass `true` from
/// `TouchPhase::Began` / `MotionAction::Down` (mobile), and `false`
/// from desktop / web `mouse_down` paths.
///
/// The flag is sticky between events — calling once per input event
/// is enough; the widget consults it during the same frame's event
/// dispatch.
pub fn set_touch_input(is_touch: bool) {
    INPUT_SOURCE_IS_TOUCH.store(is_touch, Ordering::Relaxed);
}

/// Returns true if the most recent pointer event came from a
/// touchscreen. See [`set_touch_input`] for the contract.
pub fn is_touch_input() -> bool {
    INPUT_SOURCE_IS_TOUCH.load(Ordering::Relaxed)
}

pub fn take_needs_continuous_redraw() -> bool {
    NEEDS_CONTINUOUS_REDRAW.swap(false, Ordering::SeqCst)
}

pub(super) fn request_continuous_redraw() {
    if has_focused_text_input() {
        NEEDS_CONTINUOUS_REDRAW.store(true, Ordering::SeqCst);
    }
}

pub fn request_continuous_redraw_pub() {
    request_continuous_redraw();
}

pub fn take_needs_rebuild() -> bool {
    NEEDS_REBUILD.swap(false, Ordering::SeqCst)
}

pub fn request_rebuild() {
    NEEDS_REBUILD.store(true, Ordering::SeqCst);
}

/// Check and clear the relayout flag
pub fn take_needs_relayout() -> bool {
    NEEDS_RELAYOUT.swap(false, Ordering::SeqCst)
}

/// Request a full rebuild with relayout
///
/// This triggers all three phases:
/// 1. Tree rebuild - UI tree is reconstructed from builder functions
/// 2. Layout recompute - Flexbox layout is recalculated
/// 3. Visual redraw - Frame is rendered
///
/// Use this for theme changes or other global state that affects the entire UI.
pub fn request_full_rebuild() {
    NEEDS_REBUILD.store(true, Ordering::SeqCst);
    NEEDS_RELAYOUT.store(true, Ordering::SeqCst);
    // Also trigger stateful redraw to ensure visual updates
    crate::stateful::request_redraw();
}

/// Check and clear the CSS reparse flag
pub fn take_needs_css_reparse() -> bool {
    NEEDS_CSS_REPARSE.swap(false, Ordering::SeqCst)
}

/// Request CSS stylesheet reparsing (e.g., after theme color scheme change)
pub fn request_css_reparse() {
    NEEDS_CSS_REPARSE.store(true, Ordering::SeqCst);
}

pub fn increment_focus_count() {
    let prev = GLOBAL_FOCUS_COUNT.fetch_add(1, Ordering::Relaxed);
    // If this is the first focused text widget, enable continuous redraw for cursor animation
    // and show the soft keyboard on mobile platforms
    if prev == 0 {
        notify_continuous_redraw(true);
        notify_keyboard_visibility(true);
    }
}

pub fn decrement_focus_count() {
    let prev = GLOBAL_FOCUS_COUNT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
        Some(v.saturating_sub(1))
    });
    // If no more focused text widgets, disable continuous redraw
    // and hide the soft keyboard on mobile platforms
    if let Ok(prev_val) = prev {
        if prev_val == 1 {
            notify_continuous_redraw(false);
            notify_keyboard_visibility(false);
        }
    }
}
