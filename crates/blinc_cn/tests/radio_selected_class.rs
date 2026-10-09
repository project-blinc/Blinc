//! The stylesheet's `.cn-radio--selected` rules follow the selection in
//! place: they move from the option that loses it to the one that gains it,
//! and no subtree is rebuilt.

mod common;

use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use common::*;

const RULE: &str = ".cn-radio--selected { background: #123456 }";

fn fill(tree: &RenderTree, node: LayoutNodeId) -> Option<[f32; 3]> {
    match &tree.get_render_node(node).unwrap().props.background {
        Some(blinc_core::Brush::Solid(c)) if c.a > 0.0 => Some([c.r, c.g, c.b]),
        _ => None,
    }
}

fn is_rule_colour(fill: Option<[f32; 3]>) -> bool {
    fill.is_some_and(|c| {
        (c[0] - 0x12 as f32 / 255.0).abs() < 0.01 && (c[2] - 0x56 as f32 / 255.0).abs() < 0.01
    })
}

#[test]
fn the_selected_rules_move_with_the_selection() {
    init();
    guarded(|| {
        let selected = string_state("a");
        let host = div().w(400.0).h(200.0).child(
            blinc_cn::radio_group(&selected)
                .option("a", "Alpha")
                .option("b", "Beta"),
        );
        let mut tree = RenderTree::from_element(&host);
        tree.set_stylesheet(Stylesheet::parse(RULE).expect("css"));
        tree.apply_stylesheet_layout_overrides();
        tree.apply_stylesheet_base_styles();
        tree.compute_layout(400.0, 200.0);

        let group = tree.layout_tree.children(tree.root().unwrap())[0];
        let rows = tree.layout_tree.children(group);
        let circle = |i: usize| tree.layout_tree.children(rows[i])[0];
        let (a, b) = (circle(0), circle(1));
        assert!(
            is_rule_colour(fill(&tree, a)),
            "the selected option has the rule"
        );
        assert!(!is_rule_colour(fill(&tree, b)));

        selected.set("b".to_string());
        assert!(!blinc_layout::stateful::has_pending_subtree_rebuilds());
        let updates = blinc_layout::take_pending_partial_prop_updates();
        tree.apply_partial_property_updates(updates);
        tree.compute_layout(400.0, 200.0);

        assert!(
            !is_rule_colour(fill(&tree, a)),
            "the rule outlasted the selection"
        );
        assert!(
            is_rule_colour(fill(&tree, b)),
            "the new selection did not get the rule"
        );
        assert!(!tree.element_registry().has_class(a, "cn-radio--selected"));
        assert!(tree.element_registry().has_class(b, "cn-radio--selected"));
    });
}
