//! Gradient values.
//!
//! Parses linear, radial and conic gradients, including the angle or
//! side-or-corner direction, and the color-stop list. A stop with no
//! explicit position is distributed evenly between its neighbours.

use blinc_core::{Gradient, GradientSpace, GradientStop, Point};

use crate::parser::*;

/// Parse CSS linear-gradient()
///
/// Syntax:
/// - `linear-gradient(135deg, #667eea 0%, #764ba2 100%)`
/// - `linear-gradient(to right, red, blue)`
/// - `linear-gradient(to bottom right, #fff, #000)`
/// - `linear-gradient(90deg, red 0%, yellow 50%, green 100%)`
pub(crate) fn parse_linear_gradient(input: &str) -> Option<Gradient> {
    // Strip the function wrapper
    let inner = input
        .strip_prefix("linear-gradient(")
        .and_then(|s| s.strip_suffix(')'))?
        .trim();

    // Split by commas, but be careful with colors that contain commas (rgb, rgba)
    let parts = split_gradient_parts(inner);
    if parts.is_empty() {
        return None;
    }

    // Parse angle/direction (first part might be angle or first color stop)
    let (angle_deg, color_start_idx) = parse_gradient_direction(&parts[0]);

    // Parse color stops
    let stops = parse_color_stops(&parts[color_start_idx..])?;
    if stops.len() < 2 {
        return None;
    }

    // Convert angle to start/end points (using ObjectBoundingBox space 0-1)
    let (start, end) = angle_to_gradient_points(angle_deg);

    Some(Gradient::Linear {
        start,
        end,
        stops,
        space: GradientSpace::ObjectBoundingBox,
        spread: blinc_core::GradientSpread::Pad,
    })
}

/// Parse CSS radial-gradient()
///
/// Syntax:
/// - `radial-gradient(circle, red, blue)`
/// - `radial-gradient(circle at center, red, blue)`
/// - `radial-gradient(ellipse at 25% 25%, red, blue)`
pub(crate) fn parse_radial_gradient(input: &str) -> Option<Gradient> {
    let inner = input
        .strip_prefix("radial-gradient(")
        .and_then(|s| s.strip_suffix(')'))?
        .trim();

    let parts = split_gradient_parts(inner);
    if parts.is_empty() {
        return None;
    }

    // Check for shape/position specification
    let mut center = Point::new(0.5, 0.5); // Default center
    let mut color_start_idx = 0;

    // First part might be shape/position info
    let first = parts[0].trim().to_lowercase();
    if first.starts_with("circle") || first.starts_with("ellipse") {
        // Parse "circle at X Y" or just "circle"
        if let Some(at_pos) = first.find(" at ") {
            let pos_str = &first[at_pos + 4..];
            if let Some(pos) = parse_position(pos_str) {
                center = pos;
            }
        }
        color_start_idx = 1;
    } else if first.contains(" at ") || first.starts_with("at ") {
        // Just position: "at center" or "at 50% 50%"
        let pos_str = first.strip_prefix("at ").unwrap_or(&first);
        if let Some(pos) = parse_position(pos_str) {
            center = pos;
        }
        color_start_idx = 1;
    }

    let stops = parse_color_stops(&parts[color_start_idx..])?;
    if stops.len() < 2 {
        return None;
    }

    Some(Gradient::Radial {
        center,
        radius: 0.5, // Default radius for ObjectBoundingBox space
        focal: None,
        stops,
        space: GradientSpace::ObjectBoundingBox,
        spread: blinc_core::GradientSpread::Pad,
    })
}

/// Parse CSS conic-gradient()
///
/// Syntax:
/// - `conic-gradient(red, yellow, green, blue, red)`
/// - `conic-gradient(from 45deg, red, blue)`
/// - `conic-gradient(from 0deg at center, red 0deg, blue 360deg)`
pub(crate) fn parse_conic_gradient(input: &str) -> Option<Gradient> {
    let inner = input
        .strip_prefix("conic-gradient(")
        .and_then(|s| s.strip_suffix(')'))?
        .trim();

    let parts = split_gradient_parts(inner);
    if parts.is_empty() {
        return None;
    }

    let mut start_angle: f32 = 0.0;
    let mut center = Point::new(0.5, 0.5);
    let mut color_start_idx = 0;

    // Check for "from Xdeg" and/or "at position"
    let first = parts[0].trim().to_lowercase();
    if let Some(rest) = first.strip_prefix("from ") {
        // Parse "from 45deg" or "from 45deg at center"
        if let Some(at_pos) = rest.find(" at ") {
            // Has both angle and position
            let angle_str = rest[..at_pos].trim();
            if let Some(angle) = parse_angle_value(angle_str) {
                start_angle = angle;
            }
            let pos_str = &rest[at_pos + 4..];
            if let Some(pos) = parse_position(pos_str) {
                center = pos;
            }
        } else {
            // Just angle
            if let Some(angle) = parse_angle_value(rest.trim()) {
                start_angle = angle;
            }
        }
        color_start_idx = 1;
    } else if let Some(rest) = first.strip_prefix("at ") {
        // Just position
        if let Some(pos) = parse_position(rest) {
            center = pos;
        }
        color_start_idx = 1;
    }

    let stops = parse_color_stops(&parts[color_start_idx..])?;
    if stops.len() < 2 {
        return None;
    }

    Some(Gradient::Conic {
        center,
        start_angle: start_angle * std::f32::consts::PI / 180.0, // Convert to radians
        stops,
        space: GradientSpace::ObjectBoundingBox,
    })
}

/// Split gradient arguments by commas, respecting parentheses for rgb()/rgba()
pub(crate) fn split_gradient_parts(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren_depth: i32 = 0;

    for c in input.chars() {
        match c {
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                paren_depth = (paren_depth - 1).max(0);
                current.push(c);
            }
            ',' if paren_depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    parts.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        parts.push(trimmed);
    }

    parts
}

/// Parse gradient direction (angle or `to <direction>`).
/// Returns `(angle_in_degrees, color_start_index)`.
pub(crate) fn parse_gradient_direction(first_part: &str) -> (f32, usize) {
    let part = first_part.trim().to_lowercase();

    // Try parsing as angle (e.g., "135deg", "45deg")
    if let Some(angle) = parse_angle_value(&part) {
        return (angle, 1);
    }

    // Try parsing as direction keyword
    if let Some(direction) = part.strip_prefix("to ") {
        let angle = match direction.trim() {
            "top" => 0.0,
            "right" => 90.0,
            "bottom" => 180.0,
            "left" => 270.0,
            "top right" | "right top" => 45.0,
            "bottom right" | "right bottom" => 135.0,
            "bottom left" | "left bottom" => 225.0,
            "top left" | "left top" => 315.0,
            _ => return (180.0, 0), // Default to "to bottom" if unrecognized, treat as color
        };
        return (angle, 1);
    }

    // Not a direction - default to "to bottom" (180deg) and treat first part as color
    (180.0, 0)
}

/// Convert CSS gradient angle to start/end points
/// CSS angles: 0deg = to top, 90deg = to right, 180deg = to bottom, 270deg = to left
/// In ObjectBoundingBox space (0-1 coordinates)
pub fn angle_to_gradient_points(angle_deg: f32) -> (Point, Point) {
    // CSS gradient angles are measured clockwise from top (0deg = up)
    // Convert to mathematical angle (counterclockwise from right)
    let angle_rad = (90.0 - angle_deg) * std::f32::consts::PI / 180.0;

    // Calculate direction vector
    let dx = angle_rad.cos();
    let dy = -angle_rad.sin(); // Negative because Y grows downward in screen coords

    // Find intersection with unit square
    // We want the gradient line to span the full diagonal based on angle
    let center = Point::new(0.5, 0.5);

    // Calculate the length needed to reach corners
    let len = if dx.abs() > dy.abs() {
        0.5 / dx.abs()
    } else if dy.abs() > 0.0 {
        0.5 / dy.abs()
    } else {
        0.5
    };

    let start = Point::new(center.x - dx * len, center.y - dy * len);
    let end = Point::new(center.x + dx * len, center.y + dy * len);

    (start, end)
}

/// Reverse-compute CSS gradient angle (in degrees) from ObjectBoundingBox start/end points.
/// Inverse of `angle_to_gradient_points`.
pub fn gradient_points_to_angle(start: Point, end: Point) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    // CSS: 0deg = to top, 90deg = to right, clockwise
    // atan2 gives counterclockwise from positive-x
    let math_angle = (-dy).atan2(dx); // negate dy for screen coords
    let css_deg = 90.0 - math_angle * 180.0 / std::f32::consts::PI;
    // Normalize to 0..360
    ((css_deg % 360.0) + 360.0) % 360.0
}

/// Parse color stops from gradient parts
pub(crate) fn parse_color_stops(parts: &[String]) -> Option<Vec<GradientStop>> {
    if parts.is_empty() {
        return None;
    }

    let mut stops = Vec::new();
    let total = parts.len();

    for (i, part) in parts.iter().enumerate() {
        if let Some(stop) = parse_single_color_stop(part, i, total) {
            stops.push(stop);
        }
    }

    // Ensure we have at least 2 stops
    if stops.len() < 2 {
        return None;
    }

    // Fill in missing positions (evenly distributed)
    distribute_stop_positions(&mut stops);

    Some(stops)
}

/// Parse a single color stop (e.g., "red", "#667eea 50%", "rgba(255,0,0,0.5) 25%")
pub(crate) fn parse_single_color_stop(
    part: &str,
    index: usize,
    total: usize,
) -> Option<GradientStop> {
    let part = part.trim();

    // Try to find a percentage or length at the end
    let (color_str, position) = extract_color_and_position(part, index, total);

    let color = parse_color(color_str)?;
    Some(GradientStop::new(position, color))
}

/// Extract color and position from a color stop string
pub(crate) fn extract_color_and_position(part: &str, index: usize, total: usize) -> (&str, f32) {
    // Check for percentage at the end
    if let Some(pct_pos) = part.rfind('%') {
        // Find where the number starts (work backwards from %)
        let before_pct = &part[..pct_pos];
        if let Some(space_pos) =
            before_pct.rfind(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        {
            let num_str = &part[space_pos + 1..pct_pos];
            if let Ok(pct) = num_str.trim().parse::<f32>() {
                let color_str = part[..=space_pos].trim();
                return (color_str, pct / 100.0);
            }
        } else {
            // The whole thing before % is a number
            if let Ok(pct) = before_pct.trim().parse::<f32>() {
                // This shouldn't happen for valid color stops, but handle it
                return (part, pct / 100.0);
            }
        }
    }

    // Check for pixel value at the end (less common in CSS but valid)
    if let Some(px_pos) = part.rfind("px") {
        let before_px = &part[..px_pos];
        if let Some(space_pos) =
            before_px.rfind(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        {
            let num_str = &part[space_pos + 1..px_pos];
            if let Ok(_px) = num_str.trim().parse::<f32>() {
                // For now, ignore pixel values and use default positioning
                let color_str = part[..=space_pos].trim();
                return (color_str, default_position(index, total));
            }
        }
    }

    // No explicit position - use default
    (part, default_position(index, total))
}

/// Calculate default position for a color stop
pub(crate) fn default_position(index: usize, total: usize) -> f32 {
    if total <= 1 {
        0.0
    } else {
        index as f32 / (total - 1) as f32
    }
}

/// Fill in missing/default positions with even distribution
pub(crate) fn distribute_stop_positions(_stops: &mut [GradientStop]) {
    // The positions are already set during parsing
    // This function could be enhanced to handle "auto" positions
    // For now, we rely on default_position during parsing
}

/// Parse position keywords (for radial/conic gradients)
pub(crate) fn parse_position(input: &str) -> Option<Point> {
    let input = input.trim().to_lowercase();

    // Single keyword
    match input.as_str() {
        "center" => return Some(Point::new(0.5, 0.5)),
        "top" => return Some(Point::new(0.5, 0.0)),
        "bottom" => return Some(Point::new(0.5, 1.0)),
        "left" => return Some(Point::new(0.0, 0.5)),
        "right" => return Some(Point::new(1.0, 0.5)),
        "top left" | "left top" => return Some(Point::new(0.0, 0.0)),
        "top right" | "right top" => return Some(Point::new(1.0, 0.0)),
        "bottom left" | "left bottom" => return Some(Point::new(0.0, 1.0)),
        "bottom right" | "right bottom" => return Some(Point::new(1.0, 1.0)),
        _ => {}
    }

    // Try parsing as "X% Y%" or "Xpx Ypx"
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() >= 2 {
        let x = parse_position_value(parts[0])?;
        let y = parse_position_value(parts[1])?;
        return Some(Point::new(x, y));
    }

    None
}

/// Parse a single position value (percentage or keyword)
pub(crate) fn parse_position_value(input: &str) -> Option<f32> {
    let input = input.trim();

    if let Some(pct_str) = input.strip_suffix('%') {
        return pct_str.trim().parse::<f32>().ok().map(|p| p / 100.0);
    }

    // Keywords
    match input {
        "left" | "top" => Some(0.0),
        "center" => Some(0.5),
        "right" | "bottom" => Some(1.0),
        _ => None,
    }
}

// ============================================================================
// CSS clip-path Parsing
// ============================================================================
