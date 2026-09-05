//! `:root` custom properties and `var()` resolution.

use blinc_theme::ThemeState;

use crate::parser::*;

#[test]
fn test_css_variables_root_parsing() {
    let css = r#"
        :root {
            --primary-color: #FF0000;
            --secondary-color: #00FF00;
            --card-radius: 8px;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    assert_eq!(result.stylesheet.variable_count(), 3);
    assert_eq!(
        result.stylesheet.get_variable("primary-color"),
        Some("#FF0000")
    );
    assert_eq!(
        result.stylesheet.get_variable("secondary-color"),
        Some("#00FF00")
    );
    assert_eq!(result.stylesheet.get_variable("card-radius"), Some("8px"));
}

#[test]
fn test_css_variables_usage() {
    let css = r#"
        :root {
            --main-opacity: 0.8;
        }
        #card {
            opacity: var(--main-opacity);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(result.stylesheet.contains("card"));
    let style = result.stylesheet.get("card").unwrap();
    assert_eq!(style.opacity, Some(0.8));
}

#[test]
fn test_css_variables_with_fallback() {
    let css = r#"
        #card {
            opacity: var(--undefined-var, 0.5);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    let style = result.stylesheet.get("card").unwrap();
    assert_eq!(style.opacity, Some(0.5));
}

#[test]
fn test_css_variables_color() {
    let css = r#"
        :root {
            --brand-color: #3498db;
        }
        #button {
            background: var(--brand-color);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    let style = result.stylesheet.get("button").unwrap();
    assert!(style.background.is_some());
}

#[test]
fn test_css_variables_multiple_rules() {
    let css = r#"
        :root {
            --base-radius: 4px;
            --card-opacity: 0.9;
        }
        #card {
            border-radius: var(--base-radius);
            opacity: var(--card-opacity);
        }
        #button {
            opacity: var(--card-opacity);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(result.stylesheet.contains("card"));
    assert!(result.stylesheet.contains("button"));

    let card = result.stylesheet.get("card").unwrap();
    let button = result.stylesheet.get("button").unwrap();

    assert!(card.corner_radius.is_some());
    assert_eq!(card.opacity, Some(0.9));
    assert_eq!(button.opacity, Some(0.9));
}

#[test]
fn test_css_variables_set_at_runtime() {
    let css = "#card { opacity: 0.5; }";
    let mut stylesheet = Stylesheet::parse(css).unwrap();

    // Set variable at runtime
    stylesheet.set_variable("custom-var", "#FF0000");

    assert_eq!(stylesheet.get_variable("custom-var"), Some("#FF0000"));
}

#[test]
fn test_css_variables_names_iterator() {
    let css = r#"
        :root {
            --a: 1;
            --b: 2;
            --c: 3;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    let names: Vec<_> = result.stylesheet.variable_names().collect();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
    assert!(names.contains(&"c"));
}

#[test]
fn test_css_variables_with_theme_fallback() {
    // Initialize theme (required for theme() functions)
    ThemeState::init_default();

    let css = r#"
        :root {
            --card-shadow: theme(shadow-md);
        }
        #card {
            box-shadow: var(--card-shadow);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    // The variable stores the raw value "theme(shadow-md)"
    // which gets resolved when applied to the style
    let style = result.stylesheet.get("card").unwrap();
    assert!(!style.shadow.is_empty());
}

#[test]
fn test_css_variables_nested_resolution() {
    let css = r#"
        :root {
            --base: 0.5;
            --derived: var(--base);
        }
        #test {
            opacity: var(--derived);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    let style = result.stylesheet.get("test").unwrap();
    assert_eq!(style.opacity, Some(0.5));
}

// ========================================================================
// State Modifier Tests (Pseudo-classes)
// ========================================================================
