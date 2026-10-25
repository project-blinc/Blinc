use super::opacity_group_tests::{CssSource, css_value};
use super::*;
use crate::tests::read_rendered_pixels;
use blinc_animation::{AnimatedValue, AnimationScheduler, SharedAnimatedValue};

fn tree(
    foreground: bool,
    binding: Option<(SharedAnimatedValue, SharedAnimatedValue)>,
    styled: bool,
) -> RenderTree {
    let label = text("MMMM")
        .id("label")
        .size(36.0)
        .color(Color::GREEN)
        .no_wrap();
    let label = if styled {
        label.italic().underline().strikethrough()
    } else {
        label
    };
    let content = div().child(label);
    let content = if foreground {
        content.foreground()
    } else {
        content
    };
    let owner = div()
        .id("group")
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(120.0)
        .h(100.0)
        .bg(Color::RED)
        .child(content);
    let ui = div().w(240.0).h(180.0).bg(Color::BLUE);
    let ui = if let Some((opacity, x)) = binding {
        ui.child(owner.motion().opacity(opacity).translate_x(x))
    } else {
        ui.child(owner)
    };
    let ui = ui
        .child(
            div()
                .absolute()
                .left(105.0)
                .top(90.0)
                .w(10.0)
                .h(10.0)
                .bg(Color::YELLOW),
        )
        .child(
            div()
                .absolute()
                .left(165.0)
                .top(20.0)
                .child(text("X").size(32.0).color(Color::WHITE)),
        );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(240.0, 180.0);
    if styled {
        let label = tree.query_by_id("label").unwrap();
        tree.update_render_props(label, |p| {
            p.opacity = 0.5;
            p.text_shadow = Some(blinc_core::Shadow::new(3.0, 2.0, 0.0, Color::WHITE));
        });
        let owner = tree.query_by_id("group").unwrap();
        tree.update_render_props(owner, |p| {
            p.transform = Some(blinc_core::Transform::scale(0.9, 1.05));
            p.transform_origin = Some([0.0, 0.0]);
            p.clip_path = Some(blinc_core::ClipPath::Path {
                vertices: vec![(0.0, 0.0), (120.0, 0.0), (0.0, 100.0)],
            })
        });
    }
    tree
}

fn frame(
    context: &mut RenderContext,
    tree: &RenderTree,
    state: &RenderState,
    compositor: bool,
) -> image::RgbaImage {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Grouped text target"),
        size: wgpu::Extent3d {
            width: 240,
            height: 180,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: context.texture_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    context
        .render_tree_with_motion_opt(
            tree,
            state,
            240,
            180,
            &view,
            compositor.then_some(&texture),
            true,
        )
        .unwrap();
    read_rendered_pixels(context.device(), context.queue(), &texture, 240, 180)
}

#[derive(Clone, Copy, Debug)]
enum Source {
    Static,
    Binding,
    Css(CssSource),
}

#[test]
#[ignore] // Requires GPU
fn grouped_text_static_plain_text_blends_once_in_background_and_foreground() {
    frames(Source::Static, false);
}

#[test]
#[ignore] // Requires GPU
fn grouped_text_motion_and_css_updates_recover_from_opacity_endpoints() {
    for source in [
        Source::Binding,
        Source::Css(CssSource::Animation),
        Source::Css(CssSource::Transition),
    ] {
        frames(source, false);
    }
}

#[test]
#[ignore] // Requires GPU
fn grouped_text_nested_shadow_decorations_and_path_clips_keep_ownership() {
    frames(Source::Static, true);
}

fn frames(source: Source, styled: bool) {
    for unified in [false, true] {
        for compositor in [false, true] {
            for foreground in [false, true] {
                let mut context = super::main_text_clip_tests::context(unified, 1);
                let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
                let opacity_binding = Arc::new(Mutex::new(AnimatedValue::with_default(
                    scheduler.lock().unwrap().handle(),
                    1.0,
                )));
                let x_binding = Arc::new(Mutex::new(AnimatedValue::with_default(
                    scheduler.lock().unwrap().handle(),
                    1.0,
                )));
                let mut tree = tree(
                    foreground,
                    matches!(source, Source::Binding)
                        .then(|| (opacity_binding.clone(), x_binding.clone())),
                    styled,
                );
                let node = tree.query_by_id("group").unwrap();
                let state = RenderState::new(scheduler);
                let legacy = frame(&mut context, &tree, &state, compositor);
                assert!((20..70).any(|y| (20..95).any(|x| legacy.get_pixel(x, y).0[1] > 50)));
                // Captured and legacy text can rasterize glyph edges differently.
                tree.update_render_props(node, |p| p.opacity = 0.999999);
                context.invalidate_render_cache();
                let captured = frame(&mut context, &tree, &state, compositor);
                tree.update_render_props(node, |p| p.opacity = 1.0);
                for (index, opacity) in [0.5_f32, 0.0, 0.75, 1.0, 0.5].into_iter().enumerate() {
                    let offset = if matches!(source, Source::Binding) {
                        [0, 10, 0, 5, 0][index]
                    } else {
                        0
                    };
                    eprintln!(
                        "{source:?} unified={unified} compositor={compositor} foreground={foreground} styled={styled} opacity={opacity}"
                    );
                    match source {
                        Source::Binding => {
                            opacity_binding.lock().unwrap().set_immediate(opacity);
                            x_binding.lock().unwrap().set_immediate(1.0 + offset as f32);
                        }
                        Source::Static => {
                            tree.update_render_props(node, |p| p.opacity = opacity);
                            context.invalidate_render_cache();
                        }
                        Source::Css(source) => {
                            css_value(&tree, node, source, Some(opacity));
                            tree.update_render_props(node, |p| p.opacity = opacity);
                        }
                    }
                    let baseline = if opacity == 1.0 && !matches!(source, Source::Binding) {
                        &legacy
                    } else {
                        &captured
                    };
                    for _ in 0..2 {
                        let page = frame(&mut context, &tree, &state, compositor);
                        for y in 20..70 {
                            for x in 20..95 {
                                let origin = u32::from(matches!(source, Source::Binding));
                                let base = baseline.get_pixel(x + origin, y).0;
                                let expected = [
                                    (base[0] as f32 * opacity).round() as u8,
                                    (base[1] as f32 * opacity).round() as u8,
                                    (base[2] as f32 * opacity + 255.0 * (1.0 - opacity)).round()
                                        as u8,
                                    255,
                                ];
                                super::opacity_group_tests::pixel(
                                    &page,
                                    (x + origin + offset, y),
                                    expected,
                                );
                            }
                        }
                        super::opacity_group_tests::pixel(&page, (110, 95), [255, 255, 0, 255]);
                        for y in 20..65 {
                            for x in 165..200 {
                                assert_eq!(page.get_pixel(x, y), legacy.get_pixel(x, y));
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn grouped_text_producer_insertion_rebases_aux_and_image_cursors_and_retains_ids() {
    use blinc_core::{ClipShape, LayerConfig, Path};
    use blinc_gpu::primitives::LayerCommand;
    use std::collections::HashMap;
    let mut paint = GpuPaintContext::new(240.0, 180.0);
    assert!(paint.push_cross_stream_layer(1, LayerConfig::new().opacity(0.5)));
    assert!(!paint.defer_text(10));
    paint.enable_deferred_text();
    paint.fill_rect(
        Rect::new(0.0, 0.0, 120.0, 100.0),
        0.0.into(),
        Color::RED.into(),
    );
    paint.push_opacity(0.4);
    assert!(paint.defer_text(10));
    paint.pop_opacity();
    paint.draw_rgba_pixels(&[255; 4], 1, 1, Rect::new(0.0, 0.0, 1.0, 1.0));
    paint.fill_rect(
        Rect::new(90.0, 90.0, 5.0, 5.0),
        0.0.into(),
        Color::YELLOW.into(),
    );
    paint.set_foreground_layer(true);
    assert!(paint.defer_text(11));
    paint.set_foreground_layer(false);
    paint.pop_layer();
    let mut batch = PrimitiveBatch::new();
    batch.aux_data = vec![[99.0; 4]; 7];
    batch.merge(paint.take_batch());
    let mut glyph = GpuPaintContext::new(240.0, 180.0);
    glyph.push_clip(ClipShape::Path(Path::rect(Rect::new(
        0.0, 0.0, 120.0, 100.0,
    ))));
    glyph.fill_rect(
        Rect::new(20.0, 20.0, 40.0, 40.0),
        0.0.into(),
        Color::GREEN.into(),
    );
    let mut prepared = HashMap::from([(10, glyph.take_batch()), (11, PrimitiveBatch::new())]);
    batch.resolve_deferred_text(&mut prepared);
    assert!(prepared.is_empty());
    assert_eq!(batch.primitives.len(), 3);
    assert_eq!(batch.primitives[1].color, [0.0, 1.0, 0.0, 0.4]);
    let header = batch.primitives[1].clip_shape[0] as usize - 1;
    assert!(header >= 7);
    assert_eq!(batch.aux_data[header], [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        batch.dynamic_images[0].paint_order.unwrap().primitive_index,
        2
    );
    assert!(
        batch
            .layer_commands
            .iter()
            .all(|entry| !matches!(entry.command, LayerCommand::DeferredText { .. }))
    );
    assert!(batch.cross_stream_groups.text_nodes.contains(&10));
    assert!(batch.cross_stream_groups.text_nodes.contains(&11));
    batch.clear();
    assert!(batch.cross_stream_groups.text_nodes.is_empty());
}

#[test]
fn grouped_text_capability_and_unsupported_boundaries_keep_collection_fallback() {
    super::opacity_group_tests::init_theme();
    for (enabled, unsupported) in [(false, false), (true, false), (true, true)] {
        let mut tree = tree(false, None, false);
        let group = tree.query_by_id("group").unwrap();
        tree.update_render_props(group, |p| {
            p.opacity = 0.5;
            if unsupported {
                p.background = Some(blinc_core::ImageBrush::new("placeholder").into());
            }
        });
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        let mut paint = GpuPaintContext::new(240.0, 180.0);
        if enabled {
            paint.enable_deferred_text();
        }
        tree.render_with_motion(&mut paint, &state);
        let mut batch = paint.take_batch();
        assert_eq!(
            batch.cross_stream_groups.text_nodes.len(),
            usize::from(enabled && !unsupported)
        );
        if enabled && !unsupported {
            use blinc_layout::renderer::{AmbientPaintState, DynamicKind, DynamicRegion};
            let region = DynamicRegion {
                root: group,
                screen_aabb: [20.0, 20.0, 120.0, 100.0],
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
            assert!(!fallback.take_batch().has_cross_stream_groups());
        }
        batch.resolve_deferred_text(&mut Default::default());
        assert!(
            batch
                .layer_commands
                .iter()
                .chain(&batch.foreground_layer_commands)
                .all(|entry| !matches!(
                    entry.command,
                    blinc_gpu::primitives::LayerCommand::DeferredText { .. }
                ))
        );
    }
}
