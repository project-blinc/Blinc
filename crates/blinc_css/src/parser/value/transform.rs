//! `transform` and `transform-origin`.
//!
//! Handles the 2D functions that compose into an affine matrix, plus the
//! 3D functions (`rotateX`, `perspective`, `translateZ`) that are stored as
//! separate fields because the renderer applies them outside the 2D
//! matrix.

use blinc_core::Transform;
use nom::{
    IResult,
    bytes::complete::tag_no_case,
    character::complete::char,
    combinator::opt,
    error::ParseError as NomParseError,
    number::complete::float,
    sequence::{preceded, tuple},
};

use crate::element_style::ElementStyle;
use crate::parser::*;

/// Parse a compound transform value (e.g. `rotate(45deg) scale(1.5)`).
/// Handles both 2D and 3D functions. 2D functions are composed into a single
/// Affine2D stored in style.transform; 3D functions go to dedicated fields.
/// Returns true if at least one function was parsed.
pub(crate) fn parse_transform_with_3d(value: &str, style: &mut ElementStyle) -> bool {
    use blinc_core::Affine2D;

    // Split compound transform string into individual function calls.
    // e.g. "rotate(45deg) scale(1.5)" → ["rotate(45deg)", "scale(1.5)"]
    let functions = split_transform_functions(value.trim());
    if functions.is_empty() {
        return false;
    }

    let mut affine = Affine2D::IDENTITY;
    let mut has_2d = false;
    let mut parsed_any = false;

    for func in &functions {
        // 3D: rotateX
        if let Some(deg) = parse_function_angle(func, "rotateX") {
            style.rotate_x = Some(deg);
            parsed_any = true;
        // 3D: rotateY
        } else if let Some(deg) = parse_function_angle(func, "rotateY") {
            style.rotate_y = Some(deg);
            parsed_any = true;
        // perspective
        } else if let Some(px) = parse_function_px(func, "perspective") {
            style.perspective = Some(px);
            parsed_any = true;
        // 2D: skewX
        } else if let Some(deg) = parse_function_angle(func, "skewX") {
            style.skew_x = Some(deg);
            affine = affine.then(&Affine2D::skew_x(deg.to_radians()));
            has_2d = true;
            parsed_any = true;
        // 2D: skewY
        } else if let Some(deg) = parse_function_angle(func, "skewY") {
            style.skew_y = Some(deg);
            affine = affine.then(&Affine2D::skew_y(deg.to_radians()));
            has_2d = true;
            parsed_any = true;
        // 2D: skew(x, y)
        } else if let Some((sx, sy)) = parse_skew_function(func) {
            style.skew_x = Some(sx);
            style.skew_y = Some(sy);
            affine = affine.then(&Affine2D::skew_x(sx.to_radians()));
            affine = affine.then(&Affine2D::skew_y(sy.to_radians()));
            has_2d = true;
            parsed_any = true;
        // 2D: scale
        } else if let Ok((_, (sx, sy))) = parse_scale_values::<nom::error::Error<&str>>(func) {
            style.scale_x = Some(sx);
            style.scale_y = Some(sy);
            affine = affine.then(&Affine2D::scale(sx, sy));
            has_2d = true;
            parsed_any = true;
        // 2D: rotate — parse angle directly to avoid lossy matrix decomposition
        // (atan2 wraps 360° to 0°, breaking spin animations)
        } else if let Some(deg) = parse_function_angle(func, "rotate") {
            style.rotate = Some(deg);
            affine = affine.then(&Affine2D::rotation(deg.to_radians()));
            has_2d = true;
            parsed_any = true;
        // 2D: translate / translateX / translateY
        } else if let Ok((_, t)) = parse_translate_transform::<nom::error::Error<&str>>(func) {
            if let Transform::Affine2D(ref a) = t {
                affine = affine.then(a);
            }
            has_2d = true;
            parsed_any = true;
        }
    }

    if has_2d {
        style.transform = Some(Transform::Affine2D(affine));
    }

    parsed_any
}

/// Split a compound CSS transform string into individual function calls.
/// e.g. `"rotate(45deg) scale(1.5)"` → `["rotate(45deg)", "scale(1.5)"]`
pub(crate) fn split_transform_functions(input: &str) -> Vec<&str> {
    let mut functions = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut in_func = false;

    for (i, ch) in input.char_indices() {
        match ch {
            '(' => {
                if depth == 0 {
                    // Find start of function name (skip leading whitespace)
                    if !in_func {
                        start = input[..i]
                            .rfind(|c: char| c.is_whitespace())
                            .map_or(0, |p| p + 1);
                        in_func = true;
                    }
                }
                depth += 1;
            }
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && in_func {
                    let func = input[start..=i].trim();
                    if !func.is_empty() {
                        functions.push(func);
                    }
                    in_func = false;
                    start = i + 1;
                }
            }
            _ if !ch.is_whitespace() && depth == 0 && !in_func => {
                start = i;
                in_func = true;
            }
            _ => {}
        }
    }

    functions
}

/// Parse `skew(Xdeg)` or `skew(Xdeg, Ydeg)`
pub(crate) fn parse_skew_function(input: &str) -> Option<(f32, f32)> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    if !lower.starts_with("skew(") {
        return None;
    }
    let inner = trimmed[5..].strip_suffix(')')?.trim();
    let parts: Vec<&str> = inner.split(',').collect();
    match parts.len() {
        1 => {
            let x = parse_angle_str(parts[0].trim())?;
            Some((x, 0.0))
        }
        2 => {
            let x = parse_angle_str(parts[0].trim())?;
            let y = parse_angle_str(parts[1].trim())?;
            Some((x, y))
        }
        _ => None,
    }
}

/// Parse `transform-origin: X Y` where X/Y can be percentages or keywords
/// Returns [x%, y%] where 0% = left/top, 50% = center, 100% = right/bottom
pub(crate) fn parse_transform_origin(value: &str) -> Option<[f32; 2]> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    let parse_one = |s: &str| -> Option<f32> {
        match s {
            "left" | "top" => Some(0.0),
            "center" => Some(50.0),
            "right" | "bottom" => Some(100.0),
            _ => {
                if let Some(pct) = s.strip_suffix('%') {
                    pct.trim().parse::<f32>().ok()
                } else {
                    s.parse::<f32>().ok() // bare number = percentage
                }
            }
        }
    };
    match parts.len() {
        1 => {
            let v = parse_one(parts[0])?;
            Some([v, v])
        }
        2 => {
            let x = parse_one(parts[0])?;
            let y = parse_one(parts[1])?;
            Some([x, y])
        }
        _ => None,
    }
}

/// Parse an angle string like "30deg", "0.5rad", or plain number (assumed degrees)
pub(crate) fn parse_angle_str(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(deg_str) = s.strip_suffix("deg") {
        deg_str.trim().parse::<f32>().ok()
    } else if let Some(rad_str) = s.strip_suffix("rad") {
        rad_str.trim().parse::<f32>().ok().map(|r| r.to_degrees())
    } else {
        // Plain number assumed to be degrees
        s.parse::<f32>().ok()
    }
}

/// Parse a CSS function like `funcName(30deg)` and return degrees
pub(crate) fn parse_function_angle(input: &str, func_name: &str) -> Option<f32> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    let func_lower = func_name.to_lowercase();
    if !lower.starts_with(&func_lower) {
        return None;
    }
    let rest = &trimmed[func_name.len()..].trim();
    let rest = rest.strip_prefix('(')?.trim_start();
    let rest = rest.strip_suffix(')')?.trim_end();
    parse_angle_value(rest)
}

/// Parse a CSS function like `funcName(800px)` and return pixels
pub(crate) fn parse_function_px(input: &str, func_name: &str) -> Option<f32> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    let func_lower = func_name.to_lowercase();
    if !lower.starts_with(&func_lower) {
        return None;
    }
    let rest = &trimmed[func_name.len()..].trim();
    let rest = rest.strip_prefix('(')?.trim_start();
    let rest = rest.strip_suffix(')')?.trim_end();
    parse_css_px(rest)
}

/// Parse a vec3 value like "-0.5 -1.0 0.5" (3 space-separated floats)
pub(crate) fn parse_vec3_value(value: &str) -> Option<[f32; 3]> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() == 3 {
        let x = parts[0].parse::<f32>().ok()?;
        let y = parts[1].parse::<f32>().ok()?;
        let z = parts[2].parse::<f32>().ok()?;
        Some([x, y, z])
    } else {
        None
    }
}

/// Parse scale(x) or scale(x, y) and return the raw (sx, sy) values
pub(crate) fn parse_scale_values<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, (f32, f32), E> {
    let (input, _) = ws(input)?;
    let (input, _) = tag_no_case("scale")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, sx) = float(input)?;
    let (input, _) = ws(input)?;
    let (input, sy) = opt(preceded(tuple((char(','), ws::<E>)), float))(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;
    let sy = sy.unwrap_or(sx);
    Ok((input, (sx, sy)))
}

/// Parse translate(x, y), translateX(x), or translateY(y)
pub(crate) fn parse_translate_transform<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Transform, E> {
    let (input, _) = ws(input)?;

    // Try translateX(x)
    if let Ok((rest, _)) = tag_no_case::<_, _, E>("translateX")(input) {
        let (rest, _) = ws(rest)?;
        let (rest, _) = char('(')(rest)?;
        let (rest, _) = ws(rest)?;
        let (rest, x) = parse_length(rest)?;
        let (rest, _) = ws(rest)?;
        let (rest, _) = char(')')(rest)?;
        return Ok((rest, Transform::translate(x.to_px(), 0.0)));
    }

    // Try translateY(y)
    if let Ok((rest, _)) = tag_no_case::<_, _, E>("translateY")(input) {
        let (rest, _) = ws(rest)?;
        let (rest, _) = char('(')(rest)?;
        let (rest, _) = ws(rest)?;
        let (rest, y) = parse_length(rest)?;
        let (rest, _) = ws(rest)?;
        let (rest, _) = char(')')(rest)?;
        return Ok((rest, Transform::translate(0.0, y.to_px())));
    }

    // Try translate(x, y)
    let (input, _) = tag_no_case("translate")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = ws(input)?;
    let (input, x) = parse_length(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = ws(input)?;
    let (input, y) = parse_length(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(')')(input)?;

    Ok((input, Transform::translate(x.to_px(), y.to_px())))
}
