//! `@flow` semantic forms: steps, chains, and `use`.
//!
//! A step is a named computation with keyword parameters. A chain threads a
//! value through a sequence of links, each link a function applied to the
//! running value. A `use` pulls in a named flow defined elsewhere.

use std::collections::HashMap;

use blinc_core::{
    ChainLink, FlowChain, FlowExpr, FlowGraph, FlowStep, FlowUse, StepParam, StepType,
};

use crate::parser::*;

/// Find matching closing brace, tracking nested brace depth.
/// Input starts AFTER the opening `{`.
pub(crate) fn find_flow_close_brace(input: &str) -> Option<usize> {
    let mut depth = 1i32;
    let mut in_comment = false;
    for (i, c) in input.char_indices() {
        if in_comment {
            if c == '/' && i > 0 && input.as_bytes()[i - 1] == b'*' {
                in_comment = false;
            }
            continue;
        }
        if c == '*' && i > 0 && input.as_bytes()[i - 1] == b'/' {
            in_comment = true;
            continue;
        }
        match c {
            '{' => depth += 1,
            '}' => {
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

/// Find the end of a chain declaration (`;` at depth 0, respecting parens).
pub(crate) fn find_chain_end(input: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Split chain body at top-level `|` delimiters, respecting parentheses.
pub(crate) fn split_chain_links(input: &str) -> Vec<&str> {
    let mut links = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '|' if depth == 0 => {
                links.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    links.push(&input[start..]);
    links
}

/// Parse `step <name> : <step-type> { <param>: <value>; ... }`
pub(crate) fn parse_flow_step<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("step ")?.trim_start();

    // Parse name (alphanumeric + underscore + hyphen)
    let name_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let name = &rest[..name_end];
    let rest = rest[name_end..].trim_start();

    // Consume ':'
    let rest = rest.strip_prefix(':')?.trim_start();

    // Parse step type (kebab-case: alphanumeric + hyphen)
    let type_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '-')
        .unwrap_or(rest.len());
    if type_end == 0 {
        return None;
    }
    let type_str = &rest[..type_end];
    let rest = rest[type_end..].trim_start();

    // Must have opening brace
    let rest = rest.strip_prefix('{')?;

    // Find matching closing brace
    let close = find_flow_close_brace(rest)?;
    let body = &rest[..close];
    let after = &rest[close + 1..];

    let step_type = match StepType::parse(type_str) {
        Some(st) => st,
        None => {
            errors.push(ParseError {
                severity: Severity::Error,
                message: format!("unknown step type: '{}'", type_str),
                line: 0,
                column: 0,
                fragment: type_str.to_string(),
                contexts: vec![],
                property: None,
                value: None,
            });
            return Some(after);
        }
    };

    let params = parse_step_params(body, errors);

    graph.steps.push(FlowStep {
        name: name.to_string(),
        step_type,
        params,
    });

    Some(after)
}

/// Parse key: value; pairs inside a step block body.
pub(crate) fn parse_step_params(
    body: &str,
    errors: &mut Vec<ParseError>,
) -> HashMap<String, StepParam> {
    let mut params = HashMap::new();
    let mut remaining = body.trim();

    while !remaining.is_empty() {
        // Skip comments
        if remaining.starts_with("/*") {
            if let Some(end) = remaining.find("*/") {
                remaining = remaining[end + 2..].trim_start();
                continue;
            } else {
                break;
            }
        }

        // Find colon separator
        let colon = match remaining.find(':') {
            Some(pos) => pos,
            None => break,
        };
        let key = remaining[..colon].trim();
        if key.is_empty() {
            break;
        }
        remaining = remaining[colon + 1..].trim_start();

        // Find semicolon at top level (respecting parens)
        let semi = find_step_param_end(remaining);
        let value_str = remaining[..semi].trim();
        remaining = if semi < remaining.len() && remaining.as_bytes()[semi] == b';' {
            remaining[semi + 1..].trim_start()
        } else {
            remaining[semi..].trim_start()
        };

        if value_str.is_empty() {
            continue;
        }

        // Parse value based on key name or content
        if key == "sources" {
            // Comma-separated identifiers: sources: drops1, drops2, streaks;
            let idents: Vec<String> = value_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if idents.len() == 1 {
                params.insert(
                    key.to_string(),
                    StepParam::Ident(idents.into_iter().next().unwrap()),
                );
            } else {
                params.insert(key.to_string(), StepParam::IdentList(idents));
            }
        } else if key == "weights" {
            // Comma-separated floats: weights: 1.0, 0.5, 0.3;
            let floats: Result<Vec<f32>, _> = value_str
                .split(',')
                .map(|s| s.trim().parse::<f32>())
                .collect();
            match floats {
                Ok(list) if list.len() == 1 => {
                    params.insert(key.to_string(), StepParam::Expr(FlowExpr::Float(list[0])));
                }
                Ok(list) => {
                    params.insert(key.to_string(), StepParam::FloatList(list));
                }
                Err(_) => {
                    if let Ok(expr) = parse_flow_expr(value_str) {
                        params.insert(key.to_string(), StepParam::Expr(expr));
                    }
                }
            }
        } else if key == "stops" {
            match parse_color_stop_list(value_str) {
                Ok(stops) => {
                    params.insert(key.to_string(), StepParam::ColorStops(stops));
                }
                Err(e) => {
                    errors.push(ParseError {
                        severity: Severity::Error,
                        message: format!("invalid color stops: {}", e),
                        line: 0,
                        column: 0,
                        fragment: value_str.to_string(),
                        contexts: vec![],
                        property: Some(key.to_string()),
                        value: Some(value_str.to_string()),
                    });
                }
            }
        } else if let Ok(int_val) = value_str.parse::<i32>() {
            // Check it's not a float (e.g. "4.0" parses as float, not int)
            if !value_str.contains('.') {
                params.insert(key.to_string(), StepParam::Int(int_val));
            } else if let Ok(expr) = parse_flow_expr(value_str) {
                params.insert(key.to_string(), StepParam::Expr(expr));
            }
        } else if let Ok(expr) = parse_flow_expr(value_str) {
            params.insert(key.to_string(), StepParam::Expr(expr));
        } else {
            // Bare identifier (blend modes, curve names, style names)
            params.insert(key.to_string(), StepParam::Ident(value_str.to_string()));
        }
    }

    params
}

/// Find end of a step param value: semicolon at depth 0, or end of string.
pub(crate) fn find_step_param_end(input: &str) -> usize {
    let mut depth = 0i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => return i,
            _ => {}
        }
    }
    input.len()
}

/// Parse `chain <name> : <link> | <link> | ... ;`
pub(crate) fn parse_flow_chain<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("chain ")?.trim_start();

    // Parse name
    let name_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let name = &rest[..name_end];
    let rest = rest[name_end..].trim_start();

    // Consume ':'
    let rest = rest.strip_prefix(':')?.trim_start();

    // Find terminating ';'
    let semi = find_chain_end(rest)?;
    let chain_body = &rest[..semi];
    let after = &rest[semi + 1..];

    // Split at top-level '|'
    let link_strs = split_chain_links(chain_body);
    let mut links = Vec::new();

    for link_str in link_strs {
        let link_str = link_str.trim();
        if link_str.is_empty() {
            continue;
        }

        match parse_chain_link(link_str) {
            Ok(link) => links.push(link),
            Err(e) => {
                errors.push(ParseError {
                    severity: Severity::Error,
                    message: format!("invalid chain link: {}", e),
                    line: 0,
                    column: 0,
                    fragment: link_str.to_string(),
                    contexts: vec![],
                    property: None,
                    value: None,
                });
            }
        }
    }

    if links.is_empty() {
        return Some(after);
    }

    graph.chains.push(FlowChain {
        name: name.to_string(),
        links,
    });

    Some(after)
}

/// Parse a single chain link: `step-type(key: value, key: value)` or just `step-type`.
pub(crate) fn parse_chain_link(input: &str) -> Result<ChainLink, String> {
    let trimmed = input.trim();

    // Find step type name (everything before '(' or end)
    let paren_pos = trimmed.find('(');
    let type_str = if let Some(pos) = paren_pos {
        trimmed[..pos].trim()
    } else {
        trimmed
    };

    let step_type =
        StepType::parse(type_str).ok_or_else(|| format!("unknown step type: '{}'", type_str))?;

    let mut params = HashMap::new();

    if let Some(paren_pos) = paren_pos {
        let after_paren = &trimmed[paren_pos + 1..];
        let close = find_flow_close_paren(after_paren)
            .ok_or_else(|| "unmatched '(' in chain link".to_string())?;
        let args_str = &after_paren[..close];

        // Parse named params: key: value, key: value
        for param_str in split_chain_params(args_str) {
            let param_str = param_str.trim();
            if param_str.is_empty() {
                continue;
            }

            if let Some(colon) = param_str.find(':') {
                let key = param_str[..colon].trim();
                let val_str = param_str[colon + 1..].trim();

                if key == "sources" {
                    let idents: Vec<String> = val_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if idents.len() == 1 {
                        params.insert(
                            key.to_string(),
                            StepParam::Ident(idents.into_iter().next().unwrap()),
                        );
                    } else {
                        params.insert(key.to_string(), StepParam::IdentList(idents));
                    }
                } else if key == "weights" {
                    if let Ok(list) = val_str
                        .split(',')
                        .map(|s| s.trim().parse::<f32>())
                        .collect::<Result<Vec<f32>, _>>()
                    {
                        if list.len() == 1 {
                            params
                                .insert(key.to_string(), StepParam::Expr(FlowExpr::Float(list[0])));
                        } else {
                            params.insert(key.to_string(), StepParam::FloatList(list));
                        }
                    } else if let Ok(expr) = parse_flow_expr(val_str) {
                        params.insert(key.to_string(), StepParam::Expr(expr));
                    }
                } else if key == "stops" {
                    match parse_color_stop_list(val_str) {
                        Ok(stops) => {
                            params.insert(key.to_string(), StepParam::ColorStops(stops));
                        }
                        Err(e) => return Err(format!("invalid color stops: {}", e)),
                    }
                } else if let Ok(int_val) = val_str.parse::<i32>() {
                    if !val_str.contains('.') {
                        params.insert(key.to_string(), StepParam::Int(int_val));
                    } else if let Ok(expr) = parse_flow_expr(val_str) {
                        params.insert(key.to_string(), StepParam::Expr(expr));
                    }
                } else if let Ok(expr) = parse_flow_expr(val_str) {
                    params.insert(key.to_string(), StepParam::Expr(expr));
                } else {
                    params.insert(key.to_string(), StepParam::Ident(val_str.to_string()));
                }
            }
        }
    }

    Ok(ChainLink { step_type, params })
}

/// Split chain link params at top-level commas, respecting parentheses.
pub(crate) fn split_chain_params(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

/// Parse `use <flow-name>;`
pub(crate) fn parse_flow_use<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("use ")?.trim_start();

    // Find semicolon
    let semi = rest.find(';')?;
    let flow_name = rest[..semi].trim();

    if flow_name.is_empty() {
        errors.push(ParseError {
            severity: Severity::Error,
            message: "empty flow name in 'use' declaration".to_string(),
            line: 0,
            column: 0,
            fragment: String::new(),
            contexts: vec![],
            property: None,
            value: None,
        });
        return Some(&rest[semi + 1..]);
    }

    // Validate it's a valid identifier
    if !flow_name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        errors.push(ParseError {
            severity: Severity::Error,
            message: format!("invalid flow name: '{}'", flow_name),
            line: 0,
            column: 0,
            fragment: flow_name.to_string(),
            contexts: vec![],
            property: None,
            value: None,
        });
        return Some(&rest[semi + 1..]);
    }

    graph.uses.push(FlowUse {
        flow_name: flow_name.to_string(),
    });

    Some(&rest[semi + 1..])
}

/// Parse a color stop list: `#RRGGBB 0.0, #RRGGBB 0.5, #RRGGBB 1.0`
pub(crate) fn parse_color_stop_list(input: &str) -> Result<Vec<(FlowExpr, f32)>, String> {
    let mut stops = Vec::new();
    let mut remaining = input.trim();

    while !remaining.is_empty() {
        let remaining_trimmed = remaining.trim_start();
        if remaining_trimmed.is_empty() {
            break;
        }
        remaining = remaining_trimmed;

        // Parse color (hex literal or expression)
        let (color_expr, rest) = if remaining.starts_with('#') {
            parse_flow_color(remaining)?
        } else {
            // Could be a named reference or expression — parse up to whitespace
            let end = remaining
                .find(|c: char| c.is_whitespace())
                .unwrap_or(remaining.len());
            let expr = parse_flow_expr(&remaining[..end])?;
            (expr, &remaining[end..])
        };

        let rest = rest.trim_start();

        // Parse position (float)
        let pos_end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .unwrap_or(rest.len());
        if pos_end == 0 {
            return Err("expected position value after color".to_string());
        }
        let pos: f32 = rest[..pos_end]
            .parse()
            .map_err(|e| format!("invalid position: {}", e))?;

        stops.push((color_expr, pos));

        let rest = rest[pos_end..].trim_start();
        // Consume optional comma
        remaining = rest.strip_prefix(',').map_or(rest, |s| s.trim_start());
    }

    if stops.is_empty() {
        return Err("empty color stop list".to_string());
    }

    Ok(stops)
}

// ===========================================================================
// Flow expression parser (recursive descent)
// ===========================================================================
