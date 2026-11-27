//! What a uniform border declaration does to the sides set below it.

use blinc_core::Color;

use crate::element::{BorderSide, BorderSides};

/// Applies a declared uniform width, colour or both to the sides an element
/// already holds. With both, no side is left of its own. A new width alone
/// brings back a side that was removed, in `uniform_color`.
pub(crate) fn reset_uniform_border_sides(
    sides: &mut BorderSides,
    uniform_color: Option<Color>,
    width: Option<f32>,
    color: Option<Color>,
) {
    if width.is_some() && color.is_some() {
        *sides = BorderSides::default();
        return;
    }
    let any = [sides.top, sides.right, sides.bottom, sides.left]
        .iter()
        .any(Option::is_some);
    if !any {
        return;
    }
    for slot in [
        &mut sides.top,
        &mut sides.right,
        &mut sides.bottom,
        &mut sides.left,
    ] {
        if let Some(width) = width {
            *slot = slot
                .map(|side| side.color)
                .or(uniform_color)
                .filter(|_| width > 0.0)
                .map(|color| BorderSide::new(width, color));
        } else if let (Some(color), Some(side)) = (color, slot.as_mut()) {
            side.color = color;
        }
    }
}
