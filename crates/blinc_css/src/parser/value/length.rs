//! Lengths, times, dimensions, and `calc()`.
//!
//! Covers the scalar value grammars: pixel and percentage lengths, angles,
//! durations, spacing shorthands, and the `calc()` entry point that defers
//! evaluation to the calc engine when a value contains `env()`.

use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, tag_no_case},
    combinator::opt,
    error::ParseError as NomParseError,
    number::complete::float,
};

use crate::element_style::SpacingRect;
use crate::parser::*;
use crate::units::Length;

/// Parse a CSS length value with unit suffix and return as Length enum
///
/// Supports:
/// - `px` - pixels (e.g., "16px")
/// - `sp` - spacing units, 4px grid (e.g., "4sp" = 16px)
/// - `%` - percentage (e.g., "50%")
/// - unitless - treated as pixels for backwards compatibility
pub(crate) fn parse_css_length(input: &str) -> Option<Length> {
    let input = input.trim();

    // Try percentage first
    if let Some(pct_str) = input.strip_suffix('%') {
        return pct_str.trim().parse::<f32>().ok().map(Length::Pct);
    }

    // Try spacing units (4px grid)
    if let Some(sp_str) = input.strip_suffix("sp") {
        return sp_str.trim().parse::<f32>().ok().map(Length::Sp);
    }

    // Try pixels (explicit or implicit)
    let num_str = input.strip_suffix("px").unwrap_or(input).trim();
    num_str.parse::<f32>().ok().map(Length::Px)
}

/// Parse a length value with optional px/sp suffix using nom
pub(crate) fn parse_length<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Length, E> {
    let (input, value) = float(input)?;
    // Try to match a unit suffix
    let (input, unit) = opt(alt((tag_no_case("px"), tag_no_case("sp"), tag("%"))))(input)?;

    let length = match unit {
        Some("sp") | Some("SP") => Length::Sp(value),
        Some("%") => Length::Pct(value),
        _ => Length::Px(value), // px or unitless
    };

    Ok((input, length))
}

/// Parse a length value from a string slice, returning pixels (f32)
///
/// This is a convenience wrapper that converts Length to pixels for
/// properties that need raw pixel values (like shadow offsets).
pub(crate) fn parse_length_value(input: &str) -> Option<f32> {
    parse_css_length(input).map(|len| len.to_px())
}

/// Parse opacity value
pub(crate) fn parse_opacity<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, f32, E> {
    let (input, _) = ws(input)?;
    float(input)
}

/// Parse a time value (e.g., "300ms", "0.5s", "1s")
pub(crate) fn parse_time_value(input: &str) -> Option<u32> {
    let input = input.trim();

    // Try milliseconds
    if let Some(ms_str) = input.strip_suffix("ms") {
        return ms_str.trim().parse::<f32>().ok().map(|ms| ms as u32);
    }

    // Try seconds
    if let Some(s_str) = input.strip_suffix('s') {
        return s_str
            .trim()
            .parse::<f32>()
            .ok()
            .map(|s| (s * 1000.0) as u32);
    }

    // Try plain number (assume milliseconds)
    input.parse::<f32>().ok().map(|ms| ms as u32)
}

/// Parse a CSS length value in pixels (e.g. "100px", "50", "10.5px")
pub(crate) fn parse_css_px(input: &str) -> Option<f32> {
    let trimmed = input.trim();

    // Support calc() expressions — evaluate static calcs immediately
    if trimmed.starts_with("calc(") {
        if let Some(expr) = crate::calc::parse_calc(trimmed) {
            let ctx = viewport_calc_context();
            // A calc over viewport units is only "dynamic" for want of a
            // viewport; with one in hand it evaluates like any other.
            if !expr.is_dynamic() || ctx.viewport_height > 0.0 {
                return Some(expr.eval(&ctx));
            }
        }
        return None;
    }

    if let Some(px_str) = trimmed.strip_suffix("px") {
        return px_str.trim().parse::<f32>().ok();
    }

    // Viewport units. The calc module already knows how to evaluate
    // these; they just never reached it, because this parser only
    // understood px. Without a live viewport (styles parsed before the
    // window exists) there is no sane answer, so fall through to None
    // rather than silently resolving against zero.
    for suffix in ["vh", "vw"] {
        if let Some(num) = trimmed.strip_suffix(suffix) {
            let value = num.trim().parse::<f32>().ok()?;
            let ctx = viewport_calc_context();
            if ctx.viewport_width <= 0.0 || ctx.viewport_height <= 0.0 {
                return None;
            }
            let unit = crate::calc::CalcUnit::parse(suffix)?;
            return Some(unit.to_pixels(value, &ctx));
        }
    }

    // Unitless number = px
    trimmed.parse::<f32>().ok()
}

/// `CalcContext` seeded with the live viewport, so viewport-relative
/// units resolve. Falls back to the default (zero viewport) before the
/// window exists.
pub(crate) fn viewport_calc_context() -> crate::calc::CalcContext {
    let mut ctx = crate::calc::CalcContext::default();
    if blinc_core::context_state::BlincContextState::is_initialized() {
        let (w, h) = blinc_core::context_state::BlincContextState::get().viewport_size();
        if w > 0.0 && h > 0.0 {
            ctx.viewport_width = w;
            ctx.viewport_height = h;
        }
    }
    ctx
}

/// Parse a CSS `object-position` value into [x, y] in 0.0-1.0 range.
///
/// Supports keywords (`left`, `center`, `right`, `top`, `bottom`),
/// percentages (`25%`), and two-value combinations (`left top`, `50% 25%`).
pub(crate) fn parse_object_position(input: &str) -> Option<[f32; 2]> {
    let trimmed = input.trim();

    // Helper: parse a single keyword or percentage to a 0.0-1.0 value
    fn keyword_or_pct(s: &str) -> Option<f32> {
        match s {
            "left" | "top" => Some(0.0),
            "center" => Some(0.5),
            "right" | "bottom" => Some(1.0),
            _ => {
                if let Some(pct_str) = s.strip_suffix('%') {
                    pct_str.trim().parse::<f32>().ok().map(|v| v / 100.0)
                } else {
                    None
                }
            }
        }
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    match parts.len() {
        1 => {
            // Single value: applied to x, y defaults to 50%
            let v = keyword_or_pct(parts[0])?;
            // Keywords top/bottom are y-axis → put in y, x = 50%
            match parts[0] {
                "top" | "bottom" => Some([0.5, v]),
                _ => Some([v, 0.5]),
            }
        }
        2 => {
            let x = keyword_or_pct(parts[0])?;
            let y = keyword_or_pct(parts[1])?;
            Some([x, y])
        }
        _ => None,
    }
}

/// Parse a CSS dimension value: `Npx`, `N%`, `auto`, `fit-content`, `max-content`, or `calc(...)`
pub(crate) fn parse_css_dimension(input: &str) -> Option<crate::element_style::StyleDimension> {
    use crate::element_style::StyleDimension;
    let trimmed = input.trim();

    // Support calc() expressions
    if trimmed.starts_with("calc(") {
        if let Some(expr) = crate::calc::parse_calc(trimmed) {
            if !expr.is_dynamic() {
                // Static calc — check if it contains a percentage
                // For now, evaluate as px value
                return Some(StyleDimension::Length(
                    expr.eval(&crate::calc::CalcContext::default()),
                ));
            }
        }
        return None;
    }

    match trimmed.to_lowercase().as_str() {
        "auto" | "fit-content" | "max-content" => Some(StyleDimension::Auto),
        _ => {
            if let Some(pct_str) = trimmed.strip_suffix('%') {
                let pct = pct_str.trim().parse::<f32>().ok()?;
                Some(StyleDimension::Percent(pct / 100.0))
            } else {
                parse_css_px(trimmed).map(StyleDimension::Length)
            }
        }
    }
}

/// Parse a CSS spacing value (uniform or per-side)
/// Supports: "10px", "10px 20px" (vert horiz), "10px 20px 30px 40px" (top right bottom left)
/// Also supports `calc(...)` as a single uniform value.
pub(crate) fn parse_css_spacing(input: &str) -> Option<SpacingRect> {
    let trimmed = input.trim();

    // Handle calc() as a single uniform value (don't split on whitespace inside calc)
    if trimmed.starts_with("calc(") {
        let v = parse_css_px(trimmed)?;
        return Some(SpacingRect::uniform(v));
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    match parts.len() {
        1 => {
            let v = parse_css_px(parts[0])?;
            Some(SpacingRect::uniform(v))
        }
        2 => {
            let vert = parse_css_px(parts[0])?;
            let horiz = parse_css_px(parts[1])?;
            Some(SpacingRect::xy(horiz, vert))
        }
        4 => {
            let top = parse_css_px(parts[0])?;
            let right = parse_css_px(parts[1])?;
            let bottom = parse_css_px(parts[2])?;
            let left = parse_css_px(parts[3])?;
            Some(SpacingRect::new(top, right, bottom, left))
        }
        _ => None,
    }
}

// ============================================================================
// Color Parsing
// ============================================================================

/// Parse angle value (e.g., "45deg", "0.5turn", "100grad")
/// Result of attempting to parse a CSS value as a calc() expression.
pub(crate) enum CalcParseResult {
    /// Contains env() — needs per-frame evaluation
    Dynamic(crate::calc::CalcExpr),
    /// Pure calc() with no env() — evaluate once to a fixed value
    Static(f32),
    /// Not a calc expression — fall through to normal parsing
    NotCalc,
}

/// Try to parse a CSS property value as a calc() expression.
/// Returns Dynamic if it contains env() references (needs per-frame eval),
/// Static if it's a pure calc, or NotCalc to fall through.
pub(crate) fn try_parse_calc(value: &str) -> CalcParseResult {
    let v = value.trim();
    if v.starts_with("calc(") || v.contains("env(") {
        if let Some(expr) = crate::calc::parse_calc(v) {
            if expr.is_dynamic() {
                return CalcParseResult::Dynamic(expr);
            } else {
                let ctx = crate::calc::CalcContext::default();
                return CalcParseResult::Static(expr.eval(&ctx));
            }
        }
    }
    CalcParseResult::NotCalc
}

pub(crate) fn parse_angle_value(input: &str) -> Option<f32> {
    let input = input.trim();

    if let Some(deg_str) = input.strip_suffix("deg") {
        return deg_str.trim().parse::<f32>().ok();
    }

    if let Some(turn_str) = input.strip_suffix("turn") {
        return turn_str.trim().parse::<f32>().ok().map(|t| t * 360.0);
    }

    if let Some(rad_str) = input.strip_suffix("rad") {
        return rad_str
            .trim()
            .parse::<f32>()
            .ok()
            .map(|r| r * 180.0 / std::f32::consts::PI);
    }

    if let Some(grad_str) = input.strip_suffix("grad") {
        return grad_str.trim().parse::<f32>().ok().map(|g| g * 0.9);
    }

    // Try parsing as plain number (assumed degrees)
    input.parse::<f32>().ok()
}
