//! The rich text editor's formatting toolbar, built from cn components.
//!
//! [`ToolbarPlacement::Floating`] shows it over a non-empty selection, with a pointer toward the
//! selection and a button that closes it by collapsing the selection. [`ToolbarPlacement::Fixed`]
//! shows it above the text all the time, with a menu for the block's kind. Both hold a button for
//! each boolean mark, the cn colour picker and a link button that opens a field for the URL.
//!
//! A press anywhere in the toolbar marks the editor's next press as handled, so the editor keeps
//! its selection and its focus while a control acts on them.

mod actions;
mod controls;
mod link;

use std::sync::Arc;

use blinc_core::{Color, Shadow, Signal};
use blinc_layout::div::{Div, div};
use blinc_layout::notch::notch;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

use controls::{BUTTON, DIVIDER, GAP, MARKS, Target};

use super::document::BlockKind;
use super::render::RichTextTheme;
use super::state::{PickerState, RichTextData, RichTextState};

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

/// What the toolbar shows: it is rebuilt when this changes.
pub(super) fn signature(data: &RichTextData) -> String {
    let kind = data.document.blocks.get(data.cursor.block).map(|b| &b.kind);
    format!("{:?}|{:?}|{:?}", data.active_format, data.picker, kind)
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
    let link_open = data.picker == PickerState::Link;

    // Marks, colour, link and close, with a divider after the marks and before close.
    let buttons = MARKS.len() + 3;
    let row_w = buttons as f32 * BUTTON + (buttons + 1) as f32 * GAP + 2.0 * DIVIDER;
    let width = row_w.max(if link_open { link::WIDTH } else { 0.0 }) + 2.0 * PAD_X;
    let body = ROW + if link_open { link::HEIGHT } else { 0.0 } + 2.0 * PAD_Y;
    // The notch's box holds the body and the peak.
    let height = body + PEAK_H;
    let center = x + w * 0.5;
    let left = (center - width * 0.5).max(0.0);
    // Above the selection, or below it when there is no room above.
    let above = y - height - PEAK_GAP >= 0.0;
    let top = if above {
        y - height - PEAK_GAP
    } else {
        y + h + PEAK_GAP
    };
    data.toolbar_rect = Some((left, top, width, height));
    drop(data);

    let target = Target {
        state: Arc::clone(state),
        version: *version,
    };
    let mut row = div().flex_row().items_center().gap_px(GAP);
    for button in controls::marks(&target, &format) {
        row = row.child(button);
    }
    row = row
        .child(controls::divider())
        .child(controls::color(
            &target,
            format.color.unwrap_or(Color::TRANSPARENT),
        ))
        .child(controls::link(&target, link_open))
        .child(controls::divider())
        .child(controls::close(&target));

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
    if link_open {
        shape = shape.child(link::field(&target));
    }
    let toolbar = div()
        .class("blinc-rte-toolbar")
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
pub(super) fn fixed_height(data: &RichTextData) -> f32 {
    ROW + 2.0 * PAD_Y
        + if data.picker == PickerState::Link {
            link::HEIGHT
        } else {
            0.0
        }
        + 1.0
}

/// The fixed toolbar above the text: the block kind's menu, the marks, colour and link.
pub(super) fn fixed_toolbar(state: &RichTextState, version: &Signal<u32>) -> Div {
    let Ok(data) = state.lock() else {
        return div();
    };
    let format = data.active_format.clone();
    let link_open = data.picker == PickerState::Link;
    let kind = data
        .document
        .blocks
        .get(data.cursor.block)
        .map_or(BlockKind::Paragraph, |b| b.kind.clone());
    let height = fixed_height(&data);
    drop(data);

    let target = Target {
        state: Arc::clone(state),
        version: *version,
    };
    let mut row = div()
        .flex_row()
        .items_center()
        .gap_px(GAP)
        .h(ROW)
        .child(controls::heading(&target, &kind))
        .child(controls::divider());
    for button in controls::marks(&target, &format) {
        row = row.child(button);
    }
    row = row
        .child(controls::divider())
        .child(controls::color(
            &target,
            format.color.unwrap_or(Color::TRANSPARENT),
        ))
        .child(controls::link(&target, link_open));

    let mut bar = div()
        .class("blinc-rte-fixed-toolbar")
        .w_full()
        .h(height)
        .padding_x_px(PAD_X)
        .padding_y_px(PAD_Y)
        .flex_col()
        .child(row);
    if link_open {
        bar = bar.child(link::field(&target));
    }
    keep_selection(bar, state)
}
