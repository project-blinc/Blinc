//! `box-shadow` values.
//!
//! Parses a comma-separated list of layers, each `inset? && <length>{2,4} && <color>?`: the
//! offsets, then the blur radius and the spread when given, with `inset` and the colour before
//! or after the lengths. A theme's shadows reach CSS as the `--shadow-*` variables, which are
//! such lists.

use crate::parser::*;
use blinc_core::{Color, Shadow};

/// The outer and inner layers of a shadow value, kept apart.
///
/// `blinc_core::Shadow` carries no inset flag, so the two kinds travel
/// in separate lists: the paint walk draws the outer ones before the
/// fill and the inner ones after, clipped to the padding box.
#[derive(Debug, Default, Clone)]
pub(crate) struct ShadowLayers {
    pub outer: Vec<Shadow>,
    pub inner: Vec<Shadow>,
}

impl ShadowLayers {
    fn is_empty(&self) -> bool {
        self.outer.is_empty() && self.inner.is_empty()
    }
}

/// Parse a shadow value for a property that takes a single outer layer,
/// such as `text-shadow`. An `inset` layer is not one, so it is skipped.
pub(crate) fn parse_shadow(value: &str) -> Option<Shadow> {
    parse_shadow_layers(value).and_then(|l| l.outer.into_iter().next())
}

/// Parse a CSS shadow value into its outer and inner layers.
///
/// `none` is a single transparent outer shadow. As in CSS, one layer that does not parse
/// rejects the whole value.
pub(crate) fn parse_shadow_layers(value: &str) -> Option<ShadowLayers> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("none") {
        return Some(ShadowLayers {
            outer: vec![Shadow::new(0.0, 0.0, 0.0, Color::TRANSPARENT)],
            inner: Vec::new(),
        });
    }

    let mut layers = ShadowLayers::default();
    for part in split_commas_respecting_parens(trimmed) {
        let (shadow, inset) = parse_explicit_shadow(part.trim())?;
        if inset {
            layers.inner.push(shadow);
        } else {
            layers.outer.push(shadow);
        }
    }
    if layers.is_empty() {
        None
    } else {
        Some(layers)
    }
}

/// Parse one shadow layer, and return it with whether it is `inset`.
///
/// A layer with no colour, or `currentColor`, is black: the colour an element starts with.
pub(crate) fn parse_explicit_shadow(input: &str) -> Option<(Shadow, bool)> {
    let mut inset = false;
    let mut color = None;
    let mut lengths = Vec::new();
    // Set once something follows the lengths, which have to stand together.
    let mut lengths_ended = false;
    for part in split_whitespace_respecting_parens(input) {
        if part.eq_ignore_ascii_case("inset") {
            if inset {
                return None;
            }
            inset = true;
        } else if let Some(length) = parse_length_value(&part) {
            if lengths_ended {
                return None;
            }
            lengths.push(length);
            continue;
        } else {
            let parsed = if part.eq_ignore_ascii_case("currentcolor") {
                Color::BLACK
            } else {
                parse_color(&part)?
            };
            if color.replace(parsed).is_some() {
                return None;
            }
        }
        lengths_ended = !lengths.is_empty();
    }

    let (offset_x, offset_y, blur, spread) = match lengths[..] {
        [x, y] => (x, y, 0.0, 0.0),
        [x, y, blur] => (x, y, blur, 0.0),
        [x, y, blur, spread] => (x, y, blur, spread),
        _ => return None,
    };
    if blur < 0.0 {
        return None;
    }
    let mut shadow = Shadow::new(offset_x, offset_y, blur, color.unwrap_or(Color::BLACK));
    shadow.spread = spread;
    Some((shadow, inset))
}
