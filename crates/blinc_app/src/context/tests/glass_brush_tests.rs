//! Glass brushes and materials share element ownership and sampling scope.
use super::glass_group_tests::{frame, material, state};
use super::opacity_group_tests::pixel;
use super::*;
use blinc_core::GlassStyle;

fn style(simple: bool) -> GlassStyle {
    GlassStyle::new()
        .with_simple(simple)
        .blur(0.0)
        .border(0.0)
        .noise(0.0)
        .tint(Color::rgba(1.0, 1.0, 0.0, 1.0))
}

fn glass(panel: Div, brush: bool, simple: bool) -> Div {
    if brush {
        panel.background(style(simple))
    } else {
        panel.effect(material(simple))
    }
}

fn scene(outer_brush: bool, inner_brush: bool, simple: bool) -> RenderTree {
    let mut tree = RenderTree::from_element(
        &div().w(160.0).h(120.0).bg(Color::BLUE).child(glass(
            div()
                .id("outer")
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(120.0)
                .h(100.0)
                .opacity(0.5)
                .child(glass(
                    div()
                        .id("inner")
                        .absolute()
                        .left(20.0)
                        .top(20.0)
                        .w(60.0)
                        .h(60.0)
                        .opacity(0.5)
                        .child(
                            blinc_layout::image(super::overlay_group_tests::IMAGE)
                                .size(60.0, 60.0)
                                .fade_in(std::time::Duration::ZERO),
                        ),
                    inner_brush,
                    simple,
                )),
            outer_brush,
            simple,
        )),
    );
    tree.compute_layout(160.0, 120.0);
    tree
}

#[test]
fn glass_brush_capture_derives_nesting_for_both_fluent_forms() {
    super::opacity_group_tests::init_theme();
    for outer in [false, true] {
        for inner in [false, true] {
            let tree = scene(outer, inner, true);
            assert!(tree.has_glass());
            let mut paint = GpuPaintContext::new(160.0, 120.0);
            paint.enable_deferred_text();
            paint.enable_deferred_svg();
            paint.enable_deferred_image();
            paint.enable_glass_layers();
            tree.render_snapshot(&mut paint);
            let batch = paint.take_batch();
            let flags: Vec<_> = batch
                .layer_commands
                .iter()
                .chain(&batch.foreground_layer_commands)
                .filter_map(|entry| match &entry.command {
                    blinc_gpu::LayerCommand::Glass(glass) => Some(glass.nested),
                    _ => None,
                })
                .collect();
            assert_eq!(flags, vec![false, true], "outer={outer}, inner={inner}");
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_brush_nested_pixels_match_materials_and_recover_after_zero() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        for simple in [true, false] {
            let reference = frame(&mut context, &scene(false, false, simple), &state, mode);
            for outer in [false, true] {
                for inner in [false, true] {
                    let mut tree = scene(outer, inner, simple);
                    let node = tree.query_by_id("outer").unwrap();
                    for opacity in [0.5, 0.0, 0.5] {
                        tree.update_render_props(node, |p| p.opacity = opacity);
                        context.invalidate_render_cache();
                        for _ in 0..2 {
                            let page = frame(&mut context, &tree, &state, mode);
                            if opacity == 0.0 {
                                pixel(&page, (60, 60), [0, 0, 255, 255]);
                            } else {
                                assert_eq!(
                                    page, reference,
                                    "mode={mode}, outer={outer}, inner={inner}, simple={simple}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn glass_brush_configuration_changes_are_visual_updates() {
    use blinc_layout::diff::{DivHash, diff, render_props_eq};
    let original = style(true);
    for change in 0..4 {
        let mut changed = original;
        match change {
            0 => changed.simple = false,
            1 => changed.border_color = Some(Color::RED),
            2 => changed.border_thickness = 2.0,
            _ => changed.depth = 1,
        }
        let old = div().background(original);
        let new = div().background(changed);
        assert_ne!(DivHash::compute(&old), DivHash::compute(&new));
        assert!(diff(&old, &new).changes.visual);
        assert!(!render_props_eq(&old.render_props(), &new.render_props()));
    }
    let old = div().effect(material(true));
    for changed in [
        material(false),
        material(true).border(2.0),
        material(true).shadow(blinc_layout::MaterialShadow::md()),
    ] {
        let new = div().effect(changed);
        assert_ne!(DivHash::compute(&old), DivHash::compute(&new));
        assert!(diff(&old, &new).changes.visual);
        assert!(!render_props_eq(&old.render_props(), &new.render_props()));
    }
}

#[test]
fn glass_brush_backdrop_animation_preserves_authored_style() {
    use blinc_animation::MultiKeyframeAnimation;
    use blinc_layout::render_state::ActiveCssAnimation;
    super::opacity_group_tests::init_theme();
    for transition in [false, true] {
        let mut tree = scene(true, true, false);
        let node = tree.query_by_id("inner").unwrap();
        let id = tree.stable_id(node).unwrap();
        {
            let store = tree.css_anim_store();
            let mut store = store.lock().unwrap();
            let entries = if transition {
                &mut store.transitions
            } else {
                &mut store.animations
            };
            let mut animation = ActiveCssAnimation::new(MultiKeyframeAnimation::new(1000));
            animation.current_properties.backdrop_blur = Some(7.0);
            animation.current_properties.backdrop_saturation = Some(0.75);
            animation.current_properties.backdrop_brightness = Some(0.8);
            entries.insert(id, animation);
        }
        tree.apply_all_css_animation_props();
        tree.apply_all_css_transition_props();
        let props = &tree.get_render_node(node).unwrap().props;
        assert!(props.material.is_none());
        let Some(Brush::Glass(glass)) = &props.background else {
            panic!("Animated glass must retain its background brush");
        };
        assert_eq!(glass.blur, 7.0);
        assert_eq!(glass.saturation, 0.75);
        assert_eq!(glass.brightness, 0.8);
        assert_eq!(glass.tint, style(false).tint);
        assert!(!glass.simple);
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_brush_border_shadow_and_clip_match_explicit_element_styles() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    let shadow = blinc_core::Shadow {
        offset_x: 3.0,
        offset_y: 2.0,
        blur: 2.0,
        spread: 0.0,
        color: Color::rgba(1.0, 0.0, 0.0, 0.5),
    };
    for mode in 0..4 {
        for simple in [false, true] {
            let build = |brush| {
                let panel = div()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(120.0)
                    .h(100.0)
                    .rounded(8.0)
                    .opacity(0.5)
                    .child(text("Glass"));
                let panel = if brush {
                    let mut brush = style(simple).blur(3.0).border(2.0).shadow(shadow);
                    brush.border_color = Some(Color::RED);
                    panel.background(brush)
                } else {
                    panel
                        .effect(material(simple).blur(3.0).border(2.0))
                        .shadow(shadow)
                        .border_color(Color::RED)
                };
                let mut tree =
                    RenderTree::from_element(&div().w(160.0).h(120.0).bg(Color::BLUE).child(panel));
                tree.compute_layout(160.0, 120.0);
                tree
            };
            let reference = frame(&mut context, &build(false), &state, mode);
            let actual = frame(&mut context, &build(true), &state, mode);
            assert_eq!(actual, reference, "mode={mode}, simple={simple}");
        }
    }
}

#[test]
fn glass_brush_unsupported_owners_keep_fallback() {
    super::opacity_group_tests::init_theme();
    for boundary in 0..6 {
        let mut tree = scene(true, true, true);
        let outer = tree.query_by_id("outer").unwrap();
        let inner = tree.query_by_id("inner").unwrap();
        match boundary {
            0 => tree.update_render_props(outer, |p| p.rotate_x = Some(20.0)),
            1 => tree.update_render_props(inner, |p| p.rotate_y = Some(20.0)),
            2 => tree.update_render_props(outer, |p| p.is_overlay_root = true),
            3 => tree.update_render_props(tree.root().unwrap(), |p| p.is_overlay_root = true),
            _ => {}
        }
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        paint.enable_deferred_text();
        paint.enable_deferred_svg();
        if boundary != 4 {
            paint.enable_deferred_image();
        }
        if boundary != 5 {
            paint.enable_glass_layers();
        }
        tree.render_snapshot(&mut paint);
        let batch = paint.take_batch();
        assert!(
            !batch
                .layer_commands
                .iter()
                .chain(&batch.foreground_layer_commands)
                .any(|entry| matches!(entry.command, blinc_gpu::LayerCommand::Glass(_))),
            "boundary={boundary}"
        );
    }
}

#[test]
fn glass_brush_simple_and_layered_painters_match_material_commands() {
    super::opacity_group_tests::init_theme();
    for walker in 0..3 {
        let commands = |brush| {
            let tree = scene(brush, brush, true);
            let mut paint = GpuPaintContext::new(160.0, 120.0);
            let mut glass = GpuPaintContext::new(160.0, 120.0);
            let mut foreground = GpuPaintContext::new(160.0, 120.0);
            match walker {
                0 => tree.render(&mut paint),
                1 => tree.render_layered_simple(&mut paint),
                _ => tree.render_layered(&mut paint, &mut glass, &mut foreground),
            }
            let batches = [
                paint.take_batch(),
                glass.take_batch(),
                foreground.take_batch(),
            ];
            let mut bytes = Vec::new();
            for batch in batches {
                bytes.extend_from_slice(bytemuck::cast_slice::<_, u8>(&batch.glass_primitives));
                bytes.extend_from_slice(bytemuck::cast_slice::<_, u8>(
                    &batch.nested_glass_primitives,
                ));
            }
            assert!(!bytes.is_empty());
            bytes
        };
        assert_eq!(commands(true), commands(false), "walker={walker}");
    }
}
