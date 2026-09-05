//! `clip-path` values.
//!
//! Supports the basic shapes (`circle`, `ellipse`, `inset`, `polygon`) and
//! `path()`, which is flattened to a polygon by [`super::svg_path`].

use blinc_core::{ClipLength, ClipPath};

use crate::parser::*;

/// Parse a CSS length value (px or %) into a ClipLength
/// Convert a ClipLength to a float value for animation interpolation
pub(crate) fn clip_length_to_percent(len: &ClipLength) -> f32 {
    match len {
        ClipLength::Percent(p) => *p,
        ClipLength::Px(px) => *px,
    }
}

pub(crate) fn parse_clip_length(s: &str) -> Option<ClipLength> {
    let s = s.trim();
    if let Some(stripped) = s.strip_suffix('%') {
        stripped.trim().parse::<f32>().ok().map(ClipLength::Percent)
    } else if let Some(stripped) = s.strip_suffix("px") {
        stripped.trim().parse::<f32>().ok().map(ClipLength::Px)
    } else {
        // Bare number → pixels
        s.parse::<f32>().ok().map(ClipLength::Px)
    }
}

/// Parse the `at cx cy` position suffix (default: 50% 50%)
pub(crate) fn parse_at_position(s: &str) -> ((ClipLength, ClipLength), &str) {
    let default = (ClipLength::Percent(50.0), ClipLength::Percent(50.0));
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("at") {
        let rest = rest.trim();
        let tokens: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
        if tokens.len() == 2 {
            if let (Some(cx), Some(cy)) =
                (parse_clip_length(tokens[0]), parse_clip_length(tokens[1]))
            {
                return ((cx, cy), "");
            }
        }
        // single token → cx, cy=50%
        if let Some(cx) = tokens.first().and_then(|t| parse_clip_length(t)) {
            return ((cx, ClipLength::Percent(50.0)), "");
        }
        (default, s)
    } else {
        (default, s)
    }
}

/// Parse a full CSS `clip-path` value string into a `ClipPath`
///
/// Supported functions:
/// - `circle(radius at cx cy)`
/// - `ellipse(rx ry at cx cy)`
/// - `inset(top right bottom left round radius)`
/// - `rect(top right bottom left round radius)`
/// - `xywh(x y w h round radius)`
/// - `polygon(x1 y1, x2 y2, ...)`
/// - `path("M 0 0 L ...")`
pub fn parse_clip_path(value: &str) -> Option<ClipPath> {
    let value = value.trim();

    // Extract function name and inner content
    let paren_start = value.find('(')?;
    let paren_end = value.rfind(')')?;
    if paren_end <= paren_start {
        return None;
    }
    let func_name = value[..paren_start].trim().to_lowercase();
    let inner = value[paren_start + 1..paren_end].trim();

    match func_name.as_str() {
        "circle" => {
            // circle() or circle(radius) or circle(radius at cx cy) or circle(at cx cy)
            if inner.is_empty() {
                return Some(ClipPath::Circle {
                    radius: None,
                    center: (ClipLength::Percent(50.0), ClipLength::Percent(50.0)),
                });
            }
            // Check if starts with "at" (no radius)
            if inner.starts_with("at") {
                let (center, _) = parse_at_position(inner);
                return Some(ClipPath::Circle {
                    radius: None,
                    center,
                });
            }
            // radius [at cx cy]
            let parts: Vec<&str> = inner.splitn(2, " at ").collect();
            let radius = parse_clip_length(parts[0].trim());
            let center = if parts.len() > 1 {
                let (center, _) = parse_at_position(&format!("at {}", parts[1]));
                center
            } else {
                (ClipLength::Percent(50.0), ClipLength::Percent(50.0))
            };
            Some(ClipPath::Circle { radius, center })
        }
        "ellipse" => {
            // ellipse() or ellipse(rx ry) or ellipse(rx ry at cx cy)
            if inner.is_empty() {
                return Some(ClipPath::Ellipse {
                    rx: None,
                    ry: None,
                    center: (ClipLength::Percent(50.0), ClipLength::Percent(50.0)),
                });
            }
            if inner.starts_with("at") {
                let (center, _) = parse_at_position(inner);
                return Some(ClipPath::Ellipse {
                    rx: None,
                    ry: None,
                    center,
                });
            }
            let parts: Vec<&str> = inner.splitn(2, " at ").collect();
            let radii_str = parts[0].trim();
            let radii_tokens: Vec<&str> = radii_str.split_whitespace().collect();
            let (rx, ry) = if radii_tokens.len() >= 2 {
                (
                    parse_clip_length(radii_tokens[0]),
                    parse_clip_length(radii_tokens[1]),
                )
            } else if radii_tokens.len() == 1 {
                let r = parse_clip_length(radii_tokens[0]);
                (r, r)
            } else {
                (None, None)
            };
            let center = if parts.len() > 1 {
                let (center, _) = parse_at_position(&format!("at {}", parts[1]));
                center
            } else {
                (ClipLength::Percent(50.0), ClipLength::Percent(50.0))
            };
            Some(ClipPath::Ellipse { rx, ry, center })
        }
        "inset" | "rect" => {
            // inset(top right bottom left [round radius])
            // rect(top right bottom left [round radius])
            let (inner_no_round, round_part) = if let Some(idx) = inner.find("round") {
                (inner[..idx].trim(), Some(inner[idx..].trim()))
            } else {
                (inner, None)
            };
            let round = round_part
                .and_then(|r| r.strip_prefix("round").and_then(|v| parse_css_px(v.trim())));
            let tokens: Vec<&str> = inner_no_round.split_whitespace().collect();
            let (top, right, bottom, left) = match tokens.len() {
                1 => {
                    let v = parse_clip_length(tokens[0])?;
                    (v, v, v, v)
                }
                2 => {
                    let tb = parse_clip_length(tokens[0])?;
                    let lr = parse_clip_length(tokens[1])?;
                    (tb, lr, tb, lr)
                }
                3 => {
                    let t = parse_clip_length(tokens[0])?;
                    let lr = parse_clip_length(tokens[1])?;
                    let b = parse_clip_length(tokens[2])?;
                    (t, lr, b, lr)
                }
                4.. => {
                    let t = parse_clip_length(tokens[0])?;
                    let r = parse_clip_length(tokens[1])?;
                    let b = parse_clip_length(tokens[2])?;
                    let l = parse_clip_length(tokens[3])?;
                    (t, r, b, l)
                }
                _ => return None,
            };
            if func_name == "inset" {
                Some(ClipPath::Inset {
                    top,
                    right,
                    bottom,
                    left,
                    round,
                })
            } else {
                Some(ClipPath::Rect {
                    top,
                    right,
                    bottom,
                    left,
                    round,
                })
            }
        }
        "xywh" => {
            // xywh(x y w h [round radius])
            let (inner_no_round, round_part) = if let Some(idx) = inner.find("round") {
                (inner[..idx].trim(), Some(inner[idx..].trim()))
            } else {
                (inner, None)
            };
            let round = round_part
                .and_then(|r| r.strip_prefix("round").and_then(|v| parse_css_px(v.trim())));
            let tokens: Vec<&str> = inner_no_round.split_whitespace().collect();
            if tokens.len() < 4 {
                return None;
            }
            let x = parse_clip_length(tokens[0])?;
            let y = parse_clip_length(tokens[1])?;
            let w = parse_clip_length(tokens[2])?;
            let h = parse_clip_length(tokens[3])?;
            Some(ClipPath::Xywh { x, y, w, h, round })
        }
        "polygon" => {
            // polygon(x1 y1, x2 y2, ...)
            let point_strs: Vec<&str> = inner.split(',').collect();
            let mut points = Vec::new();
            for ps in point_strs {
                let coords: Vec<&str> = ps.split_whitespace().collect();
                if coords.len() >= 2 {
                    let x = parse_clip_length(coords[0])?;
                    let y = parse_clip_length(coords[1])?;
                    points.push((x, y));
                }
            }
            if points.len() < 3 {
                return None;
            }
            Some(ClipPath::Polygon { points })
        }
        "path" => {
            // path("M 0 0 L 100 0 ...")
            // Extract the quoted string
            let inner = inner.trim();
            let path_str = if (inner.starts_with('"') && inner.ends_with('"'))
                || (inner.starts_with('\'') && inner.ends_with('\''))
            {
                &inner[1..inner.len() - 1]
            } else {
                inner
            };
            let vertices = flatten_svg_path(path_str)?;
            if vertices.len() < 3 {
                return None;
            }
            Some(ClipPath::Path { vertices })
        }
        _ => None,
    }
}
