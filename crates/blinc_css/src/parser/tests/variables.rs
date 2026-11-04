//! `:root` custom properties and `var()` resolution.

use blinc_theme::ThemeState;

use crate::parser::*;

#[test]
fn root_variables_apply_to_earlier_component_rules() {
    let css = "@layer components { :root { --amount: 0.2; } .panel { opacity: var(--amount); } }\n:root { --amount: 0.8; }";
    let sheet = Stylesheet::parse(css).unwrap();
    assert_eq!(sheet.get_class("panel").unwrap().opacity, Some(0.8));
    assert_eq!(sheet.get_variable("amount"), Some("0.8"));
}

#[test]
fn root_aliases_use_the_final_theme_override() {
    let css = ":root { --muted: var(--amount); color: #ffffff; }\n#panel { opacity: var(--muted); }\n:root { --amount: 0.7; }";
    let sheet = Stylesheet::parse(css).unwrap();
    assert_eq!(sheet.get("panel").unwrap().opacity, Some(0.7));
}

#[test]
fn root_collection_keeps_line_comments_out_of_the_grammar() {
    let sheet = Stylesheet::parse("// } an editor note\n#panel { opacity: var(--amount); }\n// palette\n:root { --amount: 0.6; }").unwrap();
    assert_eq!(sheet.get("panel").unwrap().opacity, Some(0.6));
}

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

/// The theme's tokens arrive as `:root` variables, so a rule reads them with `var()`: a colour, a
/// radius and a stacked shadow.
#[test]
fn test_theme_variables_resolve_in_a_rule() {
    ThemeState::init_default();
    let theme = ThemeState::get();
    let vars = theme.to_css_variable_map();

    let css = r#"
        #card {
            background: var(--surface);
            border-radius: var(--radius-lg);
            box-shadow: var(--shadow-md);
        }
    "#;
    let result = Stylesheet::parse_with_errors_and_variables(css, &vars);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let style = result.stylesheet.get("card").unwrap();
    assert!(style.background.is_some());
    assert_eq!(
        style.corner_radius,
        Some(blinc_core::CornerRadius::uniform(theme.radii().radius_lg))
    );
    assert_eq!(style.shadow.len(), theme.shadows().shadow_md.len());
}

/// `theme(token)` is gone: a theme's tokens are variables, and the function is not a value.
#[test]
fn test_theme_function_is_no_longer_a_value() {
    ThemeState::init_default();
    for decl in [
        "background: theme(primary)",
        "border-radius: theme(radius-lg)",
        "box-shadow: theme(shadow-md)",
    ] {
        let result = Stylesheet::parse_with_errors(&format!("#a {{ {decl}; }}"));
        assert!(!result.errors.is_empty(), "{decl} was read");
    }
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
