use super::*;
use std::path::Path;

fn catalog() -> Catalog {
    serde_json::from_str(include_str!("../../../../../web/components/catalog.json")).unwrap()
}

fn excerpt(slug: &str) -> String {
    let catalog = catalog();
    let component = catalog.components.iter().find(|c| c.slug == slug).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    component_css(
        &super::super::default_css(root).unwrap(),
        &catalog,
        component,
    )
}

#[test]
fn cn_wrappers_include_their_underlying_widget_defaults() {
    let checkbox = excerpt("checkbox");
    assert!(checkbox.contains(".blinc-checkbox {"));
    assert!(checkbox.contains(".blinc-checkbox-mark {"));
    assert!(checkbox.contains("transition:\n      background-color"));
    assert!(checkbox.contains(".cn-checkbox:checked {"));
    assert!(checkbox.contains("!important"));
    assert!(!checkbox.contains(".cn-radio"));
    assert!(checkbox.starts_with("@layer components {\n"));
    assert!(excerpt("toggle-group").contains(".blinc-toggle-group[data-variant=\"outline\"]"));
}

#[test]
fn component_names_and_shared_selectors_do_not_pull_in_other_widgets() {
    let input = excerpt("input");
    assert!(input.contains(".blinc-input:focus:not(:disabled)"));
    assert!(input.contains(".cn-input"));
    assert!(input.contains(".cn-label-required"));
    assert!(!input.contains(".cn-input-otp"));
    assert!(!input.contains(".cn-button"));
    let select = excerpt("select");
    assert!(select.contains(".cn-chevron"));
    assert!(!select.contains(".cn-accordion"));
    assert!(!select.contains(".cn-tabs"));
}

#[test]
fn referenced_animation_definitions_follow_the_component_rules() {
    let spinner = excerpt("spinner");
    assert!(spinner.contains("animation: cn-spinner-turn"));
    assert!(spinner.contains("@keyframes cn-spinner-turn {\n    from {"));
    assert!(spinner.contains("transform: rotate(360deg);"));
    assert_eq!(spinner.matches("@keyframes").count(), 1);
    let tabs = excerpt("tabs");
    assert!(tabs.contains("@keyframes cn-fade-in"));
    assert!(tabs.contains("@keyframes cn-tab-from-right"));
    assert!(!tabs.contains("@keyframes cn-nav-menu-enter"));
}

#[test]
fn builtin_widgets_include_their_controls_and_leave_drawing_examples_without_css() {
    assert!(excerpt("link").contains("color: var(--text-link);"));
    assert!(excerpt("code-editor").contains(".blinc-find-input:focus"));
    assert!(excerpt("audio-player").contains(".blinc-media-play-btn"));
    assert!(excerpt("rich-text-editor").contains(".blinc-rte-toolbar"));
    assert!(excerpt("canvas").is_empty());
    assert!(excerpt("svg").is_empty());
    assert!(excerpt("waveform").is_empty());
}

#[test]
fn component_root_variables_are_included_without_the_global_theme() {
    let charts = excerpt("charts");
    assert!(charts.contains("--cn-chart-grid: var(--border);"));
    assert!(!charts.contains("--cn-button"));
    assert!(!charts.contains(".demo-"));
}

#[test]
fn comments_and_quoted_delimiters_do_not_change_rule_boundaries() {
    let catalog = catalog();
    let button = catalog
        .components
        .iter()
        .find(|c| c.slug == "button")
        .unwrap();
    let css = r#"
        /* .cn-button { unrelated } */
        @layer components {
            .cn-button[data-label="},.cn-input"] { content: "};,"; color: var(--primary); }
            .cn-input, .cn-button { font-family: var(--font-sans); }
        }
    "#;
    let output = component_css(css, &catalog, button);
    assert!(output.contains("content: \"};,\";\n    color: var(--primary);"));
    assert!(!output.contains("unrelated"));
    assert!(!output.contains(".cn-input,"));
    assert!(output.contains("  .cn-button {\n    font-family: var(--font-sans);"));
}
