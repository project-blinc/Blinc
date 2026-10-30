use super::grouped_text_tests::frame;
use super::opacity_group_tests::{CssSource, css_value, pixel};
use super::*;
use blinc_animation::{AnimatedValue, AnimationScheduler};
use blinc_layout::{image, svg};
use slotmap::Key as _;

const SOURCE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFUlEQVR4nGNg+M/wH4gaGEAMBiAAAEbQBnso3jFKAAAAAElFTkSuQmCC";

fn icon(tree: &RenderTree) -> LayoutNodeId {
    tree.layout_tree
        .children(tree.query_by_id("group").unwrap())[0]
}

fn scene(background: bool, foreground: bool, source: &str) -> RenderTree {
    scene_with_bindings(background, foreground, source, None)
}

fn scene_with_bindings(
    background: bool,
    foreground: bool,
    source: &str,
    bindings: Option<(
        blinc_animation::SharedAnimatedValue,
        blinc_animation::SharedAnimatedValue,
    )>,
) -> RenderTree {
    let owner = div()
        .id("group")
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(140.0)
        .h(130.0)
        .bg(Color::RED);
    let owner = if background {
        owner.child(
            div()
                .w(100.0)
                .h(100.0)
                .background(blinc_core::ImageBrush::new(source)),
        )
    } else {
        owner.child(image(source).size(100.0, 100.0))
    };
    let owner = owner.child(div().absolute().left(25.0).top(25.0).child(text("M").size(24.0).color(Color::WHITE)))
        .child(div().absolute().left(75.0).top(65.0).child(svg(r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#ffff00"/></svg>"##).size(10.0,10.0)));
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
                .left(40.0)
                .top(40.0)
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
    let node = icon(&tree);
    tree.update_render_props(node, |p| {
        p.fade_duration_ms = Some(0);
        p.placeholder_type = Some(0);
        if foreground {
            p.layer = RenderLayer::Foreground;
        }
    });
    tree
}

#[test]
#[ignore] // Requires GPU
fn image_group_static_authored_alpha_and_backgrounds() {
    frames(false, false);
}

#[test]
#[ignore] // Requires GPU
fn image_group_fit_styles_and_path_clips_keep_order() {
    frames(true, false);
}

#[test]
#[ignore] // Requires GPU
fn image_group_css_and_binding_endpoints_recover() {
    frames(true, true);
}

fn frames(styled: bool, animated: bool) {
    for compositor in [false, true] {
        for background in [false, true] {
            for foreground in [false, true] {
                for mode in if animated { 1..4 } else { 0..1 } {
                    let mut context = super::main_text_clip_tests::context(true, 1);
                    let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
                    let state = RenderState::new(scheduler.clone());
                    let alpha = Arc::new(Mutex::new(AnimatedValue::with_default(
                        scheduler.lock().unwrap().handle(),
                        1.0,
                    )));
                    let x = Arc::new(Mutex::new(AnimatedValue::with_default(
                        scheduler.lock().unwrap().handle(),
                        0.0,
                    )));
                    let mut tree = scene_with_bindings(
                        background,
                        foreground,
                        SOURCE,
                        (mode == 1).then(|| (alpha.clone(), x.clone())),
                    );
                    let group = tree.query_by_id("group").unwrap();
                    let node = icon(&tree);
                    if styled {
                        tree.update_render_props(group, |p| {
                            p.transform = Some(blinc_core::Transform::scale(0.9, 1.05));
                            p.transform_origin = Some([0.0, 0.0]);
                            p.clip_path = Some(blinc_core::ClipPath::Path {
                                vertices: vec![(0.0, 0.0), (120.0, 0.0), (0.0, 120.0)],
                            });
                        });
                        tree.update_render_props(node, |p| {
                            p.object_fit = Some(1);
                            p.object_position = Some([0.3, 0.7]);
                            p.border_radius = 8.0.into();
                            p.border_width = 3.0;
                            p.border_color = Some(Color::WHITE);
                            p.filter = Some(blinc_layout::element_style::CssFilter {
                                grayscale: 0.2,
                                brightness: 0.8,
                                ..Default::default()
                            });
                        });
                    }
                    tree.update_render_props(group, |p| p.opacity = 0.999999);
                    let reference = frame(&mut context, &tree, &state, compositor);
                    for (index, value) in [0.5_f32, 0.0, 0.75, 1.0, 0.5].into_iter().enumerate() {
                        let offset = if mode == 1 { [0, 5, 0, 2, 0][index] } else { 0 };
                        match mode {
                            1 => {
                                alpha.lock().unwrap().set_immediate(value);
                                x.lock().unwrap().set_immediate(offset as f32);
                                tree.update_render_props(group, |p| p.opacity = 1.0);
                            }
                            2 | 3 => {
                                css_value(
                                    &tree,
                                    group,
                                    if mode == 2 {
                                        CssSource::Animation
                                    } else {
                                        CssSource::Transition
                                    },
                                    Some(value),
                                );
                                tree.update_render_props(group, |p| p.opacity = value);
                            }
                            _ => {
                                tree.update_render_props(group, |p| p.opacity = value);
                                context.invalidate_render_cache();
                            }
                        }
                        eprintln!(
                            "image mode={mode} styled={styled} bg={background} fg={foreground} compositor={compositor} alpha={value}"
                        );
                        for _ in 0..2 {
                            let page = frame(&mut context, &tree, &state, compositor);
                            // Opaque collected rendering retains its legacy pass order.
                            if value != 1.0 || mode == 1 {
                                for y in 26..114 {
                                    for px in 26..114 {
                                        if (39..46).contains(&px) || (39..46).contains(&y) {
                                            continue;
                                        }
                                        let p = reference.get_pixel(px, y).0;
                                        pixel(
                                            &page,
                                            (px + offset, y),
                                            [
                                                (p[0] as f32 * value).round() as u8,
                                                (p[1] as f32 * value).round() as u8,
                                                (p[2] as f32 * value + 255.0 * (1.0 - value))
                                                    .round()
                                                    as u8,
                                                255,
                                            ],
                                        );
                                    }
                                }
                                pixel(&page, (42, 42), [255, 255, 0, 255]);
                                assert!(
                                    context
                                        .cached_bg_batch
                                        .as_ref()
                                        .unwrap()
                                        .cross_stream_groups
                                        .image_nodes
                                        .contains(&node.data().as_ffi())
                                );
                            }
                            for y in 20..55 {
                                for x in 175..210 {
                                    assert_eq!(page.get_pixel(x, y), reference.get_pixel(x, y));
                                }
                            }
                        }
                    }
                    let cache = context.cached_bg_batch.clone().unwrap();
                    let retained = cache
                        .dynamic_images
                        .iter()
                        .find_map(|i| i.retained.as_ref())
                        .unwrap();
                    let view = retained.view.clone();
                    assert!(
                        cache
                            .dynamic_images
                            .iter()
                            .filter(|i| i.retained.is_some())
                            .all(|i| i.data.is_empty())
                    );
                    context.image_cache.clear();
                    assert_eq!(
                        cache
                            .dynamic_images
                            .iter()
                            .find_map(|i| i.retained.as_ref())
                            .unwrap()
                            .view,
                        view
                    );
                    let (texture, target) = crate::tests::create_test_texture(
                        context.device(),
                        context.texture_format(),
                        240,
                        180,
                    );
                    context
                        .renderer
                        .render_with_clear(&target, &cache, [0.0, 0.0, 1.0, 1.0]);
                    let replay = crate::tests::read_rendered_pixels(
                        context.device(),
                        context.queue(),
                        &texture,
                        240,
                        180,
                    );
                    for y in 26..114 {
                        for px in 26..114 {
                            if (39..46).contains(&px) || (39..46).contains(&y) {
                                continue;
                            }
                            let p = reference.get_pixel(px, y).0;
                            pixel(
                                &replay,
                                (px, y),
                                [
                                    (p[0] as f32 * 0.5).round() as u8,
                                    (p[1] as f32 * 0.5).round() as u8,
                                    (p[2] as f32 * 0.5 + 127.5).round() as u8,
                                    255,
                                ],
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn image_group_load_fade_refreshes_and_settles() {
    for compositor in [false, true] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let mut tree = scene(false, false, SOURCE);
        let group = tree.query_by_id("group").unwrap();
        tree.update_render_props(group, |p| p.opacity = 0.5);
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        frame(&mut context, &tree, &state, compositor);
        tree.update_render_props(icon(&tree), |p| p.fade_duration_ms = Some(100_000));
        context.invalidate_render_cache();
        for (elapsed, fade) in [(25_000, 0.25_f32), (75_000, 0.75), (100_001, 1.0)] {
            context.image_load_times.insert(
                SOURCE.into(),
                web_time::Instant::now() - std::time::Duration::from_millis(elapsed),
            );
            context.image_fade_deadlines.insert(
                SOURCE.into(),
                web_time::Instant::now()
                    + std::time::Duration::from_millis(100_000 - elapsed.min(100_000)),
            );
            let page = frame(&mut context, &tree, &state, compositor);
            pixel(
                &page,
                (30, 30),
                [
                    (127.5 * (1.0 - fade)).round() as u8,
                    (127.5 * fade).round() as u8,
                    128,
                    255,
                ],
            );
            assert_eq!(context.captured_images_need_repaint, fade < 1.0);
        }
        let a = frame(&mut context, &tree, &state, compositor);
        let b = frame(&mut context, &tree, &state, compositor);
        assert_eq!(a, b);
    }
}

#[test]
fn image_group_capability_and_unsupported_owners_keep_fallback() {
    super::opacity_group_tests::init_theme();
    for enabled in [false, true] {
        for unsupported in [false, true] {
            for background in [false, true] {
                let mut tree = scene(background, false, SOURCE);
                tree.update_render_props(tree.query_by_id("group").unwrap(), |p| {
                    p.opacity = 0.5;
                    if unsupported {
                        p.flow = Some("separate-pass".into());
                    }
                });
                let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
                let mut paint = GpuPaintContext::new(240.0, 180.0);
                paint.enable_deferred_text();
                paint.enable_deferred_svg();
                if enabled {
                    paint.enable_deferred_image();
                }
                tree.render_with_motion(&mut paint, &state);
                let mut batch = paint.take_batch();
                assert_eq!(
                    batch.cross_stream_groups.image_nodes.len(),
                    usize::from(enabled && !unsupported)
                );
                batch.resolve_deferred_content(&mut std::collections::HashMap::new());
                assert!(
                    batch
                        .layer_commands
                        .iter()
                        .chain(&batch.foreground_layer_commands)
                        .all(|e| !matches!(
                            e.command,
                            blinc_gpu::primitives::LayerCommand::DeferredImage { .. }
                        ))
                );
                batch.clear();
                assert!(batch.cross_stream_groups.image_nodes.is_empty());
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn image_group_placeholders_refresh_and_sources_recover() {
    for compositor in [false, true] {
        for kind in 0..=3 {
            let path = std::env::temp_dir().join(format!(
                "blinc-image-group-{}-{kind}-{compositor}.png",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&path);
            let source = path.to_str().unwrap();
            let mut context = super::main_text_clip_tests::context(true, 1);
            let mut tree = scene(false, false, source);
            tree.update_render_props(tree.query_by_id("group").unwrap(), |p| p.opacity = 0.5);
            tree.update_render_props(icon(&tree), |p| {
                p.loading_strategy = Some(1);
                p.placeholder_type = Some(kind);
                p.placeholder_color = Some([0.0, 1.0, 1.0, 1.0]);
                p.placeholder_image = Some(SOURCE.into());
            });
            let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
            context.frame_count = 0;
            let missing = frame(&mut context, &tree, &state, compositor);
            pixel(
                &missing,
                (30, 30),
                match kind {
                    0 => [128, 0, 128, 255],
                    2 => [0, 128, 128, 255],
                    _ => [0, 128, 255, 255],
                },
            );
            assert!(context.captured_images_need_repaint);
            assert_eq!(context.has_active_flows, kind == 3);
            if kind == 3 {
                context.frame_count = 30;
                let shimmer = frame(&mut context, &tree, &state, compositor);
                assert_ne!(missing.get_pixel(30, 30), shimmer.get_pixel(30, 30));
            }
            std::fs::write(&path, PNG).unwrap();
            let loaded = frame(&mut context, &tree, &state, compositor);
            pixel(&loaded, (30, 30), [0, 128, 128, 255]);
            assert!(!context.captured_images_need_repaint);
            assert!(!context.has_active_flows);
            std::fs::remove_file(&path).unwrap();
            context.invalidate_asset_path(&path);
            assert!(context.cached_bg_batch.is_none());
            context.frame_count = 0;
            let removed = frame(&mut context, &tree, &state, compositor);
            pixel(&removed, (30, 30), missing.get_pixel(30, 30).0);
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn image_group_fit_tint_mask_and_intrinsic_opacity_match_reference() {
    for compositor in [false, true] {
        for fit in 0..=4 {
            let mut context = super::main_text_clip_tests::context(true, 1);
            let owner = div()
                .id("group")
                .absolute()
                .left(20.0)
                .top(20.0)
                .w(140.0)
                .h(130.0)
                .bg(Color::RED)
                .child(
                    image(SOURCE)
                        .size(100.0, 70.0)
                        .opacity(0.8)
                        .tint_rgba(0.5, 0.8, 1.0, 0.6),
                );
            let ui = div().w(240.0).h(180.0).bg(Color::BLUE).child(owner);
            let mut tree = RenderTree::from_element(&ui);
            tree.compute_layout(240.0, 180.0);
            let group = tree.query_by_id("group").unwrap();
            tree.update_render_props(icon(&tree), |p| {
                p.fade_duration_ms = Some(0);
                p.placeholder_type = Some(0);
                p.object_fit = Some(fit);
                p.object_position = Some([0.2, 0.7]);
                p.mask_image = Some(blinc_core::MaskImage::Gradient(
                    blinc_core::Gradient::linear(
                        blinc_core::Point::new(0.0, 0.0),
                        blinc_core::Point::new(1.0, 0.0),
                        Color::WHITE,
                        Color::rgba(1.0, 1.0, 1.0, 0.2),
                    ),
                ));
                p.filter = Some(blinc_layout::element_style::CssFilter {
                    invert: 0.3,
                    contrast: 0.7,
                    ..Default::default()
                });
            });
            tree.update_render_props(group, |p| p.opacity = 0.999999);
            let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
            let reference = frame(&mut context, &tree, &state, compositor);
            tree.update_render_props(group, |p| p.opacity = 0.5);
            context.invalidate_render_cache();
            let page = frame(&mut context, &tree, &state, compositor);
            for y in 25..85 {
                for x in 25..115 {
                    let p = reference.get_pixel(x, y).0;
                    pixel(
                        &page,
                        (x, y),
                        [
                            (p[0] as f32 * 0.5).round() as u8,
                            (p[1] as f32 * 0.5).round() as u8,
                            (p[2] as f32 * 0.5 + 127.5).round() as u8,
                            255,
                        ],
                    );
                }
            }
            let image = &context.cached_bg_batch.as_ref().unwrap().dynamic_images[0];
            let retained = image.retained.as_ref().unwrap();
            assert_eq!(retained.style.tint, [0.5, 0.8, 1.0, 0.6]);
            assert_eq!(retained.style.params[1], 1.0);
            assert!(context.cached_bg_batch.as_ref().unwrap().layer_commands.iter().chain(&context.cached_bg_batch.as_ref().unwrap().foreground_layer_commands).any(|entry| {
                matches!(&entry.command, blinc_gpu::primitives::LayerCommand::Push { config } if config.opacity == 0.8)
            }));
            assert_eq!(retained.style.mask_info[0], 1.0);
            assert_eq!(retained.style.filter_a[1], 0.3);
            let bounds = image.screen_bounds();
            match fit {
                1 => {
                    assert_eq!(bounds.width(), 70.0);
                    assert_eq!(bounds.x(), 26.0);
                }
                3 | 4 => {
                    assert_eq!(bounds.width(), 2.0);
                    assert_eq!(bounds.height(), 2.0);
                }
                _ => assert_eq!(bounds.width(), 100.0),
            }
        }
    }
}

const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0,
    0, 0, 114, 182, 13, 36, 0, 0, 0, 21, 73, 68, 65, 84, 120, 156, 99, 96, 248, 207, 240, 31, 136,
    26, 24, 64, 12, 6, 32, 0, 0, 70, 208, 6, 123, 40, 222, 49, 74, 0, 0, 0, 0, 73, 69, 78, 68, 174,
    66, 96, 130,
];

#[test]
#[ignore] // Requires GPU
fn image_group_compositor_replay_keeps_evicted_texture_without_reload() {
    {
        let compositor = true;
        let mut context = super::main_text_clip_tests::context(true, 1);
        let mut tree = scene(true, false, SOURCE);
        tree.update_render_props(tree.query_by_id("group").unwrap(), |p| p.opacity = 0.5);
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        let reference = frame(&mut context, &tree, &state, compositor);
        let view = context
            .cached_bg_batch
            .as_ref()
            .unwrap()
            .dynamic_images
            .iter()
            .find_map(|i| i.retained.as_ref())
            .unwrap()
            .view
            .clone();
        context.image_cache.clear();
        for _ in 0..2 {
            let replay = frame(&mut context, &tree, &state, compositor);
            assert_eq!(reference, replay);
            assert!(context.image_cache.is_empty());
            assert!(!context.captured_images_need_repaint);
            assert_eq!(
                context
                    .cached_bg_batch
                    .as_ref()
                    .unwrap()
                    .dynamic_images
                    .iter()
                    .find_map(|i| i.retained.as_ref())
                    .unwrap()
                    .view,
                view
            );
        }
    }
}
