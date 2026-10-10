//! A CSS-animated subtree is baked into a texture and moved at composite time
//! only when nothing under it is drawn in a pass of its own. Text is, so a
//! panel with text is painted with the animation applied on every frame, and
//! its text moves and fades with it. When the animation stops, the panel is
//! left with the animation's final values.

use blinc_core::Color;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::text::text;

const CSS: &str = r#"
@keyframes pop {
    from { opacity: 0; transform: scale(0.5) translateY(-20px); }
    to   { opacity: 1; transform: scale(1) translateY(0); }
}
.panel { animation: pop 300ms linear; transform-origin: top center; }
"#;

#[derive(Clone, Copy)]
enum Inside {
    Nothing,
    Text,
    Svg,
}

fn scene(inside: Inside) -> (RenderTree, blinc_layout::LayoutNodeId) {
    let panel = div().class("panel").w(120.0).h(60.0).bg(Color::WHITE);
    let panel = match inside {
        Inside::Nothing => panel,
        Inside::Text => panel.child(text("Item")),
        Inside::Svg => panel.child(
            blinc_layout::svg::svg(r#"<svg viewBox="0 0 4 4"><rect width="4" height="4"/></svg>"#)
                .size(16.0, 16.0),
        ),
    };
    let ui = div().w(400.0).h(300.0).child(panel);
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(CSS).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    tree.start_all_css_animations();
    tree.css_anim_store().lock().unwrap().tick(100.0);
    // Painted the way the runner paints, as the fast-path gate only looks at
    // nodes the last paint reached.
    let animations = std::sync::Arc::new(std::sync::Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    ));
    let rs = blinc_layout::render_state::RenderState::new(animations);
    let mut ctx = blinc_core::RecordingContext::new(blinc_core::Size::new(400.0, 300.0));
    tree.render_with_motion(&mut ctx, &rs);
    let panel = tree.layout_tree.children(tree.root().unwrap())[0];
    tree.compute_animation_status();
    (tree, panel)
}

#[test]
fn a_panel_with_text_is_painted_with_the_animation_not_baked() {
    let (mut tree, panel) = scene(Inside::Text);
    assert!(
        !tree.composite_promotion().contains(&panel),
        "a panel with text was baked, so its text would not move with it"
    );
    assert!(
        !tree.css_active_all_composite_promotable(),
        "the fast path would skip painting a panel that is not baked"
    );
    tree.apply_all_css_animation_props();
    let props = &tree.get_render_node(panel).unwrap().props;
    assert!(
        props.opacity < 1.0 && props.transform.is_some(),
        "the animation was not applied to the panel it no longer bakes"
    );
}

#[test]
fn a_panel_with_nothing_drawn_apart_is_still_baked() {
    let (tree, panel) = scene(Inside::Nothing);
    assert!(tree.composite_promotion().contains(&panel));
    assert!(tree.css_active_all_composite_promotable());
}

#[test]
fn a_panel_with_an_svg_is_not_baked_either() {
    let (tree, panel) = scene(Inside::Svg);
    assert!(!tree.composite_promotion().contains(&panel));
}

#[test]
fn a_panel_with_text_is_painted_again_rather_than_patched() {
    let (tree, _) = scene(Inside::Text);
    assert!(
        !tree.css_active_patchable(),
        "a patch would keep the text as the last paint drew it"
    );
    let (tree, _) = scene(Inside::Nothing);
    assert!(tree.css_active_patchable());
}

#[test]
fn a_finished_animation_leaves_its_final_values() {
    let (mut tree, panel) = scene(Inside::Text);
    tree.apply_all_css_animation_props();
    assert!(tree.get_render_node(panel).unwrap().props.opacity < 0.5);

    // The rest of the animation, in one step past its end.
    for _ in 0..10 {
        tree.css_anim_store().lock().unwrap().tick(50.0);
    }
    assert!(
        tree.css_has_active(),
        "the runner would stop before the final values are written"
    );
    tree.apply_all_css_animation_props();
    let props = &tree.get_render_node(panel).unwrap().props;
    assert_eq!(
        props.opacity, 1.0,
        "the panel kept a frame from mid-animation"
    );
    if let Some(blinc_core::Transform::Affine2D(affine)) = &props.transform {
        let [a, b, c, d, _, ty] = affine.elements;
        assert!(
            (a - 1.0).abs() < 1e-4 && b.abs() < 1e-4 && c.abs() < 1e-4 && (d - 1.0).abs() < 1e-4,
            "the panel kept a scale from mid-animation: {affine:?}"
        );
        assert!(
            ty.abs() < 1e-3,
            "the panel kept an offset from mid-animation: {ty}"
        );
    }
    assert!(
        !tree.css_has_active(),
        "a settled animation still counts as active"
    );
}
