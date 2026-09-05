//! The CSS grammar: comments, declarations, rules, and the sheet driver.
//!
//! `parse_stylesheet_with_errors` is the entry point. It walks top-level
//! blocks, dispatches `@keyframes` and `@flow` to their own parsers, and
//! turns everything else into rules. A rule that fails to parse is skipped
//! to its closing brace and recorded as an error, so the rest of the sheet
//! still loads.
//!
//! Custom properties are resolved here too: `:root` variables are collected
//! first, then `var()` references are substituted before values are parsed.

use std::collections::HashMap;

use blinc_core::FlowGraph;
use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{char, multispace1},
    combinator::{opt, value},
    error::{ParseError as NomParseError, VerboseError, context},
    multi::many0,
    sequence::{delimited, preceded},
};

use crate::element_style::ElementStyle;
use crate::parser::*;

mod keyframes;
mod selector;

pub(crate) use keyframes::*;
pub(crate) use selector::*;

/// Parse whitespace and comments
pub(crate) fn ws<'a, E: NomParseError<&'a str>>(input: &'a str) -> IResult<&'a str, (), E> {
    value(
        (),
        many0(alt((
            value((), multispace1),
            value((), parse_comment),
            value((), parse_line_comment),
        ))),
    )(input)
}

/// Parse a block comment /* ... */
pub(crate) fn parse_comment<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, &'a str, E> {
    delimited(tag("/*"), take_until("*/"), tag("*/"))(input)
}

/// Parse a `//` line comment.
///
/// Not CSS, but `.blinc` `style { }` blocks are written in a file whose
/// every other comment is `//`, and authors write them here too.
/// Without this the `//` line parses as the start of a selector and
/// swallows the rule after it — silently, so the rule simply never
/// applies and nothing says why.
pub(crate) fn parse_line_comment<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, &'a str, E> {
    preceded(tag("//"), nom::bytes::complete::take_till(|c| c == '\n'))(input)
}

/// Parse an identifier (alphanumeric, hyphen, underscore)
pub(crate) fn identifier<'a, E: NomParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, &'a str, E> {
    take_while1(|c: char| c.is_alphanumeric() || c == '-' || c == '_')(input)
}

/// Parse a property name (including CSS custom properties like --var-name)
pub(crate) fn property_name(input: &str) -> ParseResult<&str> {
    context(
        "property name",
        take_while1(|c: char| c.is_alphanumeric() || c == '-' || c == '_'),
    )(input)
}

/// Parse a property value (everything until ; or })
pub(crate) fn property_value(input: &str) -> ParseResult<&str> {
    let (input, value) = context(
        "property value",
        take_while1(|c: char| c != ';' && c != '}'),
    )(input)?;
    Ok((input, value.trim()))
}

/// Parse a single property declaration: name: value;
pub(crate) fn property_declaration(input: &str) -> ParseResult<(&str, &str)> {
    let (input, _) = ws(input)?;
    let (input, name) = context("property name", property_name)(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = context("colon after property name", char(':'))(input)?;
    let (input, _) = ws(input)?;
    let (input, value) = context("property value", property_value)(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = opt(char(';'))(input)?;
    Ok((input, (name, value)))
}

/// Parse a rule block: { property: value; ... }
pub(crate) fn rule_block(input: &str) -> ParseResult<Vec<(&str, &str)>> {
    let (input, _) = ws::<VerboseError<&str>>(input)?;
    let (input, _) = context("opening brace", char('{'))(input)?;
    let (input, _) = ws::<VerboseError<&str>>(input)?;
    let (input, properties) = many0(property_declaration)(input)?;
    let (input, _) = ws::<VerboseError<&str>>(input)?;
    let (input, _) = context("closing brace", char('}'))(input)?;
    Ok((input, properties))
}

/// Parse a :root block for CSS variables
pub(crate) fn root_block(input: &str) -> ParseResult<Vec<(String, String)>> {
    let (input, _) = ws(input)?;
    let (input, _) = tag(":root")(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;

    // Parse variable declarations
    let (input, declarations) = many0(|i| {
        let (i, _) = ws(i)?;
        let (i, _) = tag("--")(i)?;
        let (i, name) = identifier(i)?;
        let (i, _) = ws(i)?;
        let (i, _) = char(':')(i)?;
        let (i, _) = ws(i)?;
        let (i, value) = property_value(i)?;
        let (i, _) = ws(i)?;
        let (i, _) = opt(char(';'))(i)?;
        Ok((i, (name.to_string(), value.to_string())))
    })(input)?;

    let (input, _) = ws(input)?;
    let (input, _) = char('}')(input)?;
    Ok((input, declarations))
}

/// Parse a complete rule: #id { ... } or #id:state { ... }
pub(crate) fn css_rule(input: &str) -> ParseResult<(String, ElementStyle)> {
    let (input, _) = ws(input)?;
    let (input, selector) = context("CSS rule selector", id_selector)(input)?;
    let (input, _) = ws(input)?;
    let (input, properties) = context("CSS rule block", rule_block)(input)?;

    let mut style = ElementStyle::new();
    for (name, value) in properties {
        apply_property(&mut style, name, value);
    }

    // Use the selector key (id or id:state)
    Ok((input, (selector.key(), style)))
}

/// Parse an entire stylesheet
#[allow(dead_code)]
pub(crate) fn parse_stylesheet(input: &str) -> ParseResult<Vec<(String, ElementStyle)>> {
    let (input, _) = ws(input)?;
    let (input, rules) = many0(css_rule)(input)?;
    let (input, _) = ws(input)?;
    Ok((input, rules))
}

pub(crate) struct ParsedStylesheet {
    pub(crate) rules: Vec<(String, ElementStyle)>,
    pub(crate) complex_rules: Vec<(ComplexSelector, ElementStyle)>,
    pub(crate) variables: HashMap<String, String>,
    pub(crate) keyframes: Vec<CssKeyframes>,
    pub(crate) flows: Vec<FlowGraph>,
}

/// Parse an entire stylesheet with error collection
pub(crate) fn parse_stylesheet_with_errors<'a>(
    css: &'a str,
    errors: &mut Vec<ParseError>,
    variables: &HashMap<String, String>,
) -> ParseResult<'a, ParsedStylesheet> {
    let (input, _) = ws(css)?;

    // Parse blocks one at a time to collect errors
    let mut rules = Vec::new();
    let mut complex_rules = Vec::new();
    let mut parsed_variables = variables.clone();
    let mut parsed_keyframes = Vec::new();
    let mut parsed_flows = Vec::new();
    let mut flow_registry: HashMap<String, FlowGraph> = HashMap::new();
    let mut remaining = input;

    loop {
        let trimmed = remaining.trim_start();
        if trimmed.is_empty() {
            break;
        }

        // Skip CSS comments at the top level
        if trimmed.starts_with("/*") {
            if let Some(end) = trimmed.find("*/") {
                remaining = &trimmed[end + 2..];
                continue;
            } else {
                break; // Unterminated comment
            }
        }

        // Try to parse a :root block first
        if trimmed.starts_with(":root") {
            match root_block(trimmed) {
                Ok((rest, vars)) => {
                    for (name, value) in vars {
                        parsed_variables.insert(name, value);
                    }
                    remaining = rest;
                    continue;
                }
                Err(_) => {
                    // Not a valid :root block, try as a rule
                }
            }
        }

        // Try to parse @keyframes block
        if trimmed.starts_with("@keyframes") {
            match keyframes_block(trimmed, errors, &parsed_variables) {
                Ok((rest, keyframes)) => {
                    parsed_keyframes.push(keyframes);
                    remaining = rest;
                    continue;
                }
                Err(_) => {
                    // Not a valid @keyframes block, try as a rule
                }
            }
        }

        // Try to parse @flow block (pass registry of already-parsed flows for `use`)
        if trimmed.starts_with("@flow") {
            let registry = if flow_registry.is_empty() {
                None
            } else {
                Some(&flow_registry)
            };
            match flow_block(trimmed, errors, registry) {
                Ok((rest, flow)) => {
                    flow_registry.insert(flow.name.clone(), flow.clone());
                    parsed_flows.push(flow);
                    remaining = rest;
                    continue;
                }
                Err(_) => {
                    // Not a valid @flow block, try as a rule
                }
            }
        }

        // Try to parse a rule (complex selector or simple #id selector)
        // Supports comma-separated selector lists: #a, #b { ... }
        match css_rule_complex_or_simple(css, errors, &parsed_variables)(trimmed) {
            Ok((rest, parsed_rules)) => {
                for rule in parsed_rules {
                    match rule {
                        ParsedRule::Simple(key, style) => rules.push((key, style)),
                        ParsedRule::Complex(selector, style) => {
                            complex_rules.push((selector, style))
                        }
                    }
                }
                remaining = rest;
            }
            Err(nom::Err::Error(_)) | Err(nom::Err::Failure(_)) => {
                // Rule failed to parse — skip past its `{ ... }` block
                // (or to the next `;` for at-rule-style declarations) and
                // continue with the next rule instead of breaking. Before
                // this recovery a single bad selector (e.g. an unknown
                // pseudo whose parser leaked `(...)` into rule_block's
                // expected `{`) silently dropped EVERY subsequent rule
                // in the stylesheet.
                if let Some(skip) = skip_failed_rule(trimmed) {
                    // `trimmed` may sit at a different offset than
                    // `remaining` (we trimmed leading whitespace earlier
                    // in the loop). Recompute the corresponding offset
                    // in `remaining` by finding the trimmed slice's
                    // start position via pointer arithmetic.
                    let trim_offset = trimmed.as_ptr() as usize - remaining.as_ptr() as usize;
                    let skip_offset = trim_offset + skip;
                    if skip_offset < remaining.len() {
                        remaining = &remaining[skip_offset..];
                        continue;
                    }
                    break;
                }
                // No recovery point found — bail.
                break;
            }
            Err(nom::Err::Incomplete(_)) => {
                break;
            }
        }
    }

    let (input, _) = ws(remaining)?;
    Ok((
        input,
        ParsedStylesheet {
            rules,
            complex_rules,
            variables: parsed_variables,
            keyframes: parsed_keyframes,
            flows: parsed_flows,
        },
    ))
}

/// Recovery helper for the stylesheet loop: when a rule fails to
/// parse, find the byte offset where the NEXT rule probably starts
/// so we can skip the bad block and keep parsing instead of breaking
/// the entire stylesheet at the first failure.
///
/// Strategy:
///   1. Scan for the matching `}` of the current rule's body, tracking
///      paren nesting so `}` inside `:has(...)` or attribute selectors
///      doesn't get mistaken for the rule terminator. Return one past
///      the closing brace.
///   2. If no `{` appears at all (the failure was inside the selector
///      with no following block), advance past the next `;` or whole
///      input to skip the malformed fragment.
pub(crate) fn skip_failed_rule(input: &str) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut paren_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut saw_brace = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth = (paren_depth - 1).max(0),
            b'{' if paren_depth == 0 => {
                brace_depth += 1;
                saw_brace = true;
            }
            b'}' if paren_depth == 0 => {
                brace_depth -= 1;
                if saw_brace && brace_depth <= 0 {
                    return Some(i + 1);
                }
            }
            b';' if paren_depth == 0 && !saw_brace => {
                return Some(i + 1);
            }
            _ => {}
        }
    }
    if input.is_empty() {
        None
    } else {
        Some(input.len())
    }
}

/// Result of parsing a CSS rule — either a simple #id rule or a complex selector rule
pub(crate) enum ParsedRule {
    /// Simple rule: key is "id" or "id:state"
    Simple(String, ElementStyle),
    /// Complex rule with a full selector chain
    Complex(ComplexSelector, ElementStyle),
}

/// Parse a CSS rule with either a complex or simple selector.
/// Supports comma-separated selector lists: `#a, .b, div > span { ... }`
pub(crate) fn css_rule_complex_or_simple<'a, 'b>(
    original_css: &'a str,
    errors: &'b mut Vec<ParseError>,
    variables: &'b HashMap<String, String>,
) -> impl FnMut(&'a str) -> ParseResult<'a, Vec<ParsedRule>> + 'b
where
    'a: 'b,
{
    move |input: &'a str| {
        let (input, _) = ws(input)?;

        // Parse first selector
        let (mut remaining, first_selector) =
            context("CSS rule selector", parse_complex_selector)(input)?;
        let (trimmed, _) = ws(remaining)?;
        remaining = trimmed;

        let mut selectors = vec![first_selector];

        // Parse additional comma-separated selectors
        while remaining.starts_with(',') {
            let after_comma = &remaining[1..];
            let (trimmed, _) = ws(after_comma)?;
            match parse_complex_selector(trimmed) {
                Ok((rest, selector)) => {
                    selectors.push(selector);
                    let (trimmed, _) = ws(rest)?;
                    remaining = trimmed;
                }
                Err(_) => break,
            }
        }

        // Parse the rule block (shared by all selectors)
        let (input, properties) = context("CSS rule block", rule_block)(remaining)?;

        let mut style = ElementStyle::new();
        for (name, value) in properties {
            let resolved_value = resolve_var_references(value, variables);
            apply_property_with_errors(
                &mut style,
                name,
                &resolved_value,
                original_css,
                input,
                errors,
            );
        }

        // Create a rule for each selector in the comma-separated list
        let mut rules = Vec::with_capacity(selectors.len());
        for selector in selectors {
            let rule = if selector.is_simple() {
                let compound = &selector.segments[0].0;
                if let Some(simple_key) = try_as_simple_selector(compound) {
                    ParsedRule::Simple(simple_key, style.clone())
                } else {
                    ParsedRule::Complex(selector, style.clone())
                }
            } else {
                ParsedRule::Complex(selector, style.clone())
            };
            rules.push(rule);
        }

        Ok((input, rules))
    }
}

/// Try to convert a compound selector to a simple "id" or "id:state" key.
/// Returns Some(key) if the compound is just #id or #id:state, None otherwise.
pub(crate) fn try_as_simple_selector(compound: &CompoundSelector) -> Option<String> {
    let mut id = None;
    let mut state = None;
    let mut pseudo_element = None;

    for part in &compound.parts {
        match part {
            SelectorPart::Id(i) => {
                if id.is_some() {
                    return None; // Multiple IDs
                }
                id = Some(i.as_str());
            }
            SelectorPart::State(s) => {
                if state.is_some() {
                    return None; // Multiple states
                }
                state = Some(s);
            }
            SelectorPart::PseudoElement(name) => {
                if pseudo_element.is_some() {
                    return None; // Multiple pseudo-elements
                }
                pseudo_element = Some(name.as_str());
            }
            // If there are type selectors, classes, structural pseudos, universal, :not(), :is(), or :has(), it's not simple
            SelectorPart::Type(_)
            | SelectorPart::Class(_)
            | SelectorPart::PseudoClass(_)
            | SelectorPart::Universal
            | SelectorPart::Not(_)
            | SelectorPart::Is(_)
            | SelectorPart::Has(_) => return None,
        }
    }

    let id = id?; // Must have an ID to be a simple selector

    // Can't have both state and pseudo-element
    if state.is_some() && pseudo_element.is_some() {
        return None;
    }

    if let Some(pe) = pseudo_element {
        Some(format!("{}::{}", id, pe))
    } else {
        match state {
            Some(s) => Some(format!("{}:{}", id, s)),
            None => Some(id.to_string()),
        }
    }
}

/// Resolve var(--name) references in a value string
pub(crate) fn resolve_var_references(value: &str, variables: &HashMap<String, String>) -> String {
    let mut result = value.to_string();
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 10; // Prevent infinite loops from circular references

    // Keep resolving until no more var() references
    while result.contains("var(") && iterations < MAX_ITERATIONS {
        iterations += 1;

        // Find var( and its matching )
        if let Some(start) = result.find("var(") {
            let after_var = &result[start + 4..];

            // Find matching closing paren (handling nested parens)
            let mut depth = 1;
            let mut end_offset = 0;
            for (i, c) in after_var.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end_offset = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if depth == 0 {
                let var_content = &after_var[..end_offset];
                let full_var = &result[start..start + 4 + end_offset + 1];

                // Parse var content: --name or --name, fallback
                let resolved = if let Some(comma_pos) = var_content.find(',') {
                    let var_name = var_content[..comma_pos].trim();
                    let fallback = var_content[comma_pos + 1..].trim();

                    if let Some(name) = var_name.strip_prefix("--") {
                        variables
                            .get(name)
                            .cloned()
                            .unwrap_or_else(|| fallback.to_string())
                    } else {
                        fallback.to_string()
                    }
                } else {
                    let var_name = var_content.trim();
                    if let Some(name) = var_name.strip_prefix("--") {
                        variables.get(name).cloned().unwrap_or_default()
                    } else {
                        String::new()
                    }
                };

                result = result.replace(full_var, &resolved);
            } else {
                // Malformed var(), break to avoid infinite loop
                break;
            }
        }
    }

    result
}

// ============================================================================
// Property Application
// ============================================================================
