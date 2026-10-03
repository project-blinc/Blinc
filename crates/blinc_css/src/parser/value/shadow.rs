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
/// Handles:
/// - `none` → a single transparent outer shadow,
/// - `theme(shadow-*)` → the token's full layer stack,
/// - one or more comma-separated explicit shadows, each of which may
///   carry the `inset` keyword.
pub(crate) fn parse_shadow_layers(value: &str) -> Option<ShadowLayers> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("none") {
        return Some(ShadowLayers {
            outer: vec![Shadow::new(0.0, 0.0, 0.0, Color::TRANSPARENT)],
            inner: Vec::new(),
        });
    }

    // Try theme() first — it preserves the multi-layer compound shadow.
    if let Ok((_, layers)) = parse_theme_shadow::<nom::error::Error<&str>>(trimmed) {
        return Some(layers);
    }

    let mut layers = ShadowLayers::default();
    for part in split_commas_respecting_parens(trimmed) {
        if let Some((shadow, inset)) = parse_explicit_shadow(part.trim()) {
            if inset {
                layers.inner.push(shadow);
            } else {
                layers.outer.push(shadow);
            }
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
) -> IResult<&'a str, ShadowLayers, E> {
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
        "shadow-inner" => &shadows.shadow_inner,
        "shadow-none" => &shadows.shadow_none,
        _ => {
            debug!(token = token_name, "Unknown theme shadow token");
            return Err(nom::Err::Error(E::from_error_kind(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
    };

    // The theme type keeps the flag; split on it here, since what crosses
    // into the render type cannot.
    let mut layers = ShadowLayers::default();
    for s in stack {
        if s.inset {
            layers.inner.push(s.into());
        } else {
            layers.outer.push(s.into());
        }
    }
    Ok((input, layers))
}

/// Parse an explicit shadow: `[inset] offset-x offset-y blur [spread] color`.
/// Returns the layer and whether it was marked `inset`.
pub(crate) fn parse_explicit_shadow(input: &str) -> Option<(Shadow, bool)> {
    let mut parts = split_whitespace_respecting_parens(input);

    // CSS lets `inset` sit anywhere among the components, and authors
    // write it first or last about equally. Lift it out, then parse what
    // is left as the ordinary offset/blur/spread/colour form.
    let before = parts.len();
    parts.retain(|p| !p.trim().eq_ignore_ascii_case("inset"));
    let inset = parts.len() != before;

    if parts.len() >= 4 {
        let offset_x = parse_length_value(&parts[0])?;
        let offset_y = parse_length_value(&parts[1])?;
        let blur = parse_length_value(&parts[2])?;
        // Try the 5-part form: offset-x offset-y blur spread color
        if parts.len() >= 5 {
            if let Some(spread) = parse_length_value(&parts[3]) {
                let color = parse_color(&parts[4])?;
                let mut shadow = Shadow::new(offset_x, offset_y, blur, color);
                shadow.spread = spread;
                return Some((shadow, inset));
            }
        }
        // 4-part form: offset-x offset-y blur color
        let color = parse_color(&parts[3])?;
        return Some((Shadow::new(offset_x, offset_y, blur, color), inset));
    }
    None
}
