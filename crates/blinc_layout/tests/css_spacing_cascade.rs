//! A padding or margin longhand cascades per side: it leaves the sides it does
//! not name to the rules below it.

use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::{Div, div};
use blinc_layout::renderer::RenderTree;
use taffy::style::{LengthPercentage, LengthPercentageAuto};

/// The layout style of `child` once `css` is applied to it.
fn styled(css: &str, child: Div) -> taffy::Style {
    let ui = div().w(400.0).h(300.0).child(child);
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    tree.layout_tree.get_style(node).unwrap()
}

/// Top, right, bottom, left.
fn padding(style: &taffy::Style) -> [LengthPercentage; 4] {
    let p = style.padding;
    [p.top, p.right, p.bottom, p.left]
}

fn margin(style: &taffy::Style) -> [LengthPercentageAuto; 4] {
    let m = style.margin;
    [m.top, m.right, m.bottom, m.left]
}

fn px(sides: [f32; 4]) -> [LengthPercentage; 4] {
    sides.map(LengthPercentage::length)
}

#[test]
fn a_longhand_keeps_the_sides_a_lower_rule_set() {
    for selectors in [
        ".a { A } .a.b { B }",
        ".a { A } #x { B }",
        "#x { A } #x { B }",
    ] {
        let css = selectors
            .replace('A', "padding: 10px 20px 30px 40px;")
            .replace('B', "padding-right: 2px;");
        let style = styled(&css, div().id("x").class("a").class("b"));
        assert_eq!(padding(&style), px([10.0, 2.0, 30.0, 40.0]), "{css}");
    }
}

#[test]
fn longhands_from_separate_rules_add_up() {
    let css = ".a { padding-top: 1px; } .b { padding-left: 4px; }";
    let style = styled(css, div().class("a").class("b"));
    assert_eq!(padding(&style), px([1.0, 0.0, 0.0, 4.0]));
}

#[test]
fn a_longhand_keeps_the_builders_other_sides() {
    let style = styled(".a { padding-top: 3px; }", div().p(2.0).class("a"));
    assert_eq!(padding(&style), px([3.0, 8.0, 8.0, 8.0]));
}

#[test]
fn a_margin_longhand_keeps_the_other_sides_and_their_auto() {
    let css = ".a { margin: 6px auto; } .a.b { margin-left: 5px; }";
    let style = styled(css, div().class("a").class("b"));
    let [six, five] = [6.0, 5.0].map(LengthPercentageAuto::length);
    let auto = LengthPercentageAuto::auto();
    assert_eq!(margin(&style), [six, auto, six, five]);
}

#[test]
fn a_later_length_replaces_an_earlier_auto_margin() {
    let css = ".a { margin: auto; } .a.b { margin: 7px; }";
    let style = styled(css, div().class("a").class("b"));
    assert_eq!(margin(&style), [LengthPercentageAuto::length(7.0); 4]);
}
