//! `filter` and `backdrop-filter`.
//!
//! Also parses the `liquid-glass()` shorthand, which expands into the same
//! [`GlassMaterial`] a backdrop filter produces.

use crate::material::GlassMaterial;
use crate::parser::*;

/// Parse CSS filter functions: `filter: grayscale(1) invert(0.5) brightness(1.2)`
///
/// Supports: grayscale, invert, sepia, hue-rotate, brightness, contrast, saturate
/// Values can be plain numbers, percentages, or degrees (for hue-rotate)
/// Parse functional `backdrop-filter` values: `blur(Npx)`, `saturate(N)`, `brightness(N)`.
/// Returns a `GlassMaterial` with `simple=true` and the extracted parameters.
pub(crate) fn parse_backdrop_filter_functions(value: &str) -> Option<GlassMaterial> {
    let mut remaining = value.trim();
    if remaining.is_empty() {
        return None;
    }

    // Defaults: subtle white tint (makes glass visually distinct from backdrop),
    // no border artifacts, simple frosted glass mode.
    let mut glass = GlassMaterial {
        blur: 0.0,
        tint: blinc_core::Color::rgba(1.0, 1.0, 1.0, 0.1),
        saturation: 1.0,
        brightness: 1.0,
        noise: 0.0,
        border_thickness: 0.0,
        shadow: None,
        simple: true,
    };
    let mut found_any = false;

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        let paren_pos = remaining.find('(')?;
        let func_name = remaining[..paren_pos].trim();
        let after_paren = &remaining[paren_pos + 1..];

        // Find matching close paren (handles nested parens)
        let close_pos = {
            let mut depth = 0i32;
            let mut found = None;
            for (i, ch) in after_paren.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        if depth == 0 {
                            found = Some(i);
                            break;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            found
        };

        if let Some(close) = close_pos {
            let arg_str = after_paren[..close].trim();
            remaining = after_paren[close + 1..].trim_start();

            match func_name.to_lowercase().as_str() {
                "blur" => {
                    if let Some(px) = parse_css_px(arg_str) {
                        glass.blur = px;
                        found_any = true;
                    }
                }
                "saturate" => {
                    if let Some(v) = arg_str
                        .strip_suffix('%')
                        .and_then(|s| s.trim().parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .or_else(|| arg_str.parse::<f32>().ok())
                    {
                        glass.saturation = v;
                        found_any = true;
                    }
                }
                "brightness" => {
                    if let Some(v) = arg_str
                        .strip_suffix('%')
                        .and_then(|s| s.trim().parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .or_else(|| arg_str.parse::<f32>().ok())
                    {
                        glass.brightness = v;
                        found_any = true;
                    }
                }
                _ => {
                    // Skip unknown functions
                    continue;
                }
            }
        } else {
            break;
        }
    }

    if found_any { Some(glass) } else { None }
}

/// Parse `liquid-glass(...)` CSS function.
///
/// Syntax: `liquid-glass(blur(Npx) saturate(N%) brightness(N%) border(N) tint(color) noise(N))`
///
/// All sub-functions are optional. Produces a `GlassMaterial` with `simple=false`
/// (liquid glass with refracted bevel borders).
pub(crate) fn parse_liquid_glass_functions(value: &str) -> Option<GlassMaterial> {
    let stripped = value.strip_prefix("liquid-glass(")?;
    // Find the matching closing paren for the outer liquid-glass(...)
    let mut depth = 0i32;
    let mut outer_close = None;
    for (i, ch) in stripped.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    outer_close = Some(i);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let inner = stripped[..outer_close?].trim();

    // Start with default liquid glass (simple=false, border_thickness=0.8)
    let mut glass = GlassMaterial::new();
    let mut found_any = false;
    let mut remaining = inner;

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        let paren_pos = match remaining.find('(') {
            Some(p) => p,
            None => break,
        };
        let func_name = remaining[..paren_pos].trim();
        let after_paren = &remaining[paren_pos + 1..];

        // Find matching close paren
        let close_pos = {
            let mut d = 0i32;
            let mut found = None;
            for (i, ch) in after_paren.char_indices() {
                match ch {
                    '(' => d += 1,
                    ')' => {
                        if d == 0 {
                            found = Some(i);
                            break;
                        }
                        d -= 1;
                    }
                    _ => {}
                }
            }
            found
        };

        if let Some(close) = close_pos {
            let arg_str = after_paren[..close].trim();
            remaining = after_paren[close + 1..].trim_start();

            match func_name.to_lowercase().as_str() {
                "blur" => {
                    if let Some(px) = parse_css_px(arg_str) {
                        glass.blur = px;
                        found_any = true;
                    }
                }
                "saturate" => {
                    if let Some(v) = arg_str
                        .strip_suffix('%')
                        .and_then(|s| s.trim().parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .or_else(|| arg_str.parse::<f32>().ok())
                    {
                        glass.saturation = v;
                        found_any = true;
                    }
                }
                "brightness" => {
                    if let Some(v) = arg_str
                        .strip_suffix('%')
                        .and_then(|s| s.trim().parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .or_else(|| arg_str.parse::<f32>().ok())
                    {
                        glass.brightness = v;
                        found_any = true;
                    }
                }
                "border" | "border-thickness" => {
                    if let Some(v) = parse_css_px(arg_str).or_else(|| arg_str.parse::<f32>().ok()) {
                        glass.border_thickness = v;
                        found_any = true;
                    }
                }
                "tint" => {
                    if let Some(color) = parse_color(arg_str) {
                        glass.tint = color;
                        found_any = true;
                    }
                }
                "noise" => {
                    if let Ok(v) = arg_str.parse::<f32>() {
                        glass.noise = v;
                        found_any = true;
                    }
                }
                _ => {
                    continue;
                }
            }
        } else {
            break;
        }
    }

    // liquid-glass() with no sub-functions still produces default liquid glass
    if found_any || inner.is_empty() {
        Some(glass)
    } else {
        None
    }
}

pub(crate) fn parse_css_filter(value: &str) -> Option<crate::element_style::CssFilter> {
    use crate::element_style::CssFilter;

    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(CssFilter::default());
    }

    let mut filter = CssFilter::default();
    let mut found_any = false;
    let mut remaining = value;

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        // Find function name and opening paren
        if let Some(paren_pos) = remaining.find('(') {
            let func_name = remaining[..paren_pos].trim();
            let after_paren = &remaining[paren_pos + 1..];

            // Find matching closing paren (handles nested parens like rgba())
            let close_pos = {
                let mut depth = 0i32;
                let mut found = None;
                for (i, ch) in after_paren.char_indices() {
                    match ch {
                        '(' => depth += 1,
                        ')' => {
                            if depth == 0 {
                                found = Some(i);
                                break;
                            }
                            depth -= 1;
                        }
                        _ => {}
                    }
                }
                found
            };
            if let Some(close_pos) = close_pos {
                let arg_str = after_paren[..close_pos].trim();
                remaining = after_paren[close_pos + 1..].trim_start();

                let func_lower = func_name.to_lowercase();

                // Handle special multi-arg functions first
                if func_lower == "blur" {
                    if let Some(px) = parse_css_px(arg_str) {
                        filter.blur = px;
                        found_any = true;
                    }
                    continue;
                }
                if func_lower == "drop-shadow" {
                    let parts = split_whitespace_respecting_parens(arg_str);
                    if parts.len() >= 3 {
                        let x = parse_length_value(&parts[0]);
                        let y = parse_length_value(&parts[1]);
                        let blur_val = parse_length_value(&parts[2]);
                        let color = if parts.len() >= 4 {
                            parse_color(&parts[3])
                        } else {
                            Some(blinc_core::Color::rgba(0.0, 0.0, 0.0, 0.5))
                        };
                        if let (Some(x), Some(y), Some(b), Some(c)) = (x, y, blur_val, color) {
                            filter.drop_shadow = Some(blinc_core::Shadow::new(x, y, b, c));
                            found_any = true;
                        }
                    }
                    continue;
                }

                // Parse the argument value for simple single-value functions
                let arg_val = if let Some(deg_str) = arg_str.strip_suffix("deg") {
                    deg_str.trim().parse::<f32>().ok()
                } else if let Some(pct_str) = arg_str.strip_suffix('%') {
                    pct_str.trim().parse::<f32>().ok().map(|v| v / 100.0)
                } else {
                    arg_str.parse::<f32>().ok()
                };

                if let Some(v) = arg_val {
                    match func_lower.as_str() {
                        "grayscale" => {
                            filter.grayscale = v;
                            found_any = true;
                        }
                        "invert" => {
                            filter.invert = v;
                            found_any = true;
                        }
                        "sepia" => {
                            filter.sepia = v;
                            found_any = true;
                        }
                        "hue-rotate" => {
                            filter.hue_rotate = v;
                            found_any = true;
                        }
                        "brightness" => {
                            filter.brightness = v;
                            found_any = true;
                        }
                        "contrast" => {
                            filter.contrast = v;
                            found_any = true;
                        }
                        "saturate" => {
                            filter.saturate = v;
                            found_any = true;
                        }
                        _ => {
                            // Unknown filter function, skip
                        }
                    }
                }
            } else {
                break; // No closing paren
            }
        } else {
            break; // No opening paren
        }
    }

    if found_any { Some(filter) } else { None }
}
