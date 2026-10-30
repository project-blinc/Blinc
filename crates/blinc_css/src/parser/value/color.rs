//! Color values.
//!
//! Accepts hex in three, six and eight digits, `rgb()` and `rgba()`, the
//! named CSS colors, and `theme(token)` references that resolve against the
//! active theme rather than a literal.

use blinc_core::Color;
use blinc_theme::{ColorToken, ThemeState};
use nom::{
    IResult,
    bytes::complete::{tag_no_case, take_while1},
    character::complete::char,
    error::ParseError as NomParseError,
    number::complete::float,
    sequence::delimited,
};
use tracing::debug;

use crate::parser::*;

/// Parse theme(token-name) for colors
pub(crate) fn parse_theme_color<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Color, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("theme")(input)?;
    let (input, _) = ws(input)?;
    let (input, token_name) =
        delimited(char('('), take_while1(|c: char| c != ')'), char(')'))(input)?;

    let token_name = token_name.trim();
    let token = match token_name.to_lowercase().as_str() {
        // Brand colors
        "primary" => ColorToken::Primary,
        "primary-hover" => ColorToken::PrimaryHover,
        "primary-active" => ColorToken::PrimaryActive,
        "secondary" => ColorToken::Secondary,
        "secondary-hover" => ColorToken::SecondaryHover,
        "secondary-active" => ColorToken::SecondaryActive,
        // Semantic colors
        "success" => ColorToken::Success,
        "success-bg" => ColorToken::SuccessBg,
        "warning" => ColorToken::Warning,
        "warning-bg" => ColorToken::WarningBg,
        "error" => ColorToken::Error,
        "error-bg" => ColorToken::ErrorBg,
        "info" => ColorToken::Info,
        "info-bg" => ColorToken::InfoBg,
        // Surface colors
        "background" => ColorToken::Background,
        "surface" => ColorToken::Surface,
        "surface-elevated" => ColorToken::SurfaceElevated,
        "surface-overlay" => ColorToken::SurfaceOverlay,
        // Text colors
        "text-primary" => ColorToken::TextPrimary,
        "text-secondary" => ColorToken::TextSecondary,
        "text-tertiary" => ColorToken::TextTertiary,
        "text-inverse" => ColorToken::TextInverse,
        "text-link" => ColorToken::TextLink,
        // Border colors
        "border" => ColorToken::Border,
        "border-secondary" => ColorToken::BorderSecondary,
        "border-hover" => ColorToken::BorderHover,
        "border-focus" => ColorToken::BorderFocus,
        "border-error" => ColorToken::BorderError,
        _ => {
            debug!(token = token_name, "Unknown theme color token");
            return Err(nom::Err::Error(E::from_error_kind(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
    };

    Ok((input, ThemeState::get().color(token)))
}

pub(crate) fn parse_blend_mode(value: &str) -> Option<blinc_core::BlendMode> {
    use blinc_core::BlendMode;
    match value.trim().to_lowercase().as_str() {
        "normal" => Some(BlendMode::Normal),
        "multiply" => Some(BlendMode::Multiply),
        "screen" => Some(BlendMode::Screen),
        "overlay" => Some(BlendMode::Overlay),
        "darken" => Some(BlendMode::Darken),
        "lighten" => Some(BlendMode::Lighten),
        "color-dodge" => Some(BlendMode::ColorDodge),
        "color-burn" => Some(BlendMode::ColorBurn),
        "hard-light" => Some(BlendMode::HardLight),
        "soft-light" => Some(BlendMode::SoftLight),
        "difference" => Some(BlendMode::Difference),
        "exclusion" => Some(BlendMode::Exclusion),
        _ => None,
    }
}

pub(crate) fn parse_color(input: &str) -> Option<Color> {
    let input = input.trim();

    // Try hex color
    if let Ok((_, color)) = parse_hex_color::<nom::error::Error<&str>>(input) {
        return Some(color);
    }

    // Try rgba()
    if let Ok((_, color)) = parse_rgba_color::<nom::error::Error<&str>>(input) {
        return Some(color);
    }

    // Try rgb()
    if let Ok((_, color)) = parse_rgb_color::<nom::error::Error<&str>>(input) {
        return Some(color);
    }

    if let Some(args) = input
        .strip_prefix("color-mix(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return parse_color_mix(args);
    }

    // Try named color
    parse_named_color(input)
}

/// Splits `text` at the top-level occurrences of `sep`, ignoring any inside
/// parentheses.
fn split_top_level(text: &str, sep: char) -> Vec<&str> {
    let (mut depth, mut start, mut parts) = (0usize, 0usize, Vec::new());
    for (i, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c == sep && depth == 0 => {
                parts.push(text[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts
}

/// `color-mix(in srgb, a p%, b q%)`: premultiplied alpha, a missing
/// percentage the rest of 100, percentages summing under 100 scaling the
/// alpha. Other colour spaces are not supported.
fn parse_color_mix(args: &str) -> Option<Color> {
    let parts = split_top_level(args, ',');
    if parts.len() != 3 || parts[0] != "in srgb" {
        return None;
    }
    let mut sides = Vec::new();
    for part in &parts[1..] {
        let tail = part.rsplit_once(char::is_whitespace);
        let (color_text, percent) = match tail {
            Some((head, last)) if last.ends_with('%') && !head.trim().is_empty() => {
                let p: f64 = last.trim_end_matches('%').parse().ok()?;
                (head.trim(), Some(p / 100.0))
            }
            _ => (*part, None),
        };
        sides.push((parse_color(color_text)?, percent));
    }
    let (mut p1, mut p2) = (sides[0].1, sides[1].1);
    match (p1, p2) {
        (None, None) => (p1, p2) = (Some(0.5), Some(0.5)),
        (None, Some(b)) => p1 = Some(1.0 - b),
        (Some(a), None) => p2 = Some(1.0 - a),
        _ => {}
    }
    let (p1, p2) = (p1?, p2?);
    let sum = p1 + p2;
    if sum <= 0.0 {
        return None;
    }
    let scale = sum.min(1.0);
    let (w1, w2) = (p1 / sum, p2 / sum);
    let (a, b) = (sides[0].0, sides[1].0);
    let alpha = a.a as f64 * w1 + b.a as f64 * w2;
    if alpha <= 0.0 {
        return Some(Color::rgba(0.0, 0.0, 0.0, 0.0));
    }
    let mixed = |x: f32, y: f32| {
        ((x as f64 * a.a as f64 * w1 + y as f64 * b.a as f64 * w2) / alpha).clamp(0.0, 1.0) as f32
    };
    Some(Color::rgba(
        mixed(a.r, b.r),
        mixed(a.g, b.g),
        mixed(a.b, b.b),
        (alpha * scale) as f32,
    ))
}

/// Parse hex color: #RGB, #RRGGBB, or #RRGGBBAA
pub(crate) fn parse_hex_color<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Color, E> {
    let (input, _) = char('#')(input)?;
    let (input, hex) = take_while1(|c: char| c.is_ascii_hexdigit())(input)?;

    let color = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            Color::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            Color::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            let a = u8::from_str_radix(&hex[6..8], 16).map_err(|_| {
                nom::Err::Error(E::from_error_kind(input, nom::error::ErrorKind::HexDigit))
            })?;
            Color::rgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            )
        }
        _ => {
            return Err(nom::Err::Error(E::from_error_kind(
                input,
                nom::error::ErrorKind::LengthValue,
            )));
        }
    };

    Ok((input, color))
}

/// Parse rgba(r, g, b, a)
pub(crate) fn parse_rgba_color<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Color, E> {
    let (input, _) = tag_no_case("rgba")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, r) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, g) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, b) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, a) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;

    // Normalize if values are 0-255 range
    let (r, g, b) = if r > 1.0 || g > 1.0 || b > 1.0 {
        (r / 255.0, g / 255.0, b / 255.0)
    } else {
        (r, g, b)
    };

    Ok((input, Color::rgba(r, g, b, a)))
}

/// Parse rgb(r, g, b)
pub(crate) fn parse_rgb_color<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Color, E> {
    let (input, _) = tag_no_case("rgb")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, r) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, g) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, b) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;

    // Normalize if values are 0-255 range
    let (r, g, b) = if r > 1.0 || g > 1.0 || b > 1.0 {
        (r / 255.0, g / 255.0, b / 255.0)
    } else {
        (r, g, b)
    };

    Ok((input, Color::rgba(r, g, b, 1.0)))
}

/// Parse named colors
pub(crate) fn parse_named_color(name: &str) -> Option<Color> {
    match name.to_lowercase().as_str() {
        "black" => Some(Color::BLACK),
        "white" => Some(Color::WHITE),
        "red" => Some(Color::RED),
        "green" => Some(Color::rgb(0.0, 0.5, 0.0)),
        "blue" => Some(Color::BLUE),
        "yellow" => Some(Color::YELLOW),
        "cyan" | "aqua" => Some(Color::CYAN),
        "magenta" | "fuchsia" => Some(Color::MAGENTA),
        "gray" | "grey" => Some(Color::GRAY),
        "orange" => Some(Color::ORANGE),
        "purple" => Some(Color::PURPLE),
        "transparent" => Some(Color::TRANSPARENT),
        _ => None,
    }
}

// ============================================================================
// Gradient Parsing
// ============================================================================
