//! Color values.
//!
//! Accepts hex in three, four, six and eight digits, `rgb()` and `rgba()` and `hsl()` and
//! `hsla()` in the comma and the space forms, `color-mix(in srgb, ..)` and the named CSS colors.
//! A theme's colours reach CSS as `var(--primary)` and the like, not as a function.

use blinc_core::Color;

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

    if let Some(hex) = input.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(args) = input
        .strip_prefix("color-mix(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return parse_color_mix(args);
    }
    let lower = input.to_ascii_lowercase();
    if let Some((name, args)) = lower.split_once('(') {
        let args = args.strip_suffix(')')?;
        return match name.trim() {
            "rgb" | "rgba" => parse_rgb(args),
            "hsl" | "hsla" => parse_hsl(args),
            _ => None,
        };
    }
    parse_named_color(&lower)
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

/// `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa`.
fn parse_hex(hex: &str) -> Option<Color> {
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|d| d * 17);
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let (r, g, b, a) = match hex.len() {
        3 => (digit(0)?, digit(1)?, digit(2)?, 255),
        4 => (digit(0)?, digit(1)?, digit(2)?, digit(3)?),
        6 => (pair(0)?, pair(2)?, pair(4)?, 255),
        8 => (pair(0)?, pair(2)?, pair(4)?, pair(6)?),
        _ => return None,
    };
    Some(Color::rgba(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ))
}

/// The three components and the alpha of a colour function's arguments, in the legacy
/// comma form (`1, 2, 3, 0.5`) or the modern space form (`1 2 3 / 0.5`).
fn components(args: &str) -> Option<(Vec<&str>, Option<&str>)> {
    let args = args.trim();
    if let Some((main, alpha)) = args.split_once('/') {
        if main.contains(',') {
            return None;
        }
        let parts: Vec<&str> = main.split_whitespace().collect();
        return (parts.len() == 3).then_some((parts, Some(alpha.trim())));
    }
    let parts: Vec<&str> = if args.contains(',') {
        args.split(',').map(str::trim).collect()
    } else {
        args.split_whitespace().collect()
    };
    match parts.len() {
        3 => Some((parts, None)),
        4 => Some((parts[..3].to_vec(), Some(parts[3]))),
        _ => None,
    }
}

/// A number, or a percentage as a fraction of `full`.
fn number_or_percent(text: &str, full: f64) -> Option<f64> {
    match text.strip_suffix('%') {
        Some(p) => Some(p.trim().parse::<f64>().ok()? / 100.0 * full),
        None => text.parse().ok(),
    }
}

fn alpha_of(alpha: Option<&str>) -> Option<f32> {
    match alpha {
        None => Some(1.0),
        Some(a) => number_or_percent(a, 1.0).map(|a| a.clamp(0.0, 1.0) as f32),
    }
}

/// `rgb()` and `rgba()`: components 0 to 255 or percentages, alpha 0 to 1 or a percentage.
fn parse_rgb(args: &str) -> Option<Color> {
    let (parts, alpha) = components(args)?;
    let channel =
        |t: &str| number_or_percent(t, 255.0).map(|v| (v.clamp(0.0, 255.0) / 255.0) as f32);
    Some(Color::rgba(
        channel(parts[0])?,
        channel(parts[1])?,
        channel(parts[2])?,
        alpha_of(alpha)?,
    ))
}

/// `hsl()` and `hsla()`: a hue in degrees (or `deg`, `grad`, `rad`, `turn`), saturation and
/// lightness as percentages.
fn parse_hsl(args: &str) -> Option<Color> {
    let (parts, alpha) = components(args)?;
    let hue = {
        let t = parts[0];
        let degrees = |suffix: &str, per_turn: f64| {
            t.strip_suffix(suffix)
                .and_then(|n| n.trim().parse::<f64>().ok())
                .map(|n| n / per_turn * 360.0)
        };
        degrees("deg", 360.0)
            .or_else(|| degrees("grad", 400.0))
            .or_else(|| degrees("rad", std::f64::consts::TAU))
            .or_else(|| degrees("turn", 1.0))
            .or_else(|| t.parse::<f64>().ok())?
    };
    let percent = |t: &str| number_or_percent(t, 1.0).map(|v| v.clamp(0.0, 1.0));
    let (s, l) = (percent(parts[1])?, percent(parts[2])?);
    let h = hue.rem_euclid(360.0) / 360.0;
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |mut t: f64| {
        t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        v as f32
    };
    Some(Color::rgba(
        channel(h + 1.0 / 3.0),
        channel(h),
        channel(h - 1.0 / 3.0),
        alpha_of(alpha)?,
    ))
}

/// CSS's named colours as 0xrrggbb, sorted by name.
const NAMED: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

/// Parse named colors
pub(crate) fn parse_named_color(name: &str) -> Option<Color> {
    let name = name.to_ascii_lowercase();
    if name == "transparent" {
        return Some(Color::TRANSPARENT);
    }
    let i = NAMED
        .binary_search_by_key(&name.as_str(), |(n, _)| n)
        .ok()?;
    let rgb = NAMED[i].1;
    Some(Color::rgb(
        (rgb >> 16 & 0xff) as f32 / 255.0,
        (rgb >> 8 & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
    ))
}

// ============================================================================
// Gradient Parsing
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// A colour as its 8-bit channels, as a stylesheet gives it to a renderer.
    fn bytes(text: &str) -> Option<[u8; 4]> {
        parse_color(text).map(|c| [c.r, c.g, c.b, c.a].map(|v| (v * 255.0).round() as u8))
    }

    #[test]
    fn every_syntax_gives_the_colour_css_does() {
        for (text, want) in [
            ("#f00", [255, 0, 0, 255]),
            ("#f00a", [255, 0, 0, 170]),
            ("#ff0000", [255, 0, 0, 255]),
            ("#ff0000cc", [255, 0, 0, 204]),
            ("rgb(255, 0, 0)", [255, 0, 0, 255]),
            ("rgb(255 0 0)", [255, 0, 0, 255]),
            ("rgb(255 0 0 / 50%)", [255, 0, 0, 128]),
            ("rgb(255 0 0 / 0.5)", [255, 0, 0, 128]),
            ("rgba(255, 0, 0, .5)", [255, 0, 0, 128]),
            ("rgb(100%, 0%, 50%)", [255, 0, 128, 255]),
            ("RGB(0 128 255)", [0, 128, 255, 255]),
            ("hsl(120, 100%, 25%)", [0, 128, 0, 255]),
            ("hsl(120 100% 25%)", [0, 128, 0, 255]),
            ("hsla(120,100%,25%,.5)", [0, 128, 0, 128]),
            ("hsl(0.5turn 100% 50%)", [0, 255, 255, 255]),
            ("hsl(180deg 100% 50% / 25%)", [0, 255, 255, 64]),
            ("rebeccapurple", [102, 51, 153, 255]),
            ("orange", [255, 165, 0, 255]),
            ("Transparent", [0, 0, 0, 0]),
        ] {
            assert_eq!(bytes(text), Some(want), "{text}");
        }
    }

    /// Components are 0 to 255, so `1` is nearly black: a value at or under 1 is not a fraction.
    #[test]
    fn small_rgb_components_are_not_fractions() {
        assert_eq!(bytes("rgb(1, 1, 1)"), Some([1, 1, 1, 255]));
        assert_eq!(bytes("rgb(0, 0, 1)"), Some([0, 0, 1, 255]));
        assert_eq!(bytes("rgba(1, 1, 1, 0.1)"), Some([1, 1, 1, 26]));
    }

    #[test]
    fn what_is_not_a_colour_is_none() {
        for text in [
            "#ff",
            "#fffff",
            "#ggg",
            "rgb(1, 2)",
            "rgb(1, 2, 3, 4, 5)",
            "rgb(1, 2, 3 / 0.5)",
            "hsl(1, 2, 3, 4, 5)",
            "red blue",
            "notacolour",
            "",
        ] {
            assert_eq!(bytes(text), None, "{text}");
        }
    }
}
