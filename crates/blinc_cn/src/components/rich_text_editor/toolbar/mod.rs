//! The rich text editor's formatting toolbar, built from cn components.
//!
//! [`ToolbarPlacement::Floating`] shows it over a non-empty selection, with a pointer toward the
//! selection and a button that closes it by collapsing the selection. [`ToolbarPlacement::Fixed`]
//! shows it above the text all the time; when the editor is narrower than its controls, they
//! scroll between two buttons. Both hold a select for the block's kind, a button for each boolean
//! mark, the cn colour picker and a link button that opens a popover for the link.
//!
//! A press anywhere in the toolbar marks the editor's next press as handled, so the editor keeps
//! its selection and its focus while a control acts on them.

mod actions;
mod controls;
mod link;

use std::sync::Arc;

use blinc_core::reactive::computed;
use blinc_core::{Shadow, Signal};
use blinc_icons::icons;
use blinc_layout::div::{Div, div};
use blinc_layout::notch::notch;
use blinc_layout::selector::ScrollBehavior;
use blinc_layout::widgets::scroll::scroll as scroll_view;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

use controls::{BUTTON, DIVIDER, GAP, MARKS, Target};

use super::document::BlockKind;
use super::render::RichTextTheme;
use super::state::{RichTextData, RichTextState};

/// Where the formatting toolbar sits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolbarPlacement {
    /// Over the selection, while there is one.
    #[default]
    Floating,
    /// Above the text, all the time.
    Fixed,
    /// Nowhere; formatting comes from shortcuts or the caller's own controls.
    Hidden,
}

/// Padding inside the toolbar, across and down.
const PAD_X: f32 = 6.0;
const PAD_Y: f32 = 4.0;
/// The height of the row of buttons.
const ROW: f32 = BUTTON + 2.0;
/// The width and height of the floating toolbar's peak.
const PEAK_W: f32 = 14.0;
const PEAK_H: f32 = 7.0;
/// The space between the peak's tip and the selection.
const PEAK_GAP: f32 = 2.0;
/// How far a press on a scroll button moves a toolbar's controls.
const SCROLL_STEP: f32 = 120.0;
/// The floating toolbar's least width: in a narrower editor it reaches past the editor's edge.
const MIN_FLOATING: f32 = 200.0;

/// What the toolbar shows: it is rebuilt when this changes.
///
/// The colour is left out: the picker shows it through its own signal, and a rebuild while the
/// picker is open would close it on each colour it reports. The link popover's open state is left
/// out for the same reason.
pub(super) fn signature(data: &RichTextData) -> String {
    let f = &data.active_format;
    let kind = data.document.blocks.get(data.cursor.block).map(|b| &b.kind);
    format!(
        "{}{}{}{}{}{}|{:?}",
        f.bold as u8,
        f.italic as u8,
        f.underline as u8,
        f.strikethrough as u8,
        f.code as u8,
        f.link.is_some() as u8,
        kind
    )
}

/// A press inside the toolbar leaves the editor's selection where it is.
fn keep_selection(bar: Div, state: &RichTextState) -> Div {
    let state = Arc::clone(state);
    bar.on_mouse_down(move |_| {
        if let Ok(mut data) = state.lock() {
            data.suppress_next_outer_click = true;
        }
    })
}

/// The width of each scroll fade.
const FADE: f32 = 24.0;

/// A toolbar's controls in `room`: the block kind's menu `lead`, `lead_w` wide, then the other
/// controls `rest`, `rest_w` wide. Where they do not fit, the menu stays put and the rest scroll
/// between two buttons, faded at an edge with more beyond it. The menu sits outside the scroll
/// because a scroll clips its content, and the menu's list hangs below it. The scroll position is
/// kept in `scroll` across rebuilds.
fn controls_lane(
    target: &Target,
    (lead, lead_w): (Div, f32),
    (rest, rest_w): (Div, f32),
    room: f32,
    scroll: &blinc_layout::selector::ScrollRef,
) -> Div {
    let row = div()
        .flex_row()
        .items_center()
        .gap_px(GAP)
        .h(ROW)
        .child(lead);
    if lead_w + GAP + rest_w <= room {
        return row.child(rest);
    }
    // A rebuild makes a new scroll; it starts where the last one was.
    let x = scroll.scroll_x();
    if x > 0.0 {
        scroll.set_scroll_offset(x, 0.0);
    }
    let step = |icon, label, dx: f32| {
        let scroll = scroll.clone();
        controls::icon_button(target, icon, label, false, move |_| {
            scroll.scroll_by_with_behavior(dx, 0.0, ScrollBehavior::Smooth);
            blinc_layout::stateful::request_redraw();
        })
    };
    // Which edges have more beyond them, for the fades.
    let offset = scroll.watch_x(x);
    let more_start = computed(move |g| g.get(offset).unwrap_or(0.0) > 1.0);
    let more_end = {
        let scroll = scroll.clone();
        computed(move |g| {
            let x = g.get(offset).unwrap_or(0.0);
            scroll.max_scroll().is_none_or(|(max_x, _)| x < max_x - 1.0)
        })
    };
    row.w_full()
        // A scroll's offset grows negative as its content moves left, so a positive step goes
        // back toward the start.
        .child(step(icons::CHEVRON_LEFT, "Scroll left", SCROLL_STEP))
        // The lane takes the width between the buttons, whatever the toolbar's width.
        .child(
            div()
                .class("blinc-rte-lane")
                .attr_when("data-more-start", "", more_start)
                .attr_when("data-more-end", "", more_end)
                .flex_grow()
                .min_w(0.0)
                .h(ROW)
                .overflow_clip()
                .child(
                    scroll_view()
                        .horizontal()
                        .scrollbar_hidden()
                        .w_full()
                        .h(ROW)
                        .bind(scroll)
                        .child(rest.h(ROW).flex_shrink_0()),
                ),
        )
        .child(step(icons::CHEVRON_RIGHT, "Scroll right", -SCROLL_STEP))
}

/// The block kind's menu and the divider after it, and their width, `compact` for the floating
/// toolbar.
fn lead(target: &Target, kind: &BlockKind, compact: bool) -> (Div, f32) {
    let width = if compact {
        controls::HEADING_COMPACT
    } else {
        controls::HEADING
    };
    let lead = div()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .gap_px(GAP)
        .child(controls::heading(target, kind, compact))
        .child(controls::divider());
    (lead, width + GAP + DIVIDER)
}

/// The width of `buttons` buttons and `dividers` dividers in a row, with a gap between each two.
fn row_width(buttons: usize, dividers: usize) -> f32 {
    buttons as f32 * BUTTON + dividers as f32 * DIVIDER + (buttons + dividers - 1) as f32 * GAP
}

/// The floating toolbar over the selection, or `None` without a non-empty selection or when the
/// toolbar is placed elsewhere.
///
/// It is a notch whose peak points at the middle of the selection. A notch draws its own fill, so
/// it takes the theme's tokens rather than a stylesheet rule.
pub fn selection_toolbar(
    state: &RichTextState,
    version: &Signal<u32>,
    theme: &RichTextTheme,
) -> Option<Div> {
    if theme.toolbar != ToolbarPlacement::Floating {
        return None;
    }
    let mut data = state.lock().ok()?;
    let (x, y, w, h) = data.selection_bounds()?;
    let format = data.active_format.clone();
    let swatch = data.swatch();
    let kind = data
        .document
        .blocks
        .get(data.cursor.block)
        .map_or(BlockKind::Paragraph, |b| b.kind.clone());

    // The block kind's menu, then the marks, colour, link and close, with dividers after the marks
    // and before close.
    let rest_w = row_width(MARKS.len() + 3, 2);
    let lead_w = controls::HEADING_COMPACT + GAP + DIVIDER;
    let row_w = lead_w + GAP + rest_w;
    // As wide as its controls, or as the editor when that is narrower.
    let width = (row_w + 2.0 * PAD_X).min(data.wrap_width.max(MIN_FLOATING));
    let scroll = data.bar_scroll.clone();
    let body = ROW + 2.0 * PAD_Y;
    // The notch's box holds the body and the peak.
    let height = body + PEAK_H;
    let center = x + w * 0.5;
    // Inside the editor where it fits, from the left edge where it does not.
    let left = (center - width * 0.5).min(data.wrap_width - width).max(0.0);
    // Above the selection, or below it when the window has no room above. The toolbar may reach
    // above the editor; the editor's place in the window is known once it has been pressed.
    let room_above = data.editor_bounds.1 + data.content_top + y;
    let above = room_above >= height + PEAK_GAP;
    let top = if above {
        y - height - PEAK_GAP
    } else {
        y + h + PEAK_GAP
    };
    // Newly shown, as opposed to rebuilt for a pressed mark or a moved selection.
    let appearing = data.toolbar_rect.is_none();
    data.toolbar_rect = Some((left, top, width, height));
    drop(data);

    let target = Target {
        state: Arc::clone(state),
        version: *version,
    };
    let mut rest = div().flex_row().items_center().gap_px(GAP);
    for button in controls::marks(&target, &format) {
        rest = rest.child(button);
    }
    rest = rest
        .child(controls::divider())
        .child(controls::color(&target, swatch))
        .child(link::link_button(&target, format.link.is_some()))
        .child(controls::divider())
        .child(controls::close(&target));
    let row = controls_lane(
        &target,
        lead(&target, &kind, true),
        (rest, rest_w),
        width - 2.0 * PAD_X,
        &scroll,
    );

    let tokens = ThemeState::get();
    let radius = tokens.radius(RadiusToken::Xl).min(body * 0.5);
    // The peak's base stays on the straight part of the edge.
    let reach = (width * 0.5 - radius - PEAK_W * 0.5).max(0.0);
    let offset = (center - left - width * 0.5).clamp(-reach, reach);
    let mut shape = notch()
        .w_full()
        .h_full()
        .rounded(radius)
        .bg(tokens.color(ColorToken::SurfaceElevated))
        .border(1.0, tokens.color(ColorToken::Border))
        .px(PAD_X)
        .flex_col()
        .justify_center();
    if let Some(s) = tokens.shadows().shadow_lg.first() {
        shape = shape.shadow(Shadow::new(s.offset_x, s.offset_y, s.blur, s.color));
    }
    shape = if above {
        shape
            .peak_bottom_at(offset, PEAK_W, PEAK_H)
            .pt(PAD_Y)
            .pb(PAD_Y + PEAK_H)
    } else {
        shape
            .peak_top_at(offset, PEAK_W, PEAK_H)
            .pt(PAD_Y + PEAK_H)
            .pb(PAD_Y)
    };
    shape = shape.child(row);
    let mut toolbar = div().class("blinc-rte-toolbar");
    if appearing {
        // Fades in and grows out of the side that faces the selection.
        toolbar = toolbar.class("blinc-rte-toolbar--enter").class(if above {
            "blinc-rte-toolbar--above"
        } else {
            "blinc-rte-toolbar--below"
        });
    }
    let toolbar = toolbar
        .absolute()
        .left(left)
        .top(top)
        .w(width)
        .h(height)
        // On the foreground layer, so its fill covers the text under it.
        .foreground()
        .child(shape);
    Some(keep_selection(toolbar, state))
}

/// The height of the fixed toolbar, which the editor's text sits below.
pub(super) fn fixed_height(_data: &RichTextData) -> f32 {
    ROW + 2.0 * PAD_Y + 1.0
}

/// The width of the fixed toolbar's controls other than the block kind's menu: the marks, a
/// divider, colour and link.
fn fixed_rest_width() -> f32 {
    row_width(MARKS.len() + 2, 1)
}

/// Whether the fixed toolbar's controls overflow an editor `width` wide, and so scroll.
pub(super) fn fixed_overflows(width: f32) -> bool {
    controls::HEADING + GAP + DIVIDER + GAP + fixed_rest_width() > width - 2.0 * PAD_X
}

/// The fixed toolbar above the text: the block kind's menu, the marks, colour and link. When the
/// editor is narrower than the controls, they scroll between two buttons.
pub(super) fn fixed_toolbar(state: &RichTextState, version: &Signal<u32>, pad: f32) -> Div {
    let Ok(mut data) = state.lock() else {
        return div();
    };
    let format = data.active_format.clone();
    let swatch = data.swatch();
    let kind = data
        .document
        .blocks
        .get(data.cursor.block)
        .map_or(BlockKind::Paragraph, |b| b.kind.clone());
    let height = fixed_height(&data);
    let data_width = data.wrap_width + 2.0 * pad;
    let scroll = data.bar_scroll.clone();
    drop(data);

    let target = Target {
        state: Arc::clone(state),
        version: *version,
    };
    let mut rest = div().flex_row().items_center().gap_px(GAP);
    for button in controls::marks(&target, &format) {
        rest = rest.child(button);
    }
    rest = rest
        .child(controls::divider())
        .child(controls::color(&target, swatch))
        .child(link::link_button(&target, format.link.is_some()));
    let controls_row = controls_lane(
        &target,
        lead(&target, &kind, false),
        (rest, fixed_rest_width()),
        data_width - 2.0 * PAD_X,
        &scroll,
    );

    let bar = div()
        .class("blinc-rte-fixed-toolbar")
        .w_full()
        .h(height)
        .padding_x_px(PAD_X)
        .padding_y_px(PAD_Y)
        .flex_col()
        .child(controls_row);
    keep_selection(bar, state)
}
