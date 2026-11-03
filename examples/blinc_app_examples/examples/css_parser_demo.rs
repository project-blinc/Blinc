//! CSS Parser Demo
//!
//! no-web: this example is a CLI-style diagnostic that prints parse
//! errors to stdout — it does not drive a `WindowedApp::run` / canvas
//! event loop, so there's nothing for the web target to render. Kept
//! desktop-only on purpose. (The `no-web:` marker tells
//! `tools/build-web-examples` to skip this file during discovery.)
//!
//! Demonstrates the CSS parser with error collection and colored diagnostics.
//!
//! Run with: cargo run -p blinc_app_examples --example css_parser_demo

use blinc_layout::prelude::{CssElementState, CssParseResult, Stylesheet};
use blinc_theme::ThemeState;

fn main() {
    // The theme's tokens arrive as `:root` variables: `var(--primary)`, `var(--radius-lg)`,
    // `var(--shadow-md)`.
    ThemeState::init_default();
    let vars = ThemeState::get().to_css_variable_map();

    println!("\n=== CSS Parser Demo ===\n");

    // Example 1: Valid CSS with state modifiers
    println!("1. Parsing CSS with state modifiers (:hover, :active, etc):");
    println!("{}", "-".repeat(50));
    let valid_css = r#"
#card {
    background: #3498db;
    border-radius: 8px;
    opacity: 0.95;
}

#button-primary {
    background: var(--primary);
    transform: scale(1.0);
    border-radius: var(--radius-default);
}

#button-primary:hover {
    opacity: 0.9;
    transform: scale(1.02);
}

#button-primary:active {
    transform: scale(0.98);
}

#button-primary:disabled {
    opacity: 0.5;
}
"#;

    let result = Stylesheet::parse_with_errors_and_variables(valid_css, &vars);
    print_result(&result, valid_css);

    // Example 2: CSS with unknown properties (warnings)
    println!("\n2. Parsing CSS with unknown properties:");
    println!("{}", "-".repeat(50));
    let css_with_unknowns = r#"
#card {
    background: #FF5733;
    margin: 10px;
    padding: 20px;
    opacity: 0.8;
    display: flex;
}
"#;

    let result = Stylesheet::parse_with_errors_and_variables(css_with_unknowns, &vars);
    print_result(&result, css_with_unknowns);

    // Example 3: CSS with invalid values
    println!("\n3. Parsing CSS with invalid values:");
    println!("{}", "-".repeat(50));
    let css_with_invalid = r#"
#widget {
    opacity: not-a-number;
    border-radius: ???;
    background: red;
    transform: invalid(42);
}
"#;

    let result = Stylesheet::parse_with_errors_and_variables(css_with_invalid, &vars);
    print_result(&result, css_with_invalid);

    // Example 4: Mixed valid and invalid
    println!("\n4. Parsing mixed valid/invalid CSS:");
    println!("{}", "-".repeat(50));
    let mixed_css = r#"
#header {
    background: var(--surface);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-md);
}

#sidebar {
    background: rgba(255, 128, 0, 0.5);
    unknown-flex: center;
    opacity: 0.9;
}

#footer {
    background: #333333;
    render-layer: foreground;
}
"#;

    let result = Stylesheet::parse_with_errors_and_variables(mixed_css, &vars);
    print_result(&result, mixed_css);

    // Example 5: Show error collection API
    println!("\n5. Error Collection API Demo:");
    println!("{}", "-".repeat(50));
    let css = "#test { unknown: x; opacity: bad; background: blue; weird-prop: y; }";
    let result = Stylesheet::parse_with_errors_and_variables(css, &vars);

    println!("CSS: {}", css);
    println!("\nUsing CssParseResult API:");
    println!("  - has_errors(): {}", result.has_errors());
    println!("  - has_warnings(): {}", result.has_warnings());
    println!(
        "  - errors_only().count(): {}",
        result.errors_only().count()
    );
    println!(
        "  - warnings_only().count(): {}",
        result.warnings_only().count()
    );
    println!("  - stylesheet.len(): {}", result.stylesheet.len());

    if !result.errors.is_empty() {
        println!("\nCollected errors/warnings:");
        for (i, err) in result.errors.iter().enumerate() {
            println!(
                "  [{}] {} (line {}, col {})",
                i + 1,
                err.severity,
                err.line,
                err.column
            );
            if let Some(ref prop) = err.property {
                println!("      property: {}", prop);
            }
            if let Some(ref val) = err.value {
                println!("      value: {}", val);
            }
            println!("      message: {}", err.message);
        }
    }

    // Example 6: CSS Variables with :root
    println!("\n6. CSS Variables (:root and var()):");
    println!("{}", "-".repeat(50));
    let css_with_vars = r#"
:root {
    --brand-color: #3498db;
    --hover-opacity: 0.85;
    --card-radius: 12px;
}

#card {
    background: var(--brand-color);
    border-radius: var(--card-radius);
    opacity: 1.0;
}

#card:hover {
    opacity: var(--hover-opacity);
}
"#;

    let result = Stylesheet::parse_with_errors_and_variables(css_with_vars, &vars);
    print_result(&result, css_with_vars);

    // Show variable access
    println!("\nCSS Variables defined:");
    for name in result.stylesheet.variable_names() {
        let value = result.stylesheet.get_variable(name).unwrap();
        println!("  --{}: {}", name, value);
    }

    // Example 7: State modifier API
    println!("\n7. State Modifier API Demo:");
    println!("{}", "-".repeat(50));
    let css = r#"
#button {
    background: blue;
    opacity: 1.0;
}
#button:hover {
    opacity: 0.9;
}
#button:active {
    transform: scale(0.95);
}
#button:focus {
    border-radius: 4px;
}
#button:disabled {
    opacity: 0.4;
}
"#;
    let result = Stylesheet::parse_with_errors_and_variables(css, &vars);

    println!("CSS: {}", css.trim());
    println!("\nQuerying state-specific styles:");

    // Show base style
    if let Some(base) = result.stylesheet.get("button") {
        println!("  #button (base): opacity={:?}", base.opacity);
    }

    // Show state styles using get_with_state
    if let Some(hover) = result
        .stylesheet
        .get_with_state("button", CssElementState::Hover)
    {
        println!("  #button:hover: opacity={:?}", hover.opacity);
    }
    if let Some(active) = result
        .stylesheet
        .get_with_state("button", CssElementState::Active)
    {
        println!("  #button:active: transform={:?}", active.transform);
    }
    if let Some(focus) = result
        .stylesheet
        .get_with_state("button", CssElementState::Focus)
    {
        println!("  #button:focus: corner_radius={:?}", focus.corner_radius);
    }
    if let Some(disabled) = result
        .stylesheet
        .get_with_state("button", CssElementState::Disabled)
    {
        println!("  #button:disabled: opacity={:?}", disabled.opacity);
    }

    // Show get_all_states API
    println!("\nUsing get_all_states():");
    let (base, states) = result.stylesheet.get_all_states("button");
    println!("  Base style: {:?}", base.map(|s| s.opacity));
    println!("  State variants: {}", states.len());
    for (state, style) in &states {
        println!(
            "    :{} => opacity={:?}, transform={:?}",
            state, style.opacity, style.transform
        );
    }

    // Example 8: Keyframe Animations
    println!("\n8. Keyframe Animations (@keyframes):");
    println!("{}", "-".repeat(50));
    let css_with_keyframes = r#"
@keyframes fade-in {
    from {
        opacity: 0;
        transform: translateY(20px);
    }
    to {
        opacity: 1;
        transform: translateY(0);
    }
}

@keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.8; transform: scale(1.05); }
}

#modal {
    background: var(--surface);
    border-radius: var(--radius-lg);
}
"#;

    let result = Stylesheet::parse_with_errors_and_variables(css_with_keyframes, &vars);
    print_result(&result, css_with_keyframes);

    println!("\nKeyframe animations defined:");
    for name in result.stylesheet.keyframe_names() {
        let keyframes = result.stylesheet.get_keyframes(name).unwrap();
        println!(
            "  @keyframes {} ({} stops):",
            name,
            keyframes.keyframes.len()
        );
        for kf in &keyframes.keyframes {
            println!(
                "    {}% => opacity={:?}, transform={:?}",
                (kf.position * 100.0) as i32,
                kf.style.opacity,
                kf.style.transform.is_some()
            );
        }
    }

    println!("\nConverting keyframes to MotionAnimation:");
    if let Some(fade_in) = result.stylesheet.get_keyframes("fade-in") {
        let motion = fade_in.to_motion_animation(300, 200);
        println!("  fade-in:");
        println!("    enter_duration_ms: {}", motion.enter_duration_ms);
        println!("    exit_duration_ms: {}", motion.exit_duration_ms);
        if let Some(ref enter) = motion.enter_from {
            println!(
                "    enter_from: opacity={:?}, translate_y={:?}",
                enter.opacity, enter.translate_y
            );
        }
        if let Some(ref exit) = motion.exit_to {
            println!(
                "    exit_to: opacity={:?}, translate_y={:?}",
                exit.opacity, exit.translate_y
            );
        }
    }

    println!("\n=== Demo Complete ===\n");
}

fn print_result(result: &CssParseResult, _css: &str) {
    // Print stylesheet info
    println!("Parsed {} rule(s):", result.stylesheet.len());
    for id in result.stylesheet.ids() {
        let style = result.stylesheet.get(id).unwrap();
        println!("  #{} => {:?}", id, style);
    }

    // Let the formatter handle colored diagnostics
    if !result.errors.is_empty() {
        println!();
    }
    result.print_colored_diagnostics();
    result.print_summary();
}
