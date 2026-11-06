//! SVG path morphing — interpolate between two SVG path data strings.
//!
//! Supports animating the `d` attribute by normalizing both paths to cubic
//! Bezier segments and interpolating control points. Paths must have compatible
//! segment counts (same number of commands after normalization).

/// A parsed SVG path segment (absolute coordinates only).
#[derive(Clone, Debug)]
pub enum PathSegment {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32), // c1x, c1y, c2x, c2y, x, y
    Close,
}

/// Parse an SVG path data string into segments.
///
/// Handles M, L, H, V, C, S, Q, T, Z commands (both absolute and relative).
/// All output is converted to absolute coordinates and simplified to
/// MoveTo, LineTo, CubicTo, Close.
pub fn parse_path_data(d: &str) -> Vec<PathSegment> {
    let mut segments = Vec::new();
    let mut cx = 0.0_f32;
    let mut cy = 0.0_f32;
    let mut start_x = 0.0_f32;
    let mut start_y = 0.0_f32;
    // For S/T smooth commands
    let mut last_c2x = 0.0_f32;
    let mut last_c2y = 0.0_f32;
    let mut last_cmd = ' ';

    let tokens = tokenize_path(d);
    let mut i = 0;

    while i < tokens.len() {
        let cmd = tokens[i].cmd;
        let nums = &tokens[i].params;
        let is_rel = cmd.is_ascii_lowercase();
        let cmd_upper = cmd.to_ascii_uppercase();

        match cmd_upper {
            'M' => {
                let mut j = 0;
                while j + 1 < nums.len() {
                    let (mut x, mut y) = (nums[j], nums[j + 1]);
                    if is_rel {
                        x += cx;
                        y += cy;
                    }
                    if j == 0 {
                        segments.push(PathSegment::MoveTo(x, y));
                        start_x = x;
                        start_y = y;
                    } else {
                        // Subsequent coordinate pairs after M are implicit LineTo
                        segments.push(PathSegment::LineTo(x, y));
                    }
                    cx = x;
                    cy = y;
                    j += 2;
                }
            }
            'L' => {
                let mut j = 0;
                while j + 1 < nums.len() {
                    let (mut x, mut y) = (nums[j], nums[j + 1]);
                    if is_rel {
                        x += cx;
                        y += cy;
                    }
                    segments.push(PathSegment::LineTo(x, y));
                    cx = x;
                    cy = y;
                    j += 2;
                }
            }
            'H' => {
                for &val in nums {
                    let x = if is_rel { cx + val } else { val };
                    segments.push(PathSegment::LineTo(x, cy));
                    cx = x;
                }
            }
            'V' => {
                for &val in nums {
                    let y = if is_rel { cy + val } else { val };
                    segments.push(PathSegment::LineTo(cx, y));
                    cy = y;
                }
            }
            'C' => {
                let mut j = 0;
                while j + 5 < nums.len() {
                    let (mut c1x, mut c1y, mut c2x, mut c2y, mut x, mut y) = (
                        nums[j],
                        nums[j + 1],
                        nums[j + 2],
                        nums[j + 3],
                        nums[j + 4],
                        nums[j + 5],
                    );
                    if is_rel {
                        c1x += cx;
                        c1y += cy;
                        c2x += cx;
                        c2y += cy;
                        x += cx;
                        y += cy;
                    }
                    segments.push(PathSegment::CubicTo(c1x, c1y, c2x, c2y, x, y));
                    last_c2x = c2x;
                    last_c2y = c2y;
                    cx = x;
                    cy = y;
                    j += 6;
                }
            }
            'S' => {
                let mut j = 0;
                while j + 3 < nums.len() {
                    // Reflect previous control point
                    let c1x = if last_cmd == 'C' || last_cmd == 'S' {
                        2.0 * cx - last_c2x
                    } else {
                        cx
                    };
                    let c1y = if last_cmd == 'C' || last_cmd == 'S' {
                        2.0 * cy - last_c2y
                    } else {
                        cy
                    };
                    let (mut c2x, mut c2y, mut x, mut y) =
                        (nums[j], nums[j + 1], nums[j + 2], nums[j + 3]);
                    if is_rel {
                        c2x += cx;
                        c2y += cy;
                        x += cx;
                        y += cy;
                    }
                    segments.push(PathSegment::CubicTo(c1x, c1y, c2x, c2y, x, y));
                    last_c2x = c2x;
                    last_c2y = c2y;
                    cx = x;
                    cy = y;
                    j += 4;
                }
            }
            'Q' => {
                let mut j = 0;
                while j + 3 < nums.len() {
                    let (mut qx, mut qy, mut x, mut y) =
                        (nums[j], nums[j + 1], nums[j + 2], nums[j + 3]);
                    if is_rel {
                        qx += cx;
                        qy += cy;
                        x += cx;
                        y += cy;
                    }
                    // Convert quadratic to cubic: C = Q0 + 2/3*(Q1-Q0), Q2 + 2/3*(Q1-Q2)
                    let c1x = cx + 2.0 / 3.0 * (qx - cx);
                    let c1y = cy + 2.0 / 3.0 * (qy - cy);
                    let c2x = x + 2.0 / 3.0 * (qx - x);
                    let c2y = y + 2.0 / 3.0 * (qy - y);
                    segments.push(PathSegment::CubicTo(c1x, c1y, c2x, c2y, x, y));
                    last_c2x = c2x;
                    last_c2y = c2y;
                    cx = x;
                    cy = y;
                    j += 4;
                }
            }
            'T' => {
                let mut j = 0;
                while j + 1 < nums.len() {
                    let qx = if last_cmd == 'Q' || last_cmd == 'T' {
                        2.0 * cx - last_c2x
                    } else {
                        cx
                    };
                    let qy = if last_cmd == 'Q' || last_cmd == 'T' {
                        2.0 * cy - last_c2y
                    } else {
                        cy
                    };
                    let (mut x, mut y) = (nums[j], nums[j + 1]);
                    if is_rel {
                        x += cx;
                        y += cy;
                    }
                    let c1x = cx + 2.0 / 3.0 * (qx - cx);
                    let c1y = cy + 2.0 / 3.0 * (qy - cy);
                    let c2x = x + 2.0 / 3.0 * (qx - x);
                    let c2y = y + 2.0 / 3.0 * (qy - y);
                    segments.push(PathSegment::CubicTo(c1x, c1y, c2x, c2y, x, y));
                    last_c2x = qx;
                    last_c2y = qy;
                    cx = x;
                    cy = y;
                    j += 2;
                }
            }
            'A' => {
                // Arc — approximate with cubic Bezier (simplified: just lineto endpoint)
                let mut j = 0;
                while j + 6 < nums.len() {
                    let (mut x, mut y) = (nums[j + 5], nums[j + 6]);
                    if is_rel {
                        x += cx;
                        y += cy;
                    }
                    // Simple approximation: straight line to endpoint
                    // For better quality, would need full arc-to-cubic conversion
                    segments.push(PathSegment::LineTo(x, y));
                    cx = x;
                    cy = y;
                    j += 7;
                }
            }
            'Z' => {
                segments.push(PathSegment::Close);
                cx = start_x;
                cy = start_y;
            }
            _ => {}
        }

        last_cmd = cmd_upper;
        i += 1;
    }

    segments
}

/// Normalize a path to all-cubic representation.
/// LineTo becomes a degenerate cubic, MoveTo stays, Close stays.
pub fn normalize_to_cubic(segments: &[PathSegment]) -> Vec<PathSegment> {
    let mut result = Vec::with_capacity(segments.len());
    let mut cx = 0.0_f32;
    let mut cy = 0.0_f32;

    for seg in segments {
        match seg {
            PathSegment::MoveTo(x, y) => {
                result.push(PathSegment::MoveTo(*x, *y));
                cx = *x;
                cy = *y;
            }
            PathSegment::LineTo(x, y) => {
                // Convert line to degenerate cubic
                let c1x = cx + (x - cx) / 3.0;
                let c1y = cy + (y - cy) / 3.0;
                let c2x = cx + 2.0 * (x - cx) / 3.0;
                let c2y = cy + 2.0 * (y - cy) / 3.0;
                result.push(PathSegment::CubicTo(c1x, c1y, c2x, c2y, *x, *y));
                cx = *x;
                cy = *y;
            }
            PathSegment::CubicTo(c1x, c1y, c2x, c2y, x, y) => {
                result.push(PathSegment::CubicTo(*c1x, *c1y, *c2x, *c2y, *x, *y));
                cx = *x;
                cy = *y;
            }
            PathSegment::Close => {
                result.push(PathSegment::Close);
            }
        }
    }

    result
}

/// Interpolate between two paths at parameter `t` (0.0 = from, 1.0 = to).
///
/// Both paths are normalized to cubic segments. If segment counts don't match,
/// returns `None`. Otherwise returns the interpolated SVG path data string.
pub fn interpolate_paths(from_d: &str, to_d: &str, t: f32) -> Option<String> {
    let from = normalize_to_cubic(&parse_path_data(from_d));
    let to = normalize_to_cubic(&parse_path_data(to_d));

    if from.len() != to.len() {
        return None;
    }

    let mut result = String::with_capacity(from_d.len());

    for (f, t_seg) in from.iter().zip(to.iter()) {
        match (f, t_seg) {
            (PathSegment::MoveTo(fx, fy), PathSegment::MoveTo(tx, ty)) => {
                let x = lerp(*fx, *tx, t);
                let y = lerp(*fy, *ty, t);
                if result.is_empty() {
                    result.push_str(&format!("M{},{}", fmt(x), fmt(y)));
                } else {
                    result.push_str(&format!(" M{},{}", fmt(x), fmt(y)));
                }
            }
            (
                PathSegment::CubicTo(fc1x, fc1y, fc2x, fc2y, fx, fy),
                PathSegment::CubicTo(tc1x, tc1y, tc2x, tc2y, tx, ty),
            ) => {
                result.push_str(&format!(
                    " C{},{} {},{} {},{}",
                    fmt(lerp(*fc1x, *tc1x, t)),
                    fmt(lerp(*fc1y, *tc1y, t)),
                    fmt(lerp(*fc2x, *tc2x, t)),
                    fmt(lerp(*fc2y, *tc2y, t)),
                    fmt(lerp(*fx, *tx, t)),
                    fmt(lerp(*fy, *ty, t)),
                ));
            }
            (PathSegment::Close, PathSegment::Close) => {
                result.push_str(" Z");
            }
            // Mismatched segment types — can't interpolate
            _ => return None,
        }
    }

    Some(result)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn fmt(v: f32) -> String {
    // Use reasonable precision, strip trailing zeros
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    s.to_string()
}

// ============================================================================
// Path data tokenizer
// ============================================================================

struct PathToken {
    cmd: char,
    params: Vec<f32>,
}

fn tokenize_path(d: &str) -> Vec<PathToken> {
    let mut tokens = Vec::new();
    let mut chars = d.chars().peekable();
    let mut current_cmd = 'M';

    while chars.peek().is_some() {
        // Skip whitespace and commas
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == ',' {
                chars.next();
            } else {
                break;
            }
        }

        if chars.peek().is_none() {
            break;
        }

        // Check if next char is a command letter
        if let Some(&c) = chars.peek()
            && c.is_ascii_alphabetic()
        {
            current_cmd = c;
            chars.next();
        }

        // Parse numbers for this command
        let mut params = Vec::new();
        loop {
            // Skip whitespace and commas
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == ',' {
                    chars.next();
                } else {
                    break;
                }
            }

            if chars.peek().is_none() {
                break;
            }

            // Check if next is a number (digit, minus sign, or dot)
            if let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
                    let num = parse_number(&mut chars);
                    params.push(num);
                } else {
                    break; // next command letter
                }
            }
        }

        tokens.push(PathToken {
            cmd: current_cmd,
            params,
        });
    }

    tokens
}

fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> f32 {
    let mut s = String::new();
    let mut has_dot = false;
    let mut has_e = false;

    // Optional sign
    if let Some(&c) = chars.peek()
        && (c == '-' || c == '+')
    {
        s.push(c);
        chars.next();
    }

    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            s.push(c);
            chars.next();
        } else if c == '.' && !has_dot && !has_e {
            has_dot = true;
            s.push(c);
            chars.next();
        } else if (c == 'e' || c == 'E') && !has_e {
            has_e = true;
            s.push(c);
            chars.next();
            // Optional sign after exponent
            if let Some(&ec) = chars.peek()
                && (ec == '-' || ec == '+')
            {
                s.push(ec);
                chars.next();
            }
        } else {
            break;
        }
    }

    s.parse().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_path() {
        let segs = parse_path_data("M10,10 L90,10 L90,90 L10,90 Z");
        assert_eq!(segs.len(), 5); // M + 3L + Z
    }

    #[test]
    fn test_parse_cubic() {
        let segs = parse_path_data("M10,80 C40,10 65,10 95,80");
        assert_eq!(segs.len(), 2); // M + C
    }

    #[test]
    fn test_normalize_lines_to_cubic() {
        let segs = parse_path_data("M0,0 L100,0 L100,100 Z");
        let normalized = normalize_to_cubic(&segs);
        // M + 2 cubics + Z
        assert_eq!(normalized.len(), 4);
        assert!(matches!(normalized[0], PathSegment::MoveTo(..)));
        assert!(matches!(normalized[1], PathSegment::CubicTo(..)));
        assert!(matches!(normalized[2], PathSegment::CubicTo(..)));
        assert!(matches!(normalized[3], PathSegment::Close));
    }

    #[test]
    fn test_interpolate_identical_paths() {
        let d = "M10,10 L90,10 L90,90 Z";
        let result = interpolate_paths(d, d, 0.5);
        assert!(result.is_some());
    }

    #[test]
    fn test_interpolate_different_paths() {
        let from = "M10,10 L90,10 L90,90 Z";
        let to = "M20,20 L80,20 L80,80 Z";
        let result = interpolate_paths(from, to, 0.5).unwrap();
        // At t=0.5, should be halfway
        assert!(result.contains("M15"));
    }

    #[test]
    fn test_incompatible_paths() {
        let from = "M10,10 L90,10 Z"; // 3 segments
        let to = "M10,10 L90,10 L90,90 Z"; // 4 segments
        let result = interpolate_paths(from, to, 0.5);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_relative() {
        let segs = parse_path_data("m10,10 l80,0 l0,80 z");
        assert_eq!(segs.len(), 4);
        if let PathSegment::LineTo(x, _) = &segs[1] {
            assert!((*x - 90.0).abs() < 0.01); // 10 + 80
        }
    }

    #[test]
    fn test_parse_quadratic() {
        let segs = parse_path_data("M10,80 Q52,10 95,80");
        assert_eq!(segs.len(), 2); // M + cubic (converted from Q)
        assert!(matches!(segs[1], PathSegment::CubicTo(..)));
    }
}
