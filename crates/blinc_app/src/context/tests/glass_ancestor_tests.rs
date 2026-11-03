//! Foreground glass ancestors retain their paint and child content in one scope.
use super::glass_group_tests::{frame, material, state};
use super::opacity_group_tests::pixel;
use super::*;

fn panel() -> Div {
    div()
        .id("panel")
        .absolute()
        .left(20.0)
        .top(10.0)
        .w(120.0)
        .h(100.0)
        .effect(material(true))
        .child(
            div().absolute().left(20.0).top(20.0).w(60.0).h(60.0).child(
                blinc_layout::image(super::overlay_group_tests::IMAGE)
                    .size(60.0, 60.0)
                    .fade_in(std::time::Duration::ZERO),
            ),
        )
}

fn scene() -> RenderTree {
    let mut tree = RenderTree::from_element(
        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .id("owner")
                .foreground()
                .absolute()
                .w(160.0)
                .h(120.0)
                .bg(Color::RED)
                .opacity(0.5)
                .child(panel()),
        ),
    );
    tree.compute_layout(160.0, 120.0);
    tree
}

#[test]
#[ignore] // Requires GPU
fn glass_ancestor_foreground_paint_and_panel_apply_joint_opacity() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        let page = frame(&mut context, &scene(), &state, mode);
        pixel(&page, (60, 60), [0, 128, 128, 255]);
        pixel(&page, (10, 5), [128, 0, 128, 255]);
        pixel(&page, (25, 100), [64, 64, 255, 255]);
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_ancestor_removing_material_keeps_opaque_collected_image_order() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        let mut tree = RenderTree::from_element(
            &div()
                .w(160.0)
                .h(120.0)
                .bg(Color::BLUE)
                .child(
                    div()
                        .id("owner")
                        .foreground()
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .bg(Color::RED)
                        .child(panel()),
                )
                .child(
                    div()
                        .absolute()
                        .left(150.0)
                        .w(10.0)
                        .h(10.0)
                        .effect(material(true)),
                ),
        );
        tree.compute_layout(160.0, 120.0);
        tree.update_render_props(tree.query_by_id("panel").unwrap(), |p| {
            p.material = None;
            p.layer = RenderLayer::Background;
            p.background = Some(Brush::Solid(Color::MAGENTA));
        });
        let page = frame(&mut context, &tree, &state, mode);
        pixel(&page, (60, 60), [0, 255, 0, 255]);
        pixel(&page, (10, 5), [255, 0, 0, 255]);
    }
}

fn blend(color: [u8; 4], alpha: f32) -> [u8; 4] {
    [
        (color[0] as f32 * alpha).round() as u8,
        (color[1] as f32 * alpha).round() as u8,
        (color[2] as f32 * alpha + 255.0 * (1.0 - alpha)).round() as u8,
        255,
    ]
}

fn mixed(simple: bool) -> RenderTree {
    let mut tree = RenderTree::from_element(
        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .id("owner")
                .foreground()
                .absolute()
                .w(160.0)
                .h(120.0)
                .bg(Color::RED)
                .child(div().absolute().w(15.0).h(120.0).bg(Color::CYAN))
                .child(super::overlay_group_tests::mixed_overlay().effect(material(simple)))
                .child(
                    div()
                        .absolute()
                        .left(110.0)
                        .top(80.0)
                        .w(30.0)
                        .h(30.0)
                        .bg(Color::WHITE),
                ),
        ),
    );
    tree.compute_layout(160.0, 120.0);
    tree.update_render_props(tree.query_by_id("overlay").unwrap(), |p| {
        p.is_overlay_root = false
    });
    tree
}

#[test]
#[ignore] // Requires GPU
fn glass_ancestor_mixed_siblings_effects_dpi_and_opacity_endpoints() {
    for unified in [false, true] {
        for samples in [1, 4] {
            let mut context = super::main_text_clip_tests::context(unified, samples);
            let state = state();
            for mode in 0..4 {
                for simple in [false, true] {
                    let mut tree = mixed(simple);
                    let owner = tree.query_by_id("owner").unwrap();
                    for scale in [1, 2] {
                        tree.set_scale_factor(scale as f32);
                        for brightness in [1.0, 0.5] {
                            tree.update_render_props(owner, |p| {
                                p.opacity = 1.0;
                                p.layer_effects =
                                    vec![blinc_core::LayerEffect::brightness(brightness)];
                            });
                            context.invalidate_render_cache();
                            let baseline = frame(&mut context, &tree, &state, mode);
                            pixel(
                                &baseline,
                                (10 * scale, 5 * scale),
                                [
                                    0,
                                    (255.0 * brightness).round() as u8,
                                    (255.0 * brightness).round() as u8,
                                    255,
                                ],
                            );
                            pixel(
                                &baseline,
                                (120 * scale, 100 * scale),
                                [
                                    (255.0 * brightness).round() as u8,
                                    (255.0 * brightness).round() as u8,
                                    (255.0 * brightness).round() as u8,
                                    255,
                                ],
                            );
                            for alpha in [0.5, 0.0, 1.0, 0.75, 0.5] {
                                tree.update_render_props(owner, |p| p.opacity = alpha);
                                context.invalidate_render_cache();
                                for _ in 0..2 {
                                    let page = frame(&mut context, &tree, &state, mode);
                                    for y in 0..120 * scale {
                                        for x in 0..160 * scale {
                                            pixel(
                                                &page,
                                                (x, y),
                                                blend(baseline.get_pixel(x, y).0, alpha),
                                            );
                                        }
                                    }
                                }
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
fn glass_ancestor_keeps_external_background_and_foreground_order() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        for order in 0..3 {
            let owner = div()
                .foreground()
                .absolute()
                .w(160.0)
                .h(120.0)
                .bg(Color::RED)
                .opacity(0.5)
                .child(panel());
            let other = div().absolute().w(160.0).h(120.0).bg(if order == 0 {
                Color::YELLOW
            } else {
                Color::CYAN
            });
            let other = if order > 0 { other.foreground() } else { other };
            let ui = div().w(160.0).h(120.0).bg(Color::BLUE);
            let ui = if order == 1 {
                ui.child(other).child(owner)
            } else {
                ui.child(owner).child(other)
            };
            let mut tree = RenderTree::from_element(&ui);
            tree.compute_layout(160.0, 120.0);
            for _ in 0..3 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(
                    &page,
                    (60, 60),
                    match order {
                        0 => [128, 255, 0, 255],
                        1 => [0, 255, 128, 255],
                        _ => [0, 255, 255, 255],
                    },
                );
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_ancestor_multiple_panels_keep_sibling_order() {
    let mut context = super::main_text_clip_tests::context(true, 4);
    let state = state();
    for mode in 0..4 {
        for reverse in [false, true] {
            let first = panel();
            let second = div()
                .absolute()
                .left(40.0)
                .top(30.0)
                .w(80.0)
                .h(60.0)
                .effect(material(false))
                .opacity(0.5)
                .child(div().absolute().w(80.0).h(60.0).bg(Color::WHITE));
            let owner = div()
                .foreground()
                .absolute()
                .w(160.0)
                .h(120.0)
                .bg(Color::RED)
                .opacity(0.5);
            let owner = if reverse {
                owner.child(second).child(first)
            } else {
                owner.child(first).child(second)
            };
            let mut tree =
                RenderTree::from_element(&div().w(160.0).h(120.0).bg(Color::BLUE).child(owner));
            tree.compute_layout(160.0, 120.0);
            for _ in 0..2 {
                pixel(
                    &frame(&mut context, &tree, &state, mode),
                    (65, 60),
                    if reverse {
                        [0, 128, 128, 255]
                    } else {
                        [64, 128, 191, 255]
                    },
                );
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_ancestor_binding_css_clips_and_zero_recovery_keep_the_owner() {
    use super::opacity_group_tests::{CssSource, css_value};
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    for mode in 0..4 {
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
            let owner = div()
                .id("owner")
                .foreground()
                .absolute()
                .w(160.0)
                .h(120.0)
                .bg(Color::RED)
                .child(panel());
            let ui = div().w(160.0).h(120.0).bg(Color::BLUE);
            let mut tree = if source == 0 {
                RenderTree::from_element(
                    &ui.child(owner.motion().opacity(alpha.clone()).translate_x(x.clone())),
                )
            } else {
                RenderTree::from_element(&ui.child(owner))
            };
            tree.compute_layout(160.0, 120.0);
            let owner = tree.query_by_id("owner").unwrap();
            tree.update_render_props(owner, |p| {
                p.clip_path = Some(blinc_core::ClipPath::Path {
                    vertices: vec![(0.0, 0.0), (160.0, 0.0), (0.0, 120.0)],
                })
            });
            let state = RenderState::new(scheduler);
            for (opacity, offset) in [(0.5, 0), (0.0, 5), (1.0, 10), (0.75, 0), (0.5, 5)] {
                let offset = if source == 0 {
                    alpha.lock().unwrap().set_immediate(opacity);
                    x.lock().unwrap().set_immediate(offset as f32);
                    offset
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
                    tree.apply_all_css_animation_props();
                    tree.apply_all_css_transition_props();
                    0
                };
                for _ in 0..2 {
                    let page = frame(&mut context, &tree, &state, mode);
                    pixel(&page, (60 + offset, 60), blend([0, 255, 0, 255], opacity));
                    pixel(&page, (140 + offset, 100), [0, 0, 255, 255]);
                    if offset > 0 {
                        pixel(&page, (2, 5), [0, 0, 255, 255]);
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_ancestor_entry_snapshots_and_material_replacement_keep_state() {
    use blinc_layout::{MotionAnimation, MotionKeyframe};
    let mut context = super::main_text_clip_tests::context(true, 1);
    for mode in 1..4 {
        let mut tree = scene();
        let owner = tree.query_by_id("owner").unwrap();
        let panel = tree.query_by_id("panel").unwrap();
        let animation = MotionAnimation {
            enter_from: Some(MotionKeyframe::new().opacity(0.25)),
            enter_duration_ms: 1000,
            ..Default::default()
        };
        tree.update_render_props(owner, |p| {
            p.opacity = 1.0;
            p.motion = Some(animation.clone());
        });
        let mut state = state();
        state.start_enter_motion(owner, animation);
        for _ in 0..3 {
            pixel(
                &frame(&mut context, &tree, &state, mode),
                (60, 60),
                blend([0, 255, 0, 255], 0.25),
            );
            pixel(
                &frame(&mut context, &tree, &state, 0),
                (60, 60),
                [0, 255, 0, 255],
            );
            assert_eq!(state.get_motion_values(owner).unwrap().opacity, Some(0.25));
        }
        tree.update_render_props(owner, |p| p.motion = None);
        for glass in [false, true, false, true] {
            tree.update_render_props(panel, |p| {
                p.material = glass.then(|| blinc_layout::Material::Glass(material(true)));
                p.layer = if glass {
                    RenderLayer::Glass
                } else {
                    RenderLayer::Background
                };
                p.background = (!glass).then_some(Brush::Solid(Color::MAGENTA));
            });
            context.invalidate_render_cache();
            pixel(
                &frame(&mut context, &tree, &state, 0),
                (60, 60),
                [0, 255, 0, 255],
            );
        }
    }
}

#[test]
fn glass_ancestor_admission_requires_compatible_foreground_content() {
    use blinc_gpu::primitives::cross_stream::GroupId;
    use slotmap::Key;
    super::opacity_group_tests::init_theme();
    for boundary in 0..11 {
        let mut tree = mixed(true);
        let owner = tree.query_by_id("owner").unwrap();
        let panel = tree.query_by_id("overlay").unwrap();
        match boundary {
            1 => tree.update_render_props(owner, |p| p.layer = RenderLayer::Background),
            2 => tree.update_render_props(tree.layout_tree.children(owner)[0], |p| {
                p.rotate_x = Some(20.0)
            }),
            3 => tree.update_render_props(tree.query_by_id("image-host").unwrap(), |p| {
                p.material = Some(blinc_layout::Material::Glass(material(true)))
            }),
            4 => tree.update_render_props(owner, |p| p.rotate_x = Some(20.0)),
            5 => tree.update_render_props(owner, |p| p.rotate_y = Some(20.0)),
            6 => tree.update_render_props(owner, |p| p.is_overlay_root = true),
            10 => tree.update_render_props(panel, |p| {
                p.material = None;
                p.layer = RenderLayer::Background;
                p.background = Some(Brush::Solid(Color::MAGENTA));
            }),
            _ => {}
        }
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        if boundary != 7 {
            paint.enable_deferred_text();
        }
        if boundary != 8 {
            paint.enable_deferred_image();
        }
        paint.enable_deferred_svg();
        if boundary != 9 {
            paint.enable_glass_layers();
        }
        tree.render_snapshot(&mut paint);
        let captured = paint
            .batch()
            .cross_stream_groups
            .captures
            .iter()
            .any(|c| c.id == GroupId::Node(owner.data().as_ffi()));
        assert_eq!(
            captured,
            boundary == 0 || boundary == 3 || boundary == 10,
            "boundary={boundary}"
        );
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_ancestor_gpu_siblings_keep_coverage_and_execute_once() {
    for mesh_scene in [false, true] {
        let mut context = super::main_text_clip_tests::context(true, 4);
        let state = state();
        for mode in 0..4 {
            let (pass, calls) = super::gpu_pass_group_tests::clear_pass(wgpu::Color {
                r: 1.0,
                g: 0.0,
                b: 1.0,
                a: 1.0,
            });
            let mesh = super::mesh_group_tests::mesh([1.0, 1.0, 0.0]);
            let mut tree = RenderTree::from_element(
                &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                    div()
                        .id("owner")
                        .foreground()
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .bg(Color::RED)
                        .child(panel())
                        .child(
                            div()
                                .absolute()
                                .left(100.0)
                                .top(80.0)
                                .w(40.0)
                                .h(30.0)
                                .overflow_clip()
                                .child(
                                    canvas(move |ctx, _| {
                                        if mesh_scene {
                                            ctx.set_camera(&super::mesh_group_tests::camera());
                                            ctx.set_3d_viewport_bounds(40.0, 30.0);
                                            ctx.draw_mesh_data(
                                                mesh.clone(),
                                                blinc_core::Mat4::IDENTITY,
                                            );
                                        } else {
                                            ctx.run_gpu_pass(&pass, None);
                                        }
                                        ctx.fill_rect(
                                            Rect::new(10.0, 10.0, 10.0, 10.0),
                                            0.0.into(),
                                            Color::GREEN.into(),
                                        );
                                    })
                                    .absolute()
                                    .w(40.0)
                                    .h(30.0),
                                ),
                        ),
                ),
            );
            tree.compute_layout(160.0, 120.0);
            let owner = tree.query_by_id("owner").unwrap();
            for scale in [1, 2] {
                tree.set_scale_factor(scale as f32);
                tree.update_render_props(owner, |p| p.opacity = 1.0);
                context.invalidate_render_cache();
                let baseline = frame(&mut context, &tree, &state, mode);
                let point = if mesh_scene {
                    (80 * scale..110 * scale)
                        .flat_map(|y| (100 * scale..140 * scale).map(move |x| (x, y)))
                        .find(|(x, y)| {
                            let c = baseline.get_pixel(*x, *y).0;
                            c[0] > 150 && c[1] > 150 && c[2] < 100
                        })
                        .expect("mesh output missing")
                } else {
                    (125 * scale, 100 * scale)
                };
                let color = baseline.get_pixel(point.0, point.1).0;
                if !mesh_scene {
                    assert_eq!(color, [255, 0, 255, 255]);
                }
                for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                    tree.update_render_props(owner, |p| p.opacity = opacity);
                    context.invalidate_render_cache();
                    for _ in 0..2 {
                        let before = calls.lock().unwrap().len();
                        let page = frame(&mut context, &tree, &state, mode);
                        pixel(&page, point, blend(color, opacity));
                        pixel(
                            &page,
                            (115 * scale, 95 * scale),
                            blend([0, 255, 0, 255], opacity),
                        );
                        pixel(
                            &page,
                            (145 * scale, 110 * scale),
                            blend([255, 0, 0, 255], opacity),
                        );
                        if !mesh_scene && opacity > 0.0 {
                            assert_eq!(calls.lock().unwrap().len(), before + 1);
                            assert_eq!(
                                *calls.lock().unwrap().last().unwrap(),
                                (160 * scale, 120 * scale, scale as f64)
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
fn glass_ancestor_spatial_effects_keep_the_outer_clip_frame() {
    let mut context = super::main_text_clip_tests::context(true, 4);
    let state = state();
    for mode in 0..4 {
        for scale in [1, 2] {
            let mut tree = RenderTree::from_element(
                &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                    div()
                        .absolute()
                        .left(30.0)
                        .top(20.0)
                        .w(80.0)
                        .h(60.0)
                        .rounded(6.0)
                        .overflow_clip()
                        .child(
                            div()
                                .id("owner")
                                .foreground()
                                .absolute()
                                .left(-30.0)
                                .top(-20.0)
                                .w(160.0)
                                .h(120.0)
                                .bg(Color::RED)
                                .opacity(0.5)
                                .child(panel()),
                        ),
                ),
            );
            tree.compute_layout(160.0, 120.0);
            tree.set_scale_factor(scale as f32);
            tree.update_render_props(tree.query_by_id("owner").unwrap(), |p| {
                p.layer_effects = vec![blinc_core::LayerEffect::blur(4.0)]
            });
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(&page, (26 * scale, 50 * scale), [0, 0, 255, 255]);
                pixel(&page, (114 * scale, 50 * scale), [0, 0, 255, 255]);
                pixel(&page, (70 * scale, 50 * scale), [0, 128, 128, 255]);
            }
        }
    }
}
