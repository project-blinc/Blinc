//! `@keyframes` block grammar.
//!
//! Parses the block body into stops. A stop's position accepts `from`,
//! `to`, or a percentage, and one stop may declare several positions at
//! once (`0%, 50% { ... }`).

use std::collections::HashMap;

use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case},
    character::complete::char,
    combinator::value,
    multi::many0,
    number::complete::float,
};

use crate::element_style::ElementStyle;
use crate::parser::*;

/// Parse a @keyframes block
///
/// Supports:
/// - `from` and `to` keywords (0% and 100%)
/// - Percentage values like `50%`
/// - Multiple stops: `0%, 100%` (same style for multiple positions)
///
/// # Example
///
/// ```ignore
/// @keyframes slide-in {
///     from { opacity: 0; transform: translateY(20px); }
///     to { opacity: 1; transform: translateY(0); }
/// }
/// ```
pub(crate) fn keyframes_block<'a>(
    css: &'a str,
    errors: &mut Vec<ParseError>,
    variables: &HashMap<String, String>,
) -> ParseResult<'a, CssKeyframes> {
    let (input, _) = ws(css)?;
    let (input, _) = tag("@keyframes")(input)?;
    let (input, _) = ws(input)?;
    let (input, name) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;

    let mut keyframes = CssKeyframes::new(name);
    let mut remaining = input;

    // Parse keyframe stops
    loop {
        let trimmed = remaining.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('}') {
            break;
        }

        match keyframe_stop(css, errors, variables)(trimmed) {
            Ok((rest, (positions, style))) => {
                for position in positions {
                    keyframes.add_keyframe(position, style.clone());
                }
                remaining = rest;
            }
            Err(_) => {
                // Can't parse more keyframe stops
                break;
            }
        }
    }

    let (input, _) = ws(remaining)?;
    let (input, _) = char('}')(input)?;
    Ok((input, keyframes))
}

/// Parse a single keyframe stop (e.g., `from { ... }`, `50% { ... }`, or `0%, 100% { ... }`)
pub(crate) fn keyframe_stop<'a, 'b>(
    original_css: &'a str,
    errors: &'b mut Vec<ParseError>,
    variables: &'b HashMap<String, String>,
) -> impl FnMut(&'a str) -> ParseResult<'a, (Vec<f32>, ElementStyle)> + 'b
where
    'a: 'b,
{
    move |input: &'a str| {
        let (input, _) = ws(input)?;
        let (input, positions) = keyframe_positions(input)?;
        let (input, _) = ws(input)?;
        let (input, properties) = rule_block(input)?;

        let mut style = ElementStyle::new();
        for (name, value) in properties {
            let resolved_value = resolve_var_references(value, variables);
            apply_property_with_errors(
                &mut style,
                name,
                &resolved_value,
                original_css,
                input,
                errors,
            );
        }

        Ok((input, (positions, style)))
    }
}

/// Parse keyframe position(s): `from`, `to`, `50%`, or `0%, 100%`
pub(crate) fn keyframe_positions(input: &str) -> ParseResult<Vec<f32>> {
    let (input, first) = keyframe_position(input)?;
    let (input, rest) = many0(|i| {
        let (i, _) = ws(i)?;
        let (i, _) = char(',')(i)?;
        let (i, _) = ws(i)?;
        keyframe_position(i)
    })(input)?;

    let mut positions = vec![first];
    positions.extend(rest);
    Ok((input, positions))
}

/// Parse a single keyframe position: `from`, `to`, or percentage like `50%`
pub(crate) fn keyframe_position(input: &str) -> ParseResult<f32> {
    alt((
        // `from` = 0%
        value(0.0, tag_no_case("from")),
        // `to` = 100%
        value(1.0, tag_no_case("to")),
        // Percentage like `50%`
        |i| {
            let (i, num) = float(i)?;
            let (i, _) = char('%')(i)?;
            Ok((i, num / 100.0))
        },
    ))(input)
}
