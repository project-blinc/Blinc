//! Splitting helpers shared by the value grammars.
//!
//! CSS values nest function calls inside comma- and space-separated lists,
//! so splitting on a bare delimiter is wrong: `rgb(1, 2, 3)` is one item,
//! not three. These helpers track paren depth while they scan.

/// Find the index of the matching closing parenthesis for the opening paren at `input[0]`.
pub(crate) fn find_matching_paren(input: &str) -> Option<usize> {
    if !input.starts_with('(') {
        return None;
    }
    let mut depth = 0i32;
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

/// Split a CSS list on commas while keeping parenthesised groups
/// (e.g. `rgba(0, 0, 0, 0.5)`) intact.
pub(crate) fn split_commas_respecting_parens(input: &str) -> Vec<String> {
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
                if !current.trim().is_empty() {
                    parts.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

/// Split a CSS value by whitespace while keeping parenthesized groups intact.
/// e.g. "2px 2px 0px rgba(0, 0, 0, 0.5)" → ["2px", "2px", "0px", "rgba(0, 0, 0, 0.5)"]
pub(crate) fn split_whitespace_respecting_parens(input: &str) -> Vec<String> {
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
            c if c.is_whitespace() && paren_depth == 0 => {
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
