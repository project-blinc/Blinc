use super::opacity_group_tests::{CssSource, css_value, init_theme, pixel, surface};
use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::read_rendered_pixels;
use blinc_animation::{AnimatedValue, AnimationScheduler};

#[test]
#[ignore] // Requires GPU
fn cross_stream_group_static_native_pixels_and_surrounding_order() {
    frames(false, false, None);
}

#[test]
#[ignore] // Requires GPU
fn cross_stream_group_motion_endpoints_and_canvas_output() {
    frames(true, false, None);
    frames(true, true, None);
}

#[test]
#[ignore] // Requires GPU
fn cross_stream_group_css_sources_repaint_joined_ranges_and_endpoints() {
    for source in [CssSource::Animation, CssSource::Transition] {
        frames(false, false, Some(source));
    }
}

fn frames(motion: bool, rgba: bool, css: Option<CssSource>) {
    init_theme();
    for compositor in [false, true] {
        let mut app = BlincApp::with_config(BlincConfig {
            sample_count: 1,
            ..Default::default()
        })
        .expect("offscreen GPU required");
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let opacity = Arc::new(Mutex::new(AnimatedValue::with_default(
            scheduler.lock().unwrap().handle(),
            1.0,
        )));
        let x = Arc::new(Mutex::new(AnimatedValue::with_default(
            scheduler.lock().unwrap().handle(),
            0.0,
        )));
        let owner = div().id("cross-owner").absolute().w(160.0).h(120.0).child(
            div()
                .absolute()
                .left(20.0)
                .top(20.0)
                .w(60.0)
                .h(60.0)
                .bg(Color::RED),
        );
        let owner = if rgba {
            owner.child(
                canvas(|ctx, _| {
                    ctx.draw_rgba_pixels(&[0, 255, 0, 255], 1, 1, Rect::new(40.0, 40.0, 60.0, 60.0))
                })
                .absolute()
                .w(160.0)
                .h(120.0)
                .foreground(),
            )
        } else {
            owner.child(
                div()
                    .absolute()
                    .left(40.0)
                    .top(40.0)
                    .w(60.0)
                    .h(60.0)
                    .foreground()
                    .bg(Color::GREEN),
            )
        };
        let ui = div().w(160.0).h(120.0).bg(Color::BLUE);
        let ui = if motion {
            ui.child(
                owner
                    .motion()
                    .opacity(opacity.clone())
                    .translate_x(x.clone()),
            )
        } else {
            ui.child(owner)
        };
        let ui = ui
            .child(
                div()
                    .absolute()
                    .left(55.0)
                    .top(55.0)
                    .w(5.0)
                    .h(5.0)
                    .bg(Color::YELLOW),
            )
            .child(
                div()
                    .absolute()
                    .left(65.0)
                    .top(65.0)
                    .w(5.0)
                    .h(5.0)
                    .foreground()
                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.5)),
            )
            .child(
                div()
                    .absolute()
                    .left(130.0)
                    .top(5.0)
                    .w(10.0)
                    .h(10.0)
                    .foreground()
                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.5)),
            );
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(160.0, 120.0);
        let owner = tree.query_by_id("cross-owner").unwrap();
        let state = RenderState::new(scheduler);
        for (frame, value) in [0.5_f32, 1.0, 0.0, 0.75, 0.5].into_iter().enumerate() {
            let offset = if motion { [0, 10, 0, 5, 0][frame] } else { 0 };
            if motion {
                opacity.lock().unwrap().set_immediate(value);
                x.lock().unwrap().set_immediate(offset as f32);
            } else {
                if let Some(source) = css {
                    css_value(&tree, owner, source, Some(value));
                }
                tree.update_render_props(owner, |p| p.opacity = value);
                if css.is_none() {
                    app.context().invalidate_render_cache();
                }
            }
            for replay in [false, true] {
                let texture = surface(&app);
                let view = texture.create_view(&Default::default());
                app.render_tree_with_motion_opt(
                    &tree,
                    &state,
                    &view,
                    compositor.then_some(&texture),
                    160,
                    120,
                    frame > 0 || replay,
                )
                .unwrap();
                let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
                eprintln!(
                    "motion={motion} rgba={rgba} compositor={compositor} value={value} replay={replay}"
                );
                pixel(
                    &page,
                    (45 + offset, 45),
                    [
                        0,
                        (255.0 * value).round() as u8,
                        (255.0 * (1.0 - value)).round() as u8,
                        255,
                    ],
                );
                pixel(
                    &page,
                    (25 + offset, 25),
                    [
                        (255.0 * value).round() as u8,
                        0,
                        (255.0 * (1.0 - value)).round() as u8,
                        255,
                    ],
                );
                pixel(
                    &page,
                    (56, 56),
                    if !motion && value == 1.0 {
                        [0, 255, 0, 255]
                    } else {
                        [255, 255, 0, 255]
                    },
                );
                pixel(
                    &page,
                    (67, 67),
                    [
                        128,
                        (127.5 + 127.5 * value).round() as u8,
                        (255.0 - 127.5 * value).round() as u8,
                        255,
                    ],
                );
                pixel(&page, (135, 10), [128, 128, 255, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn cross_stream_group_nested_canvas_paths_and_images_keep_clip_ownership() {
    use blinc_core::{ClipShape, LayerConfig, Path};
    init_theme();
    for compositor in [false, true] {
        let mut app = BlincApp::with_config(BlincConfig {
            sample_count: 1,
            ..Default::default()
        })
        .expect("offscreen GPU required");
        let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .absolute()
                .w(160.0)
                .h(120.0)
                .opacity(0.5)
                .child(
                    div()
                        .absolute()
                        .left(20.0)
                        .top(20.0)
                        .w(60.0)
                        .h(60.0)
                        .bg(Color::RED),
                )
                .child(
                    div().absolute().w(160.0).h(120.0).opacity(0.5).child(
                        canvas(|ctx, _| {
                            ctx.push_clip(ClipShape::Path(
                                Path::new()
                                    .move_to(0.0, 0.0)
                                    .line_to(140.0, 0.0)
                                    .line_to(0.0, 140.0)
                                    .close(),
                            ));
                            ctx.push_clip(ClipShape::rect(Rect::new(45.0, 45.0, 45.0, 45.0)));
                            ctx.push_layer(LayerConfig::new().opacity(0.5));
                            ctx.fill_path(
                                &Path::rect(Rect::new(40.0, 40.0, 20.0, 60.0)),
                                blinc_core::ImageBrush::new("placeholder")
                                    .tint(Color::GREEN)
                                    .into(),
                            );
                            ctx.draw_rgba_pixels(
                                &[0, 255, 0, 255],
                                1,
                                1,
                                Rect::new(60.0, 40.0, 40.0, 60.0),
                            );
                            ctx.pop_layer();
                            ctx.pop_clip();
                            ctx.pop_clip();
                        })
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .foreground(),
                    ),
                ),
        );
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(160.0, 120.0);
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        for _ in 0..2 {
            let texture = surface(&app);
            let view = texture.create_view(&Default::default());
            app.render_tree_with_motion_opt(
                &tree,
                &state,
                &view,
                compositor.then_some(&texture),
                160,
                120,
                true,
            )
            .unwrap();
            let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
            pixel(&page, (55, 55), [96, 32, 128, 255]);
            pixel(&page, (65, 55), [96, 32, 128, 255]);
            pixel(&page, (75, 75), [128, 0, 128, 255]);
            pixel(&page, (42, 55), [128, 0, 128, 255]);
            let context = app.context();
            let batch = context.cached_bg_batch.as_ref().unwrap();
            assert!(batch.has_cross_stream_groups());
            assert!(batch.foreground_primitives.is_empty());
            assert!(batch.foreground_dynamic_images.is_empty());
            assert!(!batch.aux_data.is_empty());
            assert!(!batch.paths.indices.is_empty());
            assert_eq!(batch.dynamic_images.len(), 1);
        }
    }
}

#[test]
fn cross_stream_group_producer_joins_nested_streams_and_restores_routes() {
    use blinc_core::{LayerConfig, Path};
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_motion_subtree();
    assert!(paint.push_cross_stream_layer(1, LayerConfig::new().opacity(0.5)));
    assert!(!paint.bg_batch_is_dynamic());
    paint.fill_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        0.0.into(),
        Color::RED.into(),
    );
    paint.set_foreground_layer(true);
    paint.push_layer(LayerConfig::new().opacity(0.5));
    paint.fill_path(
        &Path::rect(Rect::new(40.0, 40.0, 20.0, 20.0)),
        blinc_core::ImageBrush::new("placeholder")
            .tint(Color::GREEN)
            .into(),
    );
    paint.draw_rgba_pixels(&[0, 255, 0, 255], 1, 1, Rect::new(60.0, 40.0, 20.0, 20.0));
    paint.pop_layer();
    paint.set_foreground_layer(false);
    paint.pop_layer();
    assert!(paint.bg_batch_is_dynamic());
    paint.fill_rect(
        Rect::new(130.0, 5.0, 10.0, 10.0),
        0.0.into(),
        Color::WHITE.into(),
    );
    paint.pop_motion_subtree();
    paint.set_foreground_layer(true);
    paint.fill_rect(
        Rect::new(130.0, 100.0, 10.0, 10.0),
        0.0.into(),
        Color::WHITE.into(),
    );
    paint.set_foreground_layer(false);
    let batch = paint.take_batch();
    assert!(batch.has_cross_stream_groups());
    assert_eq!(batch.primitives.len(), 1);
    assert_eq!(batch.primitives[0].color[3], 1.0);
    assert_eq!(batch.layer_commands.len(), 4);
    assert_eq!(batch.foreground_primitives.len(), 1);
    assert!(batch.foreground_layer_commands.is_empty());
    assert_eq!(batch.dynamic_images.len(), 1);
    assert_eq!(batch.dynamic_images[0].opacity, 1.0);
    assert!(
        batch
            .paths
            .indices
            .iter()
            .all(|index| (*index as usize) < batch.paths.vertices.len())
    );
    assert!(!batch.paths.indices.is_empty());
    let mut merged = PrimitiveBatch::new();
    merged.aux_data = vec![[0.0; 4]; 7];
    merged.push_path(blinc_gpu::path::tessellate_fill(
        &Path::rect(Rect::new(110.0, 90.0, 10.0, 10.0)),
        &Color::WHITE.into(),
    ));
    let vertices = merged.paths.vertices.len() as u32;
    let indices = merged.paths.indices.len();
    merged.merge(batch);
    assert!(merged.has_cross_stream_groups());
    assert!(
        merged.paths.indices[indices..]
            .iter()
            .all(|i| *i >= vertices)
    );
    merged.clear();
    assert!(!merged.has_cross_stream_groups());
    let dynamic = paint.take_dynamic_batch();
    assert!(!dynamic.has_cross_stream_groups());
    assert_eq!(dynamic.primitives.len(), 1);
}

#[test]
fn cross_stream_group_unsupported_ancestor_keeps_foreground_fallback() {
    use blinc_core::LayerConfig;
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_layer(LayerConfig::new().opacity(0.5));
    assert!(!paint.push_cross_stream_layer(1, LayerConfig::new().opacity(0.5)));
    paint.set_foreground_layer(true);
    paint.fill_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        0.0.into(),
        Color::GREEN.into(),
    );
    paint.set_foreground_layer(false);
    paint.pop_layer();
    let batch = paint.take_batch();
    assert!(!batch.has_cross_stream_groups());
    assert_eq!(batch.foreground_primitives[0].color[3], 0.5);
    assert!(paint.push_cross_stream_layer(1, LayerConfig::new().opacity(0.5)));
    paint.pop_layer();
    paint.clear();
    assert!(!paint.batch().has_cross_stream_groups());
}

#[test]
fn cross_stream_group_collected_content_keeps_nested_native_fallback() {
    init_theme();
    for (image_background, moving) in [(false, false), (true, false), (false, true)] {
        let outer = div()
            .w(160.0)
            .h(120.0)
            .opacity(if moving { 1.0 } else { 0.5 })
            .child(
                div()
                    .w(120.0)
                    .h(120.0)
                    .opacity(0.5)
                    .child(div().w(60.0).h(60.0).bg(Color::GREEN).foreground()),
            );
        let outer = if image_background {
            outer.background(blinc_core::ImageBrush::new("placeholder"))
        } else {
            outer.child(text("separate content"))
        };
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let ui = div().w(160.0).h(120.0);
        let ui = if moving {
            let x = Arc::new(Mutex::new(AnimatedValue::with_default(
                scheduler.lock().unwrap().handle(),
                0.0,
            )));
            ui.child(outer.motion().translate_x(x))
        } else {
            ui.child(outer)
        };
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(160.0, 120.0);
        let state = RenderState::new(scheduler);
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        tree.render_with_motion(&mut paint, &state);
        let mut batch = paint.take_batch();
        batch.merge(paint.take_dynamic_batch());
        assert!(!batch.has_cross_stream_groups());
        assert_eq!(
            batch.foreground_primitives[0].color[3],
            if moving { 0.5 } else { 0.25 }
        );
    }
}

#[test]
#[ignore] // Requires GPU
fn cross_stream_group_moving_ancestor_keeps_background_before_nested_capture() {
    init_theme();
    for compositor in [false, true] {
        let mut app = BlincApp::with_config(BlincConfig {
            sample_count: 1,
            ..Default::default()
        })
        .expect("offscreen GPU required");
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let x = Arc::new(Mutex::new(AnimatedValue::with_default(
            scheduler.lock().unwrap().handle(),
            0.0,
        )));
        let owner = div().absolute().w(120.0).h(110.0).bg(Color::RED).child(
            div().absolute().w(120.0).h(110.0).opacity(0.5).child(
                div()
                    .absolute()
                    .left(40.0)
                    .top(40.0)
                    .w(40.0)
                    .h(40.0)
                    .bg(Color::GREEN)
                    .foreground(),
            ),
        );
        let ui = div()
            .w(160.0)
            .h(120.0)
            .bg(Color::BLUE)
            .child(owner.motion().translate_x(x.clone()))
            .child(
                div()
                    .absolute()
                    .left(55.0)
                    .top(55.0)
                    .w(5.0)
                    .h(5.0)
                    .bg(Color::YELLOW),
            );
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(160.0, 120.0);
        let state = RenderState::new(scheduler);
        for offset in [0.0, 10.0, 0.0] {
            x.lock().unwrap().set_immediate(offset);
            for _ in 0..2 {
                let texture = surface(&app);
                let view = texture.create_view(&Default::default());
                app.render_tree_with_motion_opt(
                    &tree,
                    &state,
                    &view,
                    compositor.then_some(&texture),
                    160,
                    120,
                    true,
                )
                .unwrap();
                let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
                pixel(&page, (45 + offset as u32, 45), [128, 128, 0, 255]);
                pixel(&page, (56, 56), [255, 255, 0, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn cross_stream_group_borrowed_batches_join_before_public_dispatch() {
    use blinc_core::LayerConfig;
    let mut app = BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required");
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    assert!(paint.push_cross_stream_layer(1, LayerConfig::new().opacity(0.5)));
    paint.fill_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        0.0.into(),
        Color::RED.into(),
    );
    paint.set_foreground_layer(true);
    paint.fill_rect(
        Rect::new(40.0, 40.0, 60.0, 60.0),
        0.0.into(),
        Color::GREEN.into(),
    );
    paint.set_foreground_layer(false);
    paint.pop_layer();
    let batch = paint.batch();
    assert!(!batch.cross_stream_groups.captures.is_empty());
    for overlay in [false, true] {
        let texture = surface(&app);
        let view = texture.create_view(&Default::default());
        let context = app.context();
        context.renderer.resize(160, 120);
        if overlay {
            context
                .renderer
                .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
            context.renderer.render_overlay_msaa(&view, batch, 1);
        } else {
            context
                .renderer
                .render_with_clear(&view, batch, [0.0, 0.0, 1.0, 1.0]);
        }
        let foreground = batch.foreground_batch();
        assert!(!foreground.has_foreground_content());
        assert!(foreground.primitives.is_empty());
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        pixel(&page, (55, 55), [0, 128, 128, 255]);
        pixel(&page, (25, 25), [128, 0, 128, 255]);
    }
}
