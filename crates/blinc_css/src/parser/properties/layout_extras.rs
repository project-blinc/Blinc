//! Layout declarations that take more than a pixel length: percent and
//! `none` for min and max sizes, `auto` margins, `flex-basis`, and the
//! per-axis gaps.

use crate::element_style::{ElementStyle, StyleDimension};
use crate::parser::value::{parse_css_dimension, parse_css_px};

/// Applies the declaration if `name` is one of these. `None` if it is not;
/// otherwise whether the value parsed.
pub(crate) fn apply_layout_extra(
    style: &mut ElementStyle,
    name: &str,
    value: &str,
) -> Option<bool> {
    match name {
        "min-width" => Some(set_size(&mut style.min_width, value)),
        "min-height" => Some(set_size(&mut style.min_height, value)),
        "max-width" => Some(set_size(&mut style.max_width, value)),
        "max-height" => Some(set_size(&mut style.max_height, value)),
        "flex-basis" => Some(set_size(&mut style.flex_basis, value)),
        "row-gap" => Some(set_px(&mut style.row_gap, value)),
        "column-gap" => Some(set_px(&mut style.column_gap, value)),
        "margin" => Some(apply_margin(style, value)),
        "margin-top" => Some(apply_margin_side(style, 0, value)),
        "margin-right" => Some(apply_margin_side(style, 1, value)),
        "margin-bottom" => Some(apply_margin_side(style, 2, value)),
        "margin-left" => Some(apply_margin_side(style, 3, value)),
        _ => None,
    }
}

/// A min or max size: a length, a percentage, or `none`/`auto` for no limit.
fn set_size(slot: &mut Option<StyleDimension>, value: &str) -> bool {
    let v = value.trim();
    let dim = if v.eq_ignore_ascii_case("none") {
        Some(StyleDimension::Auto)
    } else {
        parse_css_dimension(v)
    };
    match dim {
        Some(d) => {
            *slot = Some(d);
            true
        }
        None => false,
    }
}

fn set_px(slot: &mut Option<f32>, value: &str) -> bool {
    match parse_css_px(value) {
        Some(px) => {
            *slot = Some(px);
            true
        }
        None => false,
    }
}

/// One to four margin values, each a length or `auto`, expanded as CSS does.
fn apply_margin(style: &mut ElementStyle, value: &str) -> bool {
    let mut parsed = Vec::new();
    for part in value.split_whitespace() {
        if part.eq_ignore_ascii_case("auto") {
            parsed.push(None);
        } else if let Some(px) = parse_css_px(part) {
            parsed.push(Some(px));
        } else {
            return false;
        }
    }
    let [top, right, bottom, left] = match parsed.as_slice() {
        [a] => [*a, *a, *a, *a],
        [v, h] => [*v, *h, *v, *h],
        [t, h, b] => [*t, *h, *b, *h],
        [t, r, b, l] => [*t, *r, *b, *l],
        _ => return false,
    };
    let sides = [top, right, bottom, left];
    style.margin = Some(crate::element_style::SpacingRect {
        top: top.unwrap_or(0.0),
        right: right.unwrap_or(0.0),
        bottom: bottom.unwrap_or(0.0),
        left: left.unwrap_or(0.0),
    });
    style.margin_auto = [
        sides[0].is_none(),
        sides[1].is_none(),
        sides[2].is_none(),
        sides[3].is_none(),
    ];
    true
}

fn apply_margin_side(style: &mut ElementStyle, side: usize, value: &str) -> bool {
    let auto = value.trim().eq_ignore_ascii_case("auto");
    let px = if auto {
        0.0
    } else {
        match parse_css_px(value) {
            Some(px) => px,
            None => return false,
        }
    };
    let mut m = style.margin.unwrap_or(crate::element_style::SpacingRect {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    });
    match side {
        0 => m.top = px,
        1 => m.right = px,
        2 => m.bottom = px,
        _ => m.left = px,
    }
    style.margin = Some(m);
    style.margin_auto[side] = auto;
    true
}
