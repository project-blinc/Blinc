//! Ready-to-use TextInput widget
//!
//! Single-line text input with:
//! - Visual states: idle, hovered, focused
//! - A caret drawn by a canvas that reads the field each paint
//! - Built once: what it shows is bound to a revision signal, so an edit
//!   patches the text, colours and scroll in place
//!
//! # Example
//!
//! ```ignore
//! let input_data = text_input_data_with_placeholder("Enter username");
//! text_input(&input_data)
//!     .w(280.0)
//!     .rounded(12.0)
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use blinc_core::Color;
use blinc_theme::{ColorToken, ThemeState};

use crate::canvas::canvas;
use crate::css_parser::{ElementState, Stylesheet, active_stylesheet};
use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::stateful::{StateTransitions, TextFieldState, refresh_stateful};
use crate::text_selection::{SelectionSource, clear_selection, set_selection};
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::cursor::{CursorAnimation, SharedCursorState, cursor_state};

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

static GLOBAL_FOCUS_COUNT: AtomicU64 = AtomicU64::new(0);

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
static FOCUS_TAP_GENERATION: AtomicU64 = AtomicU64::new(0);

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
static FOCUSED_EDITABLE_NODE_ID: AtomicU64 = AtomicU64::new(0);

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
static FOCUSED_EDITABLE_BLUR_CALLBACK: Mutex<Option<Box<dyn Fn() + Send + Sync>>> =
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
static INPUT_SOURCE_IS_TOUCH: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Long-press timer state for editable widgets.
///
/// When a user begins a touch on a focused text-editable widget, the
/// widget calls [`arm_long_press_timer`] to record the start time +
/// anchor position + bounds where the menu should pop. The platform
/// runner's frame loop polls [`fire_long_press_timer_if_due`] every
/// tick, and when 500 ms have elapsed without a cancel-via-drag or
/// cancel-via-release, the helper fires `show_edit_menu` with the
/// PASTE bit set so the user can paste from the clipboard the same
/// way native iOS UITextField / Android EditText do.
///
/// The timer is cancelled by [`cancel_long_press_timer`], called
/// from `on_drag` (when the finger moves too far) and from
/// `on_mouse_up`.
///
/// Stored as a `Mutex<Option<...>>` rather than separate atomics
/// because the four fields must be read atomically together — a
/// torn read between deadline and anchor would dispatch the menu
/// at the wrong location.
struct LongPressArm {
    /// Deadline in milliseconds since app start
    /// (`text_input::elapsed_ms`). When `elapsed_ms()` >= this,
    /// the timer fires.
    deadline_ms: u64,
    /// Anchor position in window-space logical pixels — passed
    /// straight through to `show_edit_menu`'s anchor_x / anchor_y.
    anchor_x: f32,
    anchor_y: f32,
    /// Original press position used for movement-cancel check.
    start_x: f32,
    start_y: f32,
    /// Selection rect height (used as the menu's vertical extent
    /// hint). The width is intentionally 0 because the menu hugs
    /// the anchor point, not a real selection rect.
    bounds_height: f32,
    /// Optional pre-show callback. Fired immediately before
    /// `show_edit_menu` when the long-press deadline elapses, so
    /// the focused widget can update its selection state to match
    /// the iOS UITextField / Android EditText UX of selecting the
    /// word under the finger on a long press (mirroring the
    /// double-tap behavior). Captured at arm time with an `Arc` to
    /// the widget's data state and the cursor position computed
    /// from the press location, so the callback runs entirely
    /// against state owned by the widget and doesn't need to walk
    /// any registries at fire time.
    on_fire: Option<Box<dyn Fn() + Send + Sync>>,
}

#[allow(clippy::type_complexity)]
static LONG_PRESS_ARM: Mutex<Option<LongPressArm>> = Mutex::new(None);

/// Long-press deadline relative to the press start time (ms). 500ms
/// matches iOS UITextField / Android EditText long-press timing.
const LONG_PRESS_DURATION_MS: u64 = 500;

/// Maximum movement (in logical pixels) allowed before the long-press
/// is cancelled. Mirrors the existing `widgets::gesture` constant.
const LONG_PRESS_MAX_DRIFT_PX: f32 = 10.0;

/// Arm the long-press timer at the given position. Called from a
/// text-editable widget's `on_mouse_down` handler when
/// `is_touch_input()` returns true.
///
/// The runner's frame poll calls [`fire_long_press_timer_if_due`]
/// each tick to check whether the deadline has elapsed.
///
/// `on_fire` is an optional pre-show callback that runs when the
/// deadline elapses, immediately before the edit menu is shown.
/// Editable widgets pass a closure here that selects the word
/// under the press position so a long-press behaves the same as a
/// double-tap (matches iOS UITextField / Android EditText UX). The
/// closure should capture an `Arc` to the widget's data state and
/// any state needed to update the selection.
///
/// Calling this overwrites any previously armed timer — only the
/// most recent press counts, mirroring the iOS UITextField behavior
/// where re-tapping during a press cancels the previous long-press.
pub fn arm_long_press_timer(
    anchor_x: f32,
    anchor_y: f32,
    bounds_height: f32,
    on_fire: Option<Box<dyn Fn() + Send + Sync>>,
) {
    if let Ok(mut slot) = LONG_PRESS_ARM.lock() {
        *slot = Some(LongPressArm {
            deadline_ms: elapsed_ms() + LONG_PRESS_DURATION_MS,
            anchor_x,
            anchor_y,
            start_x: anchor_x,
            start_y: anchor_y,
            bounds_height,
            on_fire,
        });
    }
}

/// Cancel any armed long-press timer.
///
/// Called from a text-editable widget's `on_mouse_up` handler and
/// from `on_drag` when the finger moves more than
/// `LONG_PRESS_MAX_DRIFT_PX` from the original position. Idempotent
/// — clearing an already-empty slot is a no-op.
pub fn cancel_long_press_timer() {
    if let Ok(mut slot) = LONG_PRESS_ARM.lock() {
        *slot = None;
    }
}

/// Returns `true` if a long-press timer is currently armed and waiting
/// to fire. Used by `IOSRenderContext::needs_render` to keep the frame
/// loop ticking while the user holds a text input — without this, no
/// events would come in during the hold and the deadline poll would
/// never run.
pub fn is_long_press_armed() -> bool {
    LONG_PRESS_ARM
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(false)
}

/// Check whether an active drag has exceeded the movement budget for
/// the armed long-press, cancelling it if so. Called from `on_drag`
/// before the cursor-move logic so a small finger jitter doesn't
/// kill a still-valid long press.
pub fn check_long_press_drift(current_x: f32, current_y: f32) {
    if let Ok(mut slot) = LONG_PRESS_ARM.lock() {
        if let Some(arm) = slot.as_ref() {
            let dx = (current_x - arm.start_x).abs();
            let dy = (current_y - arm.start_y).abs();
            if dx > LONG_PRESS_MAX_DRIFT_PX || dy > LONG_PRESS_MAX_DRIFT_PX {
                *slot = None;
            }
        }
    }
}

/// Poll: if a long-press is armed and the deadline has elapsed, fire
/// `show_edit_menu` with the PASTE bit set and clear the timer.
///
/// Returns `true` if the timer fired (so the runner can request a
/// redraw), `false` otherwise. Called from the platform runner's
/// frame loop on every tick — for iOS this lives in
/// `blinc_build_frame`, for Android it lives in the `android_main`
/// poll loop.
pub fn fire_long_press_timer_if_due() -> bool {
    let arm = if let Ok(mut slot) = LONG_PRESS_ARM.lock() {
        if let Some(arm) = slot.as_ref() {
            if elapsed_ms() >= arm.deadline_ms {
                slot.take()
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if let Some(arm) = arm {
        // Long press fired. Run the widget-supplied pre-show
        // callback first so the focused editable can update its
        // selection state to match the iOS UITextField /
        // Android EditText UX of selecting the word under the
        // finger on a long press (mirroring double-tap). The
        // callback is registered at arm time and captures an
        // `Arc` to the widget's data.
        if let Some(cb) = arm.on_fire.as_ref() {
            cb();
        }
        // Then show the paste menu. We expose CUT / COPY too so
        // the user can still cut/copy the just-selected word.
        // SELECT_ALL is also useful from a long press. The bridge
        // will dim items the field reports as unavailable.
        use crate::widgets::text_edit::edit_menu_actions;
        crate::widgets::text_edit::haptic_impact_light();
        crate::widgets::text_edit::show_edit_menu(
            arm.anchor_x,
            arm.anchor_y,
            arm.anchor_x,
            arm.anchor_y,
            0.0,
            arm.bounds_height,
            edit_menu_actions::PASTE
                | edit_menu_actions::SELECT_ALL
                | edit_menu_actions::COPY
                | edit_menu_actions::CUT,
        );
        true
    } else {
        false
    }
}

static NEEDS_REBUILD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static NEEDS_RELAYOUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static NEEDS_CSS_REPARSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static NEEDS_CONTINUOUS_REDRAW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static FOCUSED_TEXT_INPUT: Mutex<Option<Weak<Mutex<TextInputData>>>> = Mutex::new(None);
static FOCUSED_TEXT_AREA: Mutex<Option<Weak<Mutex<crate::widgets::text_area::TextAreaState>>>> =
    Mutex::new(None);

/// Push a text_area onto the deferred-focus queue.
pub fn enqueue_pending_focus_area(state: Weak<Mutex<crate::widgets::text_area::TextAreaState>>) {
    let generation = PENDING_FOCUS_GENERATION.load(Ordering::Relaxed);
    if let Ok(mut pending) = PENDING_FOCUS_AREA.lock() {
        pending.push((generation, state));
    }
    crate::stateful::request_redraw();
}

/// Pending deferred-focus requests for text inputs.
///
/// `focus_text_input_deferred(&data)` enqueues an entry here. The
/// windowed-app frame loop calls [`process_pending_input_focus`] each
/// tick AFTER the tree-build phase. The processor drains entries whose
/// `TextInput` has been built into the live tree and calls the regular
/// [`focus_text_input`] on them; entries whose widget hasn't mounted
/// yet stay queued for the next frame.
///
/// Use case: an inline editor popover that auto-focuses its input on
/// open. Calling `focus_text_input` BEFORE the popover mounts triggers
/// the `notify_continuous_redraw` chain while the popover is still
/// being added to the tree, which on canvas-backed hosts causes the
/// canvas to render in a broken zoomed-out state for ~4 seconds until
/// a mouse-move forces a re-walk (verified by the 06-14 screen
/// recording). Deferring focus until AFTER mount means the popover
/// settles in the tree first, then focus side-effects apply against a
/// stable composition.
static PENDING_FOCUS_INPUT: Mutex<Vec<(u64, Weak<Mutex<TextInputData>>)>> = Mutex::new(Vec::new());
/// Bumped by [`blur_all_text_inputs`] to cancel deferred focus queued before blur.
static PENDING_FOCUS_GENERATION: AtomicU64 = AtomicU64::new(0);
static PENDING_FOCUS_AREA: Mutex<
    Vec<(u64, Weak<Mutex<crate::widgets::text_area::TextAreaState>>)>,
> = Mutex::new(Vec::new());

/// Callback for setting continuous redraw on the animation scheduler
/// This is set by the windowed app to bridge text widgets with the animation system
#[allow(clippy::type_complexity)]
static CONTINUOUS_REDRAW_CALLBACK: Mutex<Option<Box<dyn Fn(bool) + Send + Sync>>> =
    Mutex::new(None);

/// Tracks whether the soft keyboard should be visible on mobile platforms.
/// Set to `true` when the first text widget gains focus, `false` when all lose focus.
/// Polled by the platform runner (android.rs / ios.rs) in the frame loop.
static KEYBOARD_SHOULD_SHOW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
/// Set when keyboard visibility state changes and needs platform action.
static KEYBOARD_STATE_CHANGED: std::sync::atomic::AtomicBool =
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
fn notify_continuous_redraw(enabled: bool) {
    if let Ok(guard) = CONTINUOUS_REDRAW_CALLBACK.lock() {
        if let Some(ref callback) = *guard {
            callback(enabled);
        }
    }
}

/// Internal function to flag that the soft keyboard visibility should change.
/// The platform runner polls this via `take_keyboard_state_change()`.
fn notify_keyboard_visibility(show: bool) {
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

fn request_continuous_redraw() {
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

/// Repaint a field after its [`TextInputData::value`], `cursor` or
/// `selection_start` was changed from outside the widget, e.g. by the
/// `+` / `−` stepper on `cn::number_input`.
pub fn refresh_text_input(state: &SharedTextInputData) {
    notify_text_input(state);
}

/// The pointer entered or left a field: move its visual state.
fn pointer_crossed(data: &SharedTextInputData, event: u32) {
    let changed = match data.lock() {
        Ok(mut d) if !d.disabled => match d.visual.on_event(event) {
            Some(next) => {
                d.visual = next;
                true
            }
            None => false,
        },
        _ => false,
    };
    if changed {
        notify_text_input(data);
    }
}

/// The field's data changed: have what it shows follow. Called with the data
/// unlocked, since what follows reads it.
pub(crate) fn notify_text_input(state: &SharedTextInputData) {
    let revision = state.lock().ok().and_then(|d| d.revision);
    if let Some(revision) = revision {
        revision.update(|n| n.wrapping_add(1));
    }
    crate::stateful::request_redraw();
}

/// Enqueue a text_input for focus on the NEXT frame, after its widget
/// has been built into the tree.
///
/// Differs from [`focus_text_input`] only in TIMING: the actual focus
/// (visual flip + focus-count bump + tracker registration) runs from
/// [`process_pending_input_focus`] called by the windowed app's frame
/// loop after the tree-build phase. By then any overlay containing the
/// input has been mounted via `rebuild_overlay_subtree_if_dirty`, so
/// the `notify_continuous_redraw` side-effect lands against a stable
/// composition instead of racing the popover mount.
///
/// Pair with the regular [`focus_text_input`] for non-overlay
/// scenarios where the input is already in the tree.
pub fn focus_text_input_deferred(state: &SharedTextInputData) {
    let generation = PENDING_FOCUS_GENERATION.load(Ordering::Relaxed);
    if let Ok(mut pending) = PENDING_FOCUS_INPUT.lock() {
        pending.push((generation, Arc::downgrade(state)));
    }
    crate::stateful::request_redraw();
}

/// Drain the pending-focus queue, applying focus to entries whose
/// widget has been built. Entries whose widget
/// hasn't built yet are re-queued for the next frame. Entries with a
/// stale generation (blurred before they drained) are dropped.
///
/// Called by the windowed frame loop AFTER tree-build phase (after
/// `rebuild_overlay_subtree_if_dirty`) but BEFORE paint, so the focus
/// state flip is visible on the same frame the popover paints — no
/// visual delay between popover appearance and focus indicator.
pub fn process_pending_input_focus() {
    let drained: Vec<(u64, Weak<Mutex<TextInputData>>)> = {
        match PENDING_FOCUS_INPUT.lock() {
            Ok(mut p) => std::mem::take(&mut *p),
            Err(_) => return,
        }
    };
    let current_generation = PENDING_FOCUS_GENERATION.load(Ordering::Relaxed);
    let mut requeue: Vec<(u64, Weak<Mutex<TextInputData>>)> = Vec::new();
    for (generation, weak) in drained {
        if generation != current_generation {
            continue;
        }
        let Some(strong) = weak.upgrade() else {
            continue;
        };
        let mounted = strong
            .lock()
            .ok()
            .map(|d| d.revision.is_some())
            .unwrap_or(false);
        if mounted {
            focus_text_input(&strong);
        } else {
            requeue.push((generation, Arc::downgrade(&strong)));
        }
    }
    if !requeue.is_empty() {
        if let Ok(mut p) = PENDING_FOCUS_INPUT.lock() {
            p.extend(requeue);
        }
        crate::stateful::request_redraw();
    }
}

/// text_area sibling of [`process_pending_input_focus`]. The windowed
/// frame loop calls both back-to-back; either covers its own widget
/// type and the other is a no-op for empty queues.
pub fn process_pending_area_focus() {
    let drained: Vec<(u64, Weak<Mutex<crate::widgets::text_area::TextAreaState>>)> = {
        match PENDING_FOCUS_AREA.lock() {
            Ok(mut p) => std::mem::take(&mut *p),
            Err(_) => return,
        }
    };
    let current_generation = PENDING_FOCUS_GENERATION.load(Ordering::Relaxed);
    let mut requeue: Vec<(u64, Weak<Mutex<crate::widgets::text_area::TextAreaState>>)> = Vec::new();
    for (generation, weak) in drained {
        if generation != current_generation {
            continue;
        }
        let Some(strong) = weak.upgrade() else {
            continue;
        };
        let mounted = strong
            .lock()
            .ok()
            .map(|s| s.stateful_state.is_some())
            .unwrap_or(false);
        if mounted {
            crate::widgets::text_area::focus_text_area(&strong);
        } else {
            requeue.push((generation, Arc::downgrade(&strong)));
        }
    }
    if !requeue.is_empty() {
        if let Ok(mut p) = PENDING_FOCUS_AREA.lock() {
            p.extend(requeue);
        }
        crate::stateful::request_redraw();
    }
}

/// Programmatically focus a text_input by its shared state handle.
///
/// Mirrors what the click handler does internally — sets the widget's
/// visual state to `Focused`, marks it as the active focus target for
/// keyboard / IME / cursor-blink machinery, blurs any previously-focused
/// text_input or text_area, and increments the soft-keyboard refcount.
/// Use from a parent widget's open-handler when an embedded text input
/// should grab focus on appearance (e.g. cn::combobox's search field on
/// dropdown open).
pub fn focus_text_input(state: &SharedTextInputData) {
    use blinc_core::events::event_types;
    let did_change = if let Ok(mut s) = state.lock() {
        if !s.visual.is_focused() {
            if let Some(new_state) = s.visual.on_event(event_types::FOCUS) {
                s.visual = new_state;
            } else {
                s.visual = TextFieldState::Focused;
            }
            s.focus_time_ms = elapsed_ms();
            s.reset_cursor_blink();
            increment_focus_count();
            true
        } else {
            false
        }
    } else {
        false
    };
    if did_change {
        notify_text_input(state);
    }
    set_focused_text_input(state);
}

pub(crate) fn set_focused_text_input(state: &SharedTextInputData) {
    let mut focused = FOCUSED_TEXT_INPUT.lock().unwrap();

    if let Some(weak) = focused.take() {
        if let Some(prev_state) = weak.upgrade() {
            if !Arc::ptr_eq(&prev_state, state) {
                blur_text_input_state(&prev_state);
            }
        }
    }

    blur_focused_text_area();
    *focused = Some(Arc::downgrade(state));
}

fn blur_text_input_state(state: &SharedTextInputData) {
    use blinc_core::events::event_types;

    let did_change = if let Ok(mut s) = state.lock() {
        if let Some(new_state) = s.visual.on_event(event_types::BLUR) {
            s.visual = new_state;
            decrement_focus_count();
            true
        } else {
            false
        }
    } else {
        false
    };
    if did_change {
        notify_text_input(state);
    }
}

pub(crate) fn clear_focused_text_input(state: &SharedTextInputData) {
    let mut focused = FOCUSED_TEXT_INPUT.lock().unwrap();
    if let Some(weak) = focused.as_ref() {
        if let Some(prev_state) = weak.upgrade() {
            if Arc::ptr_eq(&prev_state, state) {
                *focused = None;
            }
        }
    }
}

/// Blur one specific text input, resetting its visual to a non-focused
/// state and repainting it.
pub fn blur_text_input(state: &SharedTextInputData) {
    clear_focused_text_input(state);
    blur_text_input_state(state);
}

pub(crate) fn set_focused_text_area(state: &crate::widgets::text_area::SharedTextAreaState) {
    {
        let mut focused = FOCUSED_TEXT_INPUT.lock().unwrap();
        if let Some(weak) = focused.take() {
            if let Some(prev_state) = weak.upgrade() {
                blur_text_input_state(&prev_state);
            }
        }
    }

    {
        let mut focused = FOCUSED_TEXT_AREA.lock().unwrap();
        if let Some(weak) = focused.take() {
            if let Some(prev_state) = weak.upgrade() {
                if !Arc::ptr_eq(&prev_state, state) {
                    blur_text_area_state(&prev_state);
                }
            }
        }
        *focused = Some(Arc::downgrade(state));
    }
}

fn blur_text_area_state(state: &crate::widgets::text_area::SharedTextAreaState) {
    use blinc_core::events::event_types;

    let mut stateful_to_refresh = None;
    if let Ok(mut s) = state.lock() {
        if let Some(new_state) = s.visual.on_event(event_types::BLUR) {
            s.visual = new_state;
            decrement_focus_count();
        }

        if let Some(ref stateful) = s.stateful_state {
            if let Ok(mut shared) = stateful.lock() {
                if let Some(new_fsm) = shared.state.on_event(event_types::BLUR) {
                    shared.state = new_fsm;
                    shared.needs_visual_update = true;
                    stateful_to_refresh = Some(Arc::clone(stateful));
                }
            }
        }
    }

    if let Some(ref stateful) = stateful_to_refresh {
        refresh_stateful(stateful);
    }
}

pub(crate) fn clear_focused_text_area(state: &crate::widgets::text_area::SharedTextAreaState) {
    let mut focused = FOCUSED_TEXT_AREA.lock().unwrap();
    if let Some(weak) = focused.as_ref() {
        if let Some(prev_state) = weak.upgrade() {
            if Arc::ptr_eq(&prev_state, state) {
                *focused = None;
            }
        }
    }
}

fn blur_focused_text_area() {
    let mut focused = FOCUSED_TEXT_AREA.lock().unwrap();
    if let Some(weak) = focused.take() {
        if let Some(prev_state) = weak.upgrade() {
            blur_text_area_state(&prev_state);
        }
    }
}

/// Blur all focused text inputs and text areas.
/// Called when clicking outside any text element.
pub fn blur_all_text_inputs() {
    use crate::stateful::refresh_stateful;
    use blinc_core::events::event_types;

    // Invalidate any not-yet-drained deferred focus request.
    PENDING_FOCUS_GENERATION.fetch_add(1, Ordering::Relaxed);

    // Run any registered generic-editable blur callback first so
    // widgets that don't fit the typed `text_input` / `text_area`
    // trackers (`code_editor`, `rich_text_editor`) get their local
    // `focused = false` and matching `decrement_focus_count` call
    // when the user taps outside. The callback is taken out of the
    // slot before invocation so a re-entrant blur can't run it
    // twice. After running, `clear_focused_editable_node` zeroes the
    // node id so the scroll-into-view helper doesn't reach for a
    // stale entry on the next frame.
    let editable_blur = {
        if let Ok(mut slot) = FOCUSED_EDITABLE_BLUR_CALLBACK.lock() {
            slot.take()
        } else {
            None
        }
    };
    if let Some(cb) = editable_blur {
        cb();
    }
    clear_focused_editable_node();

    // Blur focused TextInput
    {
        let mut focused = FOCUSED_TEXT_INPUT.lock().unwrap();
        if let Some(weak) = focused.take() {
            if let Some(state) = weak.upgrade() {
                let blurred = if let Ok(mut s) = state.lock() {
                    if s.visual.is_focused() {
                        if let Some(new_state) = s.visual.on_event(event_types::BLUR) {
                            s.visual = new_state;
                            decrement_focus_count();
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if blurred {
                    notify_text_input(&state);
                }
            }
        }
    }

    // Blur focused TextArea
    {
        let mut focused = FOCUSED_TEXT_AREA.lock().unwrap();
        if let Some(weak) = focused.take() {
            if let Some(state) = weak.upgrade() {
                if let Ok(mut s) = state.lock() {
                    if s.visual.is_focused() {
                        if let Some(new_state) = s.visual.on_event(event_types::BLUR) {
                            s.visual = new_state;
                            decrement_focus_count();
                        }
                        // Also update the FSM state to keep in sync
                        let stateful_ref = s.stateful_state.clone();
                        if let Some(ref stateful) = stateful_ref {
                            if let Ok(mut shared) = stateful.lock() {
                                if let Some(new_fsm) = shared.state.on_event(event_types::BLUR) {
                                    shared.state = new_fsm;
                                    shared.needs_visual_update = true;
                                }
                            }
                        }
                        // Trigger visual refresh after releasing the data lock
                        drop(s);
                        if let Some(ref stateful) = stateful_ref {
                            refresh_stateful(stateful);
                        }
                    }
                }
            }
        }
    }
}

/// Get the layout node ID of the currently focused TextInput, if any.
///
/// This allows the CSS styling system to bridge TextInput focus state
/// to the EventRouter, enabling `:focus` pseudo-class matching.
pub fn focused_text_input_node_id() -> Option<LayoutNodeId> {
    let focused = FOCUSED_TEXT_INPUT.lock().ok()?;
    let weak = focused.as_ref()?;
    let data = weak.upgrade()?;
    let guard = data.lock().ok()?;
    guard.node_id
}

/// Get the layout node ID of the currently focused TextArea, if any.
///
/// This allows the CSS styling system to bridge TextArea focus state
/// to the EventRouter, enabling `:focus` pseudo-class matching.
pub fn focused_text_area_node_id() -> Option<LayoutNodeId> {
    let focused = FOCUSED_TEXT_AREA.lock().ok()?;
    let weak = focused.as_ref()?;
    let data = weak.upgrade()?;
    let guard = data.lock().ok()?;
    let stateful = guard.stateful_state.as_ref()?;
    let shared = stateful.lock().ok()?;
    shared.node_id
}

// =============================================================================
// Input Types and Validation
// =============================================================================

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputType {
    #[default]
    Text,
    Number,
    Integer,
    Email,
    Password,
    Url,
    Tel,
    Search,
}

#[derive(Clone, Debug, Default)]
pub struct InputConstraints {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub pattern: Option<String>,
    pub required: bool,
}

impl InputConstraints {
    pub fn max_length(max: usize) -> Self {
        Self {
            max_length: Some(max),
            ..Default::default()
        }
    }

    pub fn required() -> Self {
        Self {
            required: true,
            ..Default::default()
        }
    }

    pub fn number_range(min: f64, max: f64) -> Self {
        Self {
            min_value: Some(min),
            max_value: Some(max),
            ..Default::default()
        }
    }
}

// =============================================================================
// TextInputData - the external state that persists across rebuilds
// =============================================================================

/// Shared text input data handle
pub type SharedTextInputData = Arc<Mutex<TextInputData>>;

/// Text input data (content, cursor, validation)
///
/// The state the field shows, its visual state (hover/focus) included.
#[derive(Clone)]
pub struct TextInputData {
    pub value: String,
    pub cursor: usize,
    pub selection_start: Option<usize>,
    pub placeholder: String,
    pub input_type: InputType,
    pub constraints: InputConstraints,
    pub disabled: bool,
    pub masked: bool,
    pub is_valid: bool,
    pub visual: TextFieldState,
    pub focus_time_ms: u64,
    pub cursor_state: SharedCursorState,
    /// Horizontal scroll offset for text that exceeds the input width
    pub scroll_offset_x: f32,
    /// Computed width of the text input (set after layout, used for scroll calculations)
    /// This is updated when the layout is computed and allows proper scroll behavior
    /// even when `use_full_width` is true.
    pub computed_width: Option<f32>,
    /// Layout bounds storage - updated after each layout computation
    /// Used to get the actual computed width for proper scroll behavior
    pub layout_bounds_storage: crate::renderer::LayoutBoundsStorage,
    /// Bumped whenever something the field shows changes; its content is
    /// bound to it. `None` until a `TextInput` is made over the data.
    pub(crate) revision: Option<blinc_core::reactive::Signal<u64>>,
    /// The field's node, once built.
    pub(crate) node_id: Option<LayoutNodeId>,
    /// Callback invoked when text value changes
    pub(crate) on_change_callback: Option<OnChangeCallback>,
    /// Optional stepper hook fired when the user presses ↑ / ↓ / + / −
    /// while focused. Argument is `+1` for increment, `-1` for
    /// decrement. When set, the keys are *consumed* (the default
    /// behaviour — character insertion for `+` / `−`, no-op for
    /// arrows on a single-line field — is suppressed). Used by
    /// `cn::number_input` to wire keyboard stepping to the bound
    /// `State<f64>`. Unset by default so a plain text input still
    /// accepts a typed `-` as the leading sign of a negative number.
    pub(crate) on_step_callback: Option<Arc<dyn Fn(i32) + Send + Sync>>,
    /// Forces the next render sync to apply even while focused (used by
    /// `cn::number_input`'s stepper hook). Consumed on read.
    pub force_sync_once: bool,
    /// Hook for composite inputs that need empty-Backspace behavior.
    pub(crate) on_backspace_empty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Hook for composite inputs that need to own paste handling.
    pub(crate) on_paste_override_callback: Option<crate::widgets::TextPredicate>,
    /// Hook for composite inputs that need to redirect focus before the
    /// field becomes active.
    pub(crate) on_focus_request_callback: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    /// CSS element ID for stylesheet matching (set via TextInput::id())
    pub(crate) css_element_id: Option<String>,
    /// CSS class names for stylesheet matching (set via TextInput::class())
    pub(crate) css_classes: Vec<std::sync::Arc<str>>,
    /// Last click timestamp for double-click detection
    pub(crate) last_click_time: Option<web_time::Instant>,
    /// Anchor position for drag-to-select
    pub(crate) drag_select_anchor: Option<usize>,
    /// Undo history. Each entry snapshots `(value, cursor, selection_start)`
    /// captured immediately BEFORE a text-mutating operation runs (insert,
    /// delete_*). Cmd+Z pops from this stack onto the redo stack and
    /// restores the popped entry. Capped at [`UNDO_HISTORY_MAX`] entries
    /// — older entries are dropped from the front when the cap is hit.
    pub(crate) undo_stack: Vec<UndoEntry>,
    /// Redo history. Populated by [`Self::undo`] (and cleared by any new
    /// edit since a fresh edit starts a new branch in the history). Cmd+Shift+Z
    /// (or Cmd+Y) pops from this stack onto the undo stack.
    pub(crate) redo_stack: Vec<UndoEntry>,
}

/// One snapshot in the undo / redo history. Stores the full pre-edit
/// state of the text input. We snapshot the entire `value` rather than
/// a diff because the typical input field is short enough that the
/// memory cost is negligible (a 100-entry stack of 80-char strings is
/// ~8 KB).
#[derive(Clone, Debug)]
pub struct UndoEntry {
    pub value: String,
    pub cursor: usize,
    pub selection_start: Option<usize>,
}

/// Maximum number of entries kept in the undo / redo stacks. Once
/// exceeded, the oldest entry is dropped from the front to make room.
const UNDO_HISTORY_MAX: usize = 100;

impl std::fmt::Debug for TextInputData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInputData")
            .field("value", &self.value)
            .field("cursor", &self.cursor)
            .field("selection_start", &self.selection_start)
            .field("placeholder", &self.placeholder)
            .field("input_type", &self.input_type)
            .field("constraints", &self.constraints)
            .field("disabled", &self.disabled)
            .field("masked", &self.masked)
            .field("is_valid", &self.is_valid)
            .field("visual", &self.visual)
            .field("focus_time_ms", &self.focus_time_ms)
            .finish()
    }
}

impl Default for TextInputData {
    fn default() -> Self {
        Self::new()
    }
}

impl TextInputData {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            selection_start: None,
            placeholder: String::new(),
            input_type: InputType::Text,
            constraints: InputConstraints::default(),
            disabled: false,
            masked: false,
            is_valid: true,
            visual: TextFieldState::Idle,
            focus_time_ms: 0,
            cursor_state: cursor_state(),
            scroll_offset_x: 0.0,
            computed_width: None,
            layout_bounds_storage: Arc::new(Mutex::new(None)),
            revision: None,
            node_id: None,
            on_change_callback: None,
            force_sync_once: false,
            on_step_callback: None,
            on_backspace_empty_callback: None,
            on_paste_override_callback: None,
            on_focus_request_callback: None,
            css_element_id: None,
            css_classes: Vec::new(),
            last_click_time: None,
            drag_select_anchor: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Snapshot the current `(value, cursor, selection_start)` triple
    /// onto the undo stack and clear the redo stack. Called from inside
    /// the text-mutating helpers BEFORE they apply their change so the
    /// snapshot represents the pre-edit state.
    ///
    /// New edits invalidate the redo branch — once you type something
    /// after an undo, the path you undid is no longer reachable.
    pub(crate) fn push_undo(&mut self) {
        self.undo_stack.push(UndoEntry {
            value: self.value.clone(),
            cursor: self.cursor,
            selection_start: self.selection_start,
        });
        if self.undo_stack.len() > UNDO_HISTORY_MAX {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Pop the most recent undo entry, push the current state onto the
    /// redo stack, and restore the popped state. Returns `true` if any
    /// state was actually restored (i.e. the undo stack was non-empty).
    pub fn undo(&mut self) -> bool {
        let Some(entry) = self.undo_stack.pop() else {
            return false;
        };
        self.redo_stack.push(UndoEntry {
            value: self.value.clone(),
            cursor: self.cursor,
            selection_start: self.selection_start,
        });
        if self.redo_stack.len() > UNDO_HISTORY_MAX {
            self.redo_stack.remove(0);
        }
        self.value = entry.value;
        self.cursor = entry.cursor;
        self.selection_start = entry.selection_start;
        true
    }

    /// Symmetric inverse of [`Self::undo`]. Returns `true` when the
    /// redo stack had something to apply.
    pub fn redo(&mut self) -> bool {
        let Some(entry) = self.redo_stack.pop() else {
            return false;
        };
        self.undo_stack.push(UndoEntry {
            value: self.value.clone(),
            cursor: self.cursor,
            selection_start: self.selection_start,
        });
        if self.undo_stack.len() > UNDO_HISTORY_MAX {
            self.undo_stack.remove(0);
        }
        self.value = entry.value;
        self.cursor = entry.cursor;
        self.selection_start = entry.selection_start;
        true
    }

    pub fn with_placeholder(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            ..Self::new()
        }
    }

    pub fn with_value(value: impl Into<String>) -> Self {
        let v: String = value.into();
        let cursor = v.chars().count();
        Self {
            value: v,
            cursor,
            ..Self::new()
        }
    }

    /// Get display text (masked for password, or actual value)
    pub fn display_text(&self) -> String {
        if self.masked {
            "•".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        }
    }

    /// Insert text at cursor, respecting input type constraints
    pub fn insert(&mut self, text: &str) {
        // Snapshot for undo BEFORE the mutation. We snapshot
        // unconditionally even if the eventual `filtered` text turns
        // out empty (e.g. typing a non-digit into an InputType::Number)
        // because the cost is one no-op undo entry, and the
        // alternative — snapshotting after filtering — would mean the
        // undo history conflates "I typed nothing" with "I typed
        // something that got dropped", which is the wrong UX.
        self.push_undo();
        // Delete selection first if any
        if let Some(start) = self.selection_start {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            let before: String = self.value.chars().take(from).collect();
            let after: String = self.value.chars().skip(to).collect();
            self.value = before + &after;
            self.cursor = from;
            self.selection_start = None;
        }

        // Filter based on input type
        let filtered: String = match self.input_type {
            InputType::Number => text
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .collect(),
            InputType::Integer => text
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '-')
                .collect(),
            InputType::Tel => text
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '+' || *c == '-' || *c == ' ')
                .collect(),
            _ => text.to_string(),
        };

        if filtered.is_empty() {
            return;
        }

        // Check max length
        if let Some(max) = self.constraints.max_length {
            if self.value.chars().count() + filtered.chars().count() > max {
                return;
            }
        }

        // Insert at cursor
        let before: String = self.value.chars().take(self.cursor).collect();
        let after: String = self.value.chars().skip(self.cursor).collect();
        self.value = before + &filtered + &after;
        self.cursor += filtered.chars().count();

        self.validate();
        // NOTE: Don't call trigger_content_refresh() here - caller must do it
        // after releasing the lock to avoid deadlock
    }

    pub fn delete_backward(&mut self) {
        // Snapshot for undo BEFORE the mutation. We always snapshot
        // even when there's nothing to delete (cursor at 0, no
        // selection) — the resulting no-op undo entry is harmless
        // and keeps the call sites simple.
        self.push_undo();
        if let Some(start) = self.selection_start {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            let before: String = self.value.chars().take(from).collect();
            let after: String = self.value.chars().skip(to).collect();
            self.value = before + &after;
            self.cursor = from;
            self.selection_start = None;
        } else if self.cursor > 0 {
            let before: String = self.value.chars().take(self.cursor - 1).collect();
            let after: String = self.value.chars().skip(self.cursor).collect();
            self.value = before + &after;
            self.cursor -= 1;
        }
        self.validate();
        // NOTE: Don't call trigger_content_refresh() here - caller must do it
        // after releasing the lock to avoid deadlock
    }

    pub fn delete_forward(&mut self) {
        self.push_undo();
        if let Some(start) = self.selection_start {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            let before: String = self.value.chars().take(from).collect();
            let after: String = self.value.chars().skip(to).collect();
            self.value = before + &after;
            self.cursor = from;
            self.selection_start = None;
        } else if self.cursor < self.value.chars().count() {
            let before: String = self.value.chars().take(self.cursor).collect();
            let after: String = self.value.chars().skip(self.cursor + 1).collect();
            self.value = before + &after;
        }
        self.validate();
        // NOTE: Don't call trigger_content_refresh() here - caller must do it
        // after releasing the lock to avoid deadlock
    }

    pub fn move_left(&mut self, shift: bool) {
        if shift {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor);
            }
        } else {
            self.selection_start = None;
        }
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self, shift: bool) {
        if shift {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor);
            }
        } else {
            self.selection_start = None;
        }
        if self.cursor < self.value.chars().count() {
            self.cursor += 1;
        }
    }

    pub fn move_to_start(&mut self, shift: bool) {
        if shift {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor);
            }
        } else {
            self.selection_start = None;
        }
        self.cursor = 0;
    }

    pub fn move_to_end(&mut self, shift: bool) {
        if shift {
            if self.selection_start.is_none() {
                self.selection_start = Some(self.cursor);
            }
        } else {
            self.selection_start = None;
        }
        self.cursor = self.value.chars().count();
    }

    pub fn select_all(&mut self) {
        self.selection_start = Some(0);
        self.cursor = self.value.chars().count();
    }

    pub fn selected_text(&self) -> Option<String> {
        self.selection_start.map(|start| {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            self.value.chars().skip(from).take(to - from).collect()
        })
    }

    /// Move cursor to the previous word boundary
    pub fn move_word_left(&mut self, shift: bool) {
        if shift && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !shift {
            self.selection_start = None;
        }
        self.cursor = crate::widgets::text_edit::word_boundary_left(&self.value, self.cursor);
    }

    /// Move cursor to the next word boundary
    pub fn move_word_right(&mut self, shift: bool) {
        if shift && self.selection_start.is_none() {
            self.selection_start = Some(self.cursor);
        } else if !shift {
            self.selection_start = None;
        }
        self.cursor = crate::widgets::text_edit::word_boundary_right(&self.value, self.cursor);
    }

    /// Delete from cursor to the previous word boundary
    pub fn delete_word_backward(&mut self) {
        // Push BEFORE the early-return delete_selection branch so we
        // don't end up with the selection-delete branch double-pushing
        // (delete_selection also pushes its own undo). Take the
        // selection-delete fast path WITHOUT pushing here, then bail.
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }
        self.push_undo();
        let target = crate::widgets::text_edit::word_boundary_left(&self.value, self.cursor);
        if target < self.cursor {
            let byte_start = self
                .value
                .char_indices()
                .nth(target)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let byte_end = self
                .value
                .char_indices()
                .nth(self.cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.value.len());
            self.value = format!("{}{}", &self.value[..byte_start], &self.value[byte_end..]);
            self.cursor = target;
        }
    }

    /// Delete from cursor to the next word boundary
    pub fn delete_word_forward(&mut self) {
        if self.selection_start.is_some() {
            self.delete_selection();
            return;
        }
        self.push_undo();
        let target = crate::widgets::text_edit::word_boundary_right(&self.value, self.cursor);
        if target > self.cursor {
            let byte_start = self
                .value
                .char_indices()
                .nth(self.cursor)
                .map(|(i, _)| i)
                .unwrap_or(self.value.len());
            let byte_end = self
                .value
                .char_indices()
                .nth(target)
                .map(|(i, _)| i)
                .unwrap_or(self.value.len());
            self.value = format!("{}{}", &self.value[..byte_start], &self.value[byte_end..]);
        }
    }

    /// Delete the current selection, returning true if text changed
    pub fn delete_selection(&mut self) -> bool {
        if self.selection_start.is_none() {
            return false;
        }
        // Snapshot before mutating, but only if there's actually a
        // selection to delete. Calling `push_undo` for a no-op delete
        // would otherwise pollute the history with empty entries.
        self.push_undo();
        let start = self.selection_start.take().expect("checked above");
        let (from, to) = if start < self.cursor {
            (start, self.cursor)
        } else {
            (self.cursor, start)
        };
        let byte_from = self
            .value
            .char_indices()
            .nth(from)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let byte_to = self
            .value
            .char_indices()
            .nth(to)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len());
        self.value = format!("{}{}", &self.value[..byte_from], &self.value[byte_to..]);
        self.cursor = from;
        true
    }

    pub fn validate(&mut self) {
        self.is_valid = match self.input_type {
            InputType::Email => {
                self.value.is_empty() || (self.value.contains('@') && self.value.contains('.'))
            }
            InputType::Number => self.value.is_empty() || self.value.parse::<f64>().is_ok(),
            InputType::Integer => self.value.is_empty() || self.value.parse::<i64>().is_ok(),
            InputType::Url => {
                self.value.is_empty()
                    || self.value.starts_with("http://")
                    || self.value.starts_with("https://")
            }
            _ => true,
        };

        if self.constraints.required && self.value.is_empty() {
            self.is_valid = false;
        }

        if let Some(min) = self.constraints.min_length {
            if self.value.len() < min {
                self.is_valid = false;
            }
        }
    }

    pub fn reset_cursor_blink(&mut self) {
        if let Ok(mut cs) = self.cursor_state.lock() {
            cs.reset_blink();
        }
    }

    pub fn sync_global_selection(&self) {
        if let Some(start) = self.selection_start {
            if start != self.cursor {
                let (from, to) = if start < self.cursor {
                    (start, self.cursor)
                } else {
                    (self.cursor, start)
                };
                let selected: String = self.value.chars().skip(from).take(to - from).collect();
                set_selection(selected, SelectionSource::TextInput, true);
            } else {
                clear_selection();
            }
        } else {
            clear_selection();
        }
    }

    /// Calculate cursor position from x coordinate (relative to text content area)
    ///
    /// This is used for click-to-position cursor functionality.
    /// The x coordinate should be relative to the start of the text content (after padding).
    pub fn cursor_position_from_x(&self, x: f32, font_size: f32) -> usize {
        let display = self.display_text();
        if display.is_empty() {
            return 0;
        }

        // Account for scroll offset - the click x is in viewport space,
        // so add scroll_offset to get position in text space
        let text_x = x + self.scroll_offset_x;

        // Binary search would be more efficient, but for typical text input lengths,
        // linear search is fast enough
        let char_count = display.chars().count();
        let mut best_pos = 0;
        let mut min_dist = f32::MAX;

        // Check position before each character and after the last
        for i in 0..=char_count {
            let prefix: String = display.chars().take(i).collect();
            let prefix_width = crate::text_measure::measure_text(&prefix, font_size).width;

            let dist = (prefix_width - text_x).abs();
            if dist < min_dist {
                min_dist = dist;
                best_pos = i;
            }
        }

        best_pos
    }

    /// Ensure the cursor is visible by adjusting horizontal scroll offset.
    /// This implements HTML-like behavior where text scrolls left when typing
    /// extends beyond the visible width.
    pub fn ensure_cursor_visible(&mut self, config: &TextInputConfig) {
        // Try to get computed width from layout bounds storage first
        // This is updated after each layout computation
        let layout_width = self
            .layout_bounds_storage
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|b| b.width));

        // Use layout width if available, otherwise fall back to stored computed_width
        let effective_computed_width = layout_width.or(self.computed_width);

        // For full-width inputs without computed bounds yet, don't scroll
        // This prevents incorrect scrolling before we have the real container width
        if config.use_full_width && effective_computed_width.is_none() {
            self.scroll_offset_x = 0.0;
            return;
        }

        // Calculate total text width
        let display = self.display_text();
        let total_text_width = if !display.is_empty() {
            crate::text_measure::measure_text(&display, config.font_size).width
        } else {
            0.0
        };

        // Calculate cursor x position (where cursor is in the full text)
        let cursor_x = if self.cursor > 0 && !display.is_empty() {
            let text_before: String = display.chars().take(self.cursor).collect();
            crate::text_measure::measure_text(&text_before, config.font_size).width
        } else {
            0.0
        };

        // Calculate available width for text (the visible viewport)
        // Use computed_width if available (set after layout), otherwise fall back to config.width
        // Account for padding on both sides and border
        let base_width = effective_computed_width.unwrap_or(config.width);
        let available_width = base_width - config.padding_x * 2.0 - config.border_width * 2.0;

        // Simple approach: measure if text exceeds viewport
        // If cursor is past the visible right edge, scroll to show cursor
        let visible_right = self.scroll_offset_x + available_width;
        let cursor_margin = 4.0; // Small margin so cursor isn't at the very edge

        if cursor_x > visible_right - cursor_margin {
            // Cursor is past the right edge - scroll right to show it
            self.scroll_offset_x = cursor_x - available_width + cursor_margin;
        } else if cursor_x < self.scroll_offset_x {
            // Cursor is past the left edge - scroll left to show it
            self.scroll_offset_x = cursor_x;
        }

        // Clamp: can't scroll past start, and don't scroll more than necessary
        self.scroll_offset_x = self.scroll_offset_x.max(0.0);

        // Also clamp max scroll so we don't scroll past the end of text
        let max_scroll = (total_text_width - available_width + cursor_margin).max(0.0);
        self.scroll_offset_x = self.scroll_offset_x.min(max_scroll);
    }
}

/// Create a shared text input data
pub fn text_input_data() -> SharedTextInputData {
    Arc::new(Mutex::new(TextInputData::new()))
}

/// Create a shared text input data with placeholder
pub fn text_input_data_with_placeholder(placeholder: impl Into<String>) -> SharedTextInputData {
    Arc::new(Mutex::new(TextInputData::with_placeholder(placeholder)))
}

// Backwards compatibility aliases
pub type TextInputState = TextInputData;
pub type SharedTextInputState = SharedTextInputData;

pub fn text_input_state() -> SharedTextInputData {
    text_input_data()
}

pub fn text_input_state_with_placeholder(placeholder: impl Into<String>) -> SharedTextInputData {
    text_input_data_with_placeholder(placeholder)
}

// =============================================================================
// CSS Override Resolution
// =============================================================================

/// Apply CSS stylesheet overrides to a TextInputConfig.
///
/// Resolves base styles, state-specific styles (:hover/:focus/:disabled),
/// and ::placeholder styles from the active stylesheet, then mutates the
/// config with any CSS-specified values.
fn apply_css_overrides(
    cfg: &mut TextInputConfig,
    stylesheet: &Stylesheet,
    element_id: Option<&str>,
    css_classes: &[std::sync::Arc<str>],
    visual: &TextFieldState,
) {
    // Determine the element state for state-specific lookups
    let state = match visual {
        TextFieldState::Hovered | TextFieldState::FocusedHovered => Some(ElementState::Hover),
        TextFieldState::Focused => Some(ElementState::Focus),
        TextFieldState::Disabled => Some(ElementState::Disabled),
        TextFieldState::Idle => None,
    };

    // 1. Apply class-based styles (lowest priority — overridden by ID).
    //
    // Order for FocusedHovered: base → :hover → :focus. `:focus` must run
    // LAST so its `border-color` wins over `:hover`'s — without this, a
    // hovered focused input falls back to the hover (grey) border while
    // the user is still typing in it. The user's intent on hover-while-
    // focused is "I'm interacting with this input" → focus colour wins.
    for class in css_classes {
        if let Some(base) = stylesheet.get_class(class) {
            apply_style_to_config(cfg, base, visual);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_class_with_state(class, s) {
                apply_style_to_config(cfg, state_style, visual);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(s) = stylesheet.get_class_with_state(class, ElementState::Focus) {
                apply_style_to_config(cfg, s, visual);
            }
        }
    }

    // 2. Apply ID-based styles (higher priority — overrides class).
    // Same hover-then-focus ordering as the class branch above.
    if let Some(element_id) = element_id {
        if let Some(base) = stylesheet.get(element_id) {
            apply_style_to_config(cfg, base, visual);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_with_state(element_id, s) {
                apply_style_to_config(cfg, state_style, visual);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(focus_style) = stylesheet.get_with_state(element_id, ElementState::Focus) {
                apply_style_to_config(cfg, focus_style, visual);
            }
        }

        // 3. Apply ::placeholder style (ID-only)
        if let Some(placeholder_style) = stylesheet.get_placeholder_style(element_id) {
            if let Some(color) = placeholder_style.text_color {
                cfg.placeholder_color = color;
            }
            if let Some(color) = placeholder_style.placeholder_color {
                cfg.placeholder_color = color;
            }
        }
    }
}

/// Apply an ElementStyle to a TextInputConfig (CSS properties override config values)
fn apply_style_to_config(
    cfg: &mut TextInputConfig,
    style: &crate::element_style::ElementStyle,
    visual: &TextFieldState,
) {
    // Background → applies to current state's bg color
    if let Some(ref bg) = style.background {
        let color = match bg {
            blinc_core::Brush::Solid(c) => *c,
            _ => return, // Gradients not supported for input bg
        };
        match visual {
            TextFieldState::Idle => cfg.bg_color = color,
            TextFieldState::Hovered => cfg.hover_bg_color = color,
            TextFieldState::Focused | TextFieldState::FocusedHovered => {
                cfg.focused_bg_color = color;
            }
            TextFieldState::Disabled => {} // Disabled bg is hardcoded
        }
    }

    // Border color
    if let Some(color) = style.border_color {
        match visual {
            TextFieldState::Idle => cfg.border_color = color,
            TextFieldState::Hovered => cfg.hover_border_color = color,
            TextFieldState::Focused | TextFieldState::FocusedHovered => {
                cfg.focused_border_color = color;
            }
            TextFieldState::Disabled => {}
        }
    }

    // Border width
    if let Some(w) = style.border_width {
        cfg.border_width = w;
    }

    // Corner radius
    if let Some(cr) = style.corner_radius {
        cfg.corner_radius = cr.top_left; // Use uniform radius
    }

    // Text color
    if let Some(color) = style.text_color {
        cfg.text_color = color;
    }

    // Font size
    if let Some(size) = style.font_size {
        cfg.font_size = size;
    }

    // Caret (cursor) color
    if let Some(color) = style.caret_color {
        cfg.cursor_color = color;
    }

    // Selection color
    if let Some(color) = style.selection_color {
        cfg.selection_color = color;
    }

    // Placeholder color
    if let Some(color) = style.placeholder_color {
        cfg.placeholder_color = color;
    }
}

/// Extract outline properties from stylesheet for the current state.
/// Returns (width, color, offset) if any outline is specified.
///
/// Walks class selectors first (lowest priority) and then `#id`
/// selectors (overrides), matching the precedence rules
/// `apply_css_overrides` uses for the rest of the input's style.
/// Without the class branch, rules like `.cn-input:focus { outline:
/// 2px solid var(--border-focus); }` from the cn stylesheet would
/// silently no-op because cn::input attaches via class, not id.
fn extract_outline_from_stylesheet(
    stylesheet: &Stylesheet,
    element_id: Option<&str>,
    css_classes: &[std::sync::Arc<str>],
    visual: &TextFieldState,
) -> Option<(f32, Color, f32)> {
    let mut width = None;
    let mut color = None;
    let mut offset = None;

    let state = match visual {
        TextFieldState::Hovered | TextFieldState::FocusedHovered => Some(ElementState::Hover),
        TextFieldState::Focused => Some(ElementState::Focus),
        TextFieldState::Disabled => Some(ElementState::Disabled),
        TextFieldState::Idle => None,
    };

    let mut absorb = |s: &crate::element_style::ElementStyle| {
        if let Some(w) = s.outline_width {
            width = Some(w);
        }
        if let Some(c) = s.outline_color {
            color = Some(c);
        }
        if let Some(o) = s.outline_offset {
            offset = Some(o);
        }
    };

    // 1. Class-based styles (lowest priority — overridden by ID).
    // FocusedHovered order: base → :hover → :focus, so the focus ring
    // wins over hover (same rationale as apply_css_overrides above).
    for class in css_classes {
        if let Some(base) = stylesheet.get_class(class) {
            absorb(base);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_class_with_state(class, s) {
                absorb(state_style);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(s) = stylesheet.get_class_with_state(class, ElementState::Focus) {
                absorb(s);
            }
        }
    }

    // 2. ID-based styles (overrides class)
    if let Some(element_id) = element_id {
        if let Some(base) = stylesheet.get(element_id) {
            absorb(base);
        }
        if let Some(s) = state {
            if let Some(state_style) = stylesheet.get_with_state(element_id, s) {
                absorb(state_style);
            }
        }
        if matches!(visual, TextFieldState::FocusedHovered) {
            if let Some(focus_style) = stylesheet.get_with_state(element_id, ElementState::Focus) {
                absorb(focus_style);
            }
        }
    }

    // Only return if at least width is specified
    width.map(|w| {
        (
            w,
            color.unwrap_or(Color::rgba(0.23, 0.51, 0.97, 0.5)),
            offset.unwrap_or(0.0),
        )
    })
}

// =============================================================================
// TextInputConfig - visual configuration
// =============================================================================

#[derive(Clone, Debug)]
pub struct TextInputConfig {
    pub width: f32,
    pub height: f32,
    pub use_full_width: bool,
    pub font_size: f32,
    pub text_color: Color,
    pub placeholder_color: Color,
    pub bg_color: Color,
    pub hover_bg_color: Color,
    pub focused_bg_color: Color,
    pub border_color: Color,
    pub hover_border_color: Color,
    pub focused_border_color: Color,
    pub error_border_color: Color,
    pub cursor_color: Color,
    pub selection_color: Color,
    pub corner_radius: f32,
    pub border_width: f32,
    pub padding_x: f32,
    pub placeholder: String,
    /// Horizontal alignment of the visible text inside the field.
    /// Default `Left`. `Center` is the canonical choice for
    /// numeric / OTP-style fields where the value should sit
    /// visually centred in a fixed-width cell.
    pub text_align: blinc_core::TextAlign,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        let theme = ThemeState::get();
        Self {
            width: 200.0,
            height: 44.0,
            use_full_width: false,
            font_size: 16.0,
            text_color: theme.color(ColorToken::TextPrimary),
            placeholder_color: theme.color(ColorToken::TextTertiary),
            bg_color: theme.color(ColorToken::InputBg),
            hover_bg_color: theme.color(ColorToken::InputBgHover),
            focused_bg_color: theme.color(ColorToken::InputBgFocus),
            border_color: theme.color(ColorToken::BorderSecondary),
            hover_border_color: theme.color(ColorToken::BorderHover),
            focused_border_color: theme.color(ColorToken::BorderFocus),
            error_border_color: theme.color(ColorToken::BorderError),
            cursor_color: theme.color(ColorToken::Accent),
            selection_color: theme.color(ColorToken::Selection),
            corner_radius: 8.0,
            border_width: 1.5,
            padding_x: 12.0,
            placeholder: String::new(),
            text_align: blinc_core::TextAlign::Left,
        }
    }
}

// =============================================================================
// TextInput Widget
// =============================================================================

/// Callback type for on_change events
pub type OnChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// A single-line text field.
///
/// Built once. Its handlers edit the shared [`TextInputData`] and bump its
/// revision; what it shows (the text, the selection, the caret, the scroll,
/// and the fill and border for its state) is bound to that revision, so
/// typing, clicking and dragging patch the field in place.
pub struct TextInput {
    /// The element as configured so far: layout calls and the handlers land
    /// on it as they are made. The content is added when it is first built.
    inner: std::cell::RefCell<Div>,
    built: std::cell::OnceCell<Div>,
    data: SharedTextInputData,
    config: Arc<Mutex<TextInputConfig>>,
    /// Callback invoked when text value changes
    on_change_callback: Option<OnChangeCallback>,
    /// Caller's handle onto this field, if bound.
    element_ref: Option<crate::selector::DivRef>,
}

impl TextInput {
    /// Bind an [`crate::selector::InputRef`] to this field.
    ///
    /// The value half is handed over now — the field is built from
    /// state its caller already holds, so nothing has to wait for a
    /// render. The element half is resolved by the renderer when the
    /// field is built, the same as any other bound ref.
    pub fn bind(mut self, input_ref: &crate::selector::InputRef) -> Self {
        input_ref.bind_data(&self.data);
        self.element_ref = Some(input_ref.element().clone());
        self
    }

    /// Create a text input with externally-managed data state
    pub fn new(data: SharedTextInputData) -> Self {
        let config = Arc::new(Mutex::new(TextInputConfig::default()));
        // A field with a revision is one a deferred focus can land on.
        if let Ok(mut d) = data.lock() {
            d.revision
                .get_or_insert_with(|| blinc_core::reactive::signal(0u64));
        }
        let mut inner = Self::create_inner_with_handlers(Arc::clone(&data), Arc::clone(&config));

        // HTML input behaviour in flex layouts: stretch to the parent's
        // width by default, and `min_w(0)` so the field can shrink below its
        // content. The inner clip container does the clipping; clipping here
        // would cut into the rounded border.
        {
            let cfg = config.lock().unwrap();
            if cfg.use_full_width {
                inner = inner.w_full();
            }
            inner = inner.h(cfg.height).min_w(0.0);
        }

        Self {
            inner: std::cell::RefCell::new(inner),
            built: std::cell::OnceCell::new(),
            data,
            config,
            on_change_callback: None,
            element_ref: None,
        }
    }

    /// The element, finished on first use: the content and the bound looks
    /// added to what the builder calls made.
    fn element(&self) -> &Div {
        crate::build_once::build_once(&self.built, || {
            let outer = self.inner.take();
            let cfg = self.config.lock().unwrap().clone();
            assemble(outer, &self.data, &cfg)
        })
    }

    /// Create the element with all the event handlers registered
    fn create_inner_with_handlers(
        data: SharedTextInputData,
        config: Arc<Mutex<TextInputConfig>>,
    ) -> Div {
        use blinc_core::events::event_types;

        let data_for_click = Arc::clone(&data);
        let data_for_drag = Arc::clone(&data);
        let config_for_drag = Arc::clone(&config);
        let data_for_text = Arc::clone(&data);
        let data_for_key = Arc::clone(&data);
        let config_for_click = Arc::clone(&config);
        let data_for_enter = Arc::clone(&data);
        let data_for_leave = Arc::clone(&data);

        div()
            .w_full()
            // The pointer over the field changes its look.
            .on_hover_enter(move |_| pointer_crossed(&data_for_enter, event_types::POINTER_ENTER))
            .on_hover_leave(move |_| pointer_crossed(&data_for_leave, event_types::POINTER_LEAVE))
            // Handle mouse down to focus and position cursor
            .on_mouse_down(move |ctx| {
                let needs_refresh = {
                    let mut d = match data_for_click.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled {
                        return;
                    }

                    if let Some(cb) = d.on_focus_request_callback.as_ref().map(Arc::clone) {
                        drop(d);
                        if cb() {
                            return;
                        }
                        d = match data_for_click.lock() {
                            Ok(d) => d,
                            Err(_) => return,
                        };
                    }

                    // Bump the focus-tap generation counter so the
                    // mobile runner picks this up as a "user tapped a
                    // text input" event, even if the input was already
                    // focused. This drives scroll-into-view on re-taps
                    // — see `focus_tap_generation` for the rationale
                    // and `blinc_app::android::android_main` /
                    // `blinc_app::ios::blinc_build_frame` for the
                    // consumers.
                    bump_focus_tap_generation();

                    // Register this node id as the generic
                    // focused-editable. The scroll-into-view helper
                    // consults this when the typed
                    // `focused_text_input_node_id` lookup is empty
                    // (e.g. for code_editor / rich_text_editor); we
                    // populate it here too so a single lookup site
                    // covers every editable widget.
                    //
                    // No blur callback because text_input has its own
                    // dedicated `FOCUSED_TEXT_INPUT` tracker that
                    // `blur_all_text_inputs` walks via the typed path
                    // — passing a callback here would cause double
                    // blur.
                    set_focused_editable_node(ctx.node_id, None);

                    // Get font size + the horizontal offset of the
                    // text content area inside the widget bounds.
                    // The widget renders a `padding_x`-wide spacer
                    // before the clip container that holds the text
                    // (see `content`), and the
                    // field's border adds another
                    // `border_width` on the left edge — so the very
                    // first glyph sits at
                    // `local_x = padding_x + border_width`, NOT at
                    // `local_x = 0`. Without subtracting this offset
                    // before calling `cursor_position_from_x`, every
                    // click is shifted right by ~13.5px and the very
                    // first character is unreachable: clicking on the
                    // "H" of "Hello World" lands a cursor position
                    // PAST the H, so a drag-select that starts there
                    // misses the first character.
                    let (font_size, text_origin_x, is_centered_align) = {
                        let cfg = config_for_click.lock().unwrap();
                        (
                            cfg.font_size,
                            cfg.padding_x + cfg.border_width,
                            matches!(cfg.text_align, blinc_core::TextAlign::Center),
                        )
                    };

                    // Update data state
                    if !d.visual.is_focused() {
                        d.visual = TextFieldState::Focused;
                        d.focus_time_ms = elapsed_ms();
                        d.reset_cursor_blink();
                        increment_focus_count();
                        set_focused_text_input(&data_for_click);
                        request_continuous_redraw();
                    }

                    // Store computed width from layout bounds for scroll calculations
                    // This allows ensure_cursor_visible to work correctly with w_full() inputs
                    if ctx.bounds_width > 0.0 {
                        d.computed_width = Some(ctx.bounds_width);
                    }

                    // Calculate cursor position from click x position.
                    // Translate widget-local x into text-content x by
                    // subtracting the left padding + border, then clamp
                    // to >= 0 so clicks in the padding gutter snap to
                    // the start of the text instead of going negative.
                    //
                    // EXCEPTION for centred text: the visible text
                    // doesn't start at `text_origin_x` — it sits at
                    // `text_origin_x + (clip_w - text_w) / 2` because
                    // the layout flex-centres the content. We can't
                    // know `clip_w - text_w` here without runtime
                    // measurement, and the value clicked is almost
                    // always a "select everything then retype" gesture
                    // anyway (canonical numeric-input UX). So when
                    // `text_align == Center`, single-clicking the
                    // field selects the whole value and parks the
                    // cursor at the end — subsequent typing replaces
                    // the value. Matches `<input type=number>` in most
                    // browsers + the HIG numeric-input spec. `text_x`
                    // is still computed so the touch edit-menu anchors
                    // and on-drag math below stay correct.
                    let text_x = (ctx.local_x - text_origin_x).max(0.0);
                    let cursor_pos = if is_centered_align {
                        d.value.chars().count()
                    } else {
                        d.cursor_position_from_x(text_x, font_size)
                    };

                    // Double-click detection (select word)
                    let now = web_time::Instant::now();
                    let is_double_click = d
                        .last_click_time
                        .map(|t| now.duration_since(t).as_millis() < 400)
                        .unwrap_or(false);
                    d.last_click_time = Some(now);

                    let touch = is_touch_input();

                    if is_double_click {
                        // Select word at cursor — same on touch and
                        // mouse. On touch we additionally fire an
                        // impact haptic and ask the platform to show
                        // the native edit menu (Cut / Copy / Paste /
                        // Select All) anchored at the tap position.
                        let (start, end) =
                            crate::widgets::text_edit::word_at_position(&d.value, cursor_pos);
                        d.selection_start = Some(start);
                        d.cursor = end;
                        if touch {
                            crate::widgets::text_edit::haptic_impact_light();
                            // Show edit menu — actions reflect the
                            // current state. There IS a selection
                            // (the just-selected word) so Cut / Copy
                            // are available; SELECT_ALL is always
                            // valid; PASTE depends on clipboard
                            // contents but we let the native side
                            // figure that out (it'll dim the menu
                            // item if the system clipboard is empty).
                            use crate::widgets::text_edit::edit_menu_actions;
                            crate::widgets::text_edit::show_edit_menu(
                                ctx.bounds_x + text_x,
                                ctx.bounds_y,
                                ctx.bounds_x + text_x,
                                ctx.bounds_y,
                                0.0,
                                ctx.bounds_height,
                                edit_menu_actions::CUT
                                    | edit_menu_actions::COPY
                                    | edit_menu_actions::PASTE
                                    | edit_menu_actions::SELECT_ALL,
                            );
                        }
                    } else if is_centered_align {
                        // Centred-align single-click: select everything
                        // and park the cursor at the end. Matches the
                        // `<input type=number>` browser convention —
                        // tapping the field selects the value so the
                        // next keystroke replaces it. No drag-select
                        // anchor (centred fields don't support drag
                        // selection because cursor_position_from_x
                        // can't be trusted under centred layout).
                        d.selection_start = Some(0);
                        d.cursor = cursor_pos;
                        d.drag_select_anchor = None;
                    } else {
                        // Single click: position cursor. On touch, we
                        // do NOT start a drag-select anchor — touch
                        // drag is repurposed for cursor movement (see
                        // the on_drag handler below). On mouse,
                        // single-click + drag extends selection just
                        // like the desktop UX expects.
                        d.cursor = cursor_pos;
                        d.selection_start = None;
                        if touch {
                            d.drag_select_anchor = None;
                            // Subtle haptic on single-tap focus, mirroring
                            // iOS UITextField's selection feedback.
                            crate::widgets::text_edit::haptic_selection();
                            // Hide any leftover edit menu from a
                            // previous double-tap so the user gets a
                            // clean re-engagement.
                            crate::widgets::text_edit::hide_edit_menu();
                            // Arm the long-press timer. The platform
                            // runner's frame loop polls
                            // `fire_long_press_timer_if_due` each
                            // tick, and after 500 ms (cancelled by
                            // any drift past 10 px or by mouse_up)
                            // it shows the edit menu with PASTE
                            // available — matching the iOS
                            // UITextField / Android EditText
                            // long-press-to-paste UX.
                            // Capture the data for the long-press
                            // callback. The closure runs at
                            // deadline-fire time and selects the
                            // word at the captured cursor position,
                            // matching the double-tap UX.
                            let data_for_long_press = std::sync::Arc::clone(&data_for_click);
                            let captured_cursor = cursor_pos;
                            arm_long_press_timer(
                                ctx.bounds_x + text_x,
                                ctx.bounds_y,
                                ctx.bounds_height,
                                Some(Box::new(move || {
                                    let did_update = {
                                        let mut d = match data_for_long_press.lock() {
                                            Ok(d) => d,
                                            Err(_) => return,
                                        };
                                        if !d.visual.is_focused() {
                                            return;
                                        }
                                        let (start, end) =
                                            crate::widgets::text_edit::word_at_position(
                                                &d.value,
                                                captured_cursor,
                                            );
                                        if start == end {
                                            return;
                                        }
                                        d.selection_start = Some(start);
                                        d.cursor = end;
                                        true
                                    };
                                    if did_update {
                                        notify_text_input(&data_for_long_press);
                                    }
                                })),
                            );
                        } else {
                            d.drag_select_anchor = Some(cursor_pos);
                        }
                    }
                    d.reset_cursor_blink();

                    true
                };

                if needs_refresh {
                    notify_text_input(&data_for_click);
                }
            })
            // Mouse drag to extend selection
            .on_drag({
                move |ctx| {
                    let needs_refresh = {
                        let mut d = match data_for_drag.lock() {
                            Ok(d) => d,
                            Err(_) => return,
                        };
                        if !d.visual.is_focused() {
                            return;
                        }

                        // Mirror the offset translation in
                        // on_mouse_down: convert widget-local x into
                        // text-content x by subtracting the left
                        // padding + border before mapping to a
                        // character index. Without this, the drag
                        // would cover one character less than the
                        // mouse-down anchor on the very first
                        // character.
                        let (font_size, text_origin_x) = {
                            let cfg = config_for_drag.lock().unwrap();
                            (cfg.font_size, cfg.padding_x + cfg.border_width)
                        };
                        let text_x = (ctx.local_x - text_origin_x).max(0.0);
                        let new_pos = d.cursor_position_from_x(text_x, font_size);

                        // Touch input branches its drag semantics:
                        //
                        //   * Mouse drag (desktop / web) — extends
                        //     selection from the anchor recorded by
                        //     `on_mouse_down`. Same behavior as
                        //     `text_input` has always had.
                        //
                        //   * Touch drag (mobile) — moves the caret
                        //     to wherever the finger is, without
                        //     starting a selection. Each character
                        //     boundary crossed gets a subtle
                        //     selection-changed haptic, mirroring the
                        //     UITextField / Android EditText cursor-
                        //     drag UX. Selection is reserved for
                        //     double-tap and the native edit menu.
                        //
                        // The branch is gated on
                        // `text_input::is_touch_input()`, which the
                        // platform runners flip on every touch /
                        // mouse event.
                        if is_touch_input() {
                            // Cancel any armed long-press as soon as
                            // the finger drifts past the threshold —
                            // a real drag should not also fire the
                            // paste menu mid-gesture.
                            check_long_press_drift(ctx.mouse_x, ctx.mouse_y);
                            if new_pos != d.cursor {
                                d.cursor = new_pos;
                                d.selection_start = None;
                                crate::widgets::text_edit::haptic_selection();
                            }
                        } else if let Some(anchor) = d.drag_select_anchor {
                            if new_pos != anchor {
                                d.selection_start = Some(anchor);
                                d.cursor = new_pos;
                            }
                        }
                        true
                    };
                    if needs_refresh {
                        notify_text_input(&data_for_drag);
                    }
                }
            })
            // Handle text input
            .on_event(event_types::TEXT_INPUT, move |ctx| {
                let (needs_refresh, callback_info) = {
                    let mut d = match data_for_text.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled || !d.visual.is_focused() {
                        return;
                    }

                    if let Some(c) = ctx.key_char {
                        // Stepper hook: `+` / `−` (or `=` which sits
                        // on the same physical key on US layouts) step
                        // the value when `on_step` is registered.
                        // Skip the character insert so the field
                        // doesn't end up with stray `+` chars in the
                        // value buffer.
                        if matches!(c, '+' | '-' | '=') {
                            if let Some(cb) = d.on_step_callback.as_ref().map(Arc::clone) {
                                let delta = if c == '-' { -1 } else { 1 };
                                drop(d);
                                cb(delta);
                                return;
                            }
                        }
                        d.insert(&c.to_string());
                        d.reset_cursor_blink();
                        tracing::debug!("TextInput received char: {:?}, value: {}", c, d.value);
                        // Extract callback and value for calling after lock release
                        let cb_info = d
                            .on_change_callback
                            .as_ref()
                            .map(|cb| (Arc::clone(cb), d.value.clone()));
                        (true, cb_info)
                    } else {
                        (false, None)
                    }
                }; // Lock released here

                // Callbacks may normalize data before the refresh reads it.
                if let Some((callback, new_value)) = callback_info {
                    callback(&new_value);
                }

                if needs_refresh {
                    notify_text_input(&data_for_text);
                }
            })
            // Handle key down for navigation and deletion
            .on_key_down(move |ctx| {
                let (needs_refresh, callback_info) = {
                    let mut d = match data_for_key.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled || !d.visual.is_focused() {
                        return;
                    }

                    let mut changed = true;
                    let mut should_blur = false;
                    let mut value_changed = false;
                    let mod_key = ctx.meta || ctx.ctrl;

                    match ctx.key_code {
                        8 if mod_key => {
                            // Cmd+Backspace: delete word backward
                            d.delete_word_backward();
                            value_changed = true;
                        }
                        8 if d.value.is_empty() && d.on_backspace_empty_callback.is_some() => {
                            // OTP rewinds focus when Backspace hits an empty slot.
                            if let Some(cb) = d.on_backspace_empty_callback.as_ref().map(Arc::clone)
                            {
                                drop(d);
                                cb();
                            }
                            return;
                        }
                        8 => {
                            // Backspace
                            if d.selection_start.is_some() {
                                d.delete_selection();
                            } else {
                                d.delete_backward();
                            }
                            value_changed = true;
                        }
                        127 if mod_key => {
                            // Cmd+Delete: delete word forward
                            d.delete_word_forward();
                            value_changed = true;
                        }
                        127 => {
                            // Delete
                            if d.selection_start.is_some() {
                                d.delete_selection();
                            } else {
                                d.delete_forward();
                            }
                            value_changed = true;
                        }
                        37 if mod_key => d.move_word_left(ctx.shift), // Cmd+Left
                        39 if mod_key => d.move_word_right(ctx.shift), // Cmd+Right
                        37 => d.move_left(ctx.shift),                 // Left
                        39 => d.move_right(ctx.shift),                // Right
                        36 => d.move_to_start(ctx.shift),             // Home
                        35 => d.move_to_end(ctx.shift),               // End
                        // ↑ / ↓: stepper hook if registered, else
                        // no-op on a single-line input.
                        38 | 40 if d.on_step_callback.is_some() => {
                            let delta = if ctx.key_code == 38 { 1 } else { -1 };
                            if let Some(cb) = d.on_step_callback.as_ref().map(Arc::clone) {
                                drop(d);
                                cb(delta);
                            }
                            return;
                        }
                        27 => {
                            should_blur = true;
                        }
                        _ if mod_key => {
                            match ctx.key_code {
                                // Cmd+A: select all
                                65 => d.select_all(),
                                // Cmd+C: copy
                                67 => {
                                    if let Some(text) = d.selected_text() {
                                        crate::widgets::text_edit::clipboard_write(&text);
                                    }
                                    changed = true;
                                }
                                // Cmd+X: cut
                                88 => {
                                    if let Some(text) = d.selected_text() {
                                        crate::widgets::text_edit::clipboard_write(&text);
                                        d.delete_selection();
                                        value_changed = true;
                                    }
                                }
                                // Cmd+V: paste
                                86 => {
                                    if let Some(clip) = crate::widgets::text_edit::clipboard_read()
                                    {
                                        // Remove newlines for single-line input
                                        let clean: String = clip
                                            .chars()
                                            .filter(|c| *c != '\n' && *c != '\r')
                                            .collect();
                                        if !clean.is_empty() {
                                            // Drop the lock before calling out — the
                                            // override (input_otp) may need to re-lock
                                            // this same slot's data.
                                            let override_cb = d.on_paste_override_callback.clone();
                                            if let Some(cb) = override_cb {
                                                drop(d);
                                                let handled = cb(&clean);
                                                if handled {
                                                    return;
                                                }
                                                d = match data_for_key.lock() {
                                                    Ok(d) => d,
                                                    Err(_) => return,
                                                };
                                            }
                                            if d.selection_start.is_some() {
                                                d.delete_selection();
                                            }
                                            d.insert(&clean);
                                            value_changed = true;
                                        }
                                    }
                                }
                                // Cmd+Z: undo. Cmd+Shift+Z and Cmd+Y
                                // are both treated as redo (the two
                                // are mutually exclusive convention-
                                // wise on macOS vs Windows, so we
                                // accept both — single-source-of-
                                // truth: the user pressed something
                                // that means "redo").
                                90 if ctx.shift => {
                                    if d.redo() {
                                        value_changed = true;
                                    }
                                }
                                90 => {
                                    if d.undo() {
                                        value_changed = true;
                                    }
                                }
                                89 => {
                                    if d.redo() {
                                        value_changed = true;
                                    }
                                }
                                _ => changed = false,
                            }
                        }
                        _ => changed = false,
                    }

                    if changed && !should_blur {
                        d.reset_cursor_blink();
                        d.sync_global_selection();
                    }

                    // Extract callback info if value changed
                    let cb_info = if value_changed {
                        d.on_change_callback
                            .as_ref()
                            .map(|cb| (Arc::clone(cb), d.value.clone()))
                    } else {
                        None
                    };

                    ((changed, should_blur), cb_info)
                }; // Lock released here

                // Callbacks may normalize data before the refresh reads it.
                if let Some((callback, new_value)) = callback_info {
                    callback(&new_value);
                }

                // Handle blur (Escape key)
                if needs_refresh.1 {
                    blur_all_text_inputs();
                } else if needs_refresh.0 {
                    notify_text_input(&data_for_key);
                }
            })
            // Set text cursor (I-beam) for text input
            .cursor_text()
    }

    pub fn w(mut self, px: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.width = px;
        }
        self.inner = std::cell::RefCell::new(self.inner.take().w(px));
        self
    }

    pub fn w_full(mut self) -> Self {
        self.config.lock().unwrap().use_full_width = true;
        self.inner = std::cell::RefCell::new(self.inner.take().w_full());
        self
    }

    pub fn min_w(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().min_w(px));
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.height = px;
        }
        self.inner = std::cell::RefCell::new(self.inner.take().h(px));
        self
    }

    pub fn placeholder(self, text: impl Into<String>) -> Self {
        let placeholder = text.into();
        self.config.lock().unwrap().placeholder = placeholder.clone();
        if let Ok(mut d) = self.data.lock() {
            d.placeholder = placeholder;
        }
        self
    }

    pub fn input_type(self, input_type: InputType) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.input_type = input_type;
        }
        self
    }

    /// Set horizontal text alignment inside the field. `Left` (default)
    /// renders the value flush against `padding_x`. `Center` centres
    /// the value within the available clip area — canonical for
    /// number / OTP / code inputs where the field is fixed-width.
    pub fn text_align(self, align: blinc_core::TextAlign) -> Self {
        if let Ok(mut cfg) = self.config.lock() {
            cfg.text_align = align;
        }
        self
    }

    /// Set the internal horizontal padding inside the field (left and
    /// right spacers). Default is 12 px each side, which gives a
    /// roomy form-input feel. Number / OTP / code inputs that want a
    /// tight cell hugging the centred value should drop this to
    /// 4–6 px so the visible field hugs the text instead of carrying
    /// 24 px of fixed dead space.
    pub fn padding_x(self, px: f32) -> Self {
        if let Ok(mut cfg) = self.config.lock() {
            cfg.padding_x = px.max(0.0);
        }
        self
    }

    /// Register a stepper hook fired on ↑ / ↓ / `+` / `−` keys while
    /// the field is focused. The argument is `+1` for increment,
    /// `-1` for decrement. When this is set, the keys are
    /// *consumed* — the default behaviour (character insertion for
    /// `+` / `−`, no-op for arrows on a single-line field) is
    /// skipped. Used by `cn::number_input` to wire keyboard
    /// stepping to the bound `State<f64>`.
    pub fn on_step<F>(self, callback: F) -> Self
    where
        F: Fn(i32) + Send + Sync + 'static,
    {
        if let Ok(mut d) = self.data.lock() {
            d.on_step_callback = Some(Arc::new(callback));
        }
        self
    }

    /// Override Backspace on an empty field.
    pub fn on_backspace_empty<F>(self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        if let Ok(mut d) = self.data.lock() {
            d.on_backspace_empty_callback = Some(Arc::new(callback));
        }
        self
    }

    /// Override Cmd/Ctrl+V paste handling.
    pub fn on_paste_override<F>(self, callback: F) -> Self
    where
        F: Fn(&str) -> bool + Send + Sync + 'static,
    {
        if let Ok(mut d) = self.data.lock() {
            d.on_paste_override_callback = Some(Arc::new(callback));
        }
        self
    }

    /// Override pointer focus handling. Return `true` to consume focus.
    pub fn on_focus_request<F>(self, callback: F) -> Self
    where
        F: Fn() -> bool + Send + Sync + 'static,
    {
        if let Ok(mut d) = self.data.lock() {
            d.on_focus_request_callback = Some(Arc::new(callback));
        }
        self
    }

    pub fn disabled(self, disabled: bool) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.disabled = disabled;
            if disabled {
                d.visual = TextFieldState::Disabled;
            }
        }
        self
    }

    pub fn masked(self, masked: bool) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.masked = masked;
        }
        self
    }

    pub fn max_length(self, max: usize) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.constraints.max_length = Some(max);
        }
        self
    }

    /// Set the font size for the text input (default: 16.0)
    pub fn text_size(self, size: f32) -> Self {
        self.config.lock().unwrap().font_size = size;
        self
    }

    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.lock().unwrap().corner_radius = radius;
        self.inner = std::cell::RefCell::new(self.inner.take().rounded(radius));
        self
    }

    pub fn border(mut self, width: f32, color: blinc_core::Color) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().border(width, color));
        self
    }

    pub fn border_color(mut self, color: blinc_core::Color) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().border_color(color));
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().border_width(width));
        self
    }

    pub fn shadow_sm(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().shadow_sm());
        self
    }

    pub fn shadow_md(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().shadow_md());
        self
    }

    pub fn flex_grow(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().flex_grow());
        self
    }

    /// Set the CSS element ID for stylesheet matching
    ///
    /// When set, the TextInput will query the active stylesheet for
    /// styles matching `#id`, `#id:hover`, `#id:focus`, `#id:disabled`,
    /// and `#id::placeholder`.
    pub fn id(mut self, id: &str) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.css_element_id = Some(id.to_string());
        }
        self.inner = std::cell::RefCell::new(self.inner.take().id(id));
        self
    }

    /// Add a CSS class name for selector matching
    pub fn class(mut self, name: &str) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.css_classes.push(blinc_core::intern::intern(name));
        }
        self.inner = std::cell::RefCell::new(self.inner.take().class(name));
        self
    }

    // ========== Border Color Configuration ==========

    /// Set the idle border color (when not hovered or focused)
    pub fn idle_border_color(self, color: Color) -> Self {
        self.config.lock().unwrap().border_color = color;
        self
    }

    /// Set the hover border color
    pub fn hover_border_color(self, color: Color) -> Self {
        self.config.lock().unwrap().hover_border_color = color;
        self
    }

    /// Set the focused border color
    pub fn focused_border_color(self, color: Color) -> Self {
        self.config.lock().unwrap().focused_border_color = color;
        self
    }

    /// Set the error border color (shown when is_valid is false)
    pub fn error_border_color(self, color: Color) -> Self {
        self.config.lock().unwrap().error_border_color = color;
        self
    }

    /// Set all border colors at once for consistent theming
    pub fn border_colors(self, idle: Color, hover: Color, focused: Color, error: Color) -> Self {
        let mut cfg = self.config.lock().unwrap();
        cfg.border_color = idle;
        cfg.hover_border_color = hover;
        cfg.focused_border_color = focused;
        cfg.error_border_color = error;
        drop(cfg);
        self
    }

    // ========== Background Color Configuration ==========

    /// Set the idle background color
    pub fn idle_bg_color(self, color: Color) -> Self {
        self.config.lock().unwrap().bg_color = color;
        self
    }

    /// Set the hover background color
    pub fn hover_bg_color(self, color: Color) -> Self {
        self.config.lock().unwrap().hover_bg_color = color;
        self
    }

    /// Set the focused background color
    pub fn focused_bg_color(self, color: Color) -> Self {
        self.config.lock().unwrap().focused_bg_color = color;
        self
    }

    /// Set all background colors at once
    pub fn bg_colors(self, idle: Color, hover: Color, focused: Color) -> Self {
        let mut cfg = self.config.lock().unwrap();
        cfg.bg_color = idle;
        cfg.hover_bg_color = hover;
        cfg.focused_bg_color = focused;
        drop(cfg);
        self
    }

    // ========== Text Color Configuration ==========

    /// Set the text color
    pub fn text_color(self, color: Color) -> Self {
        self.config.lock().unwrap().text_color = color;
        self
    }

    /// Set the placeholder text color
    pub fn placeholder_color(self, color: Color) -> Self {
        self.config.lock().unwrap().placeholder_color = color;
        self
    }

    /// Set the cursor color
    pub fn cursor_color(self, color: Color) -> Self {
        self.config.lock().unwrap().cursor_color = color;
        self
    }

    /// Set the selection highlight color
    pub fn selection_color(self, color: Color) -> Self {
        self.config.lock().unwrap().selection_color = color;
        self
    }

    /// Set the callback to be invoked when the text value changes
    ///
    /// The callback receives the new text value as a string slice.
    /// This is called after insert or delete operations modify the text.
    ///
    /// # Example
    ///
    /// ```ignore
    /// text_input(&data)
    ///     .on_change(|new_value| {
    ///         println!("Text changed to: {}", new_value);
    ///     })
    /// ```
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        let cb: OnChangeCallback = Arc::new(callback);
        self.on_change_callback = Some(Arc::clone(&cb));
        // Store in TextInputData so it can be accessed in event handlers
        if let Ok(mut d) = self.data.lock() {
            d.on_change_callback = Some(cb);
        }
        self
    }
}

/// The config with the stylesheet's rules for the field's state applied, and
/// the outline those rules give it.
fn resolved_config(
    cfg: &TextInputConfig,
    d: &TextInputData,
) -> (TextInputConfig, Option<(f32, Color, f32)>) {
    let mut cfg = cfg.clone();
    let has_css_target = d.css_element_id.is_some() || !d.css_classes.is_empty();
    let outline = if has_css_target {
        active_stylesheet().and_then(|stylesheet| {
            apply_css_overrides(
                &mut cfg,
                &stylesheet,
                d.css_element_id.as_deref(),
                &d.css_classes,
                &d.visual,
            );
            extract_outline_from_stylesheet(
                &stylesheet,
                d.css_element_id.as_deref(),
                &d.css_classes,
                &d.visual,
            )
        })
    } else {
        None
    };
    (cfg, outline)
}

/// How the field's box looks in its state.
#[derive(Clone)]
struct BoxLook {
    bg: Color,
    border: Color,
    border_width: f32,
    radius: f32,
    outline: (f32, Color, f32),
}

fn box_look(cfg: &TextInputConfig, d: &TextInputData) -> BoxLook {
    let (cfg, outline) = resolved_config(cfg, d);
    let (bg, border) = match d.visual {
        TextFieldState::Idle => (cfg.bg_color, cfg.border_color),
        TextFieldState::Hovered => (cfg.hover_bg_color, cfg.hover_border_color),
        TextFieldState::Focused | TextFieldState::FocusedHovered => {
            (cfg.focused_bg_color, cfg.focused_border_color)
        }
        TextFieldState::Disabled => (
            Color::rgba(0.12, 0.12, 0.15, 0.5),
            Color::rgba(0.25, 0.25, 0.3, 0.5),
        ),
    };
    let border = if !d.is_valid && !d.value.is_empty() {
        cfg.error_border_color
    } else {
        border
    };
    BoxLook {
        bg,
        border,
        border_width: cfg.border_width,
        radius: cfg.corner_radius,
        outline: outline.unwrap_or((0.0, Color::TRANSPARENT, 0.0)),
    }
}

/// The text a field shows: its value (masked if it is a password) or its
/// placeholder.
fn shown_text(d: &TextInputData, cfg: &TextInputConfig) -> String {
    if !d.value.is_empty() {
        d.display_text()
    } else if !d.placeholder.is_empty() {
        d.placeholder.clone()
    } else {
        cfg.placeholder.clone()
    }
}

/// A run of the shown text: before the selection, in it, or after it. With
/// no selection the whole text is the run before it.
#[derive(Clone, Copy)]
enum Run {
    Before,
    Selected,
    After,
}

fn text_run(d: &TextInputData, cfg: &TextInputConfig, run: Run) -> String {
    let shown = shown_text(d, cfg);
    let selection = if d.value.is_empty() {
        None
    } else {
        d.selection_start
            .map(|start| (start.min(d.cursor), start.max(d.cursor)))
    };
    match (selection, run) {
        (None, Run::Before) => shown,
        (None, _) => String::new(),
        (Some((start, _)), Run::Before) => shown.chars().take(start).collect(),
        (Some((start, end)), Run::Selected) => {
            shown.chars().skip(start).take(end - start).collect()
        }
        (Some((_, end)), Run::After) => shown.chars().skip(end).collect(),
    }
}

/// Where the caret sits in the visible area, after scrolling the text to keep
/// it in view.
fn caret_x(d: &mut TextInputData, cfg: &TextInputConfig) -> f32 {
    d.ensure_cursor_visible(cfg);
    let shown = shown_text(d, cfg);
    let before = if d.cursor > 0 && !d.value.is_empty() {
        let text_before: String = shown.chars().take(d.cursor).collect();
        crate::text_measure::measure_text(&text_before, cfg.font_size).width
    } else {
        0.0
    };
    before - d.scroll_offset_x
}

/// Add the field's content to `outer` and bind the box's look, once. Every
/// bound value reads the data's revision first, so a handler's bump brings
/// them all up to date.
fn assemble(mut outer: Div, data: &SharedTextInputData, base: &TextInputConfig) -> Div {
    use crate::binding::TypedPendingBinding;
    use crate::property::PropertyId;
    use blinc_core::reactive::{ReactiveGraph, computed, signal};

    let revision = {
        let mut d = data.lock().unwrap();
        *d.revision.get_or_insert_with(|| signal(0u64))
    };
    // What the field is built with: the config as the stylesheet has it for
    // the state the field starts in.
    let (cfg, _) = resolved_config(base, &data.lock().unwrap());

    // The box.
    let look = {
        let (data, base) = (Arc::clone(data), base.clone());
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            box_look(&base, &data.lock().unwrap())
        })
    };
    // Read once, so the look is subscribed to the revision.
    let first = look
        .try_get()
        .unwrap_or_else(|| box_look(base, &data.lock().unwrap()));
    outer = outer
        .bg(first.bg)
        .border(first.border_width, first.border)
        .rounded(first.radius)
        .outline_width(first.outline.0)
        .outline_color(first.outline.1)
        .outline_offset(first.outline.2);
    outer
        .pending_bindings
        .push(Box::new(TypedPendingBinding::from_computed(
            look.clone(),
            PropertyId::Background,
            |props, look: BoxLook| props.background = Some(blinc_core::Brush::Solid(look.bg)),
        )));
    outer
        .pending_bindings
        .push(Box::new(TypedPendingBinding::from_computed(
            look.clone(),
            PropertyId::BorderColor,
            |props, look: BoxLook| props.border_color = Some(look.border),
        )));
    outer
        .pending_bindings
        .push(Box::new(TypedPendingBinding::from_computed(
            look.clone(),
            PropertyId::CornerRadius,
            |props, look: BoxLook| {
                props.border_radius = blinc_core::CornerRadius::uniform(look.radius)
            },
        )));
    outer
        .pending_bindings
        .push(Box::new(TypedPendingBinding::from_computed(
            look.clone(),
            PropertyId::Outline,
            |props, look: BoxLook| {
                props.outline_width = look.outline.0;
                props.outline_color = Some(look.outline.1);
                props.outline_offset = look.outline.2;
            },
        )));
    // A border width a state's rule changes moves the content, so it is
    // bound as layout.
    let border_width = {
        let (data, base) = (Arc::clone(data), base.clone());
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            box_look(&base, &data.lock().unwrap()).border_width
        })
    };
    outer = outer.border_width(&border_width);

    outer.child(content(data, &cfg, base, revision))
}

/// The inside of the field: the text in its three runs, scrolled to keep the
/// caret in view, and the caret, all bound to the revision.
fn content(
    data: &SharedTextInputData,
    cfg: &TextInputConfig,
    base: &TextInputConfig,
    revision: blinc_core::reactive::Signal<u64>,
) -> Div {
    use crate::binding::IntoReactive;
    use blinc_core::reactive::{ReactiveGraph, computed};

    let inner_height = cfg.height - cfg.border_width * 2.0;
    let cursor_height = cfg.font_size * 1.2;
    let is_centered = matches!(cfg.text_align, blinc_core::TextAlign::Center);

    let run_text = |run: Run| {
        let (data, base) = (Arc::clone(data), base.clone());
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            text_run(&data.lock().unwrap(), &base, run)
        })
    };
    let text_color = {
        let (data, base) = (Arc::clone(data), base.clone());
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            let d = data.lock().unwrap();
            let (cfg, _) = resolved_config(&base, &d);
            if d.value.is_empty() {
                cfg.placeholder_color
            } else if d.disabled {
                Color::rgba(0.4, 0.4, 0.4, 1.0)
            } else {
                cfg.text_color
            }
        })
    };
    let run = |run: Run| {
        crate::text::Text::bound(run_text(run).into_reactive())
            .size(cfg.font_size)
            .color(&text_color)
            .text_left()
            .no_wrap()
            .v_center()
    };

    let runs = div()
        .flex_row()
        .items_center()
        .child(run(Run::Before))
        .child(
            div()
                .bg(cfg.selection_color)
                .rounded(cfg.corner_radius)
                .child(run(Run::Selected)),
        )
        .child(run(Run::After));

    // Centred text (number, OTP and code cells) fills the clip area and is
    // centred by the layout; otherwise it sits at the start and is moved
    // left as far as the caret needs.
    let text_wrapper = if is_centered {
        div()
            .w_full()
            .h(inner_height)
            .flex_row()
            .items_center()
            .justify_center()
            .child(runs)
    } else {
        let scroll = {
            let (data, base) = (Arc::clone(data), base.clone());
            computed(move |g: &ReactiveGraph| {
                g.get(revision);
                let mut d = data.lock().unwrap();
                let (cfg, _) = resolved_config(&base, &d);
                d.ensure_cursor_visible(&cfg);
                blinc_core::Transform::translate(-d.scroll_offset_x, 0.0)
            })
        };
        div()
            .absolute()
            .left(0.0)
            .top(0.0)
            .h(inner_height)
            .flex_row()
            .items_center()
            .transform(&scroll)
            .child(runs)
    };

    let mut clip_container = div()
        .h(inner_height)
        .relative()
        .overflow_clip()
        .flex_1()
        .min_w(0.0)
        .child(text_wrapper);

    // The caret. Not drawn for centred text: its position would need the
    // centred text's offset, which only layout knows. It shows while the
    // field is focused with nothing selected.
    if !is_centered {
        if let Ok(mut cs) = data.lock().unwrap().cursor_state.lock() {
            cs.color = cfg.cursor_color;
            cs.animation = CursorAnimation::SmoothFade;
        }
        let at = {
            let (data, base) = (Arc::clone(data), base.clone());
            computed(move |g: &ReactiveGraph| {
                g.get(revision);
                let mut d = data.lock().unwrap();
                let (cfg, _) = resolved_config(&base, &d);
                blinc_core::Transform::translate(caret_x(&mut d, &cfg), 0.0)
            })
        };
        let data_for_caret = Arc::clone(data);
        let caret = canvas(
            move |ctx: &mut dyn blinc_core::DrawContext, bounds: crate::canvas::CanvasBounds| {
                let (shown, cursor_state) = {
                    let d = data_for_caret.lock().unwrap();
                    (
                        d.visual.is_focused() && d.selection_start.is_none(),
                        Arc::clone(&d.cursor_state),
                    )
                };
                if !shown {
                    return;
                }
                let cs = cursor_state.lock().unwrap();
                let opacity = cs.current_opacity();
                if opacity < 0.01 {
                    return;
                }
                let color = blinc_core::Color::rgba(
                    cs.color.r,
                    cs.color.g,
                    cs.color.b,
                    cs.color.a * opacity,
                );
                ctx.fill_rect(
                    blinc_core::Rect::new(0.0, 0.0, cs.width, bounds.height),
                    blinc_core::CornerRadius::default(),
                    blinc_core::Brush::Solid(color),
                );
            },
        )
        .w(2.0)
        .h(cursor_height);
        clip_container = clip_container.child(
            div()
                .absolute()
                .left(0.0)
                .top((inner_height - cursor_height) / 2.0)
                .w(2.0)
                .h(cursor_height)
                .transform(&at)
                .child(caret),
        );
    }

    div()
        .h_full()
        .w_full()
        .relative()
        .flex_row()
        .items_center()
        .child(div().w(cfg.padding_x).h(inner_height).flex_shrink_0())
        .child(clip_container)
        .child(div().w(cfg.padding_x).h(inner_height).flex_shrink_0())
}

/// Create a text input widget
/// By default, uses the config's default width (200px).
/// Use .w_full() to fill parent width, or .w() to set explicit width.
pub fn text_input(data: &SharedTextInputData) -> TextInput {
    // TextInput::new() sets default width from config (200px)
    TextInput::new(Arc::clone(data))
}

impl ElementBuilder for TextInput {
    fn bound_element_ref(&self) -> Option<&crate::selector::DivRef> {
        self.element_ref.as_ref()
    }

    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let node = self.element().build(tree);
        if let Ok(mut d) = self.data.lock() {
            d.node_id = Some(node);
        }
        node
    }

    fn render_props(&self) -> RenderProps {
        self.element().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.element().children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        crate::div::ElementTypeId::Div
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("input")
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.element())
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(self.element())
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.element().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(self.element())
    }

    fn layout_bounds_storage(&self) -> Option<crate::renderer::LayoutBoundsStorage> {
        // Updated after layout, so the scroll can be worked out against the
        // field's real width.
        if let Ok(data) = self.data.lock() {
            Some(Arc::clone(&data.layout_bounds_storage))
        } else {
            None
        }
    }

    fn layout_bounds_callback(&self) -> Option<crate::renderer::LayoutBoundsCallback> {
        // A new width can scroll the text differently.
        let data = Arc::clone(&self.data);
        Some(Arc::new(move |_bounds| notify_text_input(&data)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::div::ElementBuilder;
    use crate::event_handler::EventContext;
    use crate::tree::LayoutNodeId;
    use blinc_core::events::event_types;
    use std::sync::{Arc, Mutex, Once};
    use taffy::Dimension;

    static THEME_INIT: Once = Once::new();

    fn ensure_theme_initialized() {
        THEME_INIT.call_once(ThemeState::init_default);
    }

    #[test]
    fn test_text_input_data_insert() {
        let mut data = TextInputData::new();

        data.insert("hello");
        assert_eq!(data.value, "hello");
        assert_eq!(data.cursor, 5);

        data.cursor = 0;
        data.insert("world ");
        assert_eq!(data.value, "world hello");
    }

    #[test]
    fn test_text_input_data_delete() {
        let mut data = TextInputData::with_value("hello");

        data.cursor = 5;
        data.delete_backward();
        assert_eq!(data.value, "hell");

        data.cursor = 0;
        data.delete_forward();
        assert_eq!(data.value, "ell");
    }

    #[test]
    fn test_input_type_filtering() {
        let mut data = TextInputData::new();
        data.input_type = InputType::Number;

        data.insert("123.45");
        assert_eq!(data.value, "123.45");

        data.value.clear();
        data.cursor = 0;
        data.insert("abc123");
        assert_eq!(data.value, "123");
    }

    /// Regression test: keyboard stepping while focused used to leave
    /// the displayed value stale — the focus guard meant for typing
    /// swallowed step changes too.
    #[test]
    fn force_sync_once_overrides_focus_guard_for_step_changes() {
        let mut data = TextInputData::with_value("5");
        data.visual = TextFieldState::Focused;

        // Typing case: no flag, stays guarded.
        let is_focused = data.visual.is_focused();
        let force = std::mem::take(&mut data.force_sync_once);
        assert!(!force && is_focused);

        // Step case: flag forces the sync through despite focus, then
        // resets so it doesn't leak into the next one.
        data.force_sync_once = true;
        let is_focused = data.visual.is_focused();
        let force = std::mem::take(&mut data.force_sync_once);
        assert!(force || !is_focused);
        assert!(!data.force_sync_once);
    }

    #[test]
    fn the_shown_text_is_the_value_on_change_normalized() {
        ensure_theme_initialized();

        let data = text_input_data();
        {
            let mut data = data.lock().unwrap();
            data.visual = TextFieldState::Focused;
        }

        let data_for_change = Arc::clone(&data);
        let input = text_input(&data)
            .input_type(InputType::Integer)
            .on_change(move |_| {
                let mut data = data_for_change.lock().unwrap();
                data.value.clear();
                data.cursor = 0;
                data.selection_start = None;
            });
        let mut tree = crate::renderer::RenderTree::from_element(&input);
        tree.compute_layout(300.0, 100.0);

        let ctx =
            EventContext::new(event_types::TEXT_INPUT, LayoutNodeId::default()).with_key_char('-');
        ElementBuilder::event_handlers(&input)
            .unwrap()
            .dispatch(&ctx);
        let updates = crate::stateful::take_pending_partial_prop_updates();
        tree.apply_partial_property_updates(updates);

        assert_eq!(data.lock().unwrap().value, "");
        let mut shown = Vec::new();
        let mut stack = vec![tree.root().unwrap()];
        while let Some(node) = stack.pop() {
            if let Some(crate::renderer::ElementType::Text(t)) =
                tree.get_render_node(node).map(|r| &r.element_type)
            {
                shown.push(t.content.clone());
            }
            stack.extend(tree.layout_tree.children(node));
        }
        assert!(
            shown.iter().all(|t| t.is_empty()),
            "the field shows {shown:?}, not the value on_change left"
        );
    }

    #[test]
    fn state_callback_preserves_explicit_width() {
        ensure_theme_initialized();

        let data = text_input_data();
        let input = text_input(&data).w(42.0);

        let mut tree = LayoutTree::new();
        let root = input.build(&mut tree);
        let style = tree.get_style(root).unwrap();

        assert_eq!(style.size.width, Dimension::length(42.0));
    }

    // Global focus statics (PENDING_FOCUS_INPUT, FOCUSED_TEXT_INPUT, ...)
    // are process-wide, so serialize tests that touch them.
    static FOCUS_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn blur_cancels_a_pending_deferred_focus() {
        let _guard = FOCUS_TEST_LOCK.lock().unwrap();
        ensure_theme_initialized();
        blur_all_text_inputs();

        let data = text_input_data();
        let mut tree = LayoutTree::new();
        let _root = text_input(&data).build(&mut tree);

        // Auto-advance queues a re-focus (e.g. OTP typing the last
        // digit), then the user clicks outside before it drains.
        focus_text_input_deferred(&data);
        blur_all_text_inputs();
        process_pending_input_focus();

        assert!(!data.lock().unwrap().visual.is_focused());
    }

    #[test]
    fn deferred_focus_applies_without_an_intervening_blur() {
        let _guard = FOCUS_TEST_LOCK.lock().unwrap();
        ensure_theme_initialized();
        blur_all_text_inputs();

        let data = text_input_data();
        let mut tree = LayoutTree::new();
        let _root = text_input(&data).build(&mut tree);

        focus_text_input_deferred(&data);
        process_pending_input_focus();

        assert!(data.lock().unwrap().visual.is_focused());
    }

    #[test]
    fn focusing_another_input_blurs_previous_stateful_visual_state() {
        let _guard = FOCUS_TEST_LOCK.lock().unwrap();
        ensure_theme_initialized();
        blur_all_text_inputs();

        let first = text_input_data();
        let second = text_input_data();
        let mut tree = LayoutTree::new();
        let _first_root = text_input(&first).build(&mut tree);
        let _second_root = text_input(&second).build(&mut tree);

        focus_text_input(&first);
        focus_text_input(&second);

        assert!(!first.lock().unwrap().visual.is_focused());
    }
}
