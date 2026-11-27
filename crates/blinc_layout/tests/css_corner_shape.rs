//! An animated corner shape is drawn as the animation gives it, and a
//! transition from a node with no shape of its own starts at the theme's.

use blinc_core::CornerShape;
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::element::RenderProps;
use blinc_layout::event_router::EventRouter;
use blinc_layout::renderer::RenderTree;

fn scene(css: &str) -> (RenderTree, LayoutNodeId) {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let ui = div()
        .w(400.0)
        .h(300.0)
        .child(div().class("b").w(100.0).h(100.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    (tree, node)
}

fn props(tree: &RenderTree, node: LayoutNodeId) -> RenderProps {
    tree.get_render_node(node).unwrap().props.clone()
}

fn tick(tree: &mut RenderTree, ms: f32) {
    let mut left = ms;
    while left > 0.0 {
        let step = left.min(50.0);
        tree.css_anim_store().lock().unwrap().tick(step);
        tree.apply_all_css_animation_props();
        tree.apply_all_css_transition_props();
        left -= step;
    }
}

/// Plays `keyframes` once with `fill` on a node that sets no shape; the shape
/// at the start, and whether it is kept from the theme then and at the end.
fn play(keyframes: &str, fill: &str) -> (CornerShape, bool, bool) {
    let (mut tree, node) = scene(&format!(
        "@keyframes morph {{ {keyframes} }}
         .b {{ border-radius: 20px; animation: morph 200ms linear {fill}; }}"
    ));
    tree.start_css_animation_for_class(node, "b");
    tree.apply_all_css_animation_props();
    let start = props(&tree, node);
    tick(&mut tree, 400.0);
    (
        start.corner_shape,
        start.keeps_corner_shape(),
        props(&tree, node).keeps_corner_shape(),
    )
}

#[test]
fn a_keyframe_at_round_is_drawn_round() {
    let from_round = "from { corner-shape: 1; } to { corner-shape: 2; }";
    let (start, kept_at_start, _) = play(from_round, "none");
    assert!(start.is_round() && kept_at_start, "{start:?}");
}

#[test]
fn a_finished_animation_holds_its_shape_only_when_it_fills_forwards() {
    let to_round = "from { corner-shape: 2; } to { corner-shape: 1; }";
    assert!(play(to_round, "forwards").2);
    assert!(!play(to_round, "none").2);
}

#[test]
fn a_transition_eases_from_and_to_the_theme_shape() {
    let (mut tree, node) = scene(
        ".b { border-radius: 20px; transition: corner-shape 200ms linear; }
         .b:hover { corner-shape: 2; }",
    );
    let mut router = EventRouter::new();
    let mut shape_after = |tree: &mut RenderTree, x: f32| {
        let _ = router.on_mouse_move(tree, x, 50.0);
        tree.apply_stylesheet_state_styles(&router);
        tree.apply_all_css_transition_props();
        let start = props(tree, node);
        assert!(start.keeps_corner_shape(), "{:?}", start.corner_shape);
        tick(tree, 400.0);
        (start.corner_shape, props(tree, node).corner_shape)
    };

    let (start, hovered) = shape_after(&mut tree, 50.0);
    assert!(!start.is_round() && start.top_left < 2.0, "{start:?}");
    assert_eq!(hovered, CornerShape::uniform(2.0));

    let (back_from, left) = shape_after(&mut tree, 350.0);
    assert_eq!(back_from, CornerShape::uniform(2.0));
    assert_eq!(left, start, "it ends on the theme's shape");
}
