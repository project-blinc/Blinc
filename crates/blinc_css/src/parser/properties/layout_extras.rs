//! Layout declarations shared by both parsers: min and max sizes, padding and
//! margin with their longhands, `flex-basis`, and the per-axis gaps.

use crate::element_style::{ElementStyle, SpacingRect, StyleDimension};
use crate::parser::value::{parse_css_dimension, parse_css_px, parse_css_spacing};

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
        "aspect-ratio" => {
            let ratio = if value.trim() == "auto" {
                Some(0.0)
            } else {
                let mut parts = value.split('/').map(|v| v.trim().parse::<f32>().ok());
                let numerator = parts.next().flatten();
                let denominator = parts.next().unwrap_or(Some(1.0));
                match (numerator, denominator, parts.next()) {
                    (Some(a), Some(b), None)
                        if a.is_finite()
                            && b.is_finite()
                            && a > 0.0
                            && b > 0.0
                            && (a / b).is_finite() =>
                    {
                        Some(a / b)
                    }
                    _ => None,
                }
            };
            if let Some(ratio) = ratio {
                style.aspect_ratio = Some(ratio);
            }
            Some(ratio.is_some())
        }
        "flex-basis" => Some(set_size(&mut style.flex_basis, value)),
        "row-gap" => Some(set_px(&mut style.row_gap, value)),
        "column-gap" => Some(set_px(&mut style.column_gap, value)),
        "padding" => Some(apply_padding(style, value)),
        "padding-top" => Some(set_px(&mut padding_mut(style).top, value)),
        "padding-right" => Some(set_px(&mut padding_mut(style).right, value)),
        "padding-bottom" => Some(set_px(&mut padding_mut(style).bottom, value)),
        "padding-left" => Some(set_px(&mut padding_mut(style).left, value)),
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

/// The padding to declare a side on. A value that does not parse leaves it
/// declaring nothing.
fn padding_mut(style: &mut ElementStyle) -> &mut SpacingRect {
    style.padding.get_or_insert_with(SpacingRect::default)
}

fn apply_padding(style: &mut ElementStyle, value: &str) -> bool {
    match parse_css_spacing(value) {
        Some(rect) => {
            style.padding = Some(rect);
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
    style.margin = Some(SpacingRect::new(
        top.unwrap_or(0.0),
        right.unwrap_or(0.0),
        bottom.unwrap_or(0.0),
        left.unwrap_or(0.0),
    ));
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
    let m = style.margin.get_or_insert_with(SpacingRect::default);
    match side {
        0 => m.top = Some(px),
        1 => m.right = Some(px),
        2 => m.bottom = Some(px),
        _ => m.left = Some(px),
    }
    style.margin_auto[side] = auto;
    true
}

#[cfg(test)]
mod aspect_tests {
    use super::*;
    #[test]
    fn ratios_accept_numbers_fractions_and_auto_but_reject_invalid_sizes() {
        for (value, expected) in [("16 / 9", 16.0 / 9.0), ("1.5", 1.5), ("auto", 0.0)] {
            let mut style = ElementStyle::default();
            assert_eq!(
                apply_layout_extra(&mut style, "aspect-ratio", value),
                Some(true)
            );
            assert_eq!(style.aspect_ratio, Some(expected));
            assert!(style.has_layout_props());
        }
        for value in ["0", "-1", "4/0", "NaN", "1/2/3", "1e38/1e-38"] {
            let mut style = ElementStyle::default();
            assert_eq!(
                apply_layout_extra(&mut style, "aspect-ratio", value),
                Some(false),
                "{value}"
            );
        }
    }
}
