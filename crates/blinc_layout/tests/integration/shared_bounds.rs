//! A shared visual animation key slides when its element is rebuilt under
//! another parent, and not when the element stays put while its ancestors
//! scroll or move.

use std::sync::{Arc, Mutex};

use blinc_animation::AnimationScheduler;
use blinc_core::reactive::signal;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::visual_animation::VisualAnimationConfig;

fn pill() -> blinc_layout::div::Div {
    div().w_full().h(20.0).animate_bounds(
        VisualAnimationConfig::all()
            .with_key("pill")
            .shared()
            .timed(200, blinc_animation::Easing::Linear),
    )
}

fn frame(tree: &mut RenderTree) {
    tree.process_pending_subtree_rebuilds();
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(300.0, 200.0);
}

#[test]
fn a_shared_key_slides_to_its_new_parent_only() {
    let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
    let on_left = signal(true);
    let on_right = blinc_core::reactive::computed(move |g| !g.get(on_left).unwrap_or(true));
    let above = signal(10.0f32);
    let host = div().w(300.0).h(200.0).child(
        div()
            .w(300.0)
            .h(600.0)
            .flex_col()
            .child(div().w(300.0).h(above))
            .child(
                div()
                    .flex_row()
                    .child(div().w(50.0).h(20.0).show(on_left, pill))
                    .child(div().w(120.0).h(20.0).show(on_right, pill)),
            ),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.set_animations(&scheduler);
    frame(&mut tree);
    frame(&mut tree);
    assert!(!tree.has_active_visual_animations());

    // Scrolled with no layout, as a wheel does, then laid out again.
    let page = tree.root().unwrap();
    tree.set_scroll_offset(page, 0.0, -40.0);
    frame(&mut tree);
    assert!(!tree.has_active_visual_animations(), "a scroll slid it");

    // Everything above it moves down.
    above.set(50.0);
    frame(&mut tree);
    frame(&mut tree);
    assert!(
        !tree.has_active_visual_animations(),
        "moving with its parent slid it"
    );

    on_left.set(false);
    frame(&mut tree);
    frame(&mut tree);
    assert!(
        tree.has_active_visual_animations(),
        "rebuilt under the other parent, it did not slide"
    );
}
