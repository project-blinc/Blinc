//! `box-shadow` values.
//!
//! Parses both an explicit `offset blur spread color` form and
//! `theme(shadow-*)` references, and accepts a comma-separated stack of
//! either.

use blinc_core::{Color, Shadow};
use blinc_theme::ThemeState;
use nom::{
    IResult,
    bytes::complete::{tag_no_case, take_while1},
    character::complete::char,
    error::ParseError as NomParseError,
    sequence::delimited,
};
use tracing::debug;

use crate::parser::*;

pub(crate) fn parse_shadow(value: &str) -> Option<Shadow> {
    // Returns just the first layer for callers (e.g. `text-shadow`) that
    // still take a single `Shadow`. Use `parse_shadow_stack` for
    // multi-layer box-shadow.
    parse_shadow_stack(value).and_then(|s| s.into_iter().next())
}

/// Parse a CSS shadow value into a stack of layers.
///
/// Handles:
/// - `none` → single transparent shadow,
/// - `theme(shadow-*)` → the theme's full layer stack,
/// - one or more comma-separated explicit shadows.
pub(crate) fn parse_shadow_stack(value: &str) -> Option<Vec<Shadow>> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("none") {
        return Some(vec![Shadow::new(0.0, 0.0, 0.0, Color::TRANSPARENT)]);
    }

    // Try theme() function first — preserves multi-layer compound shadow.
    if let Ok((_, stack)) = parse_theme_shadow::<nom::error::Error<&str>>(trimmed) {
        return Some(stack);
    }

    // Comma-separated explicit shadows.
    let parts = split_commas_respecting_parens(trimmed);
    let mut layers = Vec::with_capacity(parts.len().max(1));
    for part in parts {
        if let Some(layer) = parse_explicit_shadow(part.trim()) {
            layers.push(layer);
        }
    }
    if layers.is_empty() {
        None
    } else {
        Some(layers)
    }
}

/// Parse theme(shadow-*) tokens
pub(crate) fn parse_theme_shadow<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Vec<Shadow>, E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("theme")(input)?;
    let (input, _) = ws(input)?;
    let (input, token_name) =
        delimited(char('('), take_while1(|c: char| c != ')'), char(')'))(input)?;

    let token_name = token_name.trim();
    let shadows = ThemeState::get().shadows();

    let stack: &[blinc_theme::Shadow] = match token_name.to_lowercase().replace('_', "-").as_str() {
        "shadow-sm" => &shadows.shadow_sm,
        "shadow-default" => &shadows.shadow_default,
        "shadow-md" => &shadows.shadow_md,
        "shadow-lg" => &shadows.shadow_lg,
        "shadow-xl" => &shadows.shadow_xl,
        "shadow-2xl" => &shadows.shadow_2xl,
        "shadow-none" => &shadows.shadow_none,
        _ => {
            debug!(token = token_name, "Unknown theme shadow token");
            return Err(nom::Err::Error(E::from_error_kind(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
    };

    let stack: Vec<Shadow> = stack.iter().map(Shadow::from).collect();
    Ok((input, stack))
}

/// Parse explicit shadow: `offset-x offset-y blur [spread] color`
pub(crate) fn parse_explicit_shadow(input: &str) -> Option<Shadow> {
    let parts = split_whitespace_respecting_parens(input);
    if parts.len() >= 4 {
        let offset_x = parse_length_value(&parts[0])?;
        let offset_y = parse_length_value(&parts[1])?;
        let blur = parse_length_value(&parts[2])?;
        // Try 5-part form: offset-x offset-y blur spread color
        if parts.len() >= 5 {
            if let Some(spread) = parse_length_value(&parts[3]) {
                let color = parse_color(&parts[4])?;
                let mut shadow = Shadow::new(offset_x, offset_y, blur, color);
                shadow.spread = spread;
                return Some(shadow);
            }
        }
        // 4-part form: offset-x offset-y blur color
        let color = parse_color(&parts[3])?;
        return Some(Shadow::new(offset_x, offset_y, blur, color));
    }
    None
}
