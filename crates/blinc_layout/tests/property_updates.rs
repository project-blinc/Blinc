//! The property channel's drain: queued writes land in the tree and their
//! side effects come back combined, so a runner can decide whether layout
//! has to run.

use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::{LayoutNodeId, PartialPropertyUpdate, PropertyId, SideEffects};

fn visual(node: LayoutNodeId, opacity: f32) -> PartialPropertyUpdate {
    PartialPropertyUpdate {
        node_id: node,
        property: PropertyId::Opacity,
        effects: SideEffects::VISUAL,
        render_write: Some(Box::new(move |p| p.opacity = opacity)),
        layout_write: None,
    }
}

fn width(node: LayoutNodeId, px: f32) -> PartialPropertyUpdate {
    PartialPropertyUpdate {
        node_id: node,
        property: PropertyId::Width,
        effects: SideEffects::LAYOUT,
        render_write: None,
        layout_write: Some(Box::new(move |s| {
            s.size.width = taffy::style_helpers::length(px)
        })),
    }
}

fn tree() -> RenderTree {
    RenderTree::from_element(&div().w(100.0).h(50.0))
}

#[test]
fn a_visual_and_a_layout_write_both_land() {
    let mut tree = tree();
    let node = tree.root().expect("root");

    let effects = tree.apply_partial_property_updates(vec![visual(node, 0.5), width(node, 80.0)]);

    assert_eq!(tree.get_render_node(node).unwrap().props.opacity, 0.5);
    let style = tree.layout_tree.get_style(node).expect("style");
    assert_eq!(
        style.size.width,
        taffy::style_helpers::length::<_, taffy::Dimension>(80.0)
    );
    assert!(
        effects.needs_layout,
        "the layout write has to ask for layout"
    );
}

#[test]
fn effects_combine_across_updates() {
    let mut tree = tree();
    let node = tree.root().expect("root");

    // Visual only: no layout asked for.
    let only_visual = tree.apply_partial_property_updates(vec![visual(node, 0.25)]);
    assert_eq!(only_visual, SideEffects::VISUAL);

    // Any one layout update makes the whole batch need layout.
    let mixed = tree.apply_partial_property_updates(vec![
        visual(node, 0.75),
        width(node, 60.0),
        visual(node, 1.0),
    ]);
    assert!(mixed.needs_layout);

    // And an empty batch asks for nothing.
    assert_eq!(
        tree.apply_partial_property_updates(Vec::new()),
        SideEffects::default()
    );
}

#[test]
fn an_update_for_a_node_that_is_gone_is_skipped() {
    let mut tree = tree();
    let node = tree.root().expect("root");
    let gone = LayoutNodeId::default();

    // Its effects still count, so a runner that was told layout was needed
    // runs it; the write itself has nowhere to go and must not panic.
    let effects = tree.apply_partial_property_updates(vec![width(gone, 10.0), visual(node, 0.5)]);

    assert!(effects.needs_layout);
    assert_eq!(tree.get_render_node(node).unwrap().props.opacity, 0.5);
}
