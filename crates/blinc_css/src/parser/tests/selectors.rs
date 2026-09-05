//! Selectors: state modifiers, comma-separated lists, and conversions.

use crate::parser::*;

#[test]
fn test_state_modifier_hover() {
    let css = r#"
        #button {
            opacity: 1.0;
        }
        #button:hover {
            opacity: 0.8;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    // Base style
    let base = result.stylesheet.get("button").unwrap();
    assert_eq!(base.opacity, Some(1.0));

    // Hover style
    let hover = result
        .stylesheet
        .get_with_state("button", ElementState::Hover)
        .unwrap();
    assert_eq!(hover.opacity, Some(0.8));
}

#[test]
fn test_state_modifier_active() {
    let css = r#"
        #button:active {
            transform: scale(0.95);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    let active = result
        .stylesheet
        .get_with_state("button", ElementState::Active)
        .unwrap();
    assert!(active.transform.is_some());
}

#[test]
fn test_state_modifier_focus() {
    let css = r#"
        #input:focus {
            border-radius: 4px;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    let focus = result
        .stylesheet
        .get_with_state("input", ElementState::Focus)
        .unwrap();
    assert!(focus.corner_radius.is_some());
}

#[test]
fn test_state_modifier_disabled() {
    let css = r#"
        #button:disabled {
            opacity: 0.5;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    let disabled = result
        .stylesheet
        .get_with_state("button", ElementState::Disabled)
        .unwrap();
    assert_eq!(disabled.opacity, Some(0.5));
}

#[test]
fn test_multiple_state_modifiers() {
    let css = r#"
        #button {
            background: #0000FF;
            opacity: 1.0;
        }
        #button:hover {
            opacity: 0.9;
        }
        #button:active {
            opacity: 0.8;
            transform: scale(0.98);
        }
        #button:focus {
            border-radius: 4px;
        }
        #button:disabled {
            opacity: 0.4;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    // Base style
    assert!(result.stylesheet.contains("button"));
    let base = result.stylesheet.get("button").unwrap();
    assert_eq!(base.opacity, Some(1.0));

    // Check all states exist
    assert!(
        result
            .stylesheet
            .contains_with_state("button", ElementState::Hover)
    );
    assert!(
        result
            .stylesheet
            .contains_with_state("button", ElementState::Active)
    );
    assert!(
        result
            .stylesheet
            .contains_with_state("button", ElementState::Focus)
    );
    assert!(
        result
            .stylesheet
            .contains_with_state("button", ElementState::Disabled)
    );

    // Verify state styles
    let hover = result
        .stylesheet
        .get_with_state("button", ElementState::Hover)
        .unwrap();
    assert_eq!(hover.opacity, Some(0.9));

    let active = result
        .stylesheet
        .get_with_state("button", ElementState::Active)
        .unwrap();
    assert_eq!(active.opacity, Some(0.8));
    assert!(active.transform.is_some());

    let focus = result
        .stylesheet
        .get_with_state("button", ElementState::Focus)
        .unwrap();
    assert!(focus.corner_radius.is_some());

    let disabled = result
        .stylesheet
        .get_with_state("button", ElementState::Disabled)
        .unwrap();
    assert_eq!(disabled.opacity, Some(0.4));
}

#[test]
fn test_get_all_states() {
    let css = r#"
        #card {
            opacity: 1.0;
        }
        #card:hover {
            opacity: 0.9;
        }
        #card:active {
            opacity: 0.8;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    let (base, states) = result.stylesheet.get_all_states("card");

    assert!(base.is_some());
    assert_eq!(base.unwrap().opacity, Some(1.0));

    assert_eq!(states.len(), 2);

    // Check we got hover and active
    let state_types: Vec<_> = states.iter().map(|(s, _)| *s).collect();
    assert!(state_types.contains(&ElementState::Hover));
    assert!(state_types.contains(&ElementState::Active));
}

#[test]
fn test_state_modifier_with_variables() {
    let css = r#"
        :root {
            --hover-opacity: 0.85;
        }
        #button:hover {
            opacity: var(--hover-opacity);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    let hover = result
        .stylesheet
        .get_with_state("button", ElementState::Hover)
        .unwrap();
    assert_eq!(hover.opacity, Some(0.85));
}

#[test]
fn test_unknown_state_modifier_ignored() {
    // Unknown pseudo-class should parse the ID part but not set state
    let css = "#button:unknown { opacity: 0.5; }";
    let result = Stylesheet::parse_with_errors(css);

    // The selector "#button:unknown" where "unknown" is not a valid state
    // should still be stored, but with the state part as None
    // Actually, since we parse :unknown but it's not a known state,
    // the state will be None, so it just becomes "button"
    assert!(result.stylesheet.contains("button"));
    let style = result.stylesheet.get("button").unwrap();
    assert_eq!(style.opacity, Some(0.5));
}

#[test]
fn test_element_state_from_str() {
    assert_eq!(
        ElementState::parse_state("hover"),
        Some(ElementState::Hover)
    );
    assert_eq!(
        ElementState::parse_state("HOVER"),
        Some(ElementState::Hover)
    );
    assert_eq!(
        ElementState::parse_state("active"),
        Some(ElementState::Active)
    );
    assert_eq!(
        ElementState::parse_state("focus"),
        Some(ElementState::Focus)
    );
    assert_eq!(
        ElementState::parse_state("disabled"),
        Some(ElementState::Disabled)
    );
    assert_eq!(ElementState::parse_state("unknown"), None);
}

#[test]
fn test_element_state_display() {
    assert_eq!(format!("{}", ElementState::Hover), "hover");
    assert_eq!(format!("{}", ElementState::Active), "active");
    assert_eq!(format!("{}", ElementState::Focus), "focus");
    assert_eq!(format!("{}", ElementState::Disabled), "disabled");
}

#[test]
fn test_css_selector_key() {
    let selector = CssSelector::new("button");
    assert_eq!(selector.key(), "button");

    let selector_hover = CssSelector::with_state("button", ElementState::Hover);
    assert_eq!(selector_hover.key(), "button:hover");
}

// =========================================================================
// Keyframe Animation Tests
// =========================================================================

#[test]
fn test_comma_separated_id_selectors() {
    let css = "#a, #b { opacity: 0.5; }";
    let result = Stylesheet::parse_with_errors(css);
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);

    let style_a = result.stylesheet.get("a").unwrap();
    assert_eq!(style_a.opacity, Some(0.5));

    let style_b = result.stylesheet.get("b").unwrap();
    assert_eq!(style_b.opacity, Some(0.5));
}

#[test]
fn test_comma_separated_does_not_break_subsequent_rules() {
    let css = r#"
        #a, #b { opacity: 0.5; }
        #c { opacity: 0.8; }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);

    assert_eq!(result.stylesheet.get("a").unwrap().opacity, Some(0.5));
    assert_eq!(result.stylesheet.get("b").unwrap().opacity, Some(0.5));
    assert_eq!(result.stylesheet.get("c").unwrap().opacity, Some(0.8));
}

#[test]
fn test_comma_separated_mixed_selectors() {
    let css = r#"#a, .myclass { border-radius: 10px; }"#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);

    // #a should be a simple rule
    let style_a = result.stylesheet.get("a").unwrap();
    assert!(style_a.corner_radius.is_some());

    // .myclass should be in complex rules
    let complex = result.stylesheet.complex_rules();
    assert!(!complex.is_empty());
    assert!(complex[0].1.corner_radius.is_some());
}

#[test]
fn test_comma_separated_three_selectors() {
    let css = "#x, #y, #z { opacity: 0.3; }";
    let result = Stylesheet::parse_with_errors(css);
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);

    assert_eq!(result.stylesheet.get("x").unwrap().opacity, Some(0.3));
    assert_eq!(result.stylesheet.get("y").unwrap().opacity, Some(0.3));
    assert_eq!(result.stylesheet.get("z").unwrap().opacity, Some(0.3));
}

// =========================================================================
// Conversion Method Tests
// =========================================================================
