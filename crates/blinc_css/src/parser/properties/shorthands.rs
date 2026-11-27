//! Declarations that cover other declarations: `gap`, `overflow` and the
//! uniform `border` properties. As in CSS, each one drops what was declared
//! before it for the parts it covers.

use crate::element_style::{ElementStyle, StyleOverflow};
use crate::parser::util::split_whitespace_respecting_parens;
use crate::parser::value::{CalcParseResult, parse_color, parse_css_px, try_parse_calc};

/// Applies the declaration if `name` is one of these or an overflow axis.
/// `None` if it is not; otherwise whether the value parsed.
pub(crate) fn apply_shorthand(style: &mut ElementStyle, name: &str, value: &str) -> Option<bool> {
    match name {
        "gap" => Some(match parse_css_px(value) {
            Some(px) => {
                style.gap = Some(px);
                style.row_gap = None;
                style.column_gap = None;
                true
            }
            None => false,
        }),
        "overflow" => Some(match overflow(value) {
            Some(overflow) => {
                style.overflow = Some(overflow);
                style.overflow_x = None;
                style.overflow_y = None;
                true
            }
            None => false,
        }),
        "overflow-x" => Some(set_overflow(&mut style.overflow_x, value)),
        "overflow-y" => Some(set_overflow(&mut style.overflow_y, value)),
        "border" => Some(apply_border(style, value)),
        "border-width" => Some(apply_border_width(style, value)),
        "border-color" => Some(match parse_color(value) {
            Some(color) => {
                style.border_color = Some(color);
                style.reset_border_sides(false, true);
                true
            }
            None => false,
        }),
        _ => None,
    }
}

fn overflow(value: &str) -> Option<StyleOverflow> {
    match value.trim() {
        "hidden" | "clip" => Some(StyleOverflow::Clip),
        "visible" => Some(StyleOverflow::Visible),
        "scroll" | "auto" => Some(StyleOverflow::Scroll),
        _ => None,
    }
}

fn set_overflow(slot: &mut Option<StyleOverflow>, value: &str) -> bool {
    let parsed = overflow(value);
    if parsed.is_some() {
        *slot = parsed;
    }
    parsed.is_some()
}

/// `border: [width] [style] [color]` in any order. Borders are always solid,
/// and `none` or `hidden` is a width of zero.
fn apply_border(style: &mut ElementStyle, value: &str) -> bool {
    let mut width = None;
    let mut color = None;
    for part in split_whitespace_respecting_parens(value) {
        match part.as_str() {
            "none" | "hidden" => width = Some(0.0),
            "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "inset" | "outset" => {}
            part => {
                if let Some(px) = parse_css_px(part) {
                    width = Some(px);
                } else if let Some(parsed) = parse_color(part) {
                    color = Some(parsed);
                } else {
                    return false;
                }
            }
        }
    }
    style.border_width = width.or(style.border_width);
    style.border_color = color.or(style.border_color);
    style.reset_border_sides(true, true);
    true
}

fn apply_border_width(style: &mut ElementStyle, value: &str) -> bool {
    match try_parse_calc(value) {
        CalcParseResult::Dynamic(expr) => style
            .dynamic_properties
            .get_or_insert_with(Vec::new)
            .push(crate::element_style::DynamicProperty::BorderWidth(expr)),
        CalcParseResult::Static(px) => style.border_width = Some(px.max(0.0)),
        CalcParseResult::NotCalc => match parse_css_px(value) {
            Some(px) => style.border_width = Some(px),
            None => return false,
        },
    }
    style.reset_border_sides(true, false);
    true
}
