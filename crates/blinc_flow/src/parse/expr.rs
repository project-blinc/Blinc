//! `@flow` expression grammar.
//!
//! A recursive-descent parser over the usual precedence ladder: additive
//! over multiplicative over unary over primary. Primaries are numbers,
//! colors, identifiers, calls and parenthesized expressions, any of which
//! may carry a trailing swizzle such as `.xyz`.

use blinc_core::{FlowExpr, FlowFunc};

/// Parse a flow expression string into a FlowExpr AST.
///
/// Operator precedence (low → high):
/// 1. `+`, `-` (additive)
/// 2. `*`, `/` (multiplicative)
/// 3. Unary `-` (negation)
/// 4. Function calls, constructors, literals, references, parens
pub(crate) fn parse_flow_expr(input: &str) -> Result<FlowExpr, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty expression".to_string());
    }
    let (expr, rest) = parse_flow_additive(input)?;
    let rest = rest.trim();
    if !rest.is_empty() {
        return Err(format!("unexpected trailing content: '{}'", rest));
    }
    Ok(expr)
}

/// Parse additive expressions: `a + b`, `a - b`
#[allow(clippy::manual_strip)]
pub(crate) fn parse_flow_additive(input: &str) -> Result<(FlowExpr, &str), String> {
    let (mut left, mut rest) = parse_flow_multiplicative(input)?;

    loop {
        let trimmed = rest.trim_start();
        if trimmed.starts_with('+') {
            let (right, r) = parse_flow_multiplicative(&trimmed[1..])?;
            left = FlowExpr::Add(Box::new(left), Box::new(right));
            rest = r;
        } else if trimmed.starts_with('-') {
            // Distinguish binary minus from unary minus / negative literal
            // If '-' is followed by a digit and we're after an operator position, it's unary
            // Binary minus: appears after a complete expression
            let _after = trimmed[1..].trim_start();
            // Check if the character after '-' starts what could be a token
            // This is binary minus because left already parsed successfully
            let (right, r) = parse_flow_multiplicative(&trimmed[1..])?;
            left = FlowExpr::Sub(Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }

    Ok((left, rest))
}

/// Parse multiplicative expressions: `a * b`, `a / b`
#[allow(clippy::manual_strip)]
pub(crate) fn parse_flow_multiplicative(input: &str) -> Result<(FlowExpr, &str), String> {
    let (mut left, mut rest) = parse_flow_unary(input)?;

    loop {
        let trimmed = rest.trim_start();
        if trimmed.starts_with('*') {
            let (right, r) = parse_flow_unary(&trimmed[1..])?;
            left = FlowExpr::Mul(Box::new(left), Box::new(right));
            rest = r;
        } else if trimmed.starts_with('/') {
            let (right, r) = parse_flow_unary(&trimmed[1..])?;
            left = FlowExpr::Div(Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }

    Ok((left, rest))
}

/// Parse unary expressions: `-a`
#[allow(clippy::manual_strip)]
pub(crate) fn parse_flow_unary(input: &str) -> Result<(FlowExpr, &str), String> {
    let trimmed = input.trim_start();
    if trimmed.starts_with('-') {
        // Check it's not just a negative number (handled in primary)
        let after = trimmed[1..].trim_start();
        if after.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
            // Could be a negative literal — try primary first
            if let Ok(result) = parse_flow_primary(trimmed) {
                return Ok(result);
            }
        }
        let (expr, rest) = parse_flow_unary(&trimmed[1..])?;
        Ok((FlowExpr::Neg(Box::new(expr)), rest))
    } else {
        parse_flow_primary(trimmed)
    }
}

/// Parse primary expressions: literals, refs, function calls, parens, vec constructors, colors
pub(crate) fn parse_flow_primary(input: &str) -> Result<(FlowExpr, &str), String> {
    let (expr, rest) = parse_flow_primary_inner(input)?;
    // Check for swizzle access (.x, .xy, .rgb, etc.)
    Ok(try_parse_flow_swizzle(expr, rest))
}

/// Check for and consume a swizzle suffix like `.x`, `.xy`, `.rgb`
/// Tolerates whitespace around the dot (e.g. `uv . x` from stringify!())
pub(crate) fn try_parse_flow_swizzle(expr: FlowExpr, rest: &str) -> (FlowExpr, &str) {
    let trimmed = rest.trim_start();
    if !trimmed.starts_with('.') {
        return (expr, trimmed);
    }
    let after_dot = trimmed[1..].trim_start();
    let swizzle_end = after_dot
        .find(|c: char| !matches!(c, 'x' | 'y' | 'z' | 'w' | 'r' | 'g' | 'b' | 'a'))
        .unwrap_or(after_dot.len());
    if swizzle_end == 0 || swizzle_end > 4 {
        return (expr, trimmed);
    }
    // Make sure we're not accidentally consuming an identifier that starts with x/y/z/w
    // e.g. "uv.xyz_thing" — 'xyz' is valid swizzle but '_thing' shouldn't be left
    // Check that the character after the swizzle is not alphanumeric/underscore
    if swizzle_end < after_dot.len() {
        let next = after_dot.as_bytes()[swizzle_end];
        if next.is_ascii_alphanumeric() || next == b'_' {
            return (expr, trimmed);
        }
    }
    let components = &after_dot[..swizzle_end];
    (
        FlowExpr::Swizzle(Box::new(expr), components.to_string()),
        &after_dot[swizzle_end..],
    )
}

#[allow(clippy::manual_strip)]
pub(crate) fn parse_flow_primary_inner(input: &str) -> Result<(FlowExpr, &str), String> {
    let trimmed = input.trim_start();

    if trimmed.is_empty() {
        return Err("unexpected end of expression".to_string());
    }

    // Color literal: #RRGGBB or #RRGGBBAA
    if trimmed.starts_with('#') {
        return parse_flow_color(trimmed);
    }

    // Parenthesized expression
    if trimmed.starts_with('(') {
        let inner_start = &trimmed[1..];
        let close = find_flow_close_paren(inner_start)
            .ok_or_else(|| "unmatched parenthesis".to_string())?;
        let inner = inner_start[..close].trim();
        let (expr, inner_rest) = parse_flow_additive(inner)?;
        let inner_rest = inner_rest.trim();
        if !inner_rest.is_empty() {
            return Err(format!(
                "unexpected content in parenthesized expression: '{}'",
                inner_rest
            ));
        }
        return Ok((expr, &inner_start[close + 1..]));
    }

    // Number literal (including negative)
    if trimmed.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        || (trimmed.starts_with('-')
            && trimmed[1..]
                .trim_start()
                .starts_with(|c: char| c.is_ascii_digit() || c == '.'))
    {
        return parse_flow_number(trimmed);
    }

    // Identifier: could be function call, vec constructor, or reference
    if trimmed.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        let name_end = trimmed
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .unwrap_or(trimmed.len());
        let name = &trimmed[..name_end];
        let after = trimmed[name_end..].trim_start();

        // Function call or vector constructor
        if after.starts_with('(') {
            let args_start = &after[1..];
            let close = find_flow_close_paren(args_start)
                .ok_or_else(|| format!("unmatched parenthesis in call to '{}'", name))?;
            let args_str = &args_start[..close];
            let rest = &args_start[close + 1..];

            let args = parse_flow_arg_list(args_str)?;

            // Vec constructors
            match name {
                "vec2" => {
                    if args.len() != 2 {
                        return Err(format!("vec2 requires 2 arguments, got {}", args.len()));
                    }
                    let mut it = args.into_iter();
                    return Ok((
                        FlowExpr::Vec2(Box::new(it.next().unwrap()), Box::new(it.next().unwrap())),
                        rest,
                    ));
                }
                "vec3" => {
                    if args.len() != 3 {
                        return Err(format!("vec3 requires 3 arguments, got {}", args.len()));
                    }
                    let mut it = args.into_iter();
                    return Ok((
                        FlowExpr::Vec3(
                            Box::new(it.next().unwrap()),
                            Box::new(it.next().unwrap()),
                            Box::new(it.next().unwrap()),
                        ),
                        rest,
                    ));
                }
                "vec4" => {
                    if args.len() != 4 {
                        return Err(format!("vec4 requires 4 arguments, got {}", args.len()));
                    }
                    let mut it = args.into_iter();
                    return Ok((
                        FlowExpr::Vec4(
                            Box::new(it.next().unwrap()),
                            Box::new(it.next().unwrap()),
                            Box::new(it.next().unwrap()),
                            Box::new(it.next().unwrap()),
                        ),
                        rest,
                    ));
                }
                _ => {
                    // Look up as built-in function
                    if let Some(func) = FlowFunc::parse(name) {
                        return Ok((FlowExpr::Call { func, args }, rest));
                    } else {
                        return Err(format!("unknown function '{}'", name));
                    }
                }
            }
        }

        // Plain reference
        return Ok((FlowExpr::Ref(name.to_string()), &trimmed[name_end..]));
    }

    Err(format!("unexpected character: '{}'", &trimmed[..1]))
}

/// Parse a comma-separated argument list (within already-matched parens)
pub(crate) fn parse_flow_arg_list(input: &str) -> Result<Vec<FlowExpr>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut args = Vec::new();
    let mut remaining = trimmed;

    loop {
        let remaining_trimmed = remaining.trim();
        if remaining_trimmed.is_empty() {
            break;
        }

        // Split at top-level commas (not nested in parens)
        let split_pos = find_top_level_comma(remaining_trimmed);

        let arg_str = if let Some(pos) = split_pos {
            let s = remaining_trimmed[..pos].trim();
            remaining = &remaining_trimmed[pos + 1..];
            s
        } else {
            remaining = "";
            remaining_trimmed
        };

        if arg_str.is_empty() {
            break;
        }

        let (expr, rest) = parse_flow_additive(arg_str)?;
        let rest = rest.trim();
        if !rest.is_empty() {
            return Err(format!("unexpected content in argument: '{}'", rest));
        }
        args.push(expr);

        if split_pos.is_none() {
            break;
        }
    }

    Ok(args)
}

/// Find the position of the next top-level comma (not nested in parens)
pub(crate) fn find_top_level_comma(input: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Find the matching close paren for a flow expression.
/// Input starts AFTER the opening '(' — starts at depth=1.
pub(crate) fn find_flow_close_paren(input: &str) -> Option<usize> {
    let mut depth = 1i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a numeric literal (float or integer)
pub(crate) fn parse_flow_number(input: &str) -> Result<(FlowExpr, &str), String> {
    let trimmed = input.trim_start();
    let mut end = 0;
    let chars: Vec<char> = trimmed.chars().collect();

    // Optional leading minus
    if end < chars.len() && chars[end] == '-' {
        end += 1;
    }

    // Digits before decimal point
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }

    // Optional decimal point and fractional digits
    if end < chars.len() && chars[end] == '.' {
        end += 1;
        while end < chars.len() && chars[end].is_ascii_digit() {
            end += 1;
        }
    }

    if end == 0 || (end == 1 && chars[0] == '-') {
        return Err("expected number".to_string());
    }

    let num_str = &trimmed[..end];
    let value: f32 = num_str
        .parse()
        .map_err(|_| format!("invalid number: '{}'", num_str))?;

    Ok((FlowExpr::Float(value), &trimmed[end..]))
}

/// Parse a color literal: #RGB, #RRGGBB, or #RRGGBBAA
pub(crate) fn parse_flow_color(input: &str) -> Result<(FlowExpr, &str), String> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('#') {
        return Err("expected '#' for color literal".to_string());
    }

    let hex_start = trimmed[1..].trim_start();
    let hex_end = hex_start
        .find(|c: char| !c.is_ascii_hexdigit())
        .unwrap_or(hex_start.len());
    let hex = &hex_start[..hex_end];

    let (r, g, b, a) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).unwrap_or(0);
            (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            let a = u8::from_str_radix(&hex[6..8], 16).unwrap_or(255);
            (
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            )
        }
        _ => return Err(format!("invalid color hex length: {}", hex.len())),
    };

    Ok((FlowExpr::Color(r, g, b, a), &hex_start[hex_end..]))
}
