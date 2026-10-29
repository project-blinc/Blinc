//! Inline overlay roots compose together above ordinary foreground content.
use super::gpu_pass_group_tests::frame;
use super::*;

pub(super) const IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNg+M/wHwAEAQH/cetH5QAAAABJRU5ErkJggg==";

fn scene() -> RenderTree {
    let mut tree = RenderTree::from_element(
        &div()
            .w(160.0)
            .h(120.0)
            .bg(Color::BLUE)
            .child(
                div()
                    .id("overlay")
                    .overlay_root()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(120.0)
                    .h(100.0)
                    .bg(Color::RED)
                    .opacity(0.5)
                    .child(
                        div().absolute().left(20.0).top(20.0).w(60.0).h(60.0).child(
                            blinc_layout::image(IMAGE)
                                .size(60.0, 60.0)
                                .fade_in(std::time::Duration::ZERO),
                        ),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .bg(Color::YELLOW)
                    .foreground(),
            ),
    );
    tree.compute_layout(160.0, 120.0);
    tree
}

pub(super) fn mixed_overlay() -> Div {
    div().id("overlay").overlay_root().absolute().left(20.0).top(10.0).w(120.0).h(100.0)
        .bg(Color::RED).opacity(0.5)
        .child(div().id("image-host").absolute().left(20.0).top(20.0).w(60.0).h(60.0)
            .child(blinc_layout::image(IMAGE).size(60.0,60.0).fade_in(std::time::Duration::ZERO)))
        .child(div().absolute().left(75.0).top(65.0).child(blinc_layout::svg(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#ffff00"/></svg>"##).size(20.0,20.0)))
        .child(div().absolute().left(5.0).top(0.0).child(text("M").size(20.0).color(Color::WHITE)))
        .child(div().absolute().left(45.0).top(0.0).child(blinc_layout::rich_text(
            "<span color=\"#00ffff\">M</span>").size(20.0)))
        .child(canvas(|ctx,_| ctx.fill_rect(Rect::new(20.0,20.0,30.0,30.0),0.0.into(),Color::MAGENTA.into()))
            .absolute().w(120.0).h(100.0).foreground())
}

pub(super) fn with_plate(overlay: impl blinc_layout::Element + 'static) -> RenderTree {
    let mut tree = RenderTree::from_element(
        &div()
            .id("host")
            .w(160.0)
            .h(120.0)
            .bg(Color::BLUE)
            .child(overlay)
            .child(
                div()
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .bg(Color::YELLOW)
                    .foreground(),
            ),
    );
    tree.compute_layout(160.0, 120.0);
    tree
}

pub(super) fn blend(color: [f32; 3], opacity: f32, brightness: f32) -> [u8; 4] {
    [
        ((color[0] * brightness * opacity + 1.0 - opacity) * 255.0).round() as u8,
        ((color[1] * brightness * opacity + 1.0 - opacity) * 255.0).round() as u8,
        (color[2] * brightness * opacity * 255.0).round() as u8,
        255,
    ]
}

#[test]
#[ignore] // Requires GPU
fn overlay_group_mixed_content_effects_and_endpoints_keep_dynamic_stacking() {
    for unified in [false, true] {
        for samples in [1, 4] {
            let mut context = super::main_text_clip_tests::context(unified, samples);
            let state = RenderState::new(Arc::new(Mutex::new(
                blinc_animation::AnimationScheduler::new(),
            )));
            for mode in 0..3 {
                for foreground in [false, true] {
                    let overlay = mixed_overlay();
                    let mut tree = with_plate(if foreground {
                        overlay.foreground()
                    } else {
                        overlay
                    });
                    let root = tree.query_by_id("overlay").unwrap();
                    for scale in [1, 2] {
                        tree.set_scale_factor(scale as f32);
                        for (opacity, brightness) in
                            [(0.5, 1.0), (0.0, 1.0), (1.0, 1.0), (0.75, 0.5), (0.5, 1.0)]
                        {
                            tree.update_render_props(root, |p| {
                                p.opacity = opacity;
                                p.layer_effects =
                                    vec![blinc_core::LayerEffect::brightness(brightness)];
                            });
                            context.invalidate_render_cache();
                            for _ in 0..2 {
                                let page = frame(&mut context, &tree, &state, mode);
                                for (xy, color) in [
                                    ((60, 75), [0.0, 1.0, 0.0]),
                                    ((105, 85), [1.0, 1.0, 0.0]),
                                    ((55, 45), [1.0, 0.0, 1.0]),
                                    ((25, 105), [1.0, 0.0, 0.0]),
                                ] {
                                    super::opacity_group_tests::pixel(
                                        &page,
                                        (xy.0 * scale, xy.1 * scale),
                                        blend(color, opacity, brightness),
                                    );
                                }
                                let white = (12 * scale..30 * scale)
                                    .flat_map(|y| (25 * scale..43 * scale).map(move |x| (x, y)))
                                    .map(|(x, y)| page.get_pixel(x, y).0[2])
                                    .max()
                                    .unwrap();
                                assert!(
                                    (white as f32 - opacity * brightness * 255.0).abs() < 5.0,
                                    "plain text missing: {white}"
                                );
                                let styled = (12 * scale..30 * scale)
                                    .flat_map(|y| (65 * scale..85 * scale).map(move |x| (x, y)))
                                    .map(|(x, y)| page.get_pixel(x, y).0[2])
                                    .max()
                                    .unwrap();
                                assert!(
                                    (styled as f32 - opacity * brightness * 255.0).abs() < 5.0,
                                    "styled text missing: {styled}"
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
#[ignore] // Requires GPU
fn overlay_group_images_blend_once_above_static_foreground() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let mut tree = scene();
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let owner = tree.query_by_id("overlay").unwrap();
    for mode in 0..3 {
        for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
            tree.update_render_props(owner, |p| p.opacity = opacity);
            context.invalidate_render_cache();
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, mode);
                super::opacity_group_tests::pixel(
                    &page,
                    (60, 50),
                    blend([0.0, 1.0, 0.0], opacity, 1.0),
                );
                super::opacity_group_tests::pixel(
                    &page,
                    (25, 15),
                    blend([1.0, 0.0, 0.0], opacity, 1.0),
                );
                super::opacity_group_tests::pixel(&page, (5, 5), [255, 255, 0, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_group_live_motion_and_css_keep_late_clips_and_zero_recovery() {
    use super::opacity_group_tests::{CssSource, css_value};
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    for mode in [1, 2] {
        for source in 0..3 {
            let mut context = super::main_text_clip_tests::context(true, 1);
            let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
            let alpha = Arc::new(Mutex::new(AnimatedValue::with_default(
                scheduler.lock().unwrap().handle(),
                0.5,
            )));
            let x = Arc::new(Mutex::new(AnimatedValue::with_default(
                scheduler.lock().unwrap().handle(),
                0.0,
            )));
            let root = div()
                .id("overlay")
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(120.0)
                .h(100.0)
                .bg(Color::RED)
                .child(
                    div().absolute().left(20.0).top(20.0).w(60.0).h(60.0).child(
                        blinc_layout::image(IMAGE)
                            .size(60.0, 60.0)
                            .fade_in(std::time::Duration::ZERO),
                    ),
                );
            let mut tree = if source == 0 {
                with_plate(
                    div()
                        .overlay_root()
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .child(root.motion().opacity(alpha.clone()).translate_x(x.clone())),
                )
            } else {
                with_plate(root.overlay_root())
            };
            let owner = tree.query_by_id("overlay").unwrap();
            tree.update_render_props(owner, |p| {
                p.clip_path = Some(blinc_core::ClipPath::Path {
                    vertices: vec![(0.0, 0.0), (120.0, 0.0), (0.0, 100.0)],
                })
            });
            let state = RenderState::new(scheduler);
            for (index, opacity) in [0.5, 0.0, 1.0, 0.75, 0.5].into_iter().enumerate() {
                let offset = if source == 0 {
                    [0, 5, 10, 0, 5][index]
                } else {
                    0
                };
                if source == 0 {
                    alpha.lock().unwrap().set_immediate(opacity);
                    x.lock().unwrap().set_immediate(offset as f32);
                } else {
                    css_value(
                        &tree,
                        owner,
                        if source == 1 {
                            CssSource::Animation
                        } else {
                            CssSource::Transition
                        },
                        Some(opacity),
                    );
                    tree.update_render_props(owner, |p| p.opacity = opacity);
                }
                for _ in 0..2 {
                    let page = frame(&mut context, &tree, &state, mode);
                    if page.get_pixel(60 + offset, 50).0 != blend([0.0, 1.0, 0.0], opacity, 1.0) {
                        eprintln!(
                            "live mode={mode} source={source} opacity={opacity} offset={offset} binds={:?}",
                            tree.motion_bindings_map().keys().collect::<Vec<_>>()
                        );
                    }
                    super::opacity_group_tests::pixel(
                        &page,
                        (60 + offset, 50),
                        blend([0.0, 1.0, 0.0], opacity, 1.0),
                    );
                    super::opacity_group_tests::pixel(&page, (95 + offset, 85), [255, 255, 0, 255]);
                    super::opacity_group_tests::pixel(
                        &page,
                        (25 + offset, 15),
                        blend([1.0, 0.0, 0.0], opacity, 1.0),
                    );
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_group_sibling_order_and_owner_replacement_do_not_leave_stale_output() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    for mode in 0..3 {
        for reverse in [false, true] {
            let first = div()
                .id("first")
                .overlay_root()
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(100.0)
                .h(100.0)
                .bg(Color::RED)
                .opacity(0.5)
                .child(
                    div().absolute().left(20.0).top(20.0).child(
                        blinc_layout::image(IMAGE)
                            .size(60.0, 60.0)
                            .fade_in(std::time::Duration::ZERO),
                    ),
                );
            let second = div()
                .overlay_root()
                .absolute()
                .left(40.0)
                .top(30.0)
                .w(80.0)
                .h(80.0)
                .bg(Color::BLUE)
                .opacity(0.5);
            let children = if reverse {
                div().child(second).child(first)
            } else {
                div().child(first).child(second)
            };
            let mut tree = with_plate(children);
            let owner = tree.query_by_id("first").unwrap();
            for active in [true, false, true] {
                tree.update_render_props(owner, |p| p.is_overlay_root = active);
                context.invalidate_render_cache();
                for _ in 0..2 {
                    let page = frame(&mut context, &tree, &state, mode);
                    let expected = if !active {
                        [128, 128, 128, 255]
                    } else if reverse {
                        [64, 191, 64, 255]
                    } else {
                        [64, 128, 128, 255]
                    };
                    super::opacity_group_tests::pixel(&page, (60, 50), expected);
                }
            }
        }
    }
}

#[test]
fn overlay_group_capabilities_and_native_ancestors_keep_distinct_owners() {
    super::opacity_group_tests::init_theme();
    for missing in 0..4 {
        for ancestor in 0..4 {
            let scheduler = blinc_animation::AnimationScheduler::new();
            let alpha = Arc::new(Mutex::new(blinc_animation::AnimatedValue::with_default(
                scheduler.handle(),
                0.5,
            )));
            let mut tree = if ancestor == 3 {
                with_plate(mixed_overlay().motion().opacity(alpha))
            } else {
                with_plate(mixed_overlay())
            };
            if ancestor > 0 && ancestor < 3 {
                tree.update_render_props(tree.query_by_id("host").unwrap(), |p| {
                    if ancestor == 1 {
                        p.opacity = 0.75;
                    } else {
                        p.layer_effects = vec![blinc_core::LayerEffect::brightness(0.5)];
                    }
                });
            }
            let mut paint = GpuPaintContext::new(160.0, 120.0);
            if missing != 1 {
                paint.enable_deferred_text();
            }
            if missing != 2 {
                paint.enable_deferred_svg();
            }
            if missing != 3 {
                paint.enable_deferred_image();
            }
            tree.render_snapshot(&mut paint);
            let bg = paint.take_batch();
            let dynamic = paint.take_dynamic_batch();
            assert!(bg.cross_stream_groups.text_nodes.is_empty());
            assert!(bg.cross_stream_groups.svg_nodes.is_empty());
            assert!(bg.cross_stream_groups.image_nodes.is_empty());
            let captured = missing == 0;
            assert_eq!(!dynamic.cross_stream_groups.text_nodes.is_empty(), captured);
            assert_eq!(!dynamic.cross_stream_groups.svg_nodes.is_empty(), captured);
            assert_eq!(
                !dynamic.cross_stream_groups.image_nodes.is_empty(),
                captured
            );
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_group_gpu_callbacks_and_meshes_keep_late_coverage_and_order() {
    use blinc_core::Mat4;
    for mesh_scene in [false, true] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let state = RenderState::new(Arc::new(Mutex::new(
            blinc_animation::AnimationScheduler::new(),
        )));
        for mode in 0..3 {
            for foreground in [false, true] {
                let triangle = super::mesh_group_tests::mesh([1.0, 1.0, 0.0]);
                let pass = super::release_boundary_tests::marker_pass();
                let mut root = div()
                    .id("overlay")
                    .overlay_root()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(120.0)
                    .h(100.0)
                    .bg(Color::RED)
                    .child(
                        canvas(move |ctx, _| {
                            if mesh_scene {
                                ctx.set_camera(&super::mesh_group_tests::camera());
                                ctx.set_3d_viewport_bounds(80.0, 80.0);
                                ctx.draw_mesh_data(triangle.clone(), Mat4::IDENTITY);
                            } else {
                                ctx.run_gpu_pass(&pass, Some(Rect::new(20.0, 20.0, 60.0, 60.0)));
                            }
                            ctx.fill_rect(
                                Rect::new(30.0, 30.0, 20.0, 20.0),
                                0.0.into(),
                                Color::GREEN.into(),
                            );
                        })
                        .absolute()
                        .w(120.0)
                        .h(100.0),
                    );
                if foreground {
                    root = root.foreground();
                }
                let mut tree = with_plate(root);
                let owner = tree.query_by_id("overlay").unwrap();
                context.invalidate_render_cache();
                let reference = frame(&mut context, &tree, &state, mode).get_pixel(65, 65).0;
                if mesh_scene {
                    assert!(
                        reference[0] > 150
                            && reference[1] > 150
                            && reference[0] > reference[2] + 20,
                        "mesh output missing: {reference:?}"
                    );
                } else {
                    assert_eq!(reference, [0, 255, 255, 255]);
                }
                for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                    tree.update_render_props(owner, |p| p.opacity = opacity);
                    context.invalidate_render_cache();
                    for _ in 0..2 {
                        let page = frame(&mut context, &tree, &state, mode);
                        super::opacity_group_tests::pixel(
                            &page,
                            (65, 65),
                            blend(
                                [
                                    reference[0] as f32 / 255.0,
                                    reference[1] as f32 / 255.0,
                                    reference[2] as f32 / 255.0,
                                ],
                                opacity,
                                1.0,
                            ),
                        );
                        super::opacity_group_tests::pixel(
                            &page,
                            (60, 50),
                            blend([0.0, 1.0, 0.0], opacity, 1.0),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn overlay_group_unsupported_materials_transforms_and_nested_roots_keep_fallback() {
    super::opacity_group_tests::init_theme();
    for boundary in 0..4 {
        let mut tree = with_plate(mixed_overlay());
        let owner = tree.query_by_id("overlay").unwrap();
        if boundary == 3 {
            tree.update_render_props(tree.query_by_id("image-host").unwrap(), |p| {
                p.is_overlay_root = true
            });
        } else {
            tree.update_render_props(owner, |p| match boundary {
                0 => p.material = Some(blinc_layout::Material::Glass(GlassMaterial::new())),
                1 => p.rotate_y = Some(20.0),
                _ => p.flow = Some("separate-pass".into()),
            });
        }
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        paint.enable_deferred_text();
        paint.enable_deferred_svg();
        paint.enable_deferred_image();
        tree.render_snapshot(&mut paint);
        for batch in [paint.take_batch(), paint.take_dynamic_batch()] {
            assert!(!batch.has_cross_stream_groups());
            assert!(batch.cross_stream_groups.text_nodes.is_empty());
            assert!(batch.cross_stream_groups.svg_nodes.is_empty());
            assert!(batch.cross_stream_groups.image_nodes.is_empty());
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_group_entry_motion_and_snapshot_frames_keep_separate_opacity() {
    use blinc_layout::{MotionAnimation, MotionKeyframe};
    for mode in [1, 2] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let mut tree = with_plate(mixed_overlay().opacity(1.0));
        let owner = tree.query_by_id("overlay").unwrap();
        let animation = MotionAnimation {
            enter_from: Some(MotionKeyframe::new().opacity(0.25)),
            enter_duration_ms: 1000,
            ..Default::default()
        };
        tree.update_render_props(owner, |p| p.motion = Some(animation.clone()));
        let mut state = RenderState::new(Arc::new(Mutex::new(
            blinc_animation::AnimationScheduler::new(),
        )));
        state.start_enter_motion(owner, animation);
        for _ in 0..3 {
            let page = frame(&mut context, &tree, &state, mode);
            super::opacity_group_tests::pixel(&page, (60, 75), blend([0.0, 1.0, 0.0], 0.25, 1.0));
            let page = frame(&mut context, &tree, &state, 0);
            super::opacity_group_tests::pixel(&page, (60, 75), [0, 255, 0, 255]);
            assert_eq!(state.get_motion_values(owner).unwrap().opacity, Some(0.25));
        }
    }
}
