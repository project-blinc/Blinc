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
        let config_for_click = Arc::clone(&config);
        let config_for_drag = Arc::clone(&config);
        let data_for_enter = Arc::clone(&data);
        let data_for_leave = Arc::clone(&data);

        div()
            // The pointer over the field changes its look.
            .on_hover_enter(move |_| pointer_crossed(&data_for_enter, event_types::POINTER_ENTER))
            .on_hover_leave(move |_| pointer_crossed(&data_for_leave, event_types::POINTER_LEAVE))
            // Handle mouse down to focus and position cursor
            .on_mouse_down(move |ctx| {
                // First, forcibly blur any previously focused text input/area
                set_focused_text_area(&data_for_click);

                // Get click position from context for cursor positioning.
                // `ctx.local_x` / `local_y` are relative to THIS handler's
                // node — the field's box — not the inner text
                // content. The wrapper renders a `padding_x`-wide left
                // spacer + a `border_width` border before the scroll
                // container, and a `padding_y` top spacer before the
                // middle row. Subtract both to get text-content-relative
                // coordinates; otherwise every click lands `padding_x +
                // border_width` ≈ 13.5px (≈ 2 chars at the default font)
                // to the right of the intended character. Same fix shape
                // text_input uses at its own click handler.
                let cfg = config_for_click.lock().unwrap();
                let font_size = cfg.font_size;
                let line_height = cfg.font_size * cfg.line_height;
                let available_width =
                    cfg.effective_width() - cfg.padding_x * 2.0 - cfg.border_width * 2.0;
                let wrap_enabled = cfg.wrap;
                let text_origin_x = cfg.padding_x + cfg.border_width;
                let text_origin_y = cfg.padding_y + cfg.border_width;
                drop(cfg);
                let click_x = ctx.local_x - text_origin_x;
                let click_y = ctx.local_y - text_origin_y;

                let needs_refresh = {
                    let mut d = match data_for_click.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled {
                        return;
                    }

                    // Bump the focus-tap generation counter so the
                    // mobile runner sees this as a "user tapped a text
                    // area" event even if it was already focused. See
                    // `text_input::focus_tap_generation` docs.
                    crate::widgets::text_input::bump_focus_tap_generation();

                    // Register the layout node id with the generic
                    // focused-editable atomic so the scroll-into-view
                    // helper has a single lookup that covers every
                    // text-editable widget regardless of which typed
                    // tracker (text_input vs text_area) holds the data.
                    //
                    // No blur callback — text_area has its own
                    // dedicated `FOCUSED_TEXT_AREA` tracker that the
                    // typed blur path walks.
                    crate::widgets::text_input::set_focused_editable_node(ctx.node_id, None);

                    // Update data state
                    let was_focused = d.visual.is_focused();
                    if !was_focused {
                        d.visual = TextFieldState::Focused;
                        increment_focus_count();
                        request_continuous_redraw_pub();
                    }

                    // Update cached config values for visual line computation
                    d.font_size = font_size;
                    d.line_height = line_height;
                    d.available_width = available_width;
                    d.wrap_enabled = wrap_enabled;

                    // Ensure visual lines are computed before positioning cursor
                    // This is needed because click handler may run before the callback
                    // that computes visual lines during rebuild
                    if d.visual_lines.is_empty() || d.wrap_enabled {
                        d.compute_visual_lines();
                    }

                    // Position cursor at click location
                    // Use clicked_visual_line if set by a line element's click handler,
                    // otherwise fall back to y-coordinate calculation
                    let text_x = click_x.max(0.0);

                    let new_pos = if let Some(visual_line_idx) = d.clicked_visual_line.take() {
                        // Line element told us which visual line was clicked
                        // Use that for accurate positioning
                        d.cursor_position_from_visual_line(visual_line_idx, text_x)
                    } else {
                        // Fallback: try to compute from y coordinate
                        let text_y = click_y.max(0.0);
                        d.cursor_position_from_xy(text_x, text_y)
                    };
                    d.cursor = new_pos;
                    d.selection_start = None; // Clear any selection
                    // Seed the drag-select anchor for desktop mouse
                    // clicks (touch leaves anchor = None so a drag
                    // becomes cursor reposition, mirroring
                    // text_input's behaviour). The .on_drag handler
                    // below reads this and extends `selection_start`
                    // as the pointer moves while pressed.
                    if crate::widgets::text_input::is_touch_input() {
                        d.drag_select_anchor = None;
                    } else {
                        d.drag_select_anchor = Some(new_pos);
                    }
                    d.reset_cursor_blink();

                    // Mobile UX touches: light haptic on every tap +
                    // hide any leftover edit menu from a previous
                    // double-tap. The edit menu itself fires from a
                    // double-tap detector below. The double-tap
                    // tracking lives on the data lock so we don't
                    // need extra fields here.
                    //
                    // Also arm the long-press timer so a press-and-
                    // hold of 500 ms shows the edit menu with
                    // PASTE available — matches the iOS
                    // UITextField / Android EditText long-press UX.
                    if crate::widgets::text_input::is_touch_input() {
                        crate::widgets::text_edit::haptic_selection();
                        crate::widgets::text_edit::hide_edit_menu();
                        // No word-selection callback here: text_area
                        // doesn't yet implement double-tap word
                        // selection either, so passing `None` keeps
                        // both gestures consistent within the widget
                        // (long press just shows the menu without
                        // selecting anything). When text_area gains
                        // double-tap word selection, mirror the
                        // text_input/code pattern and pass a closure
                        // that calls `text_edit::word_at_position`
                        // against the cursor's line.
                        crate::widgets::text_input::arm_long_press_timer(
                            ctx.bounds_x + click_x,
                            ctx.bounds_y + click_y,
                            ctx.bounds_height.clamp(24.0, 48.0),
                            None,
                        );
                    }

                    true // needs refresh
                }; // Lock released here

                // Trigger incremental refresh AFTER releasing the data lock
                if needs_refresh {
                    notify_text_area(&data_for_click);
                }
            })
            // Mouse drag to extend selection.
            //
            // Mirrors text_input's on_drag at text_input.rs ~2491-2560:
            // while pointer is pressed, recompute cursor position from
            // the current local_x / local_y, then if drag_select_anchor
            // is Some (seeded by on_mouse_down for desktop mouse), set
            // selection_start = Some(anchor) and cursor = new_pos so
            // the selection-rendering pass at
            // text_area.rs:2217-2277 draws the highlight rects.
            // Touch leaves anchor = None (set in on_mouse_down) so
            // dragging on touch repositions the cursor instead of
            // selecting — matches text_input's touch UX.
            .on_drag(move |ctx| {
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
                    // Recompute the cursor position from the live drag
                    // coordinates. clicked_visual_line is set by the
                    // per-visual-line click handlers but only on
                    // POINTER_DOWN — during drag we fall through to the
                    // xy lookup. Subtract the left padding + border so
                    // text_x lands in text-content space; same correction
                    // the on_mouse_down handler applies.
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
                    } else if let Some(anchor) = d.drag_select_anchor {
                        if new_pos != anchor {
                            d.selection_start = Some(anchor);
                            d.cursor = new_pos;
                            true
                        } else {
                            // Drag still on the anchor — clear any
                            // residual selection so a zero-distance
                            // drag matches a plain click.
                            if d.selection_start.is_some() {
                                d.selection_start = None;
                                true
                            } else {
                                false
                            }
                        }
                    } else {
                        false
                    }
                };
                if needs_refresh {
                    notify_text_area(&data_for_drag);
                }
            })
            // Handle text input
            .on_event(event_types::TEXT_INPUT, move |ctx| {
                let (needs_refresh, change_signal, edited) = {
                    let mut d = match data_for_text.lock() {
                        Ok(d) => d,
                        Err(_) => return,
                    };

                    if d.disabled || !d.visual.is_focused() {
                        return;
                    }

                    // Skip control characters and modifier combos (Cmd+C/V/Z etc.)
                    if ctx.meta || ctx.ctrl {
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
                    let mod_key = ctx.meta || ctx.ctrl;

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
