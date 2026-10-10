//! A tooltip fades in over the theme's fast duration, growing out of the edge
//! that faces its trigger, and its text fades and scales with it.

mod common;

use blinc_cn::TooltipSide;
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::renderer::RenderTree;
use common::*;

fn sheet() -> Stylesheet {
    let vars = blinc_theme::ThemeState::get().to_css_variable_map();
    Stylesheet::parse_with_variables(blinc_cn::cn_styles::CN_STYLES, &vars).expect("cn styles")
}

/// Hover a tooltip's trigger and mount what the overlay layer then holds,
/// under the cn stylesheet. Returns the tree and the tooltip's node.
fn hover_open(side: TooltipSide) -> (RenderTree, LayoutNodeId) {
    init();
    let widget = blinc_cn::tooltip(|| blinc_layout::div::div().w(80.0).h(24.0))
        .text("Copy")
        .side(side)
        .open_delay_ms(0);
    let mut harness = Harness::new(Box::new(widget), Vec::new(), |_, _, _| {});
    harness.move_to((20.0, 12.0));
    let layer = overlay_stack().lock().unwrap().build_overlay_layer();

    let mut tree = RenderTree::from_element(&layer);
    tree.set_stylesheet(sheet());
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    tree.start_all_css_animations();
    let node = find_class(&tree, "cn-tooltip").expect("hovering opened no tooltip");
    (tree, node)
}

fn find_class(tree: &RenderTree, class: &str) -> Option<LayoutNodeId> {
    let registry = tree.element_registry();
    let mut stack = vec![tree.root()?];
    while let Some(node) = stack.pop() {
        if registry.has_class(node, class) {
            return Some(node);
        }
        stack.extend(tree.layout_tree.children(node));
    }
    None
}

#[test]
fn the_tooltip_enter_uses_the_themes_fast_duration() {
    guarded(|| {
        init();
        let sheet = sheet();
        let style = sheet.get_class("cn-tooltip").expect("no .cn-tooltip rule");
        let anim = style
            .animation
            .as_ref()
            .expect("the tooltip does not animate in");
        assert_eq!(anim.name, "cn-tooltip-enter");
        let fast = blinc_theme::ThemeState::get().animations().duration_fast;
        assert_eq!(
            anim.duration_ms as u64, fast,
            "not the theme's fast duration"
        );
    })
}

#[test]
fn a_tooltip_grows_out_of_the_edge_facing_its_trigger() {
    guarded(|| {
        for (side, origin) in [
            (TooltipSide::Top, [50.0, 100.0]),
            (TooltipSide::Bottom, [50.0, 0.0]),
            (TooltipSide::Left, [100.0, 50.0]),
            (TooltipSide::Right, [0.0, 50.0]),
        ] {
            let (tree, node) = hover_open(side);
            let props = &tree.get_render_node(node).unwrap().props;
            assert_eq!(props.transform_origin, Some(origin), "{side:?}");
            overlay_stack().lock().unwrap().close_all();
        }
    })
}

#[test]
fn a_tooltip_fades_in_with_its_text_and_settles_opaque() {
    guarded(|| {
        let (mut tree, node) = hover_open(TooltipSide::Top);
        let fast = blinc_theme::ThemeState::get().animations().duration_fast as f32;

        tree.css_anim_store().lock().unwrap().tick(fast / 2.0);
        tree.apply_all_css_animation_props();
        let props = &tree.get_render_node(node).unwrap().props;
        assert!(
            props.opacity > 0.0 && props.opacity < 1.0,
            "half way in, the tooltip's opacity is {}",
            props.opacity
        );
        assert!(
            !tree.css_active_patchable(),
            "the tooltip's text would keep the alpha of the frame it was first painted in"
        );

        for _ in 0..10 {
            tree.css_anim_store().lock().unwrap().tick(fast / 4.0);
        }
        tree.apply_all_css_animation_props();
        assert_eq!(tree.get_render_node(node).unwrap().props.opacity, 1.0);
        assert!(!tree.css_has_active());
    })
}
