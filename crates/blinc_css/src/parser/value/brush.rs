//! Fills, corner radii, and render layers.

use blinc_core::{Brush, CornerRadius, CornerShape, ImageBrush};
use nom::{
    IResult, branch::alt, bytes::complete::tag_no_case, combinator::value,
    error::ParseError as NomParseError,
};

use crate::material::RenderLayer;
use crate::parser::*;
use crate::units::Length;

pub(crate) fn parse_brush(value: &str) -> Option<Brush> {
    let trimmed = value.trim();

    // Try url() for background images
    if trimmed.starts_with("url(") {
        if let Some(source) = parse_url_value(trimmed) {
            return Some(Brush::Image(ImageBrush::new(source)));
        }
    }

    // Try linear-gradient()
    if trimmed.starts_with("linear-gradient(") {
        return parse_linear_gradient(trimmed).map(Brush::Gradient);
    }

    // Try radial-gradient()
    if trimmed.starts_with("radial-gradient(") {
        return parse_radial_gradient(trimmed).map(Brush::Gradient);
    }

    // Try conic-gradient()
    if trimmed.starts_with("conic-gradient(") {
        return parse_conic_gradient(trimmed).map(Brush::Gradient);
    }

    // Try parsing as color
    parse_color(trimmed).map(Brush::Solid)
}

/// Parse `url("path")` or `url('path')` or `url(path)` and return the inner path string.
pub(crate) fn parse_url_value(value: &str) -> Option<String> {
    let inner = value.strip_prefix("url(")?.strip_suffix(')')?.trim();
    // Strip optional quotes
    let path = if (inner.starts_with('"') && inner.ends_with('"'))
        || (inner.starts_with('\'') && inner.ends_with('\''))
    {
        &inner[1..inner.len() - 1]
    } else {
        inner
    };
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

pub(crate) fn parse_radius(value: &str) -> Option<CornerRadius> {
    // Two to four values name the corners: top-left, top-right,
    // bottom-right, bottom-left, repeated as CSS does for the missing ones.
    let words = crate::parser::util::split_whitespace_respecting_parens(value);
    if (2..=4).contains(&words.len()) {
        let corners = words
            .iter()
            .map(|w| {
                parse_css_length(w).map(|l| match l {
                    Length::Pct(v) if v > 0.0 => 9999.0,
                    Length::Pct(_) => 0.0,
                    other => other.to_px(),
                })
            })
            .collect::<Option<Vec<f32>>>()?;
        let [tl, tr, br, bl] = match corners.as_slice() {
            [a, b] => [*a, *b, *a, *b],
            [a, b, c] => [*a, *b, *c, *b],
            [a, b, c, d] => [*a, *b, *c, *d],
            _ => return None,
        };
        return Some(CornerRadius::new(tl, tr, br, bl));
    }

    // Handle percentage values for border-radius.
    // CSS border-radius percentages are relative to the element's dimensions,
    // which we can't resolve at parse time. Use 9999px as a large sentinel —
    // the renderer clamps border-radius to half the element size, so any
    // percentage >= 50% produces a fully-rounded (circular/pill) shape.
    if let Some(len) = parse_css_length(value) {
        return match len {
            Length::Pct(v) if v > 0.0 => Some(CornerRadius::uniform(9999.0)),
            Length::Pct(_) => Some(CornerRadius::uniform(0.0)),
            _ => Some(CornerRadius::uniform(len.to_px())),
        };
    }

    None
}

pub(crate) fn parse_corner_shape_value(value: &str) -> Option<CornerShape> {
    let trimmed = value.trim();
    let parse_one = |s: &str| -> Option<f32> {
        match s.trim() {
            "round" => Some(1.0),
            "bevel" => Some(0.0),
            "squircle" => Some(2.0),
            "scoop" => Some(-1.0),
            "notch" => Some(-100.0),
            "square" => Some(100.0),
            other => {
                if let Some(inner) = other
                    .strip_prefix("superellipse(")
                    .and_then(|s| s.strip_suffix(')'))
                {
                    inner.trim().parse().ok()
                } else {
                    other.parse().ok()
                }
            }
        }
    };

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    match parts.len() {
        1 => Some(CornerShape::uniform(parse_one(parts[0])?)),
        2 => {
            let a = parse_one(parts[0])?;
            let b = parse_one(parts[1])?;
            Some(CornerShape::new(a, b, a, b))
        }
        3 => {
            let a = parse_one(parts[0])?;
            let b = parse_one(parts[1])?;
            let c = parse_one(parts[2])?;
            Some(CornerShape::new(a, b, c, b))
        }
        4 => {
            let tl = parse_one(parts[0])?;
            let tr = parse_one(parts[1])?;
            let br = parse_one(parts[2])?;
            let bl = parse_one(parts[3])?;
            Some(CornerShape::new(tl, tr, br, bl))
        }
        _ => None,
    }
}

/// Parse render layer
pub(crate) fn parse_render_layer<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, RenderLayer, E> {
    let (input, _) = ws(input)?;
    alt((
        value(RenderLayer::Foreground, tag_no_case("foreground")),
        value(RenderLayer::Glass, tag_no_case("glass")),
        value(RenderLayer::Background, tag_no_case("background")),
    ))(input)
}

// ============================================================================
// Animation Parsing
// ============================================================================
