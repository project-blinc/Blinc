//! What the text input does with the pointer and the keyboard.

use super::*;

/// The pointer entered or left a field: move its visual state.
pub(super) fn pointer_crossed(data: &SharedTextInputData, event: u32) {
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

impl TextInput {
    /// Create the element with all the event handlers registered
    pub(super) fn create_inner_with_handlers(
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
                        13 if d.on_submit_callback.is_some() => {
                            if let Some(cb) = d.on_submit_callback.as_ref().map(Arc::clone) {
                                drop(d);
                                cb(ctx.shift);
                            }
                            return;
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
}
