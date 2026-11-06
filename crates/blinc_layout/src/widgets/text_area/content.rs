//! What the text area shows, bound to its state: one row per visual line, and
//! the caret.

use super::*;

/// The text a visual line shows: its own text, or the placeholder on the
/// first line of an empty field.
pub(super) fn row_text(d: &TextAreaState, base: &TextAreaConfig, row: usize) -> String {
    if d.is_empty() {
        if row > 0 {
            String::new()
        } else if !d.placeholder.is_empty() {
            d.placeholder.clone()
        } else {
            base.placeholder.clone()
        }
    } else {
        d.visual_lines
            .get(row)
            .map(|vl| vl.text.clone())
            .unwrap_or_default()
    }
}

/// Where the selection's highlight sits on a visual line: its left edge
/// and width, `(0, 0)` where the line has none. Shown while focused.
pub(super) fn selection_span(d: &TextAreaState, row: usize) -> (f32, f32) {
    let none = (0.0, 0.0);
    if !d.visual.is_focused() || d.ime.composing.is_some() {
        return none;
    }
    let Some(sel_start) = d.selection_start else {
        return none;
    };
    let (start, end) = d.order_positions(sel_start, d.cursor);
    let Some(vl) = d.visual_lines.get(row) else {
        return none;
    };
    if start == end || vl.logical_line < start.line || vl.logical_line > end.line {
        return none;
    }
    let line_chars = d
        .lines
        .get(vl.logical_line)
        .map(|l| l.chars().count())
        .unwrap_or(0);
    let from = if vl.logical_line == start.line {
        start.column
    } else {
        0
    };
    let to = if vl.logical_line == end.line {
        end.column
    } else {
        line_chars
    };
    // The part of the selection on this visual line.
    let from = from.max(vl.start_char).min(vl.end_char);
    let to = to.max(vl.start_char).min(vl.end_char);
    if to <= from {
        return none;
    }
    let width_to = |col: usize| {
        let prefix: String = vl.text.chars().take(col - vl.start_char).collect();
        crate::text_measure::measure_text(&prefix, d.font_size).width
    };
    let (x0, x1) = (width_to(from), width_to(to));
    // A selection running on past this line ends in a sliver for the break.
    let runs_on =
        to >= vl.end_char.min(line_chars) && !(vl.logical_line == end.line && to == end.column);
    let sliver = if runs_on { d.font_size * 0.5 } else { 0.0 };
    (x0, x1 - x0 + sliver)
}

/// The field as built, around the content. The box is the element: it carries
/// `disabled` as an attribute bound to the state's revision, and the
/// `.blinc-input` rules do the rest.
pub(super) fn assemble(mut outer: Div, state: &SharedTextAreaState, base: &TextAreaConfig) -> Div {
    use blinc_core::reactive::{ReactiveGraph, computed, signal};

    let revision = {
        let mut d = state.lock().unwrap();
        // Laid out for the state the field starts in.
        let cfg = resolved_config(base, &d);
        take_metrics(&mut d, &cfg);
        d.compute_visual_lines();
        *d.revision.get_or_insert_with(|| signal(0u64))
    };

    let disabled = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            state.lock().unwrap().disabled
        })
    };
    let invalid = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            state.lock().unwrap().invalid
        })
    };
    let cfg = resolved_config(base, &state.lock().unwrap());
    outer = outer
        .attr_when("disabled", "", &disabled)
        .attr_when("data-invalid", "", &invalid)
        .border_width(cfg.border_width)
        .rounded(cfg.corner_radius);

    outer.child(content(state, &cfg, base, revision))
}

/// The inside of the field: one row per visual line in a vertical scroll,
/// and the caret, all bound to the revision.
pub(super) fn content(
    state: &SharedTextAreaState,
    cfg: &TextAreaConfig,
    base: &TextAreaConfig,
    revision: blinc_core::reactive::Signal<u64>,
) -> Div {
    use blinc_core::reactive::{ReactiveGraph, computed};

    let line_height = cfg.font_size * cfg.line_height;
    let cursor_height = cfg.font_size * 1.2;

    let text_color = {
        let (state, base) = (Arc::clone(state), base.clone());
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            let d = state.lock().unwrap();
            let cfg = resolved_config(&base, &d);
            if d.is_empty() {
                cfg.placeholder_color
            } else if d.disabled {
                Color::rgba(0.4, 0.4, 0.4, 1.0)
            } else {
                cfg.text_color
            }
        })
    };
    let rows = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            let count = state.lock().unwrap().visual_lines.len().max(1);
            (0..count).collect::<Vec<usize>>()
        })
    };
    // Wrapped rows are as wide as the text area; unwrapped ones as their text.
    let row_width = cfg.wrap.then(|| {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            state.lock().unwrap().available_width
        })
    });

    let row = {
        let (state, base) = (Arc::clone(state), base.clone());
        let (font_size, selection_color) = (cfg.font_size, cfg.selection_color);
        move |index: usize| {
            text_row(
                &state,
                &base,
                revision,
                index,
                line_height,
                font_size,
                selection_color,
                row_width.as_ref(),
                &text_color,
            )
        }
    };

    // The caret, moved to the cursor's line and column. Drawn while focused.
    if let Ok(mut cs) = state.lock().unwrap().cursor_state.lock() {
        cs.color = cfg.cursor_color;
        cs.animation = CursorAnimation::SmoothFade;
    }
    let at = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            let d = state.lock().unwrap();
            let (x, y) = d.cursor_position_from_visual_lines();
            // Fonts keep descender space at the bottom of the line, which
            // sits the glyphs high: the caret is raised to match.
            let top = y + (d.line_height - cursor_height) / 2.0 - d.font_size * 0.1;
            blinc_core::Transform::translate(x, top)
        })
    };
    let state_for_caret = Arc::clone(state);
    let state_for_frames = Arc::clone(state);
    let caret = canvas(
        move |ctx: &mut dyn blinc_core::DrawContext, bounds: crate::canvas::CanvasBounds| {
            let (shown, cursor_state) = {
                let d = state_for_caret.lock().unwrap();
                (d.visual.is_focused(), Arc::clone(&d.cursor_state))
            };
            if !shown {
                return;
            }
            let cs = cursor_state.lock().unwrap();
            let opacity = cs.blink_opacity();
            if opacity < 0.01 {
                return;
            }
            let color = Color::rgba(cs.color.r, cs.color.g, cs.color.b, cs.color.a * opacity);
            ctx.fill_rect(
                blinc_core::Rect::new(0.0, 0.0, cs.width, bounds.height),
                blinc_core::CornerRadius::default(),
                blinc_core::Brush::Solid(color),
            );
        },
    )
    .redraw_while(move || state_for_frames.lock().unwrap().visual.is_focused())
    .w(2.0)
    .h(cursor_height);

    let text_content = div()
        .flex_col()
        .justify_start()
        .items_start()
        .relative()
        .overflow_visible()
        .for_each(&rows, |index| *index, row)
        .child(
            div()
                .absolute()
                .left(0.0)
                .top(0.0)
                .w(2.0)
                .h(cursor_height)
                .transform(&at)
                .child(caret),
        );

    // The scroll clips the rows and carries the vertical offset; the outer
    // box does the rounding.
    let scrollable_content =
        Scroll::with_physics(Arc::clone(&state.lock().unwrap().scroll_physics))
            .direction(ScrollDirection::Vertical)
            .no_bounce()
            .flex_grow()
            .child(text_content);

    // Heights are explicit so the box has proper intrinsic dimensions; the
    // width is only pinned when the caller asked for one. A measured width
    // must never be emitted here -- see `TextAreaConfig::width_is_explicit`.
    // Left auto, the wrapper is stretched by its parent, which is what a
    // full-width field wants and what a `w_fit()` ancestor can shrink.
    let (padding_x, padding_y) = (cfg.padding_x, cfg.padding_y);
    let pin_width = cfg.width_is_explicit;
    let content_width = cfg.effective_width();
    let content_height = cfg.effective_height();
    let inner_height = content_height - padding_y * 2.0;
    let pinned = |d: Div| if pin_width { d.w(content_width) } else { d };

    pinned(div().flex_col())
        .h(content_height)
        .child(pinned(div().h(padding_y)))
        .child(
            pinned(div().flex_row().h(inner_height))
                .child(div().w(padding_x).h(inner_height))
                .child(scrollable_content)
                .child(div().w(padding_x).h(inner_height)),
        )
        .child(pinned(div().h(padding_y)))
}

/// One visual line: its text, and the selection's highlight behind it.
#[allow(clippy::too_many_arguments)]
pub(super) fn text_row(
    state: &SharedTextAreaState,
    base: &TextAreaConfig,
    revision: blinc_core::reactive::Signal<u64>,
    index: usize,
    line_height: f32,
    font_size: f32,
    selection_color: Color,
    width: Option<&blinc_core::reactive::Computed<f32>>,
    color: &blinc_core::reactive::Computed<Color>,
) -> Div {
    use crate::binding::IntoReactive;
    use blinc_core::reactive::{ReactiveGraph, computed};

    let run = |part: usize| {
        let (state, base) = (Arc::clone(state), base.clone());
        let text = computed(move |g: &ReactiveGraph| {
            g.get(revision);
            super::ime::row_run(&state.lock().unwrap(), &base, index, part)
        });
        let text = crate::text::Text::bound(text.into_reactive())
            .size(font_size)
            .color(color)
            .text_left()
            .no_wrap();
        if part == 1 {
            text.class("blinc-ime-composition")
        } else {
            text
        }
    };
    let highlight_at = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            let (x, _) = selection_span(&state.lock().unwrap(), index);
            blinc_core::Transform::translate(x, 0.0)
        })
    };
    let highlight_width = {
        let state = Arc::clone(state);
        computed(move |g: &ReactiveGraph| {
            g.get(revision);
            selection_span(&state.lock().unwrap(), index).1
        })
    };
    // Tells the field's own handler which line a press landed on.
    let state_for_press = Arc::clone(state);
    let row = div()
        .h(line_height)
        .flex_row()
        .items_center()
        .relative()
        .on_mouse_down(move |_| {
            if let Ok(mut d) = state_for_press.lock() {
                d.clicked_visual_line = Some(index);
            }
        })
        .child(
            div()
                .absolute()
                .left(0.0)
                .top(0.0)
                .h(line_height)
                .w(&highlight_width)
                .transform(&highlight_at)
                .bg(selection_color)
                .rounded(2.0),
        )
        .child(run(0))
        .child(run(1))
        .child(run(2));
    match width {
        Some(width) => row.w(width),
        None => row,
    }
}
