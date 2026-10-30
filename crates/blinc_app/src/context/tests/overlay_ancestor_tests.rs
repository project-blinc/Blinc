//! Portals preserve native ancestor state while retaining their late paint order.
use super::gpu_pass_group_tests::frame;
use super::opacity_group_tests::pixel;
use super::overlay_group_tests::{IMAGE, blend, mixed_overlay, with_plate};
use super::*;

#[test]
#[ignore] // Requires GPU
fn overlay_ancestor_live_motion_keeps_collected_content_and_zero_recovery() {
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    for mode in [1, 2] {
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
        let tree = with_plate(
            mixed_overlay()
                .opacity(1.0)
                .motion()
                .opacity(alpha.clone())
                .translate_x(x.clone()),
        );
        let state = RenderState::new(scheduler);
        for (opacity, offset) in [(0.5, 0), (0.0, 5), (1.0, 10), (0.75, 0), (0.5, 5)] {
            alpha.lock().unwrap().set_immediate(opacity);
            x.lock().unwrap().set_immediate(offset as f32);
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(
                    &page,
                    (60 + offset, 75),
                    blend([0.0, 1.0, 0.0], opacity, 1.0),
                );
                pixel(
                    &page,
                    (55 + offset, 45),
                    blend([1.0, 0.0, 1.0], opacity, 1.0),
                );
                pixel(
                    &page,
                    (25 + offset, 105),
                    blend([1.0, 0.0, 0.0], opacity, 1.0),
                );
                pixel(&page, (5, 5), [255, 255, 0, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_ancestor_opacity_and_effects_reach_both_streams_once() {
    for unified in [false, true] {
        for samples in [1, 4] {
            let mut context = super::main_text_clip_tests::context(unified, samples);
            let state = RenderState::new(Arc::new(Mutex::new(
                blinc_animation::AnimationScheduler::new(),
            )));
            for mode in 0..3 {
                for foreground in [false, true] {
                    let parent = div()
                        .id("ancestor")
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .child(mixed_overlay());
                    let mut tree = with_plate(if foreground {
                        parent.foreground()
                    } else {
                        parent
                    });
                    let owner = tree.query_by_id("ancestor").unwrap();
                    for scale in [1, 2] {
                        tree.set_scale_factor(scale as f32);
                        for (opacity, brightness) in
                            [(0.5, 1.0), (0.0, 1.0), (1.0, 1.0), (0.75, 0.5), (0.5, 1.0)]
                        {
                            tree.update_render_props(owner, |p| {
                                p.opacity = opacity;
                                p.layer_effects =
                                    vec![blinc_core::LayerEffect::brightness(brightness)];
                            });
                            context.invalidate_render_cache();
                            for _ in 0..2 {
                                let page = frame(&mut context, &tree, &state, mode);
                                for (xy, color) in [
                                    ((60, 75), [0.0, 1.0, 0.0]),
                                    ((55, 45), [1.0, 0.0, 1.0]),
                                    ((105, 85), [1.0, 1.0, 0.0]),
                                    ((25, 105), [1.0, 0.0, 0.0]),
                                ] {
                                    pixel(
                                        &page,
                                        (xy.0 * scale, xy.1 * scale),
                                        blend(color, opacity * 0.5, brightness),
                                    );
                                }
                                let blue = (12 * scale..30 * scale)
                                    .flat_map(|y| (25 * scale..43 * scale).map(move |x| (x, y)))
                                    .map(|(x, y)| page.get_pixel(x, y).0[2])
                                    .max()
                                    .unwrap();
                                assert!(
                                    (blue as f32 - opacity * 0.5 * brightness * 255.0).abs() < 5.0,
                                    "text missing or double alpha: {blue}"
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
fn overlay_ancestor_css_endpoints_keep_live_portal_content() {
    use super::opacity_group_tests::{CssSource, css_value};
    for mode in [1, 2] {
        for source in [CssSource::Animation, CssSource::Transition] {
            let mut context = super::main_text_clip_tests::context(true, 1);
            let state = RenderState::new(Arc::new(Mutex::new(
                blinc_animation::AnimationScheduler::new(),
            )));
            let mut tree = with_plate(
                div()
                    .id("ancestor")
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .child(mixed_overlay().opacity(1.0)),
            );
            let owner = tree.query_by_id("ancestor").unwrap();
            for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                css_value(&tree, owner, source, Some(opacity));
                tree.update_render_props(owner, |p| p.opacity = opacity);
                for _ in 0..2 {
                    let page = frame(&mut context, &tree, &state, mode);
                    pixel(&page, (60, 75), blend([0.0, 1.0, 0.0], opacity, 1.0));
                    pixel(&page, (55, 45), blend([1.0, 0.0, 1.0], opacity, 1.0));
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_ancestor_scope_restores_siblings_and_keeps_portals_independent() {
    for mode in 0..3 {
        for reverse in [false, true] {
            let mut context = super::main_text_clip_tests::context(true, 1);
            let state = RenderState::new(Arc::new(Mutex::new(
                blinc_animation::AnimationScheduler::new(),
            )));
            let first = mixed_overlay().opacity(1.0);
            let second = div()
                .overlay_root()
                .absolute()
                .left(40.0)
                .top(30.0)
                .w(80.0)
                .h(80.0)
                .bg(Color::BLUE);
            let parent = div()
                .id("ancestor")
                .absolute()
                .w(160.0)
                .h(120.0)
                .opacity(0.5)
                .child(div().absolute().w(15.0).h(120.0).bg(Color::CYAN));
            let parent = if reverse {
                parent.child(second).child(first)
            } else {
                parent.child(first).child(second)
            };
            let mut tree = RenderTree::from_element(
                &div()
                    .w(160.0)
                    .h(120.0)
                    .bg(Color::BLUE)
                    .child(
                        parent.child(
                            div()
                                .absolute()
                                .left(145.0)
                                .w(15.0)
                                .h(120.0)
                                .bg(Color::CYAN)
                                .child(
                                    blinc_layout::image(IMAGE)
                                        .size(15.0, 120.0)
                                        .fade_in(std::time::Duration::ZERO),
                                ),
                        ),
                    )
                    .child(
                        div()
                            .absolute()
                            .w(110.0)
                            .h(120.0)
                            .bg(Color::YELLOW)
                            .foreground(),
                    ),
            );
            tree.compute_layout(160.0, 120.0);
            for _ in 0..3 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(&page, (5, 50), [255, 255, 0, 255]);
                pixel(&page, (150, 50), [0, 128, 128, 255]);
                pixel(
                    &page,
                    (60, 75),
                    if reverse {
                        [64, 191, 64, 255]
                    } else {
                        [64, 128, 128, 255]
                    },
                );
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_ancestor_clips_and_nested_effects_match_local_portal_state() {
    for mode in 0..3 {
        let mut context = super::main_text_clip_tests::context(true, 4);
        let state = RenderState::new(Arc::new(Mutex::new(
            blinc_animation::AnimationScheduler::new(),
        )));
        let mut expected: Option<image::RgbaImage> = None;
        for ancestor in [false, true] {
            let mut tree = with_plate(
                div()
                    .id("outer")
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .opacity(0.5)
                    .child(
                        div()
                            .id("inner")
                            .absolute()
                            .w(160.0)
                            .h(120.0)
                            .child(mixed_overlay()),
                    ),
            );
            let outer = tree.query_by_id("outer").unwrap();
            let inner = tree.query_by_id("inner").unwrap();
            let portal = tree.query_by_id("overlay").unwrap();
            // Move the portal boundary without changing group geometry or effect order.
            tree.update_render_props(portal, |p| p.is_overlay_root = ancestor);
            tree.update_render_props(inner, |p| {
                p.layer_effects = vec![
                    blinc_core::LayerEffect::brightness(0.5),
                    blinc_core::LayerEffect::blur(2.0),
                ]
            });
            tree.update_render_props(outer, |p| {
                p.is_overlay_root = !ancestor;
                p.transform = Some(Transform::translate(5.0, 0.0));
                p.clip_path = Some(blinc_core::ClipPath::Path {
                    vertices: vec![(0.0, 0.0), (160.0, 0.0), (0.0, 120.0)],
                });
            });
            let page = frame(&mut context, &tree, &state, mode);
            if let Some(expected) = &expected {
                for y in 0..120 {
                    for x in 0..160 {
                        pixel(&page, (x, y), expected.get_pixel(x, y).0);
                    }
                }
                pixel(&page, (65, 65), blend([0.0, 1.0, 0.0], 0.25, 0.5));
                pixel(&page, (105, 95), [255, 255, 0, 255]);
            } else {
                expected = Some(page);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_ancestor_entry_motion_and_snapshot_frames_keep_independent_state() {
    use blinc_layout::{MotionAnimation, MotionKeyframe};
    for mode in [1, 2] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let mut tree = with_plate(
            div()
                .id("ancestor")
                .absolute()
                .w(160.0)
                .h(120.0)
                .child(mixed_overlay()),
        );
        let owner = tree.query_by_id("ancestor").unwrap();
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
            pixel(&page, (60, 75), blend([0.0, 1.0, 0.0], 0.125, 1.0));
            let page = frame(&mut context, &tree, &state, 0);
            pixel(&page, (60, 75), blend([0.0, 1.0, 0.0], 0.5, 1.0));
            assert_eq!(state.get_motion_values(owner).unwrap().opacity, Some(0.25));
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_ancestor_gpu_callbacks_and_meshes_match_local_group_effects() {
    for mesh_scene in [false, true] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let state = RenderState::new(Arc::new(Mutex::new(
            blinc_animation::AnimationScheduler::new(),
        )));
        for mode in 0..3 {
            let mut expected: Option<image::RgbaImage> = None;
            for ancestor in [false, true] {
                let mesh = super::mesh_group_tests::mesh([1.0, 1.0, 0.0]);
                let pass = super::release_boundary_tests::marker_pass();
                let portal = div()
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
                                ctx.draw_mesh_data(mesh.clone(), blinc_core::Mat4::IDENTITY);
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
                let mut tree = if ancestor {
                    with_plate(
                        div()
                            .id("ancestor")
                            .absolute()
                            .w(160.0)
                            .h(120.0)
                            .opacity(0.5)
                            .child(portal.opacity(0.5)),
                    )
                } else {
                    with_plate(portal.opacity(0.25))
                };
                let owner = tree
                    .query_by_id(if ancestor { "ancestor" } else { "overlay" })
                    .unwrap();
                tree.update_render_props(owner, |p| {
                    p.layer_effects = vec![blinc_core::LayerEffect::brightness(0.5)]
                });
                let page = frame(&mut context, &tree, &state, mode);
                if let Some(expected) = &expected {
                    for y in 0..120 {
                        for x in 0..160 {
                            pixel(&page, (x, y), expected.get_pixel(x, y).0);
                        }
                    }
                    pixel(&page, (60, 50), blend([0.0, 1.0, 0.0], 0.25, 0.5));
                } else {
                    expected = Some(page);
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_ancestor_blends_match_local_portal_backdrops() {
    use blinc_core::BlendMode;
    for mode in 0..3 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let state = RenderState::new(Arc::new(Mutex::new(
            blinc_animation::AnimationScheduler::new(),
        )));
        for blend_mode in [
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Difference,
        ] {
            let mut expected: Option<image::RgbaImage> = None;
            for ancestor in [false, true] {
                let mut tree = with_plate(
                    div()
                        .id("ancestor")
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .opacity(0.5)
                        .child(mixed_overlay()),
                );
                let outer = tree.query_by_id("ancestor").unwrap();
                let portal = tree.query_by_id("overlay").unwrap();
                tree.update_render_props(outer, |p| {
                    p.mix_blend_mode = Some(blend_mode);
                    p.is_overlay_root = !ancestor;
                });
                tree.update_render_props(portal, |p| p.is_overlay_root = ancestor);
                let page = frame(&mut context, &tree, &state, mode);
                if let Some(expected) = &expected {
                    for y in 0..120 {
                        for x in 0..160 {
                            pixel(&page, (x, y), expected.get_pixel(x, y).0);
                        }
                    }
                } else {
                    expected = Some(page);
                }
            }
        }
    }
}
