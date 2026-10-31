//! `[attr]` selectors match the attributes an element sets with `attr`.

use blinc_core::Color;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

fn bg(host: &impl blinc_layout::div::ElementBuilder, css: &str) -> Option<Color> {
    let mut tree = RenderTree::from_element(host);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    let node = tree.layout_tree.children(tree.root().expect("root"))[0];
    match tree.get_render_node(node)?.props.background.as_ref()? {
        blinc_core::Brush::Solid(c) => Some(*c),
        _ => None,
    }
}

fn red() -> Option<Color> {
    Some(Color::rgb(1.0, 0.0, 0.0))
}

fn host_with(child: blinc_layout::div::Div) -> impl blinc_layout::div::ElementBuilder {
    div()
        .w(100.0)
        .h(100.0)
        .child(child.w(10.0).h(10.0).class("x"))
}

#[test]
fn equality_matches_the_value() {
    let css = ".x[data-variant=\"danger\"] { background: #ff0000; }";
    assert_eq!(bg(&host_with(div().data("variant", "danger")), css), red());
    assert_eq!(bg(&host_with(div().data("variant", "ok")), css), None);
    assert_eq!(bg(&host_with(div()), css), None);
}

#[test]
fn presence_matches_any_value_including_none() {
    let css = ".x[data-current] { background: #ff0000; }";
    assert_eq!(bg(&host_with(div().data("current", "")), css), red());
    assert_eq!(bg(&host_with(div().data("current", "page")), css), red());
    assert_eq!(bg(&host_with(div()), css), None);
}

#[test]
fn the_other_operators_compare_the_value() {
    let on = |op: &str, actual: &str, wanted: &str| {
        let css = format!(".x[data-a{op}\"{wanted}\"] {{ background: #ff0000; }}");
        bg(&host_with(div().data("a", actual)), &css) == red()
    };
    assert!(on("~=", "one two three", "two"));
    assert!(!on("~=", "onetwo", "two"));
    assert!(on("|=", "en-US", "en"));
    assert!(on("^=", "prefix-rest", "prefix"));
    assert!(on("$=", "rest-suffix", "suffix"));
    assert!(on("*=", "has-middle-part", "middle"));
    assert!(!on("^=", "x", ""));
}

#[test]
fn an_attribute_adds_to_specificity_like_a_class() {
    // The attribute rule is declared first but is more specific than `.x`.
    let css = ".x[data-v=\"1\"] { background: #ff0000; } .x { background: #0000ff; }";
    assert_eq!(bg(&host_with(div().data("v", "1")), css), red());
}

#[test]
fn it_matches_through_a_wrapper_that_forwards_classes() {
    let host = div().w(100.0).h(100.0).child(Box::new(
        div().class("x").data("variant", "danger").w(10.0).h(10.0),
    ) as Box<dyn blinc_layout::div::ElementBuilder>);
    assert_eq!(
        bg(&host, ".x[data-variant=danger] { background: #ff0000; }"),
        red()
    );
}

#[test]
fn an_unsupported_form_still_skips_the_rule_with_a_warning() {
    let r =
        Stylesheet::parse_with_errors(".x[ { background: #ff0000; } .y { background: #0000ff; }");
    assert!(r.stylesheet.get_class("y").is_some());
}
