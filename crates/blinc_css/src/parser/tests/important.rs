use crate::{Stylesheet, element_style::ElementStyle};
use blinc_core::{Brush, Color};

fn background(style: &ElementStyle) -> Color {
    match style.background.as_ref().expect("background") {
        Brush::Solid(color) => *color,
        _ => panic!("solid background"),
    }
}

#[test]
fn important_colour_survives_a_later_normal_declaration() {
    let sheet = Stylesheet::parse(
        "#sample { background: red !important; background-color: blue; color: green; }",
    )
    .expect("valid CSS");
    let style = sheet.get("sample").unwrap();
    assert_eq!(background(style), Color::from_hex(0xff0000));
    assert_eq!(style.text_color, Some(Color::from_hex(0x008000)));
}

#[test]
fn important_colours_merge_per_property_across_sheets() {
    let mut sheet =
        Stylesheet::parse("#sample { background: red !IMPORTANT; color: white; }").unwrap();
    sheet.merge(
        Stylesheet::parse("#sample { background: blue; color: green !important; }").unwrap(),
    );
    let style = sheet.get("sample").unwrap();
    assert_eq!(background(style), Color::from_hex(0xff0000));
    assert_eq!(style.text_color, Some(Color::from_hex(0x008000)));
}

#[test]
fn later_equal_priority_important_colour_wins() {
    let mut sheet = Stylesheet::parse("#sample { color: red !important; }").unwrap();
    sheet.merge(Stylesheet::parse("#sample { color: blue !important; }").unwrap());
    assert_eq!(
        sheet.get("sample").unwrap().text_color,
        Some(Color::from_hex(0x0000ff))
    );
}

#[test]
fn unsupported_importance_is_reported_instead_of_silently_ignored() {
    let result = Stylesheet::parse_with_errors("#sample { width: 20px !important; }");
    assert!(result.has_errors());
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("!important"))
    );
}

#[test]
fn repeated_rules_keep_earlier_important_colours_and_later_normal_properties() {
    let sheet = Stylesheet::parse(
        "#sample { color: red !important; } #sample { color: blue; width: 20px; }",
    )
    .unwrap();
    assert_eq!(
        sheet.get("sample").unwrap().text_color,
        Some(Color::from_hex(0xff0000))
    );
    assert!(sheet.get("sample").unwrap().width.is_some());
}
