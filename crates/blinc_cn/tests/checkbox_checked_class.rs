//! The stylesheet's `.cn-checkbox--checked` rules follow the checkbox's state
//! in place: they apply when it is checked, stop when it is not, and no
//! subtree is rebuilt.

mod common;

use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use common::*;

const RULE: &str = ".cn-checkbox--checked { background: #123456; border-color: #654321 }";

fn solid(tree: &RenderTree, node: LayoutNodeId) -> [f32; 3] {
    match &tree.get_render_node(node).unwrap().props.background {
        Some(blinc_core::Brush::Solid(c)) => [c.r, c.g, c.b],
        other => panic!("not a solid fill: {other:?}"),
    }
}

fn build(checked: &blinc_core::reactive::State<bool>) -> (RenderTree, LayoutNodeId) {
    let host = div().w(400.0).h(200.0).child(blinc_cn::checkbox(checked));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(Stylesheet::parse(RULE).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 200.0);
    let wrapper = tree.layout_tree.children(tree.root().unwrap())[0];
    let node = tree.layout_tree.children(wrapper)[0];
    (tree, node)
}

fn frame(tree: &mut RenderTree) {
    assert!(
        !blinc_layout::stateful::has_pending_subtree_rebuilds(),
        "a change queued a subtree rebuild"
    );
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(400.0, 200.0);
}

#[test]
fn the_checked_rules_come_and_go_with_the_state() {
    init();
    guarded(|| {
        let checked = bool_state(false);
        let (mut tree, node) = build(&checked);
        let unchecked_fill = solid(&tree, node);
        assert_ne!(
            unchecked_fill,
            [
                0x12 as f32 / 255.0,
                0x34 as f32 / 255.0,
                0x56 as f32 / 255.0
            ]
        );

        checked.set(true);
        frame(&mut tree);
        let fill = solid(&tree, node);
        assert!(
            (fill[0] - 0x12 as f32 / 255.0).abs() < 0.01
                && (fill[2] - 0x56 as f32 / 255.0).abs() < 0.01,
            "the checked rule did not apply: {fill:?}"
        );
        assert!(
            tree.element_registry()
                .has_class(node, "cn-checkbox--checked")
        );

        checked.set(false);
        frame(&mut tree);
        assert_eq!(
            solid(&tree, node),
            unchecked_fill,
            "the rule outlasted the state"
        );
        assert!(
            !tree
                .element_registry()
                .has_class(node, "cn-checkbox--checked")
        );
    });
}

#[test]
fn a_checkbox_that_starts_checked_gets_the_rules_and_loses_them() {
    init();
    guarded(|| {
        // What an unchecked one looks like, for comparison.
        let never = bool_state(false);
        let (never_tree, never_node) = build(&never);
        let unchecked_fill = solid(&never_tree, never_node);

        let checked = bool_state(true);
        let (mut tree, node) = build(&checked);
        let fill = solid(&tree, node);
        assert!((fill[0] - 0x12 as f32 / 255.0).abs() < 0.01, "{fill:?}");

        checked.set(false);
        frame(&mut tree);
        assert_eq!(solid(&tree, node), unchecked_fill);
        assert!(
            !tree
                .element_registry()
                .has_class(node, "cn-checkbox--checked")
        );
    });
}
