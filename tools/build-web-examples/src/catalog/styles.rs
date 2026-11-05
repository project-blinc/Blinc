//! Component stylesheet excerpts for the public code examples.
use super::{Catalog, Component};
use std::collections::{BTreeMap, BTreeSet};

fn widget_families(component: &Component) -> &'static [&'static str] {
    // CN wrappers retain these layout widget classes and their base look.
    match component.module.as_str() {
        "cn::button" => &["blinc-button"],
        "cn::checkbox" => &["blinc-checkbox"],
        "cn::input" => &["blinc-input"],
        "cn::toggle" => &["blinc-toggle"],
        "cn::toggle_group" => &["blinc-toggle", "blinc-toggle-group"],
        "cn::radio" => &["blinc-radio"],
        _ => &[],
    }
}

fn primary_families(component: &Component) -> BTreeSet<String> {
    let mut names: BTreeSet<_> = widget_families(component)
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    if let Some(module) = component.module.strip_prefix("cn::") {
        names.insert(format!("cn-{}", module.replace('_', "-")));
    }
    if let Some(name) = component
        .classes
        .iter()
        .filter_map(|name| name.strip_prefix('.'))
        .find(|name| name.starts_with("cn-") || name.starts_with("blinc-"))
    {
        names.insert(name.into());
    }
    names
}

fn families(component: &Component) -> BTreeSet<String> {
    let mut names = primary_families(component);
    names.extend(
        component
            .classes
            .iter()
            .filter_map(|name| name.strip_prefix('.'))
            .filter(|name| name.starts_with("cn-") || name.starts_with("blinc-"))
            .map(str::to_owned),
    );
    // Shared controls are part of these widgets, rather than gallery chrome.
    for (widget, controls) in [
        ("blinc-media-controls", "blinc-media"),
        ("blinc-code", "blinc-find"),
        ("blinc-rich-text", "blinc-rte"),
    ] {
        if names.contains(widget) {
            names.insert(controls.into());
        }
    }
    names
}

fn belongs(class: &str, family: &str) -> bool {
    class == family
        || class
            .strip_prefix(family)
            .is_some_and(|suffix| suffix.starts_with('-') || suffix.starts_with("__"))
}

fn first_class(selector: &str) -> Option<&str> {
    let mut quote = None;
    let mut escaped = false;
    let mut brackets = 0;
    for (index, ch) in selector.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '[' => brackets += 1,
            ']' => brackets -= 1,
            '.' if brackets == 0 => {
                let name = &selector[index + 1..];
                let end = name
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                    .unwrap_or(name.len());
                return Some(&name[..end]);
            }
            _ => {}
        }
    }
    None
}

fn strip_comments(css: &str) -> String {
    let mut output = String::new();
    let mut chars = css.chars().peekable();
    let mut quote = None;
    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            output.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    output.push(next);
                }
            } else if ch == q {
                quote = None;
            }
        } else if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            output.push(' ');
        } else {
            if ch == '\'' || ch == '"' {
                quote = Some(ch);
            }
            output.push(ch);
        }
    }
    output
}

#[cfg(test)]
#[path = "styles/tests.rs"]
mod tests;

struct Rule<'a> {
    header: &'a str,
    body: &'a str,
}

fn blocks(css: &str) -> Vec<Rule<'_>> {
    let mut result = Vec::new();
    let (mut start, mut open, mut depth) = (0, 0, 0);
    let (mut quote, mut escaped) = (None, false);
    for (index, ch) in css.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '{' => {
                if depth == 0 {
                    open = index;
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    result.push(Rule {
                        header: css[start..open].trim(),
                        body: &css[open + 1..index],
                    });
                    start = index + 1;
                }
            }
            _ => {}
        }
    }
    result
}

fn rules(css: &str) -> Vec<Rule<'_>> {
    blocks(css)
        .into_iter()
        .flat_map(|rule| {
            if rule.header.starts_with("@layer ") {
                rules(rule.body)
            } else {
                vec![rule]
            }
        })
        .collect()
}

fn split(css: &str, delimiter: char) -> Vec<&str> {
    let mut result = Vec::new();
    let (mut start, mut depth) = (0, 0);
    let (mut quote, mut escaped) = (None, false);
    for (index, ch) in css.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ch if ch == delimiter && depth == 0 => {
                result.push(css[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    result.push(css[start..].trim());
    result
}

fn format_rule(header: &str, body: &str, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);
    output.push_str(&indent);
    output.push_str(&header.replace('\n', &format!("\n{indent}")));
    output.push_str(" {\n");
    let nested = blocks(body);
    if nested.is_empty() {
        for declaration in split(body, ';').into_iter().filter(|d| !d.is_empty()) {
            output.push_str(&indent);
            output.push_str("  ");
            let mut lines = declaration.lines().map(str::trim);
            output.push_str(lines.next().unwrap_or_default());
            for line in lines {
                output.push('\n');
                output.push_str(&indent);
                output.push_str("    ");
                output.push_str(line);
            }
            output.push_str(";\n");
        }
    } else {
        for rule in nested {
            format_rule(rule.header, rule.body, depth + 1, output);
        }
    }
    output.push_str(&indent);
    output.push_str("}\n");
}

pub(super) fn component_css(css: &str, catalog: &Catalog, component: &Component) -> String {
    let owned = families(component);
    let known: BTreeSet<_> = catalog
        .components
        .iter()
        .flat_map(primary_families)
        .collect();
    let matches = |selector: &str| {
        let Some(class) = first_class(selector) else {
            return false;
        };
        let Some(family) = owned
            .iter()
            .filter(|family| belongs(class, family))
            .max_by_key(|family| family.len())
        else {
            return false;
        };
        // A longer registered name is another component: input-otp is not input.
        !known.iter().any(|other| {
            other.len() > family.len() && belongs(class, other) && !owned.contains(other)
        })
    };
    let css = strip_comments(css);
    let rules = rules(&css);
    let mut selected = BTreeMap::new();
    for (index, rule) in rules.iter().enumerate() {
        if rule.header == ":root" {
            let declarations: Vec<_> = split(rule.body, ';')
                .into_iter()
                .filter(|declaration| {
                    declaration.split_once(':').is_some_and(|(name, _)| {
                        owned
                            .iter()
                            .any(|family| name.trim().starts_with(&format!("--{family}-")))
                    })
                })
                .collect();
            if !declarations.is_empty() {
                selected.insert(index, (":root".into(), declarations.join(";")));
            }
        } else if !rule.header.starts_with('@') {
            let selectors: Vec<_> = split(rule.header, ',')
                .into_iter()
                .filter(|selector| matches(selector))
                .collect();
            if !selectors.is_empty() {
                selected.insert(index, (selectors.join(",\n"), rule.body.to_string()));
            }
        }
    }
    let bodies = selected
        .values()
        .map(|(_, body)| body.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for (index, rule) in rules.iter().enumerate() {
        if let Some(name) = rule.header.strip_prefix("@keyframes ") {
            if bodies
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                .any(|token| token == name.trim())
            {
                selected.insert(index, (rule.header.into(), rule.body.into()));
            }
        }
    }
    if selected.is_empty() {
        return String::new();
    }
    let mut output = "@layer components {\n".to_string();
    for (_, (header, body)) in selected {
        format_rule(&header, &body, 1, &mut output);
        output.push('\n');
    }
    output.pop();
    output.push_str("}\n");
    output
}
