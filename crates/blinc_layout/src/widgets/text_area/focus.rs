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
    super::ime::finish_on_blur(state);
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
                let cfg = resolved_config(base, &d);
                layout_lines(&mut d, &cfg);
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
    let measured = d
        .layout_bounds_storage
        .lock()
        .ok()
        .and_then(|bounds| *bounds);
    let requested_height = cfg.effective_height();
    let height = measured.map_or(requested_height, |b| {
        // Pixel rounding must not turn a requested row count into overflow.
        if cfg.rows.is_some() && (b.height - requested_height).abs() <= 0.5 {
            requested_height
        } else {
            b.height
        }
    });
    let width = measured.map_or_else(|| cfg.effective_width(), |b| b.width);
    d.viewport_height = (height - cfg.padding_y * 2.0 - cfg.border_width * 2.0).max(0.0);
    d.font_size = cfg.font_size;
    d.face = cfg.face.clone();
    d.available_width = (width - cfg.padding_x * 2.0 - cfg.border_width * 2.0).max(0.0);
    d.wrap_enabled = cfg.wrap;
}

/// Reconcile the viewport and its offset with the lines that will be drawn.
pub(super) fn layout_lines(d: &mut TextAreaState, cfg: &TextAreaConfig) {
    let previous_height = d.viewport_height;
    take_metrics(d, cfg);
    let reflowed = d.laid_out != Some(d.layout_key());
    if reflowed {
        d.compute_visual_lines();
    }
    {
        let mut physics = d.scroll_physics.lock().unwrap();
        physics.viewport_width = d.available_width;
        physics.viewport_height = d.viewport_height;
        physics.content_height = d.content_height_from_visual_lines();
        let max_scroll = (physics.content_height - physics.viewport_height).max(0.0);
        physics.offset_y = physics.offset_y.clamp(-max_scroll, 0.0);
    }
    // Scrolling alone must not pull the view back to the caret.
    if d.is_focused() && (reflowed || previous_height != d.viewport_height) {
        d.ensure_cursor_visible(d.line_height, d.viewport_height);
    }
}

/// The config with the stylesheet's rules for the field's state applied.
pub(super) fn resolved_config(base: &TextAreaConfig, d: &TextAreaState) -> TextAreaConfig {
    let mut cfg = base.clone();
    if d.css_element_id.is_none() && d.css_classes.is_empty() {
        return cfg;
    }
    if let Some(stylesheet) = active_stylesheet() {
        apply_css_overrides_textarea(
            &mut cfg,
            &stylesheet,
            d.css_element_id.as_deref(),
            &d.css_classes,
            &d.visual,
        );
    }
    cfg
}
