//! Focusing a text area, and repainting it after its state changes.

use super::*;

/// Enqueue a text_area for focus on the NEXT frame, after its widget
/// has been built into the tree.
///
/// Companion to [`crate::widgets::text_input::focus_text_input_deferred`].
/// See that function for the timing rationale: focusing BEFORE the
/// matching overlay mounts triggers continuous_redraw against an
/// unstable composition; deferring until after mount avoids that.
pub fn focus_text_area_deferred(state: &SharedTextAreaState) {
    crate::widgets::text_input::enqueue_pending_focus_area(Arc::downgrade(state));
}

/// Programmatically focus a text_area by its shared state handle.
///
/// Mirrors [`crate::widgets::text_input::focus_text_input`]: sets the
/// widget's visual state to `Focused`, marks it as the active focus
/// target for keyboard / IME / cursor-blink machinery, blurs any
/// previously-focused text_input or text_area, and increments the
/// soft-keyboard refcount. Use from a parent widget's open-handler
/// when an embedded text area should grab focus on appearance (e.g.
/// an inline editor popover that should land cursor-ready).
///
/// Safe to call before the matching [`TextArea`] is built: it is built
/// showing the state's visual state, so it starts out focused.
pub fn focus_text_area(state: &SharedTextAreaState) {
    use crate::widgets::text_input::{increment_focus_count, set_focused_text_area};
    use blinc_core::events::event_types;
    let focused = if let Ok(mut s) = state.lock() {
        if !s.visual.is_focused() {
            if let Some(new_state) = s.visual.on_event(event_types::FOCUS) {
                s.visual = new_state;
            } else {
                s.visual = TextFieldState::Focused;
            }
            s.focus_time_ms = crate::widgets::text_input::elapsed_ms();
            s.reset_cursor_blink();
            increment_focus_count();
            true
        } else {
            false
        }
    } else {
        false
    };
    set_focused_text_area(state);
    if focused {
        notify_text_area(state);
    }
}

/// Lay the lines out again and repaint the field: everything it shows is
/// bound to the revision. Called after any change to the state.
pub(crate) fn notify_text_area(state: &SharedTextAreaState) {
    // The config is copied out first: a layout callback locks the config
    // and then the state.
    let base = state
        .lock()
        .ok()
        .and_then(|d| d.config.clone())
        .and_then(|c| c.lock().ok().map(|c| c.clone()));
    let revision = match state.lock() {
        Ok(mut d) => {
            if let Some(base) = &base {
                let (cfg, _) = resolved_config(base, &d);
                take_metrics(&mut d, &cfg);
            }
            if d.laid_out != Some(d.layout_key()) {
                d.compute_visual_lines();
            }
            d.revision
        }
        Err(_) => None,
    };
    if let Some(revision) = revision {
        revision.update(|n| n.wrapping_add(1));
    }
    crate::stateful::request_redraw();
}

/// Repaint a text area after its state was changed from outside the widget.
pub fn refresh_text_area(state: &SharedTextAreaState) {
    notify_text_area(state);
}

/// Copy the sizes the state lays its lines out with from `cfg`.
pub(super) fn take_metrics(d: &mut TextAreaState, cfg: &TextAreaConfig) {
    d.line_height = cfg.font_size * cfg.line_height;
    d.viewport_height = cfg.effective_height() - cfg.padding_y * 2.0 - cfg.border_width * 2.0;
    d.font_size = cfg.font_size;
    d.available_width = cfg.effective_width() - cfg.padding_x * 2.0 - cfg.border_width * 2.0;
    d.wrap_enabled = cfg.wrap;
}

/// The config with the stylesheet's rules for the field's state applied,
/// and the outline those rules give it.
pub(super) fn resolved_config(
    base: &TextAreaConfig,
    d: &TextAreaState,
) -> (TextAreaConfig, Option<(f32, Color, f32)>) {
    let mut cfg = base.clone();
    if d.css_element_id.is_none() && d.css_classes.is_empty() {
        return (cfg, None);
    }
    let Some(stylesheet) = active_stylesheet() else {
        return (cfg, None);
    };
    apply_css_overrides_textarea(
        &mut cfg,
        &stylesheet,
        d.css_element_id.as_deref(),
        &d.css_classes,
        &d.visual,
    );
    let outline = extract_outline_from_textarea_stylesheet(
        &stylesheet,
        d.css_element_id.as_deref(),
        &d.css_classes,
        &d.visual,
    );
    (cfg, outline)
}
