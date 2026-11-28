//! A rule with a pointer state is ordered against the other rules by specificity and position,
//! as in a browser.

use blinc_core::{Brush, Color};
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::event_router::EventRouter;
use blinc_layout::renderer::RenderTree;

const FIRST: &str = "#336699";
const SECOND: &str = "#228844";

/// The fill of a checked 40px box with id `cb` and classes `x y` under `css`: idle, then under
/// the pointer, then with the pointer gone again.
fn fills(css: &str) -> [Option<String>; 3] {
    let ui = div().w(100.0).h(100.0).child(
        div()
            .id("cb")
            .class("x")
            .class("y")
            .attr("checked", "")
            .w(40.0)
            .h(40.0),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_all_stylesheet_styles();
    tree.compute_layout(100.0, 100.0);
    let mut router = EventRouter::new();
    let node = tree.query_by_id("cb").expect("the box");
    let fill = |tree: &RenderTree| match tree.get_render_node(node).unwrap().props.background {
        Some(Brush::Solid(c)) => Some(hex(c)),
        _ => None,
    };
    let mut step = |tree: &mut RenderTree, x: f32| {
        router.on_mouse_move(tree, x, 20.0);
        tree.apply_stylesheet_state_styles(&router);
        fill(tree)
    };
    let idle = step(&mut tree, 80.0);
    let over = step(&mut tree, 20.0);
    // A second pass with the pointer still there changes nothing.
    assert_eq!(step(&mut tree, 21.0), over);
    [idle, over, step(&mut tree, 80.0)]
}

fn hex(c: Color) -> String {
    let byte = |v: f32| (v * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
}

fn is(css: &str, idle: Option<&str>, over: &str) {
    let idle = idle.map(str::to_string);
    assert_eq!(
        fills(css),
        [idle.clone(), Some(over.to_string()), idle],
        "{css}"
    );
}

#[test]
fn the_later_of_two_equal_rules_wins_under_the_pointer() {
    for target in ["#cb", ".x"] {
        let hover = format!("{target}:hover {{ background: {FIRST} }}");
        let checked = format!("{target}:checked {{ background: {SECOND} }}");
        is(&format!("{hover} {checked}"), Some(SECOND), SECOND);
        is(&format!("{checked} {hover}"), Some(SECOND), FIRST);
    }
}

#[test]
fn a_plain_rule_of_equal_weight_beats_an_earlier_state_rule() {
    let hover = format!(".x:hover {{ background: {FIRST} }}");
    let plain = format!(".x.y {{ background: {SECOND} }}");
    is(&format!("{hover} {plain}"), Some(SECOND), SECOND);
    is(&format!("{plain} {hover}"), Some(SECOND), FIRST);
}

#[test]
fn an_id_rule_outranks_a_class_state_rule() {
    let hover = format!(".x:hover {{ background: {FIRST} }}");
    let id = format!("#cb {{ background: {SECOND} }}");
    is(&format!("{id} {hover}"), Some(SECOND), SECOND);
}

#[test]
fn a_state_rule_that_outranks_the_others_applies() {
    let weak = format!(".x {{ background: {SECOND} }}");
    let hover = format!(".x:hover {{ background: {FIRST} }}");
    is(&format!("{hover} {weak}"), Some(SECOND), FIRST);
}
