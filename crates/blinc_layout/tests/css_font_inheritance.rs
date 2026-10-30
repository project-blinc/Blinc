//! Font properties declared on a container reach its text, and a call made
//! on the text itself still wins.

#![cfg(feature = "text_measurer")]

use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::{GenericFont, div};
use blinc_layout::renderer::{ElementType, RenderTree, TextData};
use blinc_layout::text::text;

fn text_data(host: &impl blinc_layout::div::ElementBuilder, css: &str) -> TextData {
    blinc_layout::text_measurer::init_text_measurer();
    let mut tree = RenderTree::from_element(host);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 200.0);
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(ElementType::Text(t)) = tree.get_render_node(id).map(|n| &n.element_type) {
            return t.clone();
        }
        stack.extend(tree.layout_tree.children(id));
    }
    panic!("no text node");
}

const RULE: &str = ".box { font-size: 20px; font-weight: 700; line-height: 1.5; \
                    letter-spacing: 3px; font-family: \"Fira Code\", monospace; }";

#[test]
fn a_container_rule_reaches_its_text() {
    let host = div()
        .w(400.0)
        .h(200.0)
        .child(div().class("box").child(text("abc")));
    let t = text_data(&host, RULE);
    assert_eq!(t.font_size, 20.0);
    assert_eq!(t.weight.weight(), 700);
    assert_eq!(t.line_height, 1.5);
    assert_eq!(t.letter_spacing, 3.0);
    assert_eq!(t.font_family.name.as_deref(), Some("Fira Code"));
    assert_eq!(t.font_family.generic, GenericFont::Monospace);
}

#[test]
fn it_passes_through_a_plain_wrapper() {
    let host = div()
        .w(400.0)
        .h(200.0)
        .child(div().class("box").child(div().child(text("abc"))));
    assert_eq!(text_data(&host, RULE).font_size, 20.0);
}

#[test]
fn a_call_on_the_text_beats_the_container() {
    let host = div().w(400.0).h(200.0).child(
        div().class("box").child(
            text("abc")
                .size(11.0)
                .bold()
                .letter_spacing(0.5)
                .line_height(1.1)
                .monospace(),
        ),
    );
    let t = text_data(
        &host,
        ".box { font-size: 20px; font-weight: 300; line-height: 2; letter-spacing: 3px; font-family: serif; }",
    );
    assert_eq!(t.font_size, 11.0);
    assert_eq!(t.weight.weight(), 700);
    assert_eq!(t.letter_spacing, 0.5);
    assert_eq!(t.line_height, 1.1);
    assert_eq!(t.font_family.generic, GenericFont::Monospace);
}

#[test]
fn a_rule_on_the_text_beats_its_own_call() {
    let host = div().w(400.0).h(200.0).child(
        div()
            .class("box")
            .child(text("abc").class("own").size(11.0)),
    );
    let t = text_data(&host, ".box { font-size: 20px; } .own { font-size: 30px; }");
    assert_eq!(t.font_size, 30.0);
}

#[test]
fn without_a_rule_the_text_keeps_what_it_was_built_with() {
    let host = div()
        .w(400.0)
        .h(200.0)
        .child(div().child(text("abc").size(13.0)));
    assert_eq!(text_data(&host, "").font_size, 13.0);
}
