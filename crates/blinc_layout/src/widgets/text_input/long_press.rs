//! The long-press timer that opens the edit menu on touch.

use super::*;

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
pub(super) struct LongPressArm {
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
pub(super) static LONG_PRESS_ARM: Mutex<Option<LongPressArm>> = Mutex::new(None);

/// Long-press deadline relative to the press start time (ms). 500ms
/// matches iOS UITextField / Android EditText long-press timing.
pub(super) const LONG_PRESS_DURATION_MS: u64 = 500;

/// Maximum movement (in logical pixels) allowed before the long-press
/// is cancelled. Mirrors the existing `widgets::gesture` constant.
pub(super) const LONG_PRESS_MAX_DRIFT_PX: f32 = 10.0;

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
