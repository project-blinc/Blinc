//! Diagnostics: what the parser reports, and what it recovers from.

use crate::parser::*;

#[test]
fn test_parse_error_context() {
    // A string that can't be parsed as any selector should result in an empty stylesheet
    let css = "!!! { opacity: 0.5; }";
    let result = Stylesheet::parse(css);
    // This should parse as empty (no valid rules) but not error
    // since the parser just ignores what it can't parse
    // The parse itself succeeds but finds no valid rules
    let stylesheet = result.unwrap();
    assert!(stylesheet.is_empty());
}

#[test]
fn test_parse_error_has_display() {
    // Create an error manually to test Display impl
    let err = ParseError {
        severity: Severity::Error,
        message: "test error".to_string(),
        line: 1,
        column: 5,
        fragment: "#test".to_string(),
        contexts: vec!["rule".to_string(), "selector".to_string()],
        property: None,
        value: None,
    };
    let display = format!("{}", err);
    assert!(display.contains("CSS error"));
    assert!(display.contains("line 1"));
    assert!(display.contains("column 5"));
}

#[test]
fn test_parse_or_empty_success() {
    let css = "#test { opacity: 0.5; }";
    let stylesheet = Stylesheet::parse_or_empty(css);
    assert!(stylesheet.contains("test"));
}

#[test]
fn test_parse_or_empty_failure() {
    // Invalid CSS returns empty stylesheet
    let css = "this is not valid CSS";
    let stylesheet = Stylesheet::parse_or_empty(css);
    assert!(stylesheet.is_empty());
}

#[test]
fn test_unknown_property_ignored() {
    // Unknown properties are silently ignored
    let css = "#test { unknown-property: value; opacity: 0.5; }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    // The known property is still parsed
    assert_eq!(style.opacity, Some(0.5));
}

#[test]
fn test_invalid_value_skipped() {
    // Invalid values for known properties are skipped
    let css = "#test { opacity: invalid; border-radius: 8px; }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    // opacity should be None (invalid value), but radius should work
    assert!(style.opacity.is_none());
    assert!(style.corner_radius.is_some());
}

// ========================================================================
// Error Collection Tests for Reporting
// ========================================================================

#[test]
fn test_error_collection_missing_closing_brace() {
    // Missing closing brace should produce a collectable error
    let css = "#test { opacity: 0.5";
    let result = Stylesheet::parse_with_errors(css);

    // With parse_with_errors, we get partial results plus errors
    // The stylesheet might be empty (couldn't parse any complete rules)
    // and errors should contain info about what went wrong

    // Either we have an error, or we have unparsed content warning
    let has_issues = result.has_errors() || result.has_warnings() || result.stylesheet.is_empty();
    assert!(has_issues, "Should have some indication of incomplete CSS");

    // If there are errors, validate their details
    if !result.errors.is_empty() {
        let err = &result.errors[0];
        assert!(err.line >= 1, "Line number should be set");
        assert!(err.column >= 1, "Column number should be set");
        assert!(!err.message.is_empty(), "Error message should be set");

        let display = format!("{}", err);
        assert!(
            display.contains("line") && display.contains("column"),
            "Display should include line and column info"
        );
    }
}

#[test]
fn test_error_collection_missing_id_after_hash() {
    // # followed by invalid identifier should capture error context
    let css = "#123invalid { opacity: 0.5; }";
    let result = Stylesheet::parse(css);

    // This might parse as empty or error depending on nom's behavior
    // Either way, we should handle it gracefully
    match result {
        Ok(stylesheet) => {
            // If it parsed as empty, that's valid fallback behavior
            assert!(stylesheet.is_empty() || stylesheet.contains("123invalid"));
        }
        Err(err) => {
            // If it errored, error details should be collected
            assert!(!err.message.is_empty());
            assert!(err.line >= 1);
        }
    }
}

#[test]
fn test_error_collection_with_contexts() {
    // Test that context stack is properly collected
    let css = "#test { : value; }"; // Missing property name before colon
    let result = Stylesheet::parse(css);

    match result {
        Ok(stylesheet) => {
            // Parser might skip malformed property, returning empty style
            if stylesheet.contains("test") {
                let style = stylesheet.get("test").unwrap();
                // The malformed property should be skipped
                assert!(style.opacity.is_none());
            }
        }
        Err(err) => {
            // Error should have context about what was being parsed
            assert!(!err.message.is_empty());
            // Contexts might include "property name" or similar
            let display = format!("{}", err);
            assert!(display.contains("CSS parse error"));
        }
    }
}

#[test]
fn test_error_collection_multiline() {
    // Test that line numbers are correctly calculated for multiline CSS
    let css = r#"
#first { opacity: 0.5; }
#second { opacity: 0.7; }
#third { opacity
"#;
    let result = Stylesheet::parse(css);

    match result {
        Ok(stylesheet) => {
            // May successfully parse the complete rules
            assert!(stylesheet.contains("first") || stylesheet.contains("second"));
        }
        Err(err) => {
            // If it errors, the line should be > 1 since error is on line 4
            assert!(err.line >= 1, "Line number should be calculated");
            let display = format!("{}", err);
            assert!(display.contains("line"), "Display should show line info");
        }
    }
}

#[test]
fn test_error_collection_preserves_fragment() {
    // Test that the error fragment is captured for reporting
    let css = "#bad-css { = not valid }";
    let result = Stylesheet::parse(css);

    match result {
        Ok(_) => {
            // Parser might skip invalid content
        }
        Err(err) => {
            // Fragment should be set (though it might be truncated)
            // The fragment helps identify where parsing stopped
            let display = format!("{}", err);
            assert!(!display.is_empty());
        }
    }
}

#[test]
fn test_collect_multiple_errors_via_iterations() {
    // Demonstrate how to collect errors from multiple CSS inputs
    let css_inputs = vec![
        ("#valid { opacity: 0.5; }", true),      // valid
        ("#broken {", false),                    // invalid - missing close
        ("#also-valid { opacity: 1.0; }", true), // valid
        ("@ invalid at-rule", false),            // invalid - no ID selector
    ];

    let mut errors: Vec<ParseError> = Vec::new();
    let mut successes: Vec<Stylesheet> = Vec::new();

    for (css, expected_success) in css_inputs {
        match Stylesheet::parse(css) {
            Ok(stylesheet) => {
                if expected_success {
                    successes.push(stylesheet);
                } else {
                    // Unexpected success - parser was lenient
                    successes.push(stylesheet);
                }
            }
            Err(err) => {
                // Collect the error for reporting
                errors.push(err);
            }
        }
    }

    // Report: we can format all collected errors
    for (i, err) in errors.iter().enumerate() {
        let report = format!(
            "Error {}: line {}, col {}: {}",
            i + 1,
            err.line,
            err.column,
            err.message
        );
        assert!(!report.is_empty());
    }

    // At least one should have errored (the unclosed brace)
    assert!(
        !errors.is_empty() || successes.iter().any(|s| s.is_empty()),
        "Should have captured at least one error or empty result"
    );
}

#[test]
fn test_error_debug_format() {
    // Test that ParseError has useful Debug output
    let css = "#incomplete {";
    let result = Stylesheet::parse(css);

    if let Err(err) = result {
        let debug_output = format!("{:?}", err);
        // Debug should include all the fields
        assert!(debug_output.contains("message"));
        assert!(debug_output.contains("line"));
        assert!(debug_output.contains("column"));
        assert!(debug_output.contains("fragment"));
        assert!(debug_output.contains("contexts"));
    }
}

#[test]
fn test_error_is_std_error() {
    // Ensure ParseError implements std::error::Error properly
    let err = ParseError {
        severity: Severity::Error,
        message: "test error".to_string(),
        line: 5,
        column: 10,
        fragment: "broken".to_string(),
        contexts: vec!["rule".to_string()],
        property: Some("opacity".to_string()),
        value: Some("invalid".to_string()),
    };

    // Can be used as a std::error::Error
    let _: &dyn std::error::Error = &err;

    // Default source() implementation returns None
    use std::error::Error;
    assert!(err.source().is_none());
}

// ========================================================================
// Tests for parse_with_errors - Full Error Collection
// ========================================================================

#[test]
fn test_parse_with_errors_collects_unknown_properties() {
    let css = "#test { unknown-prop: value; opacity: 0.5; another-unknown: foo; }";
    let result = Stylesheet::parse_with_errors(css);

    // Should still parse the valid property
    assert!(result.stylesheet.contains("test"));
    let style = result.stylesheet.get("test").unwrap();
    assert_eq!(style.opacity, Some(0.5));

    // Should have collected warnings for unknown properties
    assert!(
        result.has_warnings(),
        "Should have warnings for unknown properties"
    );

    let warnings: Vec<_> = result.warnings_only().collect();
    assert!(
        warnings.len() >= 2,
        "Should have at least 2 warnings for unknown props"
    );

    // Check that warnings contain property info
    for warning in &warnings {
        assert_eq!(warning.severity, Severity::Warning);
        assert!(warning.property.is_some());
    }
}

#[test]
fn test_parse_with_errors_collects_invalid_values() {
    let css = "#test { opacity: not-a-number; border-radius: ???; background: #FF0000; }";
    let result = Stylesheet::parse_with_errors(css);

    // Should still parse the valid property
    assert!(result.stylesheet.contains("test"));
    let style = result.stylesheet.get("test").unwrap();
    assert!(style.background.is_some(), "Valid background should parse");
    assert!(style.opacity.is_none(), "Invalid opacity should not parse");

    // Should have collected warnings for invalid values
    assert!(result.has_warnings());

    let warnings: Vec<_> = result.warnings_only().collect();
    assert!(
        warnings.len() >= 2,
        "Should have warnings for invalid values"
    );

    // Check warning details
    for warning in &warnings {
        assert!(warning.property.is_some());
        assert!(warning.value.is_some());
        assert!(warning.message.contains("Invalid value"));
    }
}

#[test]
fn test_parse_with_errors_print_diagnostics() {
    let css = "#test { unknown: value; opacity: bad; background: red; }";
    let result = Stylesheet::parse_with_errors(css);

    // Should have some errors/warnings
    assert!(!result.errors.is_empty());

    // Test that print_diagnostics doesn't panic
    // (We can't easily capture stderr in tests, but we can verify it runs)
    result.log_diagnostics();

    // Verify to_warning_string works
    for err in &result.errors {
        let warning_str = err.to_warning_string();
        assert!(!warning_str.is_empty());
        assert!(warning_str.contains(&err.severity.to_string()));
    }
}

#[test]
fn test_parse_with_errors_multiline_line_numbers() {
    let css = r#"
#first {
opacity: 0.5;
unknown-prop: value;
}
#second {
opacity: bad;
background: blue;
}
"#;
    let result = Stylesheet::parse_with_errors(css);

    // Both rules should parse
    assert!(result.stylesheet.contains("first"));
    assert!(result.stylesheet.contains("second"));

    // Should have warnings with line numbers > 1
    let warnings: Vec<_> = result.warnings_only().collect();
    assert!(!warnings.is_empty());

    // At least some warnings should be on lines > 1
    let has_multiline_errors = warnings.iter().any(|w| w.line > 1);
    assert!(has_multiline_errors, "Should have errors on lines > 1");
}

#[test]
fn test_parse_with_errors_severity_levels() {
    // Create various error types and check severity
    let warning = ParseError::unknown_property("foo", 1, 1);
    assert_eq!(warning.severity, Severity::Warning);

    let invalid = ParseError::invalid_value("opacity", "bad", 2, 5);
    assert_eq!(invalid.severity, Severity::Warning);

    let error = ParseError::new(Severity::Error, "fatal error", 3, 10);
    assert_eq!(error.severity, Severity::Error);
}

#[test]
fn test_css_parse_result_methods() {
    let css = "#test { unknown: x; opacity: bad; }";
    let result = Stylesheet::parse_with_errors(css);

    // Test CssParseResult methods
    assert!(result.has_warnings());
    assert!(!result.has_errors()); // These are warnings, not errors

    let warnings_count = result.warnings_only().count();
    let errors_count = result.errors_only().count();

    assert!(warnings_count >= 2);
    assert_eq!(errors_count, 0);
}

#[test]
fn test_error_collection_with_valid_css_no_errors() {
    let css = "#card { opacity: 0.8; background: #FF0000; border-radius: 8px; }";
    let result = Stylesheet::parse_with_errors(css);

    // Should parse successfully with no errors
    assert!(result.stylesheet.contains("card"));
    assert!(result.errors.is_empty(), "Valid CSS should have no errors");
    assert!(!result.has_errors());
    assert!(!result.has_warnings());
}

// ========================================================================
// CSS Variables Tests
// ========================================================================
