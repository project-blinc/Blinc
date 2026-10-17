//! What the text input shows, bound to its state: the box's look, the text
//! in its runs, the scroll and the caret.

use super::*;

/// How the field's box looks in its state.
#[derive(Clone)]
pub(super) struct BoxLook {
    bg: Color,
    border: Color,
    border_width: f32,
    radius: f32,
    outline: (f32, Color, f32),
}

pub(super) fn box_look(cfg: &TextInputConfig, d: &TextInputData) -> BoxLook {
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
pub(super) fn shown_text(d: &TextInputData, cfg: &TextInputConfig) -> String {
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
pub(super) enum Run {
    Before,
    Selected,
    After,
}

pub(super) fn text_run(d: &TextInputData, cfg: &TextInputConfig, run: Run) -> String {
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
pub(super) fn caret_x(d: &mut TextInputData, cfg: &TextInputConfig) -> f32 {
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
pub(super) fn assemble(mut outer: Div, data: &SharedTextInputData, base: &TextInputConfig) -> Div {
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
pub(super) fn content(
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
                let opacity = cs.blink_opacity();
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
