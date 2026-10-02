//! A text node's measure context changes in place, and the next layout
//! measures what it now holds.
use blinc_layout::div::GenericFont;
use blinc_layout::{LayoutNodeId, LayoutTree, TextMeasureContext};
use taffy::prelude::*;

fn context(content: &str) -> TextMeasureContext {
    TextMeasureContext {
        content: content.into(),
        font_size: 16.0,
        line_height: 1.2,
        wrap: false,
        font_name: None,
        generic_font: GenericFont::System,
        font_weight: 400,
        italic: false,
    }
}

fn width(tree: &mut LayoutTree, root: LayoutNodeId, node: LayoutNodeId) -> f32 {
    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(800.0),
            height: AvailableSpace::Definite(600.0),
        },
    );
    tree.get_absolute_bounds(node).expect("laid out").width
}

#[test]
fn updated_content_is_measured_again() {
    let mut tree = LayoutTree::new();
    let root = tree.create_node(Style::default());
    let text = tree.create_text_node(Style::default(), context("ab"));
    tree.add_child(root, text);

    let short = width(&mut tree, root, text);
    assert!(tree.update_text(text, |ctx| ctx.content = "abcdefghij".into()));
    let long = width(&mut tree, root, text);

    assert!(long > short * 3.0, "{short} -> {long}");
    assert_eq!(
        tree.text_context(text).map(|ctx| ctx.content.as_str()),
        Some("abcdefghij")
    );
}

#[test]
fn updated_font_size_is_measured_again() {
    let mut tree = LayoutTree::new();
    let root = tree.create_node(Style::default());
    let text = tree.create_text_node(Style::default(), context("abcdef"));
    tree.add_child(root, text);

    let small = width(&mut tree, root, text);
    assert!(tree.update_text(text, |ctx| ctx.font_size = 32.0));
    let large = width(&mut tree, root, text);

    assert!(large > small * 1.5, "{small} -> {large}");
}

#[test]
fn a_node_without_text_is_left_alone() {
    let mut tree = LayoutTree::new();
    let node = tree.create_node(Style::default());

    assert!(!tree.update_text(node, |ctx| ctx.content = "x".into()));
    assert!(tree.text_context(node).is_none());
}
