//! `border-top`, `border-right`, `border-bottom`, `border-left` and their
//! `-width`, `-color` and `-style` longhands.

use crate::element_style::{BorderSideStyle, ElementStyle};
use crate::parser::value::{parse_color, parse_css_px};

const SIDES: [&str; 4] = ["top", "right", "bottom", "left"];

fn side_mut<'a>(style: &'a mut ElementStyle, side: &str) -> &'a mut Option<BorderSideStyle> {
    match side {
        "top" => &mut style.border_top,
        "right" => &mut style.border_right,
        "bottom" => &mut style.border_bottom,
        _ => &mut style.border_left,
    }
}

/// Applies a per-side border declaration. `None` if `name` is not one;
/// otherwise whether the value parsed.
pub(crate) fn apply_border_side_property(
    style: &mut ElementStyle,
    name: &str,
    value: &str,
) -> Option<bool> {
    let rest = name.strip_prefix("border-")?;
    let (side, kind) = match rest.split_once('-') {
        Some((side, kind)) => (side, kind),
        None => (rest, ""),
    };
    if !SIDES.contains(&side) {
        return None;
    }
    let slot = side_mut(style, side).get_or_insert_with(BorderSideStyle::default);
    match kind {
        "" => {
            // [width] [style] [color], any order; `none` removes the side.
            let mut parsed = BorderSideStyle::default();
            for part in value.split_whitespace() {
                match part {
                    "none" | "hidden" => parsed.width = Some(0.0),
                    "solid" | "dashed" | "dotted" => {}
                    p => {
                        if let Some(px) = parse_css_px(p) {
                            parsed.width = Some(px);
                        } else if let Some(color) = parse_color(p) {
                            parsed.color = Some(color);
                        } else {
                            return Some(false);
                        }
                    }
                }
            }
            *slot = parsed;
            Some(true)
        }
        "width" => match parse_css_px(value) {
            Some(px) => {
                slot.width = Some(px.max(0.0));
                Some(true)
            }
            None => Some(false),
        },
        "color" => match parse_color(value) {
            Some(color) => {
                slot.color = Some(color);
                Some(true)
            }
            None => Some(false),
        },
        "style" => Some(true),
        _ => None,
    }
}

impl BorderSideStyle {
    /// Combines two declarations of one side; `other` wins per field.
    pub fn merge(self, other: Self) -> Self {
        Self {
            width: other.width.or(self.width),
            color: other.color.or(self.color),
        }
    }

    pub fn is_set(&self) -> bool {
        self.width.is_some() || self.color.is_some()
    }
}
