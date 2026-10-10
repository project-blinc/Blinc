//! The stylesheet's `.cn-tabs-trigger--active` rules move with the selected
//! tab in place: nothing is rebuilt when the selection changes.

mod common;

use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::text::text;
use common::*;

const RULE: &str = ".cn-tabs-trigger--active { background: #123456 }";

fn rule_applies(tree: &RenderTree, node: LayoutNodeId) -> bool {
    match &tree.get_render_node(node).unwrap().props.background {
        Some(blinc_core::Brush::Solid(c)) => {
            (c.r - 0x12 as f32 / 255.0).abs() < 0.01 && (c.b - 0x56 as f32 / 255.0).abs() < 0.01
        }
        _ => false,
    }
}

#[test]
fn the_active_rules_move_with_the_selected_tab() {
    init();
    guarded(|| {
        let selected = string_state("a");
        let host = div().w(400.0).h(200.0).child(
            blinc_cn::tabs(&selected)
                .transition(blinc_cn::TabsTransition::None)
                .tab("a", "Alpha", || div().child(text("alpha")))
                .tab("b", "Beta", || div().child(text("beta"))),
        );
        let mut tree = RenderTree::from_element(&host);
        tree.set_stylesheet(Stylesheet::parse(RULE).expect("css"));
        tree.apply_stylesheet_layout_overrides();
        tree.apply_stylesheet_base_styles();
        tree.compute_layout(400.0, 200.0);

        let tabs = tree.layout_tree.children(tree.root().unwrap())[0];
        let strip = tree.layout_tree.children(tabs)[0];
        let triggers = tree.layout_tree.children(strip);
        assert!(
            rule_applies(&tree, triggers[0]),
            "the selected tab has the rule"
        );
        assert!(!rule_applies(&tree, triggers[1]));

        selected.set("b".to_string());
        assert!(blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty());
        let updates = blinc_layout::take_pending_partial_prop_updates();
        tree.apply_partial_property_updates(updates);
        tree.compute_layout(400.0, 200.0);

        assert!(
            !rule_applies(&tree, triggers[0]),
            "the rule outlasted the selection"
        );
        assert!(
            rule_applies(&tree, triggers[1]),
            "the new tab did not get the rule"
        );
    });
}
