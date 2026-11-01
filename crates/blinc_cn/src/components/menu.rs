//! The parts every cn menu is built from: the panel, an item's label,
//! shortcut and submenu chevron, and a separator.
//!
//! How they look is the `.cn-menu*` rules of `CN_STYLES`. The dropdown,
//! context and menubar menus add their own classes beside these for what
//! differs between them (enter animation, row padding).

use blinc_layout::div::{Div, div};
use blinc_layout::svg::svg;
use blinc_layout::text::text;

const CHEVRON_RIGHT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;

/// A menu panel; the caller sets its width. No padding, so the rows'
/// highlight runs to its rounded edge, which clips them; the radius is set
/// here for that clip.
pub(crate) fn panel(radius: f32) -> Div {
    div()
        .class("cn-menu")
        .flex_col()
        .rounded(radius)
        .overflow_clip()
        .h_fit()
}

/// The two sides of an item row: its icon and label, and its shortcut or, for
/// an item that opens a submenu, a chevron. A label that may not wrap widens
/// a panel sized to fit its rows.
pub(crate) fn item_parts(
    icon: Option<&str>,
    label: &str,
    shortcut: Option<&str>,
    has_submenu: bool,
    font_size: f32,
    gap: f32,
    wrap: bool,
) -> (Div, Option<Div>) {
    let mut left = div()
        .class("cn-menu-item-label")
        .w_fit()
        .h_fit()
        .flex_row()
        .items_center()
        .gap(gap);
    if let Some(icon) = icon {
        left = left.child(svg(icon).size(16.0, 16.0));
    }
    let mut label = text(label)
        .size(font_size)
        .no_cursor()
        .pointer_events_none();
    if !wrap {
        label = label.no_wrap();
    }
    let left = left.child(label).pointer_events_none();

    let right = if let Some(shortcut) = shortcut {
        Some(
            div()
                .class("cn-menu-shortcut")
                .child(text(shortcut).size(font_size - 2.0).monospace().no_cursor()),
        )
    } else if has_submenu {
        Some(
            div()
                .class("cn-menu-chevron")
                .child(svg(CHEVRON_RIGHT).size(12.0, 12.0))
                .pointer_events_none(),
        )
    } else {
        None
    };
    (left, right)
}

/// A line between groups of items.
pub(crate) fn separator() -> Div {
    div().class("cn-menu-separator").w_full().h(1.0)
}
