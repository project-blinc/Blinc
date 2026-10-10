//! A class rule's `animation:` starts when the node is built and plays.

use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

const CSS: &str = r#"
@keyframes fade { from { opacity: 0 } to { opacity: 1 } }
.menu { animation: fade 300ms linear; }
"#;

#[test]
fn a_class_animation_starts_and_plays() {
    let ui = div().w(200.0).child(div().class("menu").w(50.0).h(20.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(CSS).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.start_all_css_animations();
    tree.compute_layout(200.0, 100.0);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];

    assert!(tree.has_css_animation(node), "the animation did not start");

    // Tick, and the animation's opacity follows the keyframes.
    let store = tree.css_anim_store();
    store.lock().unwrap().tick(150.0);
    let props = tree
        .get_css_animation_properties(node)
        .expect("the animation has properties");
    let opacity = props.opacity.expect("opacity is animated");
    assert!(
        opacity > 0.1 && opacity < 0.9,
        "half way through, the opacity is {opacity}"
    );
}

#[test]
fn a_class_animation_starts_on_a_node_that_arrives_in_a_subtree_rebuild() {
    use blinc_layout::stateful::queue_subtree_rebuild;
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let ui = div().w(200.0).h(100.0).child(div().w(10.0).h(10.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(CSS).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(200.0, 100.0);
    let host = tree.layout_tree.children(tree.root().unwrap())[0];

    // What an overlay does: the content comes as the new child of a node.
    queue_subtree_rebuild(host, div().child(div().class("menu").w(50.0).h(20.0)));
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(200.0, 100.0);
    tree.start_all_css_animations();

    let menu = tree.layout_tree.children(host)[0];
    assert!(
        tree.has_css_animation(menu),
        "the animation did not start for a node that came in a rebuild"
    );
}

/// The windowed loop keeps rendering only while an animation is on a node the
/// last paint reached, so a playing animation has to show up as painted.
#[test]
fn a_playing_class_animation_is_on_a_painted_node() {
    use blinc_core::{RecordingContext, Size};
    use blinc_layout::render_state::RenderState;
    use std::sync::{Arc, Mutex};

    let ui = div().w(200.0).child(div().class("menu").w(50.0).h(20.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(CSS).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(200.0, 100.0);
    tree.start_all_css_animations();

    let animations = Arc::new(Mutex::new(blinc_animation::AnimationScheduler::new()));
    let mut rs = RenderState::new(animations);
    rs.begin_stable_motion_frame();
    tree.initialize_motion_animations(&mut rs);
    rs.end_stable_motion_frame();
    let mut ctx = RecordingContext::new(Size::new(200.0, 100.0));
    tree.render_with_motion(&mut ctx, &rs);

    let painted = tree.painted_stable_ids();
    let store = tree.css_anim_store();
    assert!(
        store.lock().unwrap().has_visible_active(&painted),
        "a playing animation is not on a painted node, so nothing keeps the frame loop awake"
    );
}
