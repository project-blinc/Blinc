//! `gap`, `overflow` and the uniform border properties drop what an earlier or
//! lower declaration set for the parts they cover.

use blinc_core::Color;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::{Div, div};
use blinc_layout::element::RenderProps;
use blinc_layout::renderer::RenderTree;
use taffy::style::{LengthPercentage, Overflow};

/// `rules` as two rules and as the declarations of one.
fn both_ways(first: &str, second: &str) -> [String; 2] {
    [
        format!(".a {{ {first} }} .a.b {{ {second} }}"),
        format!(".a {{ {first} {second} }}"),
    ]
}

fn styled(css: &str, child: Div) -> (taffy::Style, RenderProps) {
    let ui = div().w(400.0).h(300.0).child(child);
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    (
        tree.layout_tree.get_style(node).unwrap(),
        tree.get_render_node(node).unwrap().props.clone(),
    )
}

fn child() -> Div {
    div().class("a").class("b")
}

/// Width and colour of each border drawn: top, right, bottom, left.
fn drawn(props: &RenderProps) -> [Option<(f32, Color)>; 4] {
    let s = &props.border_sides;
    let sides = [s.top, s.right, s.bottom, s.left];
    if sides.iter().any(Option::is_some) {
        return sides.map(|side| side.map(|b| (b.width, b.color)));
    }
    let uniform = props
        .border_color
        .filter(|_| props.border_width > 0.0)
        .map(|c| (props.border_width, c));
    [uniform; 4]
}

#[test]
fn gap_replaces_the_axis_gaps() {
    for css in both_ways("row-gap: 4px; column-gap: 5px;", "gap: 10px;") {
        let (style, _) = styled(&css, child());
        assert_eq!(
            [style.gap.width, style.gap.height],
            [LengthPercentage::length(10.0); 2],
            "{css}"
        );
    }
}

#[test]
fn an_axis_gap_after_gap_still_applies() {
    for css in both_ways("gap: 10px;", "row-gap: 4px;") {
        let (style, _) = styled(&css, child());
        assert_eq!(style.gap.height, LengthPercentage::length(4.0), "{css}");
        assert_eq!(style.gap.width, LengthPercentage::length(10.0), "{css}");
    }
}

#[test]
fn overflow_replaces_the_axes() {
    for css in both_ways("overflow-x: scroll;", "overflow: hidden;") {
        let (style, _) = styled(&css, child());
        assert_eq!(style.overflow.x, Overflow::Clip, "{css}");
    }
}

#[test]
fn border_replaces_every_side() {
    for css in both_ways(
        "border-top: 6px solid #ff0000;",
        "border: 2px solid #0000ff;",
    ) {
        let (_, props) = styled(&css, child());
        assert_eq!(drawn(&props), [Some((2.0, Color::BLUE)); 4], "{css}");
    }
}

#[test]
fn border_none_removes_the_border() {
    for css in both_ways("border: 2px solid #ff0000;", "border: none;") {
        let (_, props) = styled(&css, child());
        assert_eq!(drawn(&props), [None; 4], "{css}");
    }
}

#[test]
fn border_width_brings_every_side_to_that_width() {
    for css in both_ways(
        "border: 1px solid #ff0000; border-top: 6px solid #0000ff; border-left: none;",
        "border-width: 3px;",
    ) {
        let (_, props) = styled(&css, child());
        assert_eq!(
            drawn(&props).map(|s| s.map(|s| s.0)),
            [Some(3.0); 4],
            "{css}"
        );
    }
}

#[test]
fn border_color_recolors_every_side_and_keeps_its_width() {
    for css in both_ways(
        "border: 1px solid #ff0000; border-top: 6px solid #ff0000;",
        "border-color: #0000ff;",
    ) {
        let (_, props) = styled(&css, child());
        let blue = |width| Some((width, Color::BLUE));
        assert_eq!(
            drawn(&props),
            [blue(6.0), blue(1.0), blue(1.0), blue(1.0)],
            "{css}"
        );
    }
}

#[test]
fn a_border_rule_replaces_a_side_the_builder_set() {
    let (_, props) = styled(
        ".a { border: 2px solid #0000ff; }",
        div().border_left(5.0, Color::RED).class("a"),
    );
    assert_eq!(drawn(&props), [Some((2.0, Color::BLUE)); 4]);
}

#[test]
fn an_unknown_border_part_is_reported() {
    let parsed = Stylesheet::parse_with_errors(".a { border: 2px wavy #0000ff; }");
    assert!(!parsed.errors.is_empty());
}

#[test]
fn a_border_color_with_spaces_in_it_is_read_whole() {
    let half_blue = Color::rgba(0.0, 0.0, 1.0, 0.5);
    for color in [
        "rgba(0, 0, 255, 0.5)",
        "color-mix(in srgb, #0000ff 50%, transparent)",
    ] {
        let (_, props) = styled(&format!(".a {{ border: 2px solid {color}; }}"), child());
        assert_eq!(drawn(&props), [Some((2.0, half_blue)); 4], "{color}");
        let css = format!(".a {{ border-top: 2px solid {color}; }}");
        let (_, props) = styled(&css, child());
        assert_eq!(drawn(&props)[0], Some((2.0, half_blue)), "{css}");
    }
}
