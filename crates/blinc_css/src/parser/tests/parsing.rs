//! End-to-end parsing: rules, comments, colors, transforms, materials.

use crate::element_style::*;
use crate::material::*;
use crate::parser::*;

/// A `//` line comment between two rules must not take the second
/// one with it.
///
/// It used to parse as the start of a selector, so the rule after
/// it silently never applied — and a stylesheet that drops a rule
/// without complaining is indistinguishable from one whose author
/// mistyped a class name.
#[test]
fn a_line_comment_between_rules_keeps_both() {
    let sheet = Stylesheet::parse(
        ".row { flex-direction: row }\n             // stacking inside a section\n             .col { flex-direction: column }\n",
    )
    .expect("parses");

    let row = sheet.get_class("row").expect("the rule before the comment");
    let col = sheet.get_class("col").expect("and the one after it");
    assert_eq!(row.flex_direction, Some(StyleFlexDirection::Row));
    assert_eq!(col.flex_direction, Some(StyleFlexDirection::Column));
}

/// The same comment leading the block, which is where authors put
/// them most often.
#[test]
fn a_leading_line_comment_keeps_the_first_rule() {
    let sheet =
        Stylesheet::parse("// the shell\n.shell { flex-direction: row }\n").expect("parses");
    assert_eq!(
        sheet
            .get_class("shell")
            .expect("the first rule survives")
            .flex_direction,
        Some(StyleFlexDirection::Row)
    );
}

#[test]
fn test_parse_empty() {
    let stylesheet = Stylesheet::parse("").unwrap();
    assert!(stylesheet.is_empty());
}

#[test]
fn test_parse_single_rule() {
    let css = "#card { opacity: 0.5; }";
    let stylesheet = Stylesheet::parse(css).unwrap();

    assert!(stylesheet.contains("card"));
    let style = stylesheet.get("card").unwrap();
    assert_eq!(style.opacity, Some(0.5));
}

#[test]
fn test_parse_multiple_rules() {
    let css = r#"
        #card {
            opacity: 0.9;
            border-radius: 8px;
        }
        #button {
            opacity: 1.0;
        }
    "#;
    let stylesheet = Stylesheet::parse(css).unwrap();

    assert_eq!(stylesheet.len(), 2);
    assert!(stylesheet.contains("card"));
    assert!(stylesheet.contains("button"));
}

#[test]
fn test_parse_hex_colors() {
    let css = "#test { background: #FF0000; }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.background.is_some());
}

#[test]
fn test_parse_transform_scale() {
    let css = "#test { transform: scale(1.5); }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.transform.is_some());
}

#[test]
fn test_parse_transform_scale_two_args() {
    let css = "#test { transform: scale(1.5, 2.0); }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.transform.is_some());
}

#[test]
fn test_parse_transform_rotate() {
    let css = "#test { transform: rotate(45deg); }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.transform.is_some());
}

#[test]
fn test_parse_transform_translate() {
    let css = "#test { transform: translate(10px, 20px); }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.transform.is_some());
}

#[test]
fn test_parse_transform_translate_x() {
    let css = "#test { transform: translateX(10px); }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.transform.is_some());
}

#[test]
fn test_parse_transform_translate_y() {
    let css = "#test { transform: translateY(20px); }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.transform.is_some());
}

#[test]
fn test_parse_comments() {
    let css = r#"
        /* This is a comment */
        #card {
            /* inline comment */
            opacity: 0.5;
        }
    "#;
    let stylesheet = Stylesheet::parse(css).unwrap();
    assert!(stylesheet.contains("card"));
}

#[test]
fn test_parse_rgb_color() {
    let css = "#test { background: rgb(255, 128, 0); }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.background.is_some());
}

#[test]
fn test_parse_rgba_color() {
    let css = "#test { background: rgba(255, 128, 0, 0.5); }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.background.is_some());
}

#[test]
fn test_parse_named_color() {
    let css = "#test { background: red; }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.background.is_some());
}

#[test]
fn test_parse_short_hex() {
    let css = "#test { background: #F00; }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.background.is_some());
}

#[test]
fn test_parse_render_layer() {
    let css = "#test { render-layer: foreground; }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert_eq!(style.render_layer, Some(RenderLayer::Foreground));
}

#[test]
fn test_parse_backdrop_filter_glass() {
    let css = "#test { backdrop-filter: glass; }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.material.is_some());
    assert_eq!(style.render_layer, Some(RenderLayer::Glass));
}

#[test]
fn test_parse_backdrop_filter_blur() {
    let css = "#test { backdrop-filter: blur(10px); }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.material.is_some());
    assert_eq!(style.render_layer, Some(RenderLayer::Glass));
}

#[test]
fn test_parse_backdrop_filter_metallic() {
    let css = "#test { backdrop-filter: chrome; }";
    let stylesheet = Stylesheet::parse(css).unwrap();
    let style = stylesheet.get("test").unwrap();
    assert!(style.material.is_some());
}
