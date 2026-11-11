//! A collapsible is built once: its content opens and shuts in place when
//! the bound signal changes, from anywhere, by its `data-state` rule, and its
//! trigger's chevron and fill follow rules, with no subtree rebuilt.

mod common;

use blinc_core::reactive::{Signal, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::RenderTree;
use common::*;

fn tree_of(host: &blinc_layout::div::Div) -> RenderTree {
    let vars = blinc_theme::ThemeState::get().to_css_variable_map();
    let mut tree = RenderTree::from_element(host);
    tree.set_stylesheet(
        Stylesheet::parse_with_variables(blinc_cn::cn_styles::CN_STYLES, &vars).expect("css"),
    );
    tree.apply_all_stylesheet_styles();
    tree.compute_layout(400.0, 400.0);
    tree
}

fn frame(tree: &mut RenderTree) {
    assert!(
        !blinc_layout::stateful::has_pending_subtree_rebuilds(),
        "the collapsible queued a subtree rebuild"
    );
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    // A runner applies the layout rules again whenever layout is needed.
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(400.0, 400.0);
}

fn find(tree: &RenderTree, class: &str) -> LayoutNodeId {
    let registry = tree.element_registry();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if registry.has_class(node, class) {
            return node;
        }
        stack.extend(tree.layout_tree.children(node));
    }
    panic!("no `{class}`");
}

fn layout_height(tree: &RenderTree, node: LayoutNodeId) -> f32 {
    tree.layout_tree
        .get_bounds(node, (0.0, 0.0))
        .unwrap()
        .height
}

fn section(open: Signal<bool>) -> RenderTree {
    let section = blinc_cn::collapsible(open).content(|| div().w_full().h(80.0));
    tree_of(&div().w(400.0).h(400.0).flex_col().child(section))
}

#[test]
fn the_content_follows_its_signal_in_place() {
    guarded(|| {
        init();
        let open = signal(false);
        let mut tree = section(open);
        let body = find(&tree, "cn-collapsible-content");
        assert_eq!(
            layout_height(&tree, body),
            0.0,
            "a closed section has height"
        );

        open.set(true);
        frame(&mut tree);
        assert_eq!(layout_height(&tree, body), 80.0, "the section did not open");

        open.set(false);
        frame(&mut tree);
        assert_eq!(layout_height(&tree, body), 0.0, "the section did not shut");
    })
}

#[test]
fn the_content_opens_again_after_it_has_shut() {
    guarded(|| {
        init();
        let open = signal(false);
        let mut tree = section(open);
        let body = find(&tree, "cn-collapsible-content");
        for round in 0..3 {
            open.set(true);
            frame(&mut tree);
            assert_eq!(layout_height(&tree, body), 80.0, "round {round}: opening");
            open.set(false);
            frame(&mut tree);
            assert_eq!(layout_height(&tree, body), 0.0, "round {round}: shutting");
        }
    })
}

#[test]
fn an_initially_open_section_starts_open() {
    guarded(|| {
        init();
        let tree = section(signal(true));
        let body = find(&tree, "cn-collapsible-content");
        assert_eq!(layout_height(&tree, body), 80.0);
    })
}

#[test]
fn the_trigger_turns_its_chevron_and_follows_the_pointer() {
    guarded(|| {
        init();
        let open = signal(false);
        let section = blinc_cn::collapsible_section("Details", open, || div().w_full().h(40.0));
        let mut tree = tree_of(&div().w(400.0).h(400.0).child(section));
        let trigger = find(&tree, "cn-collapsible-trigger");
        let fill = |tree: &RenderTree| {
            format!(
                "{:?}",
                tree.get_render_node(trigger).unwrap().props.background
            )
        };
        // The chevron turns by a `[data-state="open"]` rule.
        let open_attr = |tree: &RenderTree| {
            tree.element_registry()
                .has_class(trigger, "data-state=open")
        };
        let rest_fill = fill(&tree);
        assert!(!open_attr(&tree));

        let b = tree.get_absolute_bounds(trigger).unwrap();
        let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
        let mut router = EventRouter::new();
        for (node, event) in router.on_mouse_move(&tree, x, y) {
            tree.dispatch_event(node, event, x, y);
        }
        frame(&mut tree);
        tree.apply_stylesheet_state_styles(&router);
        assert_ne!(fill(&tree), rest_fill, "hovering did not change the fill");

        let mut events = router.on_mouse_down(&tree, x, y, MouseButton::Left);
        events.extend(router.on_mouse_up(&tree, x, y, MouseButton::Left));
        for (node, event) in events {
            tree.dispatch_event(node, event, x, y);
        }
        frame(&mut tree);
        assert!(open.get(), "clicking the trigger did not open the section");
        assert!(open_attr(&tree), "the trigger is not marked open");
    })
}
