//! The editor's find and replace bar: an overlay of classed elements whose
//! look lives in [`super::CODE_EDITOR_STYLES`].

use super::*;

/// Close the search overlay and reset search state
pub(super) fn close_search_overlay(data: &mut CodeEditorData) {
    data.search_active = false;
    data.replace_active = false;
    data.search_query.clear();
    data.search_matches.clear();
    if let Some(handle) = data.search_overlay_handle.take() {
        if let Some(ctx) = crate::overlay_state::OverlayContext::try_get() {
            ctx.overlay_manager().lock().unwrap().close(handle);
        }
    }
    if let Ok(mut i) = data.search_input_state.lock() {
        i.value.clear();
        i.cursor = 0;
    }
    if let Ok(mut i) = data.replace_input_state.lock() {
        i.value.clear();
        i.cursor = 0;
    }
}

/// Open the find bar as an overlay, bound to a signal the editor bumps.
pub(super) fn open_search_overlay(shared_state: &SharedCodeEditorState, instance_key: &str) {
    use crate::overlay_state::get_overlay_manager;
    use crate::widgets::overlay::{OverlayAnimation, OverlayManagerExt};

    let mgr = get_overlay_manager();

    // Create a signal for search state changes
    let signal = blinc_core::context_state::use_signal_keyed(
        &format!("{}:search_signal", instance_key),
        || 0u64,
    );
    let signal_id = signal.id();

    // Store signal ID in data so button callbacks can fire it
    {
        let mut d = shared_state.lock().unwrap();
        d.search_signal_id = Some(signal_id);
    }

    // Position at top-right of editor bounds
    let (ex, ey, ew, _eh) = shared_state.lock().unwrap().editor_bounds;
    let bar_w = bar_width(shared_state.lock().unwrap().config.font_size);
    let overlay_x = (ex + ew - bar_w - 8.0).max(ex);
    let overlay_y = ey + 4.0;

    let state_for_content = Arc::clone(shared_state);
    let key = instance_key.to_string();

    let handle = mgr
        .dropdown()
        .at(overlay_x, overlay_y)
        .animation(OverlayAnimation::none())
        .dismiss_on_escape(true)
        .on_close({
            let state_for_close = Arc::clone(shared_state);
            move || {
                if let Ok(mut d) = state_for_close.lock() {
                    d.search_active = false;
                    d.replace_active = false;
                    d.search_overlay_handle = None;
                    d.search_signal_id = None;
                }
                // The matches leave the editor with the bar.
                refresh_editor(&state_for_close);
            }
        })
        .content(move || {
            let revision = blinc_core::reactive::Signal::<u64>::from_id(signal_id);
            build_search_bar(&state_for_content, &key, revision)
        })
        .show();

    shared_state.lock().unwrap().search_overlay_handle = Some(handle);
}

/// Width of the find row with its count showing: the padding, the chevron,
/// the field, the count and the three buttons, with their gaps.
fn bar_width(font_size: f32) -> f32 {
    let button = font_size + 12.0;
    let count = 76.0;
    16.0 + button * 4.0 + FIELD_WIDTH + count + 6.0 * 5.0
}

/// Redraw the editor itself: its matches and cursor follow the search.
pub(super) fn refresh_editor(shared_state: &SharedCodeEditorState) {
    let stateful = shared_state
        .lock()
        .ok()
        .and_then(|d| d.editor_stateful.clone());
    if let Some(stateful) = stateful {
        crate::stateful::refresh_stateful(&stateful);
    }
}

/// A change to the search: the bar and the editor both follow it.
fn notify_search(shared_state: &SharedCodeEditorState) {
    fire_search_signal(shared_state);
    refresh_editor(shared_state);
}

/// Repaint the search bar: what it shows is bound to the search signal.
pub(super) fn fire_search_signal(shared_state: &SharedCodeEditorState) {
    let signal_id = shared_state.lock().ok().and_then(|d| d.search_signal_id);
    if let Some(signal_id) = signal_id {
        blinc_core::reactive::Signal::<u64>::from_id(signal_id).update(|n| n.wrapping_add(1));
    }
}

/// Width of the find and replace fields.
const FIELD_WIDTH: f32 = 240.0;

// SVG icons for the bar (outline style using stroke)
const ICON_UP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 10l4-4 4 4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const ICON_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const ICON_CLOSE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 4l8 8M12 4l-8 8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>"#;
const ICON_REPLACE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M3 8h8m-3-3l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const ICON_REPLACE_ALL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M2 5h7m-3-3l3 3-3 3M2 11h7m-3-3l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

/// An icon button. Its look, hover and press are the stylesheet's, through
/// `.blinc-find-button` and the `class` that names it.
fn find_button(
    class: &'static str,
    size: f32,
    on_click: impl Fn() + Send + Sync + 'static,
    icon: impl ElementBuilder + 'static,
) -> Div {
    div()
        .class("blinc-find-button")
        .class(class)
        .w(size)
        .h(size)
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_click(move |_| on_click())
        .child(icon)
}

/// A text toggle (`Aa`, `ab`, `.*`), on while its option is. The stylesheet
/// styles `.blinc-find-toggle` and `.blinc-find-toggle--on`.
#[allow(clippy::too_many_arguments)]
fn find_toggle(
    label: &'static str,
    state: &SharedCodeEditorState,
    revision: blinc_core::reactive::Signal<u64>,
    is_on: fn(&CodeEditorData) -> bool,
    font_size: f32,
    height: f32,
    on_click: impl Fn() + Send + Sync + 'static,
) -> Div {
    use blinc_core::reactive::{ReactiveGraph, computed};
    let on = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            state.lock().map(|d| is_on(&d)).unwrap_or(false)
        })
    };
    div()
        .class("blinc-find-toggle")
        .class_when("blinc-find-toggle--on", &on)
        .h(height)
        .padding_x_px(5.0)
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_click(move |_| on_click())
        .child(text(label).size(font_size).monospace().no_wrap())
}

/// Build the find bar (VS Code style), once: the match count, the chevron,
/// the toggles' state and the replace row are bound to `revision`, which
/// [`fire_search_signal`] bumps.
pub(super) fn build_search_bar(
    shared_state: &SharedCodeEditorState,
    instance_key: &str,
    revision: blinc_core::reactive::Signal<u64>,
) -> Div {
    use crate::binding::IntoReactive;
    use blinc_core::reactive::{ReactiveGraph, computed};

    let _ = instance_key;
    let (config, search_state, replace_state) = {
        let data = shared_state.lock().unwrap();
        (
            data.config.clone(),
            Arc::clone(&data.search_input_state),
            Arc::clone(&data.replace_input_state),
        )
    };
    let small_font = config.font_size * 0.85;
    let icon_size = config.font_size + 2.0;
    let button = icon_size + 10.0;
    let text_col = config.text_color;
    let input_w = FIELD_WIDTH;

    // Something read from the editor state, each revision.
    let read = |f: fn(&CodeEditorData) -> bool| {
        let state = Arc::clone(shared_state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            state.lock().map(|d| f(&d)).unwrap_or(false)
        })
    };
    let replace_open = read(|d| d.replace_active);
    let has_query = read(|d| !d.search_query.is_empty());
    let no_match = read(|d| !d.search_query.is_empty() && d.search_matches.is_empty());
    let match_info = {
        let state = Arc::clone(shared_state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            let Ok(d) = state.lock() else {
                return String::new();
            };
            if d.search_matches.is_empty() {
                "No results".to_string()
            } else {
                format!("{} of {}", d.search_match_idx + 1, d.search_matches.len())
            }
        })
    };

    // A click handler that changes the editor state, then repaints.
    let act = |f: fn(&mut CodeEditorData)| {
        let state = Arc::clone(shared_state);
        move || {
            if let Ok(mut d) = state.lock() {
                f(&mut d);
            }
            notify_search(&state);
        }
    };
    let icon = move |source: &'static str| {
        crate::svg::svg(source)
            .size(icon_size, icon_size)
            .color(text_col)
    };

    // --- Chevron toggle (expand/collapse replace) ---
    let chevron = {
        let replace_open = replace_open.clone();
        find_button(
            "blinc-find-chevron",
            button,
            act(|d| d.replace_active = !d.replace_active),
            div().show_or(
                &replace_open,
                move || icon(FOLD_EXPANDED_SVG),
                move || icon(FOLD_COLLAPSED_SVG),
            ),
        )
    };

    // --- Toggle buttons (Aa, ab, .*) — inside the find input's right edge ---
    let toggle =
        |label: &'static str, is_on: fn(&CodeEditorData) -> bool, flip: fn(&mut CodeEditorData)| {
            find_toggle(
                label,
                shared_state,
                revision,
                is_on,
                small_font,
                button - 8.0,
                act(flip),
            )
        };
    let toggles = div()
        .class("blinc-find-toggles")
        .absolute()
        .right(4.0)
        .top(0.0)
        .h_full()
        .flex_row()
        .items_center()
        .gap_px(2.0)
        .child(toggle(
            "Aa",
            |d| d.search_case_sensitive,
            |d| {
                d.search_case_sensitive = !d.search_case_sensitive;
                d.execute_search();
            },
        ))
        .child(toggle(
            "ab",
            |d| d.search_whole_word,
            |d| {
                d.search_whole_word = !d.search_whole_word;
                d.execute_search();
            },
        ))
        .child(toggle(
            ".*",
            |d| d.search_regex,
            |d| {
                d.search_regex = !d.search_regex;
                d.execute_search();
            },
        ));

    // The find field: Enter steps to the next match, Shift+Enter the previous.
    let find_input = text_input(&search_state)
        .class("blinc-find-input")
        .w_full()
        .text_size(small_font)
        .on_change({
            let state = Arc::clone(shared_state);
            move |query| {
                if let Ok(mut d) = state.lock() {
                    d.apply_search_query(query.to_string());
                }
                notify_search(&state);
            }
        })
        .on_submit({
            let state = Arc::clone(shared_state);
            move |shift| {
                if let Ok(mut d) = state.lock() {
                    if shift {
                        d.search_prev();
                    } else {
                        d.search_next();
                    }
                }
                notify_search(&state);
            }
        });
    let field = div()
        .class("blinc-find-field")
        .relative()
        .w(input_w)
        .child(find_input)
        .child(toggles);

    // --- Nav buttons ---
    let prev_btn = find_button(
        "blinc-find-prev",
        button,
        act(|d| d.search_prev()),
        icon(ICON_UP),
    );
    let next_btn = find_button(
        "blinc-find-next",
        button,
        act(|d| d.search_next()),
        icon(ICON_DOWN),
    );
    let close_btn = find_button(
        "blinc-find-close",
        button,
        {
            let state = Arc::clone(shared_state);
            move || {
                if let Ok(mut d) = state.lock() {
                    close_search_overlay(&mut d);
                }
                refresh_editor(&state);
            }
        },
        icon(ICON_CLOSE),
    );

    // === FIND ROW: [>] [input+toggles] [count] [↑] [↓] [×] ===
    // The count is a region of the row itself, so while it is hidden it
    // takes no gap.
    let find_row = div()
        .class("blinc-find-row")
        .flex_row()
        .items_center()
        .gap_px(4.0)
        .child(chevron)
        .child(field)
        .show(&has_query, move || {
            div()
                .class("blinc-find-count")
                .class_when("blinc-find-count--none", &no_match)
                .child(
                    crate::text::Text::bound(match_info.clone().into_reactive())
                        .size(small_font)
                        .no_wrap(),
                )
        })
        .child(prev_btn)
        .child(next_btn)
        .child(close_btn);

    // === REPLACE ROW: [spacer] [input] [⇄] [⇄⇄] ===
    let replace_row = {
        let state = Arc::clone(shared_state);
        move || {
            let act = |f: fn(&mut CodeEditorData)| {
                let state = Arc::clone(&state);
                move || {
                    if let Ok(mut d) = state.lock() {
                        f(&mut d);
                    }
                    notify_search(&state);
                }
            };
            let state_for_replace = Arc::clone(&state);
            let replace_input = div().class("blinc-find-field").w(input_w).child(
                text_input(&replace_state)
                    .class("blinc-find-input")
                    .w_full()
                    .text_size(small_font)
                    .on_change(move |text| {
                        if let Ok(mut d) = state_for_replace.lock() {
                            d.replace_text = text.to_string();
                        }
                    }),
            );
            let replace_btn = find_button(
                "blinc-find-replace",
                button,
                act(|d| d.replace_current()),
                icon(ICON_REPLACE),
            );
            let replace_all_btn = find_button(
                "blinc-find-replace-all",
                button,
                act(|d| d.replace_all()),
                icon(ICON_REPLACE_ALL),
            );
            // The spacer keeps the field under the find field.
            div()
                .class("blinc-find-row")
                .class("blinc-find-row--replace")
                .flex_row()
                .items_center()
                .gap_px(4.0)
                .child(div().w(button))
                .child(replace_input)
                .child(replace_btn)
                .child(replace_all_btn)
        }
    };

    // === CONTAINER (sized by content, not fixed) ===
    // The plain look here is what the bar has without the stylesheet.
    div()
        .class("blinc-find")
        .w_fit()
        .flex_col()
        .gap_px(4.0)
        .p_px(6.0)
        .bg(config.bg_color)
        .border(1.0, config.gutter_separator_color)
        .rounded(4.0)
        .shadow_md()
        .child(find_row)
        .show(&replace_open, replace_row)
}
