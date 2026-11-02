//! A stylesheet resolves a colour through its variables, its own `:root`
//! ones over those it was parsed with.

use std::collections::HashMap;

use blinc_core::Color;
use blinc_css::Stylesheet;

#[test]
fn a_colour_resolves_through_variable_chains() {
    let theme: HashMap<String, String> = [("primary".to_string(), "#ff0000".to_string())].into();
    let sheet = Stylesheet::parse_with_variables(
        ":root { --chart-1: var(--primary); --chart-2: var(--chart-1); --chart-3: #00ff00; }",
        &theme,
    )
    .expect("css");
    assert_eq!(
        sheet.resolve_color("var(--chart-2)"),
        Some(Color::rgb(1.0, 0.0, 0.0))
    );
    assert_eq!(
        sheet.resolve_color("var(--chart-3)"),
        Some(Color::rgb(0.0, 1.0, 0.0))
    );
    assert_eq!(
        sheet.resolve_color("#0000ff"),
        Some(Color::rgb(0.0, 0.0, 1.0))
    );
    assert_eq!(sheet.resolve_color("var(--missing)"), None);
}

/// A `:root` block may set ordinary properties beside its variables, as CSS
/// allows.
#[test]
fn a_root_block_with_properties_keeps_its_variables() {
    let theme: HashMap<String, String> = [("border".to_string(), "#ff0000".to_string())].into();
    let sheet = Stylesheet::parse_with_variables(
        ":root { color: #00ff00; /* the grid */ --grid: var(--border); }",
        &theme,
    )
    .expect("css");
    assert_eq!(sheet.get_variable("grid"), Some("var(--border)"));
    assert_eq!(
        sheet.resolve_color("var(--grid)"),
        Some(Color::rgb(1.0, 0.0, 0.0))
    );
}
