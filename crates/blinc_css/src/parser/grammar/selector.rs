//! Selector grammar.
//!
//! Parses a selector chain into the [`ComplexSelector`] model: compounds
//! separated by descendant, child, and sibling combinators, each compound a
//! run of type, id, class, attribute and pseudo-class parts.

use nom::{
    bytes::complete::take_while1,
    character::complete::char,
    combinator::{cut, opt},
    error::{ParseError as NomParseError, VerboseError, context},
};

use crate::parser::*;

/// Parse an ID selector: #identifier or #identifier:state
pub(crate) fn id_selector(input: &str) -> ParseResult<CssSelector> {
    context("ID selector", |input| {
        let (input, _) = char('#')(input)?;
        let (input, id) = cut(identifier)(input)?;

        // Check for optional state modifier
        let (input, state) = opt(|i| {
            let (i, _) = char(':')(i)?;
            let (i, state_name) = identifier(i)?;
            Ok((i, state_name))
        })(input)?;

        let element_state = state.and_then(ElementState::parse_state);

        Ok((
            input,
            CssSelector {
                id: id.to_string(),
                state: element_state,
            },
        ))
    })(input)
}

/// Parse a complex selector: handles #id, .class, :state, :pseudo, combinators
///
/// Examples:
///   `#card`
///   `#card:hover`
///   `.item:first-child`
///   `#parent:hover > .child`
///   `#list .item:last-child`
///   `#parent:hover > #child:first-child`
pub(crate) fn parse_complex_selector(input: &str) -> ParseResult<ComplexSelector> {
    let mut segments = Vec::new();
    let mut remaining = input;

    loop {
        // Parse a compound selector (one or more simple selectors with no combinator)
        let (rest, compound) = parse_compound_selector(remaining)?;
        remaining = rest;

        // Look ahead for a combinator or the start of `{`
        let trimmed = remaining.trim_start();

        if trimmed.starts_with('{') || trimmed.is_empty() {
            // End of selector — this is the last (target) segment
            segments.push((compound, None));
            break;
        }

        // Check for combinators: > (child), + (adjacent sibling), ~ (general sibling)
        if let Some(after_gt) = trimmed.strip_prefix('>') {
            remaining = after_gt.trim_start();
            segments.push((compound, Some(Combinator::Child)));
        } else if let Some(after_plus) = trimmed.strip_prefix('+') {
            remaining = after_plus.trim_start();
            segments.push((compound, Some(Combinator::AdjacentSibling)));
        } else if let Some(after_tilde) = trimmed.strip_prefix('~') {
            remaining = after_tilde.trim_start();
            segments.push((compound, Some(Combinator::GeneralSibling)));
        } else {
            // Must be a descendant combinator (whitespace between compound selectors)
            // Check that next char is a valid selector start (#, ., :, *, or alpha)
            let next_ch = trimmed.chars().next().unwrap_or('{');
            if next_ch == '#'
                || next_ch == '.'
                || next_ch == ':'
                || next_ch == '*'
                || next_ch.is_alphabetic()
            {
                remaining = trimmed;
                segments.push((compound, Some(Combinator::Descendant)));
            } else {
                // Not a selector continuation — end here
                segments.push((compound, None));
                break;
            }
        }
    }

    if segments.is_empty() {
        return Err(nom::Err::Error(VerboseError::from_error_kind(
            remaining,
            nom::error::ErrorKind::Many1,
        )));
    }

    Ok((remaining, ComplexSelector { segments }))
}

/// Parse a comma-separated list of compound selectors (for :is() / :where()).
pub(crate) fn parse_selector_list(input: &str) -> Vec<CompoundSelector> {
    let mut selectors = Vec::new();
    for part in input.split(',') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            if let Ok((_, compound)) = parse_compound_selector(trimmed) {
                selectors.push(compound);
            }
        }
    }
    selectors
}

/// Like `parse_selector_list` but the inner items can contain
/// combinators (`>`, `+`, `~`, descendant whitespace), so each
/// element parses as a `ComplexSelector` instead of a single
/// `CompoundSelector`. Used for the inside of `:has(...)`.
///
/// Splits on TOP-LEVEL commas only — commas inside nested
/// parens (e.g. `:has(.a, :is(.b, .c))`) are preserved. Empty
/// items are skipped.
pub(crate) fn parse_relative_selector_list(input: &str) -> Vec<ComplexSelector> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth: i32 = 0;
    for ch in input.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    parts.push(current);

    let mut selectors = Vec::new();
    for part in parts {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            if let Ok((_, complex)) = parse_complex_selector(trimmed) {
                selectors.push(complex);
            }
        }
    }
    selectors
}

/// Parse a compound selector: one or more simple selector parts with no combinator.
/// e.g. `#id.class:hover:first-child`
pub(crate) fn parse_compound_selector(input: &str) -> ParseResult<CompoundSelector> {
    let mut parts = Vec::new();
    let mut remaining = input;

    loop {
        // Type selector: bare identifier like button, a, ul (must be first part)
        if parts.is_empty()
            && !remaining.is_empty()
            && remaining
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && !remaining.starts_with(':')
        {
            let end = remaining
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                .unwrap_or(remaining.len());
            let type_name = &remaining[..end];
            parts.push(SelectorPart::Type(type_name.to_lowercase()));
            remaining = &remaining[end..];
            continue;
        }

        if remaining.starts_with('#') {
            // ID selector
            let (rest, _) = char('#')(remaining)?;
            let (rest, id) = identifier::<VerboseError<&str>>(rest)?;
            parts.push(SelectorPart::Id(id.to_string()));
            remaining = rest;
        } else if remaining.starts_with('.') {
            // Class selector
            let (rest, _) = char('.')(remaining)?;
            let (rest, class) = identifier::<VerboseError<&str>>(rest)?;
            parts.push(SelectorPart::Class(class.to_string()));
            remaining = rest;
        } else if remaining.starts_with('*') {
            // Universal selector
            remaining = &remaining[1..];
            parts.push(SelectorPart::Universal);
        } else if remaining.starts_with("::") {
            // Pseudo-element (::placeholder, ::selection, etc.)
            let rest = &remaining[2..];
            let (rest, name) = identifier::<VerboseError<&str>>(rest)?;
            parts.push(SelectorPart::PseudoElement(name.to_string()));
            remaining = rest;
        } else if remaining.starts_with(':') {
            // Pseudo-class (state or structural)
            let (rest, _) = char(':')(remaining)?;
            let (rest, name) = identifier::<VerboseError<&str>>(rest)?;

            // Check if it's a structural pseudo-class
            match name.to_lowercase().as_str() {
                "first-child" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::FirstChild));
                    remaining = rest;
                }
                "last-child" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::LastChild));
                    remaining = rest;
                }
                "only-child" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::OnlyChild));
                    remaining = rest;
                }
                "empty" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::Empty));
                    remaining = rest;
                }
                "root" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::Root));
                    remaining = rest;
                }
                "nth-child" => {
                    // Parse nth-child(N)
                    if rest.starts_with('(') {
                        let (rest2, _) = char('(')(rest)?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, n_str) =
                            take_while1::<_, _, VerboseError<&str>>(|c: char| c.is_ascii_digit())(
                                rest2,
                            )?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, _) = char(')')(rest2)?;
                        if let Ok(n) = n_str.parse::<usize>() {
                            parts.push(SelectorPart::PseudoClass(StructuralPseudo::NthChild(n)));
                        }
                        remaining = rest2;
                    } else {
                        remaining = rest;
                    }
                }
                "nth-last-child" => {
                    // Parse nth-last-child(N)
                    if rest.starts_with('(') {
                        let (rest2, _) = char('(')(rest)?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, n_str) =
                            take_while1::<_, _, VerboseError<&str>>(|c: char| c.is_ascii_digit())(
                                rest2,
                            )?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, _) = char(')')(rest2)?;
                        if let Ok(n) = n_str.parse::<usize>() {
                            parts
                                .push(SelectorPart::PseudoClass(StructuralPseudo::NthLastChild(n)));
                        }
                        remaining = rest2;
                    } else {
                        remaining = rest;
                    }
                }
                "not" => {
                    // Parse :not(selector)
                    if rest.starts_with('(') {
                        let (rest2, _) = char('(')(rest)?;
                        let rest2 = rest2.trim_start();
                        // Parse the inner compound selector
                        if let Ok((rest3, inner)) = parse_compound_selector(rest2) {
                            let rest3 = rest3.trim_start();
                            if let Ok((rest4, _)) = char::<&str, VerboseError<&str>>(')')(rest3) {
                                parts.push(SelectorPart::Not(Box::new(inner)));
                                remaining = rest4;
                            } else {
                                remaining = rest;
                            }
                        } else {
                            remaining = rest;
                        }
                    } else {
                        remaining = rest;
                    }
                }
                "is" | "where" => {
                    // Parse :is(selector, ...) or :where(selector, ...)
                    if rest.starts_with('(') {
                        if let Some(close) = find_matching_paren(rest) {
                            let inner = &rest[1..close];
                            let selectors = parse_selector_list(inner);
                            if !selectors.is_empty() {
                                parts.push(SelectorPart::Is(selectors));
                            }
                            remaining = &rest[close + 1..];
                        } else {
                            remaining = rest;
                        }
                    } else {
                        remaining = rest;
                    }
                }
                "has" => {
                    // Parse :has(relative_selector, ...) — relational
                    // pseudo. Unlike :is/:where the inner items can carry
                    // combinators (`> .child`, `+ .sibling`, `~ .later`,
                    // or descendant whitespace), so the inner is a
                    // `ComplexSelector` list, not a `CompoundSelector`
                    // list. Empty parens or unparseable inner → silently
                    // drop the :has (rule still applies to the bare
                    // compound part before it, same as :is's fallback).
                    if rest.starts_with('(') {
                        if let Some(close) = find_matching_paren(rest) {
                            let inner = &rest[1..close];
                            let selectors = parse_relative_selector_list(inner);
                            if !selectors.is_empty() {
                                parts.push(SelectorPart::Has(selectors));
                            }
                            remaining = &rest[close + 1..];
                        } else {
                            remaining = rest;
                        }
                    } else {
                        remaining = rest;
                    }
                }
                "first-of-type" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::FirstOfType));
                    remaining = rest;
                }
                "last-of-type" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::LastOfType));
                    remaining = rest;
                }
                "only-of-type" => {
                    parts.push(SelectorPart::PseudoClass(StructuralPseudo::OnlyOfType));
                    remaining = rest;
                }
                "nth-of-type" => {
                    if rest.starts_with('(') {
                        let (rest2, _) = char('(')(rest)?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, n_str) =
                            take_while1::<_, _, VerboseError<&str>>(|c: char| c.is_ascii_digit())(
                                rest2,
                            )?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, _) = char(')')(rest2)?;
                        if let Ok(n) = n_str.parse::<usize>() {
                            parts.push(SelectorPart::PseudoClass(StructuralPseudo::NthOfType(n)));
                        }
                        remaining = rest2;
                    } else {
                        remaining = rest;
                    }
                }
                "nth-last-of-type" => {
                    if rest.starts_with('(') {
                        let (rest2, _) = char('(')(rest)?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, n_str) =
                            take_while1::<_, _, VerboseError<&str>>(|c: char| c.is_ascii_digit())(
                                rest2,
                            )?;
                        let (rest2, _) = ws::<VerboseError<&str>>(rest2)?;
                        let (rest2, _) = char(')')(rest2)?;
                        if let Ok(n) = n_str.parse::<usize>() {
                            parts.push(SelectorPart::PseudoClass(StructuralPseudo::NthLastOfType(
                                n,
                            )));
                        }
                        remaining = rest2;
                    } else {
                        remaining = rest;
                    }
                }
                _ => {
                    // Try as element state
                    if let Some(state) = ElementState::parse_state(name) {
                        parts.push(SelectorPart::State(state));
                    }
                    // Skip any functional argument list `(...)` so an
                    // unknown pseudo like `:unknown(.x)` doesn't leak its
                    // open-paren into the rest of the selector, which
                    // would later poison `rule_block` and (via the
                    // stylesheet loop's break-on-error recovery) drop
                    // every rule after the offending one. Same hardening
                    // approach as :is/:has — consume the balanced parens
                    // even though we have no semantic for them.
                    if rest.starts_with('(') {
                        if let Some(close) = find_matching_paren(rest) {
                            remaining = &rest[close + 1..];
                        } else {
                            remaining = rest;
                        }
                    } else {
                        remaining = rest;
                    }
                }
            }
        } else {
            break;
        }
    }

    if parts.is_empty() {
        return Err(nom::Err::Error(VerboseError::from_error_kind(
            input,
            nom::error::ErrorKind::Many1,
        )));
    }

    Ok((remaining, CompoundSelector { parts }))
}
