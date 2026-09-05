//! `@flow` DAG declarations.
//!
//! A `@flow` block declares a shader graph: a target, optional workgroup
//! size, typed inputs, computation nodes, and outputs. The result is a
//! [`FlowGraph`] the GPU layer compiles.
//!
//! Statement-level parsing lives here; the semantic forms (`step`, chain,
//! `use`) are in [`semantic`] and the expression grammar in [`expr`].

use std::collections::HashMap;

use blinc_core::{
    FlowGraph, FlowInput, FlowInputSource, FlowNode, FlowOutput, FlowOutputTarget, FlowTarget,
    FlowType,
};
use nom::{bytes::complete::tag, character::complete::char};

use crate::parser::*;

mod expr;
mod semantic;

pub(crate) use expr::*;
pub(crate) use semantic::*;

/// Parse a `@flow` block into a validated FlowGraph DAG.
///
/// # Syntax
///
/// ```css
/// @flow ripple-effect {
///   target: fragment;
///   input uv;
///   input time;
///   node dist = distance(uv, vec2(0.5, 0.5));
///   node wave = sin(dist * 20.0 - time * 4.0);
///   output color = vec4(wave, wave, wave, 1.0);
/// }
/// ```
pub(crate) fn flow_block<'a>(
    css: &'a str,
    errors: &mut Vec<ParseError>,
    flow_registry: Option<&HashMap<String, FlowGraph>>,
) -> ParseResult<'a, FlowGraph> {
    let (input, _) = ws(css)?;
    let (input, _) = tag("@flow")(input)?;
    let (input, _) = ws(input)?;
    let (input, name) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;

    let mut graph = FlowGraph::new(name);
    let mut remaining = input;

    loop {
        let trimmed = remaining.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('}') {
            break;
        }

        // Skip comments inside @flow blocks
        if trimmed.starts_with("/*") {
            if let Some(end) = trimmed.find("*/") {
                remaining = &trimmed[end + 2..];
                continue;
            } else {
                break;
            }
        }

        // Try to parse a flow declaration
        if let Some(rest) = parse_flow_declaration(trimmed, &mut graph, errors) {
            remaining = rest;
        } else {
            // Error recovery: skip brace-delimited blocks (step) or to next semicolon
            let brace_pos = trimmed.find('{');
            let semi_pos = trimmed.find(';');
            match (brace_pos, semi_pos) {
                (Some(b), Some(s)) if b < s => {
                    if let Some(close) = find_flow_close_brace(&trimmed[b + 1..]) {
                        remaining = &trimmed[b + 1 + close + 1..];
                    } else {
                        break;
                    }
                }
                (_, Some(s)) => remaining = &trimmed[s + 1..],
                (Some(b), None) => {
                    if let Some(close) = find_flow_close_brace(&trimmed[b + 1..]) {
                        remaining = &trimmed[b + 1 + close + 1..];
                    } else {
                        break;
                    }
                }
                (None, None) => break,
            }
        }
    }

    let (input, _) = ws(remaining)?;
    let (input, _) = char('}')(input)?;

    // Validate the DAG (cycle detection, type inference, semantic expansion)
    if let Err(flow_errors) = graph.validate(flow_registry) {
        for err in flow_errors {
            errors.push(ParseError {
                severity: Severity::Error,
                message: format!("@flow '{}': {}", graph.name, err),
                line: 0,
                column: 0,
                fragment: String::new(),
                contexts: vec![],
                property: None,
                value: None,
            });
        }
    }

    Ok((input, graph))
}

/// Parse a standalone `@flow name { ... }` string into a validated FlowGraph.
///
/// Used by the `flow!` macro to convert stringified Rust tokens into a FlowGraph at runtime.
pub fn parse_flow_string(src: &str) -> Result<FlowGraph, String> {
    // Normalize: stringify!() may insert literal \n between tokens when
    // the macro body spans multiple source lines. Replace them with spaces
    // so the parser sees a single continuous line of declarations.
    let normalized = src.replace('\n', " ");
    let mut errors = Vec::new();
    match flow_block(&normalized, &mut errors, None) {
        Ok((_, graph)) => {
            let fatal: Vec<_> = errors
                .iter()
                .filter(|e| e.severity == Severity::Error)
                .map(|e| e.message.clone())
                .collect();
            if fatal.is_empty() {
                Ok(graph)
            } else {
                Err(fatal.join("; "))
            }
        }
        Err(_) => Err(format!(
            "failed to parse @flow block: {:?}",
            src.chars().take(80).collect::<String>()
        )),
    }
}

/// Parse a single declaration inside a @flow block.
/// Returns the remaining input, or None if parsing failed.
pub(crate) fn parse_flow_declaration<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let trimmed = input.trim_start();

    // target: fragment | compute;
    if trimmed.starts_with("target") {
        return parse_flow_target(trimmed, graph);
    }

    // workgroup: N;
    if trimmed.starts_with("workgroup") {
        return parse_flow_workgroup(trimmed, graph);
    }

    // input <name> [: buffer(name, type)];
    if trimmed.starts_with("input ") {
        return parse_flow_input(trimmed, graph, errors);
    }

    // step <name> : <step-type> { <param>: <value>; ... }
    if trimmed.starts_with("step ") {
        return parse_flow_step(trimmed, graph, errors);
    }

    // chain <name> : <link> | <link> | ... ;
    if trimmed.starts_with("chain ") {
        return parse_flow_chain(trimmed, graph, errors);
    }

    // use <flow-name>;
    if trimmed.starts_with("use ") {
        return parse_flow_use(trimmed, graph, errors);
    }

    // node <name> = <expr>;
    if trimmed.starts_with("node ") {
        return parse_flow_node(trimmed, graph, errors);
    }

    // output <target> [= <expr>];
    if trimmed.starts_with("output ") {
        return parse_flow_output(trimmed, graph, errors);
    }

    None
}

pub(crate) fn parse_flow_target<'a>(input: &'a str, graph: &mut FlowGraph) -> Option<&'a str> {
    let rest = input.strip_prefix("target")?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();

    let semi = rest.find(';')?;
    let value = rest[..semi].trim();
    match value {
        "fragment" => graph.target = FlowTarget::Fragment,
        "compute" => graph.target = FlowTarget::Compute,
        "vertex" => graph.target = FlowTarget::Vertex,
        "material" => graph.target = FlowTarget::Material,
        _ => return None,
    }
    Some(&rest[semi + 1..])
}

pub(crate) fn parse_flow_workgroup<'a>(input: &'a str, graph: &mut FlowGraph) -> Option<&'a str> {
    let rest = input.strip_prefix("workgroup")?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();

    let semi = rest.find(';')?;
    let value = rest[..semi].trim();
    graph.workgroup_size = value.parse::<u32>().ok();
    Some(&rest[semi + 1..])
}

pub(crate) fn parse_flow_input<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    _errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("input")?.trim_start();

    let semi = rest.find(';')?;
    let decl = rest[..semi].trim();

    // Check for typed declaration: input name: buffer(buf-name, type);
    if let Some(colon_pos) = decl.find(':') {
        let name = decl[..colon_pos].trim();
        let type_decl = decl[colon_pos + 1..].trim();

        if type_decl.starts_with("builtin(") {
            // builtin(var-name) — explicit builtin source
            let inner = type_decl.strip_prefix("builtin(")?.strip_suffix(')')?;
            if let Some(builtin) = blinc_core::flow::BuiltinVar::parse(inner.trim()) {
                let ty = builtin.output_type();
                graph.inputs.push(FlowInput {
                    name: name.to_string(),
                    source: FlowInputSource::Builtin(builtin),
                    ty: Some(ty),
                });
            }
        } else if type_decl.starts_with("buffer(") {
            // buffer(name, type)
            let inner = type_decl.strip_prefix("buffer(")?.strip_suffix(')')?;
            let parts: Vec<&str> = inner.splitn(2, ',').collect();
            if parts.len() == 2 {
                let buf_name = parts[0].trim().to_string();
                let ty = match parts[1].trim() {
                    "float" | "f32" => FlowType::Float,
                    "vec2" => FlowType::Vec2,
                    "vec3" => FlowType::Vec3,
                    "vec4" => FlowType::Vec4,
                    _ => FlowType::Vec4,
                };
                graph.inputs.push(FlowInput {
                    name: name.to_string(),
                    source: FlowInputSource::Buffer { name: buf_name, ty },
                    ty: Some(ty),
                });
            }
        } else if type_decl.starts_with("css(") {
            // css(property-name)
            let inner = type_decl.strip_prefix("css(")?.strip_suffix(')')?;
            graph.inputs.push(FlowInput {
                name: name.to_string(),
                source: FlowInputSource::CssProperty(inner.trim().to_string()),
                ty: Some(FlowType::Float),
            });
        } else if type_decl.starts_with("env(") {
            // env(var-name)
            let inner = type_decl.strip_prefix("env(")?.strip_suffix(')')?;
            graph.inputs.push(FlowInput {
                name: name.to_string(),
                source: FlowInputSource::EnvVar(inner.trim().to_string()),
                ty: Some(FlowType::Float),
            });
        }
    } else {
        // Simple declaration: input name;
        let name = decl;
        let source = if let Some(builtin) = blinc_core::flow::BuiltinVar::parse(name) {
            let ty = builtin.output_type();
            graph.inputs.push(FlowInput {
                name: name.to_string(),
                source: FlowInputSource::Builtin(builtin),
                ty: Some(ty),
            });
            return Some(&rest[semi + 1..]);
        } else if name.starts_with("env(") {
            let env_name = name.strip_prefix("env(")?.strip_suffix(')')?;
            FlowInputSource::EnvVar(env_name.to_string())
        } else {
            FlowInputSource::Auto
        };
        graph.inputs.push(FlowInput {
            name: name.to_string(),
            source,
            ty: None,
        });
    }

    Some(&rest[semi + 1..])
}

pub(crate) fn parse_flow_node<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("node")?.trim_start();

    // Find the '=' separating name from expression
    let eq_pos = rest.find('=')?;
    let name = rest[..eq_pos].trim();

    // Find the semicolon that ends this declaration
    // Need to handle nested parens — can't just find first ';'
    let expr_start = &rest[eq_pos + 1..];
    let semi_pos = find_statement_end(expr_start)?;
    let expr_str = expr_start[..semi_pos].trim();

    match parse_flow_expr(expr_str) {
        Ok(expr) => {
            graph.nodes.push(FlowNode {
                name: name.to_string(),
                expr,
                inferred_type: None,
            });
        }
        Err(msg) => {
            errors.push(ParseError {
                severity: Severity::Error,
                message: format!("@flow node '{}': {}", name, msg),
                line: 0,
                column: 0,
                fragment: expr_str.to_string(),
                contexts: vec![],
                property: Some(format!("node {}", name)),
                value: Some(expr_str.to_string()),
            });
        }
    }

    Some(&expr_start[semi_pos + 1..])
}

pub(crate) fn parse_output_target(name: &str) -> FlowOutputTarget {
    match name {
        "color" => FlowOutputTarget::Color,
        "alpha" => FlowOutputTarget::Alpha,
        "displacement" => FlowOutputTarget::Displacement,
        // 3D vertex outputs
        "position" => FlowOutputTarget::Position,
        "world_normal_out" | "world_normal" => FlowOutputTarget::WorldNormalOut,
        "world_position_out" | "world_position" => FlowOutputTarget::WorldPositionOut,
        // 3D material outputs
        "albedo" | "base_color" => FlowOutputTarget::Albedo,
        "metallic" => FlowOutputTarget::Metallic,
        "roughness" => FlowOutputTarget::Roughness,
        "emissive" => FlowOutputTarget::Emissive,
        "surface_normal" => FlowOutputTarget::SurfaceNormal,
        "alpha_out" => FlowOutputTarget::AlphaOut,
        _ => FlowOutputTarget::Color,
    }
}

pub(crate) fn parse_flow_output<'a>(
    input: &'a str,
    graph: &mut FlowGraph,
    errors: &mut Vec<ParseError>,
) -> Option<&'a str> {
    let rest = input.strip_prefix("output")?.trim_start();
    let semi_pos = find_statement_end(rest)?;
    let decl = rest[..semi_pos].trim();

    // Detect output target and optional expression
    let (target, name, expr_str) = if decl.starts_with("buffer(") {
        // output buffer(name) = expr;
        let close_paren = decl.find(')')?;
        let buf_inner = decl[7..close_paren].trim();
        let after = decl[close_paren + 1..].trim();
        let expr_str = after.strip_prefix('=').map(|s| s.trim());
        (
            FlowOutputTarget::Buffer {
                name: buf_inner.to_string(),
            },
            buf_inner.to_string(),
            expr_str,
        )
    } else if let Some(eq_pos) = decl.find('=') {
        // output <name> = <expr>;
        let name = decl[..eq_pos].trim();
        let expr_s = decl[eq_pos + 1..].trim();
        let target = parse_output_target(name);
        (target, name.to_string(), Some(expr_s))
    } else {
        // Bare output: output color;
        let name = decl.trim();
        let target = parse_output_target(name);
        (target, name.to_string(), None)
    };

    let parsed_expr = if let Some(es) = expr_str {
        match parse_flow_expr(es) {
            Ok(expr) => Some(expr),
            Err(msg) => {
                errors.push(ParseError {
                    severity: Severity::Error,
                    message: format!("@flow output '{}': {}", name, msg),
                    line: 0,
                    column: 0,
                    fragment: es.to_string(),
                    contexts: vec![],
                    property: Some(format!("output {}", name)),
                    value: Some(es.to_string()),
                });
                None
            }
        }
    } else {
        None
    };

    graph.outputs.push(FlowOutput {
        name,
        target,
        expr: parsed_expr,
    });

    Some(&rest[semi_pos + 1..])
}

/// Find the end of a flow statement (semicolon), respecting parentheses nesting.
pub(crate) fn find_statement_end(input: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => return Some(i),
            '}' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

// ===========================================================================
// Flow semantic layer parsers (step, chain, use)
// ===========================================================================
