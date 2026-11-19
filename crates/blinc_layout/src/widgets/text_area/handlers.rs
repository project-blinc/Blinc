//! What the text area does with the pointer and the keyboard.

use super::*;

impl TextArea {
    /// Create the element with all the event handlers registered
    pub(super) fn create_inner_with_handlers(
        data: SharedTextAreaState,
        config: Arc<Mutex<TextAreaConfig>>,
    ) -> Div {
        use blinc_core::events::event_types;

        let data_for_click = Arc::clone(&data);
        let data_for_text = Arc::clone(&data);
        let data_for_key = Arc::clone(&data);
        let data_for_drag = Arc::clone(&data);
        let data_for_release = Arc::clone(&data);
        let config_for_click = Arc::clone(&config);
        let config_for_drag = Arc::clone(&config);
        let data_for_enter = Arc::clone(&data);
        let data_for_leave = Arc::clone(&data);

        div()
            // The pointer over the field changes its look.
            .on_hover_enter(move |_| pointer_crossed(&data_for_enter, event_types::POINTER_ENTER))
            .on_hover_leave(move |_| pointer_crossed(&data_for_leave, event_types::POINTER_LEAVE))
            .on_mouse_down(move |ctx| {
                {
                    let mut d = match data_for_click.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    if ctx.mouse_button != 0 || d.disabled {
                        d.clicked_visual_line = None;
                        return;
                    }
                }
                super::ime::commit(&data_for_click);
                set_focused_text_area(&data_for_click);
                let base = config_for_click.lock().unwrap().clone();
                let touch = crate::widgets::text_input::is_touch_input();
                let (clicks, position) = {
                    let mut d = match data_for_click.lock() {
                        Ok(d) if !d.disabled => d,
                        _ => return,
                    };
                    crate::widgets::text_input::bump_focus_tap_generation();
                    crate::widgets::text_input::set_focused_editable_node(ctx.node_id, None);
                    if !d.visual.is_focused() {
                        d.visual = TextFieldState::Focused;
                        d.clicks = Default::default();
                        increment_focus_count();
                        request_continuous_redraw_pub();
                    }
                    let cfg = resolved_config(&base, &d);
                    take_metrics(&mut d, &cfg);
                    if d.laid_out != Some(d.layout_key()) {
                        d.compute_visual_lines();
                    }
                    let x = (ctx.local_x - cfg.padding_x - cfg.border_width).max(0.0);
                    let y = (ctx.local_y - cfg.padding_y - cfg.border_width).max(0.0);
                    let position = if let Some(line) = d.clicked_visual_line.take() {
                        d.cursor_position_from_visual_line(line, x)
                    } else {
                        d.cursor_position_from_xy(x, y)
                    };
                    let clicks =
                        d.select_pointer(position, (ctx.local_x, ctx.local_y), ctx.shift, touch);
                    d.reset_cursor_blink();
                    (clicks, position)
                };
                notify_text_area(&data_for_click);

                if touch {
                    let x = ctx.bounds_x + ctx.local_x;
                    let y = ctx.bounds_y + ctx.local_y;
                    let height = ctx.bounds_height.clamp(24.0, 48.0);
                    if clicks > 1 {
                        crate::widgets::text_input::cancel_long_press_timer();
                        crate::widgets::text_edit::haptic_impact_light();
                        use crate::widgets::text_edit::edit_menu_actions;
                        crate::widgets::text_edit::show_edit_menu(
                            x,
                            y,
                            x,
                            y,
                            0.0,
                            height,
                            edit_menu_actions::CUT
                                | edit_menu_actions::COPY
                                | edit_menu_actions::PASTE
                                | edit_menu_actions::SELECT_ALL,
                        );
                    } else {
                        crate::widgets::text_edit::haptic_selection();
                        crate::widgets::text_edit::hide_edit_menu();
                        let state = Arc::clone(&data_for_click);
                        crate::widgets::text_input::arm_long_press_timer(
                            x,
                            y,
                            height,
                            Some(Box::new(move || {
                                {
                                    let mut d = match state.lock() {
                                        Ok(d) if !d.disabled && d.is_focused() => d,
                                        _ => return,
                                    };
                                    d.select_word_at(position);
                                }
                                notify_text_area(&state);
                            })),
                        );
                    }
                }
            })
            .on_mouse_up(move |ctx| {
                if ctx.mouse_button != 0 {
                    return;
                }
                if let Ok(mut d) = data_for_release.lock() {
                    d.drag_selection = None;
                }
                crate::widgets::text_input::cancel_long_press_timer();
            })
            // Mouse drags extend the selected unit; touch drags move the caret.
            .on_drag(move |ctx| {
                crate::widgets::text_input::check_long_press_drift(
                    ctx.bounds_x + ctx.local_x,
                    ctx.bounds_y + ctx.local_y,
                );
                let cfg = config_for_drag.lock().unwrap();
                let wrap_enabled = cfg.wrap;
                let text_origin_x = cfg.padding_x + cfg.border_width;
                let text_origin_y = cfg.padding_y + cfg.border_width;
                drop(cfg);

                let needs_refresh = {
                    let mut d = match data_for_drag.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    if d.disabled || !d.visual.is_focused() {
                        return;
                    }
                    if d.visual_lines.is_empty() || wrap_enabled {
                        d.compute_visual_lines();
                    }
                    // Drag coordinates are relative to the field, not its text.
                    let text_x = (ctx.local_x - text_origin_x).max(0.0);
                    let text_y = (ctx.local_y - text_origin_y).max(0.0);
                    let new_pos = d.cursor_position_from_xy(text_x, text_y);

                    if crate::widgets::text_input::is_touch_input() {
                        // Touch drag = cursor reposition, no select.
                        if new_pos != d.cursor {
                            d.cursor = new_pos;
                            d.selection_start = None;
                            true
                        } else {
                            false
                        }
                    } else {
                        d.extend_pointer_selection(new_pos)
                    }
                };
                if needs_refresh {
                    notify_text_area(&data_for_drag);
                }
            })
            // Handle text input
            .on_event(event_types::TEXT_INPUT, move |ctx| {
                if ctx.command_modifier()
                    || !ctx
                        .key_char
                        .is_some_and(|ch| !ch.is_control() || ch == '\t')
                {
                    return;
                }
                super::ime::commit(&data_for_text);
                let (needs_refresh, change_signal, edited) = {
                    let mut d = match data_for_text.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled || !d.visual.is_focused() {
                        return;
                    }

                    // Skip control characters and modifier combos (Cmd+C/V/Z etc.)
                    if ctx.command_modifier() {
                        return;
                    }

                    if let Some(c) = ctx.key_char {
                        if c.is_control() && c != '\t' {
                            return;
                        }
                        d.insert(&c.to_string());
                        d.reset_cursor_blink();
                        // Recompute visual lines after text change
                        d.compute_visual_lines();
                        // Ensure cursor is visible after text insertion (use cached values)
                        let line_height = d.line_height;
                        let viewport_height = d.viewport_height;
                        d.ensure_cursor_visible(line_height, viewport_height);
                        tracing::debug!("TextArea received char: {:?}, value: {}", c, d.value());
                        (true, d.change_signal_id, changed_text(&d))
                    } else {
                        (false, None, None)
                    }
                }; // Lock released here

                // Trigger incremental refresh AFTER releasing the data lock
                if needs_refresh {
                    notify_text_area(&data_for_text);
                }

                // Notify external listeners of text change
                if let Some(signal_id) = change_signal {
                    crate::stateful::check_stateful_deps(&[signal_id]);
                }
                fire_on_change(edited);
            })
            // Handle key down for navigation and deletion
            .on_key_down(move |ctx| {
                if matches!(ctx.key_code, 8 | 13 | 27 | 35..=40 | 46)
                    || (ctx.command_modifier()
                        && matches!(ctx.key_code, 65 | 67 | 86 | 88 | 89 | 90))
                {
                    super::ime::commit(&data_for_key);
                }
                let needs_refresh = {
                    let mut d = match data_for_key.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled || !d.visual.is_focused() {
                        return;
                    }

                    let mut cursor_changed = true;
                    let mut should_blur = false;
                    let mut text_changed = false;
                    let mod_key = ctx.command_modifier();

                    match ctx.key_code {
                        8 if mod_key => {
                            // Cmd+Backspace: delete word backward
                            d.delete_word_backward();
                            text_changed = true;
                        }
                        8 => {
                            // Backspace
                            d.delete_backward();
                            text_changed = true;
                        }
                        127 if mod_key => {
                            // Cmd+Delete: delete word forward
                            d.delete_word_forward();
                            text_changed = true;
                        }
                        127 => {
                            // Delete
                            d.delete_forward();
                            text_changed = true;
                        }
                        13 => {
                            // Enter - insert newline
                            d.insert_newline();
                            text_changed = true;
                        }
                        37 => {
                            // Left arrow
                            if mod_key {
                                d.move_word_left(ctx.shift);
                            } else {
                                d.move_left(ctx.shift);
                            }
                        }
                        39 => {
                            // Right arrow
                            if mod_key {
                                d.move_word_right(ctx.shift);
                            } else {
                                d.move_right(ctx.shift);
                            }
                        }
                        38 => {
                            // Up arrow
                            d.move_up(ctx.shift);
                        }
                        40 => {
                            // Down arrow
                            d.move_down(ctx.shift);
                        }
                        36 => {
                            // Home
                            d.move_to_line_start(ctx.shift);
                        }
                        35 => {
                            // End
                            d.move_to_line_end(ctx.shift);
                        }
                        27 => {
                            // Escape - blur the textarea
                            should_blur = true;
                            cursor_changed = true;
                        }
                        _ => {
                            if mod_key {
                                match ctx.key_code {
                                    65 => d.select_all(), // Cmd+A
                                    67 => {
                                        // Cmd+C — Copy
                                        if let Some(sel) = d.selected_text() {
                                            crate::widgets::text_edit::clipboard_write(&sel);
                                        }
                                        cursor_changed = false;
                                    }
                                    88 => {
                                        // Cmd+X — Cut
                                        if let Some(sel) = d.selected_text() {
                                            crate::widgets::text_edit::clipboard_write(&sel);
                                            d.delete_selection();
                                            text_changed = true;
                                        }
                                    }
                                    86 => {
                                        // Cmd+V — Paste
                                        if let Some(clip) =
                                            crate::widgets::text_edit::clipboard_read()
                                        {
                                            d.insert(&clip);
                                            text_changed = true;
                                        }
                                    }
                                    _ => {
                                        cursor_changed = false;
                                    }
                                }
                            } else {
                                cursor_changed = false;
                            }
                        }
                    }

                    // Recompute visual lines after text changes
                    if text_changed {
                        d.compute_visual_lines();
                    }

                    if cursor_changed && !should_blur {
                        d.reset_cursor_blink();
                        // Ensure cursor is visible (auto-scroll if needed, use cached values)
                        let line_height = d.line_height;
                        let viewport_height = d.viewport_height;
                        d.ensure_cursor_visible(line_height, viewport_height);
                    }

                    // Get change signal ID if text changed
                    let change_signal = if text_changed {
                        d.change_signal_id
                    } else {
                        None
                    };
                    let edited = if text_changed { changed_text(&d) } else { None };

                    (cursor_changed, should_blur, change_signal, edited)
                }; // Lock released here

                // Handle blur (Escape key)
                if needs_refresh.1 {
                    crate::widgets::text_input::blur_all_text_inputs();
                } else if needs_refresh.0 {
                    // Trigger incremental refresh AFTER releasing the data lock
                    notify_text_area(&data_for_key);
                }

                // Notify external listeners of text change
                if let Some(signal_id) = needs_refresh.2 {
                    crate::stateful::check_stateful_deps(&[signal_id]);
                }
                fire_on_change(needs_refresh.3);
            })
            // Set text cursor (I-beam) for text area
            .cursor_text()
        // Note: Scroll events are handled by the scroll() widget inside build_content
    }
}

/// The callback and the text to hand it, read while the state lock is
/// already held.
///
/// Split from firing it on purpose: the callback commonly writes a
/// signal, which can refresh a stateful that locks this same state, so
/// it must run only after the lock is released.
pub(super) fn changed_text(d: &TextAreaState) -> Option<(crate::widgets::TextCallback, String)> {
    d.on_change.as_ref().map(|cb| (Arc::clone(cb), d.value()))
}

/// Fire what [`changed_text`] captured, now that the lock is gone.
pub(super) fn fire_on_change(edited: Option<(crate::widgets::TextCallback, String)>) {
    if let Some((cb, text)) = edited {
        cb(&text);
    }
}

/// The pointer entered or left a field: move its visual state.
pub(super) fn pointer_crossed(state: &SharedTextAreaState, event: u32) {
    let changed = match state.lock() {
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
        notify_text_area(state);
    }
}
