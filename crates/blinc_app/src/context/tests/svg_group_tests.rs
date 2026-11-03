use super::grouped_text_tests::frame;
use super::opacity_group_tests::pixel;
use super::separate_pass_flow;
use super::*;
use blinc_animation::AnimationScheduler;
use blinc_layout::svg;

const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect width="70" height="100" fill="#00ff00"/><circle cx="80" cy="50" r="18" fill="#ff00ff" opacity="0.5"/></svg>"##;

use super::opacity_group_tests::{CssSource, css_value};
use blinc_animation::{AnimatedValue, SharedAnimatedValue};

#[derive(Clone, Copy, Debug)]
enum Source {
    Static,
    Binding,
    Css(CssSource),
}

fn scene(
    foreground: bool,
    style: bool,
    bindings: Option<(SharedAnimatedValue, SharedAnimatedValue)>,
) -> RenderTree {
    let source = if style {
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><path d="M0 0H70V100H0Z" fill="currentColor"/><circle cx="80" cy="50" r="18" fill="currentColor" opacity="0.5"/></svg>"##
    } else {
        ICON
    };
    let icon = svg(source).id("icon").size(100.0, 100.0);
    let icon = if style {
        icon.tint(Color::rgba(0.1, 0.8, 0.6, 0.7))
    } else {
        icon
    };
    let icon = if foreground { icon.foreground() } else { icon };
    let owner = div()
        .id("group")
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(140.0)
        .h(130.0)
        .bg(Color::RED)
        .child(icon);
    let owner = if style {
        owner.child(
            div()
                .absolute()
                .left(25.0)
                .top(25.0)
                .child(text("M").id("mixed").size(24.0).color(Color::WHITE)),
        )
    } else {
        owner
    };
    let ui = div().w(240.0).h(180.0).bg(Color::BLUE);
    let ui = if let Some((alpha, x)) = bindings {
        ui.child(owner.motion().opacity(alpha).translate_x(x))
    } else {
        ui.child(owner)
    };
    let ui = ui
        .child(
            div()
                .absolute()
                .left(30.0)
                .top(30.0)
                .w(5.0)
                .h(5.0)
                .bg(Color::YELLOW),
        )
        .child(
            div()
                .absolute()
                .left(175.0)
                .top(20.0)
                .child(text("X").size(24.0).color(Color::WHITE)),
        );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(240.0, 180.0);
    if style {
        let group = tree.query_by_id("group").unwrap();
        tree.update_render_props(group, |p| {
            p.transform = Some(blinc_core::Transform::scale(0.9, 1.05));
            p.transform_origin = Some([0.0, 0.0]);
            p.clip_path = Some(blinc_core::ClipPath::Path {
                vertices: vec![(0.0, 0.0), (100.0, 0.0), (0.0, 120.0)],
            });
        });
    }
    tree
}

#[test]
#[ignore] // Requires GPU
fn svg_group_static_and_foreground_composite_once_with_authored_alpha() {
    frames(Source::Static, false);
}

#[test]
#[ignore] // Requires GPU
fn svg_group_tint_overrides_text_order_and_path_clips_keep_owner() {
    frames(Source::Static, true);
}

#[test]
#[ignore] // Requires GPU
fn svg_group_motion_and_css_recover_and_replay_at_opacity_endpoints() {
    for source in [
        Source::Binding,
        Source::Css(CssSource::Animation),
        Source::Css(CssSource::Transition),
    ] {
        frames(source, true);
    }
}

fn frames(source: Source, style: bool) {
    use slotmap::Key as _;
    for compositor in [false, true] {
        for foreground in [false, true] {
            let mut context = super::main_text_clip_tests::context(true, 1);
            let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
            let alpha = Arc::new(Mutex::new(AnimatedValue::with_default(
                scheduler.lock().unwrap().handle(),
                1.0,
            )));
            let x = Arc::new(Mutex::new(AnimatedValue::with_default(
                scheduler.lock().unwrap().handle(),
                1.0,
            )));
            let mut tree = scene(
                foreground,
                style,
                matches!(source, Source::Binding).then(|| (alpha.clone(), x.clone())),
            );
            let group = tree.query_by_id("group").unwrap();
            let icon = tree.query_by_id("icon").unwrap();
            let state = RenderState::new(scheduler);
            let legacy = frame(&mut context, &tree, &state, compositor);
            tree.update_render_props(group, |p| p.opacity = 0.999999);
            context.invalidate_render_cache();
            let captured = frame(&mut context, &tree, &state, compositor);
            tree.update_render_props(group, |p| p.opacity = 1.0);
            for (index, value) in [0.5_f32, 0.0, 0.75, 1.0, 0.5].into_iter().enumerate() {
                let offset = if matches!(source, Source::Binding) {
                    [0, 5, 0, 2, 0][index]
                } else {
                    0
                };
                match source {
                    Source::Static => {
                        tree.update_render_props(group, |p| p.opacity = value);
                        context.invalidate_render_cache();
                    }
                    Source::Binding => {
                        alpha.lock().unwrap().set_immediate(value);
                        x.lock().unwrap().set_immediate(1.0 + offset as f32);
                    }
                    Source::Css(source) => {
                        css_value(&tree, group, source, Some(value));
                        tree.update_render_props(group, |p| p.opacity = value);
                    }
                }
                eprintln!(
                    "{source:?} style={style} fg={foreground} compositor={compositor} alpha={value}"
                );
                let reference = if value == 1.0 && !matches!(source, Source::Binding) {
                    &legacy
                } else {
                    &captured
                };
                let origin = u32::from(matches!(source, Source::Binding));
                for _ in 0..2 {
                    let page = frame(&mut context, &tree, &state, compositor);
                    for y in 25..115 {
                        for px in 25..115 {
                            if (29..36).contains(&px) || (29..36).contains(&y) {
                                continue;
                            }
                            let p = reference.get_pixel(px + origin, y).0;
                            pixel(
                                &page,
                                (px + origin + offset, y),
                                [
                                    (p[0] as f32 * value).round() as u8,
                                    (p[1] as f32 * value).round() as u8,
                                    (p[2] as f32 * value + 255.0 * (1.0 - value)).round() as u8,
                                    255,
                                ],
                            );
                        }
                    }
                    let expected = if value == 1.0 && !matches!(source, Source::Binding) {
                        legacy.get_pixel(32, 32).0
                    } else {
                        [255, 255, 0, 255]
                    };
                    pixel(&page, (32, 32), expected);
                    for py in 20..55 {
                        for px in 175..210 {
                            assert_eq!(page.get_pixel(px, py), legacy.get_pixel(px, py));
                        }
                    }
                    if value < 1.0 || matches!(source, Source::Binding) {
                        let batch = context.cached_bg_batch.as_ref().unwrap();
                        assert!(
                            batch
                                .cross_stream_groups
                                .svg_nodes
                                .contains(&icon.data().as_ffi())
                        );
                        if value > 0.0 {
                            assert_eq!(batch.dynamic_images.len(), 1);
                            if style {
                                assert!(!batch.dynamic_images[0].clip_aux.is_empty());
                            }
                        }
                    }
                }
            }
            if style {
                let previous = context.cached_bg_batch.as_ref().unwrap().dynamic_images[0]
                    .data
                    .clone();
                tree.update_render_props(group, |p| p.opacity = 0.5);
                tree.update_render_props(icon, |p| {
                    p.fill = Some([1.0, 0.5, 0.0, 0.6]);
                    p.stroke = Some([0.0, 1.0, 1.0, 1.0]);
                    p.stroke_width = Some(3.0);
                    p.stroke_dasharray = Some(vec![4.0, 3.0]);
                    p.stroke_dashoffset = Some(2.0);
                    p.svg_path_data = Some("M0 0H60V100H0Z".into());
                    p.svg_tag_styles.insert(
                        "circle".into(),
                        blinc_layout::element::SvgTagStyle {
                            fill: Some([1.0, 0.0, 1.0, 0.5]),
                            opacity: Some(0.4),
                            ..Default::default()
                        },
                    );
                });
                context.invalidate_render_cache();
                let changed = frame(&mut context, &tree, &state, compositor);
                assert_ne!(changed, captured);
                assert_ne!(
                    context.cached_bg_batch.as_ref().unwrap().dynamic_images[0].data,
                    previous
                );
                let cache = context.cached_bg_batch.clone().unwrap();
                context.svg_atlas.clear();
                let (texture, view) = crate::tests::create_test_texture(
                    context.device(),
                    context.texture_format(),
                    240,
                    180,
                );
                context
                    .renderer
                    .render_with_clear(&view, &cache, [0.0, 0.0, 1.0, 1.0]);
                let replay = crate::tests::read_rendered_pixels(
                    context.device(),
                    context.queue(),
                    &texture,
                    240,
                    180,
                );
                for py in 25..115 {
                    for px in 25..115 {
                        pixel(&replay, (px, py), changed.get_pixel(px, py).0);
                    }
                }
            }
        }
    }
}

#[test]
fn svg_group_capabilities_and_unsupported_boundaries_keep_fallback() {
    use blinc_core::{DrawContext, LayerConfig, Rect};
    use blinc_gpu::primitives::LayerCommand;
    use blinc_layout::renderer::{AmbientPaintState, DynamicKind, DynamicRegion};
    use std::collections::HashMap;
    super::opacity_group_tests::init_theme();
    for (enabled, unsupported) in [(false, 0), (true, 0), (true, 1), (true, 2)] {
        let mut tree = scene(false, false, None);
        let group = tree.query_by_id("group").unwrap();
        tree.update_render_props(group, |p| {
            p.opacity = 0.5;
            if unsupported == 1 {
                p.background = Some(blinc_core::ImageBrush::new("placeholder").into());
            } else if unsupported == 2 {
                p.flow_shader = Some(separate_pass_flow());
            }
        });
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        let mut paint = GpuPaintContext::new(240.0, 180.0);
        if enabled {
            paint.enable_deferred_svg();
        }
        tree.render_with_motion(&mut paint, &state);
        let mut batch = paint.take_batch();
        assert_eq!(
            batch.cross_stream_groups.svg_nodes.len(),
            usize::from(enabled && unsupported == 0)
        );
        batch.resolve_deferred_content(&mut HashMap::new());
        assert!(
            batch
                .layer_commands
                .iter()
                .chain(&batch.foreground_layer_commands)
                .all(|e| !matches!(e.command, LayerCommand::DeferredSvg { .. }))
        );
        let region = DynamicRegion {
            root: group,
            screen_aabb: [20.0, 20.0, 140.0, 130.0],
            visit_seq: 0,
            ambient: AmbientPaintState {
                affine: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                opacity: 1.0,
                clip_aabb: None,
                z_layer: 0,
            },
            kind: DynamicKind::MotionSubtree,
        };
        let mut fallback = GpuPaintContext::new(240.0, 180.0);
        tree.render_dynamic_region(&mut fallback, &region, &state);
        assert!(
            fallback
                .take_batch()
                .cross_stream_groups
                .svg_nodes
                .is_empty()
        );
    }
    let mut paint = GpuPaintContext::new(240.0, 180.0);
    paint.enable_deferred_text();
    paint.enable_deferred_svg();
    assert!(paint.push_cross_stream_layer(1, LayerConfig::new().opacity(0.5)));
    assert!(paint.defer_text(2));
    paint.push_opacity(0.4);
    assert!(paint.defer_svg(3));
    paint.pop_opacity();
    paint.fill_rect(
        Rect::new(0.0, 0.0, 1.0, 1.0),
        0.0.into(),
        Color::YELLOW.into(),
    );
    paint.draw_rgba_pixels(&[255; 4], 1, 1, Rect::new(0.0, 0.0, 1.0, 1.0));
    paint.set_foreground_layer(true);
    assert!(paint.defer_svg(4));
    paint.set_foreground_layer(false);
    paint.pop_layer();
    let mut text = PrimitiveBatch::new();
    text.push(GpuPrimitive::rect(0.0, 0.0, 1.0, 1.0).with_color(0.0, 1.0, 0.0, 1.0));
    let mut image = GpuPaintContext::new(240.0, 180.0);
    image.draw_rgba_pixels(&[0, 255, 0, 255], 1, 1, Rect::new(0.0, 0.0, 1.0, 1.0));
    let image = image.take_batch();
    let mut prepared = HashMap::from([(2, text), (3, image.clone()), (4, image)]);
    let mut batch = paint.take_batch();
    batch.resolve_deferred_content(&mut prepared);
    assert!(prepared.is_empty());
    assert_eq!(batch.dynamic_images.len(), 3);
    assert_eq!(batch.dynamic_images[0].opacity, 0.4);
    assert_eq!(
        batch.dynamic_images[0].paint_order.unwrap().primitive_index,
        1
    );
    assert_eq!(
        batch.dynamic_images[1].paint_order.unwrap().primitive_index,
        2
    );
    assert_eq!(
        batch.dynamic_images[2].paint_order.unwrap().primitive_index,
        2
    );
    assert_eq!(batch.cross_stream_groups.svg_nodes.len(), 2);
    batch.clear();
    assert!(batch.cross_stream_groups.svg_nodes.is_empty());
}

#[test]
#[ignore] // Requires GPU
fn svg_group_invalid_and_recovered_sources_replace_cached_pixels() {
    for compositor in [false, true] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        for source in [ICON, "<svg broken", ICON] {
            let ui = div().w(240.0).h(180.0).bg(Color::BLUE).child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(20.0)
                    .w(140.0)
                    .h(130.0)
                    .bg(Color::RED)
                    .opacity(0.5)
                    .child(svg(source).size(100.0, 100.0)),
            );
            let mut tree = RenderTree::from_element(&ui);
            tree.compute_layout(240.0, 180.0);
            let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
            context.invalidate_render_cache();
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, compositor);
                assert_eq!(
                    context
                        .cached_bg_batch
                        .as_ref()
                        .unwrap()
                        .dynamic_images
                        .len(),
                    usize::from(source == ICON)
                );
                pixel(
                    &page,
                    (25, 25),
                    if source == ICON {
                        [0, 128, 128, 255]
                    } else {
                        [128, 0, 128, 255]
                    },
                );
            }
            context.svg_atlas.clear();
        }
    }
}
