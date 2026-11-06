//! Which text field has focus, deferred focus, blurring, and repainting a
//! field after its state changes.

use super::*;

pub(super) static FOCUSED_TEXT_INPUT: Mutex<Option<Weak<Mutex<TextInputData>>>> = Mutex::new(None);

pub(super) static FOCUSED_TEXT_AREA: Mutex<
    Option<Weak<Mutex<crate::widgets::text_area::TextAreaState>>>,
> = Mutex::new(None);

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
pub(super) static PENDING_FOCUS_INPUT: Mutex<Vec<(u64, Weak<Mutex<TextInputData>>)>> =
    Mutex::new(Vec::new());

/// Bumped by [`blur_all_text_inputs`] to cancel deferred focus queued before blur.
pub(super) static PENDING_FOCUS_GENERATION: AtomicU64 = AtomicU64::new(0);

pub(super) static PENDING_FOCUS_AREA: Mutex<
    Vec<(u64, Weak<Mutex<crate::widgets::text_area::TextAreaState>>)>,
> = Mutex::new(Vec::new());

/// Repaint a field after its [`TextInputData::value`], `cursor` or
/// `selection_start` was changed from outside the widget, e.g. by the
/// `+` / `−` stepper on `cn::number_input`.
pub fn refresh_text_input(state: &SharedTextInputData) {
    notify_text_input(state);
}

/// A reactive read of whether this field owns text focus.
pub fn text_input_focused(data: &SharedTextInputData) -> blinc_core::reactive::Computed<bool> {
    use blinc_core::reactive::{computed, signal};
    let focused = {
        let mut data = data.lock().unwrap();
        let initial = data.visual.is_focused();
        *data.focus_signal.get_or_insert_with(|| signal(initial))
    };
    computed(move |graph| graph.get(focused).unwrap_or(false))
}

/// The field's data changed: have what it shows follow. Called with the data
/// unlocked, since what follows reads it.
pub(crate) fn notify_text_input(state: &SharedTextInputData) {
    super::ime::finish_on_blur(state);
    let (revision, focus) = state
        .lock()
        .ok()
        .map(|d| {
            (
                d.revision,
                d.focus_signal.map(|signal| (signal, d.visual.is_focused())),
            )
        })
        .unwrap_or_default();
    if let Some((signal, focused)) = focus {
        if signal.try_get() != Some(focused) {
            signal.set(focused);
        }
    }
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
        let mounted = strong.lock().ok().map(|d| d.mounted).unwrap_or(false);
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
            .map(|s| s.revision.is_some())
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
    let previous = FOCUSED_TEXT_INPUT
        .lock()
        .unwrap()
        .replace(Arc::downgrade(state));
    if let Some(previous) = previous.and_then(|weak| weak.upgrade()) {
        if !Arc::ptr_eq(&previous, state) {
            blur_text_input_state(&previous);
        }
    }
    blur_focused_text_area();
}

pub(super) fn blur_text_input_state(state: &SharedTextInputData) {
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
    let input = FOCUSED_TEXT_INPUT.lock().unwrap().take();
    if let Some(input) = input.and_then(|weak| weak.upgrade()) {
        blur_text_input_state(&input);
    }
    let previous = FOCUSED_TEXT_AREA
        .lock()
        .unwrap()
        .replace(Arc::downgrade(state));
    if let Some(previous) = previous.and_then(|weak| weak.upgrade()) {
        if !Arc::ptr_eq(&previous, state) {
            blur_text_area_state(&previous);
        }
    }
}

pub(super) fn blur_text_area_state(state: &crate::widgets::text_area::SharedTextAreaState) {
    use blinc_core::events::event_types;

    let blurred = match state.lock() {
        Ok(mut s) => match s.visual.on_event(event_types::BLUR) {
            Some(new_state) => {
                s.visual = new_state;
                decrement_focus_count();
                true
            }
            None => false,
        },
        Err(_) => false,
    };
    if blurred {
        crate::widgets::text_area::notify_text_area(state);
    }
}

pub(super) fn blur_focused_text_area() {
    let previous = FOCUSED_TEXT_AREA.lock().unwrap().take();
    if let Some(previous) = previous.and_then(|weak| weak.upgrade()) {
        blur_text_area_state(&previous);
    }
}

/// Blur all focused text inputs and text areas.
/// Called when clicking outside any text element.
pub fn blur_all_text_inputs() {
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

    let input = FOCUSED_TEXT_INPUT.lock().unwrap().take();
    let area = FOCUSED_TEXT_AREA.lock().unwrap().take();
    if let Some(input) = input.and_then(|weak| weak.upgrade()) {
        blur_text_input_state(&input);
    }
    if let Some(area) = area.and_then(|weak| weak.upgrade()) {
        blur_text_area_state(&area);
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
    guard.node_id
}

// =============================================================================
// Input Types and Validation
// =============================================================================

/// Focus is process-wide (`FOCUSED_TEXT_INPUT`, the pending focus queue), and the tests that
/// focus, blur or send key events through it, in this module and the OTP input, take turns.
#[cfg(test)]
pub(crate) static FOCUS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
