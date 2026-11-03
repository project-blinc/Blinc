//! Nested glass samples its visual owner's prior paint and shares its isolation.
use super::glass_group_tests::{frame, material, state};
use super::opacity_group_tests::pixel;
use super::separate_pass_flow;
use super::*;

fn scene(simple: bool) -> RenderTree {
    let mut tree =
        RenderTree::from_element(&div().w(160.0).h(120.0).bg(Color::BLUE).child(panel(simple)));
    tree.compute_layout(160.0, 120.0);
    tree
}

fn panel(simple: bool) -> Div {
    div()
        .id("outer")
        .absolute()
        .left(20.0)
        .top(10.0)
        .w(120.0)
        .h(100.0)
        .effect(material(simple))
        .opacity(0.5)
        .child(
            div()
                .id("inner")
                .absolute()
                .left(20.0)
                .top(20.0)
                .w(60.0)
                .h(60.0)
                .effect(material(simple).tint(Color::TRANSPARENT))
                .opacity(0.5)
                .child(
                    blinc_layout::image(super::overlay_group_tests::IMAGE)
                        .size(60.0, 60.0)
                        .fade_in(std::time::Duration::ZERO),
                ),
        )
}

#[test]
#[ignore] // Requires GPU
fn glass_nested_parent_and_child_apply_opacity_once() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        let page = frame(&mut context, &scene(true), &state, mode);
        pixel(&page, (60, 60), [32, 96, 191, 255]);
        pixel(&page, (25, 100), [64, 64, 255, 255]);
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

#[test]
#[ignore] // Requires GPU
fn glass_nested_samples_prior_native_paint_and_keeps_first_level_backdrops() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        for after in [false, true] {
            let outer = div()
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(120.0)
                .h(100.0)
                .effect(material(true))
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
                    div()
                        .absolute()
                        .left(20.0)
                        .top(20.0)
                        .w(60.0)
                        .h(60.0)
                        .effect(material(true).tint(Color::TRANSPARENT)),
                )
                .child(
                    div()
                        .absolute()
                        .left(20.0)
                        .top(20.0)
                        .w(10.0)
                        .h(10.0)
                        .bg(Color::GREEN),
                );
            let independent = div()
                .absolute()
                .left(60.0)
                .top(50.0)
                .w(20.0)
                .h(20.0)
                .effect(material(true).tint(Color::TRANSPARENT));
            let ui = div().w(160.0).h(120.0).bg(Color::BLUE);
            let ui = if after {
                ui.child(outer).child(independent)
            } else {
                ui.child(independent).child(outer)
            };
            let mut tree = RenderTree::from_element(&ui);
            tree.compute_layout(160.0, 120.0);
            for _ in 0..3 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(&page, (55, 45), [128, 0, 128, 255]);
                pixel(&page, (45, 35), [0, 128, 128, 255]);
                pixel(
                    &page,
                    (65, 55),
                    if after {
                        [0, 0, 255, 255]
                    } else {
                        [128, 0, 128, 255]
                    },
                );
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_nested_mixed_content_effects_and_opacity_endpoints() {
    for unified in [false, true] {
        for samples in [1, 4] {
            let mut context = super::main_text_clip_tests::context(unified, samples);
            let state = state();
            for mode in 0..4 {
                for simple in [false, true] {
                    let mut tree = RenderTree::from_element(
                        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                            div()
                                .id("outer")
                                .absolute()
                                .w(160.0)
                                .h(120.0)
                                .effect(material(simple))
                                .child(
                                    super::overlay_group_tests::mixed_overlay()
                                        .effect(material(simple).tint(Color::TRANSPARENT)),
                                ),
                        ),
                    );
                    tree.compute_layout(160.0, 120.0);
                    let outer = tree.query_by_id("outer").unwrap();
                    tree.update_render_props(tree.query_by_id("overlay").unwrap(), |p| {
                        p.is_overlay_root = false;
                        p.opacity = 1.0;
                    });
                    for scale in [1, 2] {
                        tree.set_scale_factor(scale as f32);
                        for brightness in [1.0, 0.5] {
                            tree.update_render_props(outer, |p| {
                                p.opacity = 1.0;
                                p.layer_effects =
                                    vec![blinc_core::LayerEffect::brightness(brightness)];
                            });
                            context.invalidate_render_cache();
                            let baseline = frame(&mut context, &tree, &state, mode);
                            pixel(
                                &baseline,
                                (60 * scale, 75 * scale),
                                [0, (255.0 * brightness).round() as u8, 0, 255],
                            );
                            for alpha in [0.5, 0.0, 1.0, 0.75, 0.5] {
                                tree.update_render_props(outer, |p| p.opacity = alpha);
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
fn glass_nested_deep_panels_and_siblings_keep_order() {
    let mut context = super::main_text_clip_tests::context(true, 4);
    let state = state();
    for mode in 0..4 {
        let neutral = material(true).tint(Color::TRANSPARENT);
        let green = div().absolute().w(80.0).h(80.0).bg(Color::GREEN);
        let deep = div()
            .absolute()
            .w(160.0)
            .h(120.0)
            .effect(neutral.clone())
            .opacity(0.5)
            .child(
                div()
                    .absolute()
                    .w(120.0)
                    .h(100.0)
                    .effect(neutral.clone())
                    .opacity(0.5)
                    .child(
                        div()
                            .absolute()
                            .w(80.0)
                            .h(80.0)
                            .effect(neutral.clone())
                            .opacity(0.5)
                            .child(green),
                    ),
            );
        let mut tree =
            RenderTree::from_element(&div().w(160.0).h(120.0).bg(Color::BLUE).child(deep));
        tree.compute_layout(160.0, 120.0);
        pixel(
            &frame(&mut context, &tree, &state, mode),
            (40, 40),
            [0, 32, 223, 255],
        );
        for reverse in [false, true] {
            let first = div()
                .absolute()
                .left(20.0)
                .top(20.0)
                .w(60.0)
                .h(60.0)
                .effect(neutral.clone())
                .child(div().w(60.0).h(60.0).bg(Color::GREEN));
            let second = div()
                .absolute()
                .left(40.0)
                .top(30.0)
                .w(60.0)
                .h(60.0)
                .effect(neutral.clone())
                .opacity(0.5)
                .child(div().w(60.0).h(60.0).bg(Color::WHITE));
            let outer = div()
                .absolute()
                .w(160.0)
                .h(120.0)
                .effect(material(true))
                .opacity(0.5);
            let outer = if reverse {
                outer.child(second).child(first)
            } else {
                outer.child(first).child(second)
            };
            let mut tree =
                RenderTree::from_element(&div().w(160.0).h(120.0).bg(Color::BLUE).child(outer));
            tree.compute_layout(160.0, 120.0);
            for _ in 0..2 {
                pixel(
                    &frame(&mut context, &tree, &state, mode),
                    (60, 60),
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
fn glass_nested_background_ancestors_keep_joint_opacity_and_order() {
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
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .bg(Color::RED)
                        .opacity(0.5)
                        .child(panel(true)),
                )
                .child(
                    div()
                        .absolute()
                        .left(55.0)
                        .top(55.0)
                        .w(10.0)
                        .h(10.0)
                        .bg(Color::YELLOW),
                ),
        );
        tree.compute_layout(160.0, 120.0);
        for _ in 0..3 {
            let page = frame(&mut context, &tree, &state, mode);
            pixel(&page, (65, 65), [96, 48, 128, 255]);
            pixel(&page, (56, 56), [255, 255, 0, 255]);
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_nested_binding_and_css_clips_recover_after_zero() {
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
            let mut outer = panel(true);
            if source == 0 {
                outer = outer.opacity(1.0);
            }
            let ui = div().w(160.0).h(120.0).bg(Color::BLUE);
            let mut tree = if source == 0 {
                RenderTree::from_element(
                    &ui.child(outer.motion().opacity(alpha.clone()).translate_x(x.clone())),
                )
            } else {
                RenderTree::from_element(&ui.child(outer))
            };
            tree.compute_layout(160.0, 120.0);
            let outer = tree.query_by_id("outer").unwrap();
            tree.update_render_props(outer, |p| {
                p.clip_path = Some(blinc_core::ClipPath::Path {
                    vertices: vec![(0.0, 0.0), (120.0, 0.0), (0.0, 100.0)],
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
                        outer,
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
                    pixel(
                        &page,
                        (60 + offset, 60),
                        blend([64, 191, 128, 255], opacity),
                    );
                    pixel(&page, (130 + offset, 100), [0, 0, 255, 255]);
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_nested_entry_snapshots_and_material_changes_keep_state() {
    use blinc_layout::{MotionAnimation, MotionKeyframe};
    let mut context = super::main_text_clip_tests::context(true, 1);
    for mode in 1..4 {
        let mut tree = scene(true);
        let outer = tree.query_by_id("outer").unwrap();
        let inner = tree.query_by_id("inner").unwrap();
        let animation = MotionAnimation {
            enter_from: Some(MotionKeyframe::new().opacity(0.25)),
            enter_duration_ms: 1000,
            ..Default::default()
        };
        tree.update_render_props(outer, |p| {
            p.opacity = 1.0;
            p.motion = Some(animation.clone());
        });
        let mut state = state();
        state.start_enter_motion(outer, animation);
        for _ in 0..3 {
            pixel(
                &frame(&mut context, &tree, &state, mode),
                (60, 60),
                blend([64, 191, 128, 255], 0.25),
            );
            pixel(
                &frame(&mut context, &tree, &state, 0),
                (60, 60),
                [64, 191, 128, 255],
            );
            assert_eq!(state.get_motion_values(outer).unwrap().opacity, Some(0.25));
        }
        tree.update_render_props(outer, |p| {
            p.opacity = 0.5;
            p.motion = None;
        });
        for glass in [false, true, false, true] {
            tree.update_render_props(inner, |p| {
                p.material = glass.then(|| {
                    blinc_layout::Material::Glass(material(true).tint(Color::TRANSPARENT))
                });
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
                [32, 96, 191, 255],
            );
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_nested_spatial_effects_keep_ancestor_clip_coordinates() {
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
                        .child(panel(true).left(-10.0).top(-10.0)),
                ),
            );
            tree.compute_layout(160.0, 120.0);
            tree.set_scale_factor(scale as f32);
            tree.update_render_props(tree.query_by_id("outer").unwrap(), |p| {
                p.layer_effects = vec![blinc_core::LayerEffect::blur(4.0)]
            });
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(&page, (26 * scale, 50 * scale), [0, 0, 255, 255]);
                pixel(&page, (114 * scale, 50 * scale), [0, 0, 255, 255]);
                pixel(&page, (70 * scale, 50 * scale), [32, 96, 191, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_nested_accepts_a_destination_without_texture_binding() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Glass destination"),
        size: wgpu::Extent3d {
            width: 160,
            height: 120,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: context.texture_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    context
        .render_tree(
            &scene(true),
            160,
            120,
            &texture.create_view(&Default::default()),
        )
        .unwrap();
    let page =
        crate::tests::read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    pixel(&page, (60, 60), [32, 96, 191, 255]);
}

#[test]
fn glass_nested_capture_marks_depth_and_keeps_unsupported_owner_guards() {
    super::opacity_group_tests::init_theme();
    for boundary in 0..7 {
        let mut tree = scene(true);
        let outer = tree.query_by_id("outer").unwrap();
        let inner = tree.query_by_id("inner").unwrap();
        match boundary {
            1 => tree.update_render_props(outer, |p| p.flow_shader = Some(separate_pass_flow())),
            2 => tree.update_render_props(inner, |p| p.rotate_y = Some(20.0)),
            3 => tree.update_render_props(outer, |p| p.is_overlay_root = true),
            4 => tree.update_render_props(tree.root().unwrap(), |p| p.is_overlay_root = true),
            _ => {}
        }
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        paint.enable_deferred_text();
        paint.enable_deferred_svg();
        if boundary != 5 {
            paint.enable_deferred_image();
        }
        if boundary != 6 {
            paint.enable_glass_layers();
        }
        tree.render_snapshot(&mut paint);
        let batch = paint.take_batch();
        let flags: Vec<_> = batch
            .layer_commands
            .iter()
            .chain(&batch.foreground_layer_commands)
            .filter_map(|entry| {
                if let blinc_gpu::primitives::LayerCommand::Glass(glass) = &entry.command {
                    Some(glass.nested)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            flags,
            if boundary == 0 {
                vec![false, true]
            } else {
                vec![]
            },
            "boundary={boundary}"
        );
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_nested_polygon_clip_retains_the_glass_owner_frame() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for polygon in [false, true] {
        for mode in 0..4 {
            for scale in [1, 2] {
                let mut tree = RenderTree::from_element(
                    &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                        div()
                            .id("glass")
                            .absolute()
                            .left(20.0)
                            .top(10.0)
                            .w(120.0)
                            .h(100.0)
                            .effect(material(true))
                            .opacity(0.5),
                    ),
                );
                tree.compute_layout(160.0, 120.0);
                tree.set_scale_factor(scale as f32);
                tree.update_render_props(tree.query_by_id("glass").unwrap(), |p| {
                    p.clip_path = Some(if polygon {
                        use blinc_core::ClipLength::Px;
                        blinc_core::ClipPath::Polygon {
                            points: vec![
                                (Px(0.0), Px(0.0)),
                                (Px(120.0), Px(0.0)),
                                (Px(0.0), Px(100.0)),
                            ],
                        }
                    } else {
                        blinc_core::ClipPath::Path {
                            vertices: vec![(0.0, 0.0), (120.0, 0.0), (0.0, 100.0)],
                        }
                    })
                });
                let page = frame(&mut context, &tree, &state, mode);
                pixel(&page, (60 * scale, 60 * scale), [64, 64, 255, 255]);
                pixel(&page, (110 * scale, 80 * scale), [0, 0, 255, 255]);
            }
        }
    }
}

#[test]
fn glass_nested_cache_fingerprint_tracks_sampling_scope() {
    super::opacity_group_tests::init_theme();
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.enable_deferred_text();
    paint.enable_deferred_svg();
    paint.enable_deferred_image();
    paint.enable_glass_layers();
    scene(true).render_snapshot(&mut paint);
    let mut batch = paint.take_batch();
    batch.resolve_cross_stream_groups();
    let mut batch = batch.foreground_batch();
    let before = RenderContext::hash_composite_scratch(&batch);
    let glass = batch
        .layer_commands
        .iter_mut()
        .find_map(|entry| match &mut entry.command {
            blinc_gpu::primitives::LayerCommand::Glass(glass) => Some(glass),
            _ => None,
        })
        .expect("glass capture missing");
    glass.nested = !glass.nested;
    assert_ne!(
        before,
        RenderContext::hash_composite_scratch(&batch),
        "Sampling scope changes must invalidate cached bakes"
    );
}

#[test]
#[ignore] // Requires GPU
fn glass_nested_gpu_prefix_keeps_coverage_and_executes_callbacks_once() {
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
                        .id("outer")
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .effect(material(true).tint(Color::TRANSPARENT))
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
                                    })
                                    .absolute()
                                    .w(40.0)
                                    .h(30.0),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(100.0)
                                .top(80.0)
                                .w(40.0)
                                .h(30.0)
                                .effect(material(true).tint(Color::TRANSPARENT))
                                .child(
                                    div()
                                        .absolute()
                                        .left(10.0)
                                        .top(10.0)
                                        .w(10.0)
                                        .h(10.0)
                                        .bg(Color::GREEN),
                                ),
                        ),
                ),
            );
            tree.compute_layout(160.0, 120.0);
            let outer = tree.query_by_id("outer").unwrap();
            for scale in [1, 2] {
                tree.set_scale_factor(scale as f32);
                tree.update_render_props(outer, |p| p.opacity = 1.0);
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
                    pixel(&baseline, point, [255, 0, 255, 255]);
                }
                for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                    tree.update_render_props(outer, |p| p.opacity = opacity);
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
                        pixel(&page, (145 * scale, 110 * scale), [0, 0, 255, 255]);
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
fn glass_nested_first_level_material_pixels_match_without_the_inner_panel() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        for simple in [false, true] {
            for blur in [0.0, 3.0] {
                for scale in [1, 2] {
                    let mut tree = RenderTree::from_element(
                        &div()
                            .w(160.0)
                            .h(120.0)
                            .bg(Color::BLUE)
                            .child(
                                div()
                                    .absolute()
                                    .left(5.0)
                                    .top(5.0)
                                    .w(40.0)
                                    .h(105.0)
                                    .bg(Color::YELLOW),
                            )
                            .child(div().absolute().left(95.0).w(45.0).h(100.0).bg(Color::RED))
                            .child(
                                div()
                                    .id("outer")
                                    .absolute()
                                    .left(20.0)
                                    .top(10.0)
                                    .w(120.0)
                                    .h(100.0)
                                    .rounded(10.0)
                                    .effect(material(simple).blur(blur).border(2.0))
                                    .child(
                                        div()
                                            .id("inner")
                                            .absolute()
                                            .left(20.0)
                                            .top(20.0)
                                            .w(60.0)
                                            .h(60.0)
                                            .effect(material(true).tint(Color::TRANSPARENT)),
                                    ),
                            ),
                    );
                    tree.compute_layout(160.0, 120.0);
                    tree.set_scale_factor(scale as f32);
                    let outer = tree.query_by_id("outer").unwrap();
                    let inner = tree.query_by_id("inner").unwrap();
                    for opacity in [0.5, 1.0] {
                        tree.update_render_props(outer, |p| p.opacity = opacity);
                        tree.update_render_props(inner, |p| p.opacity = 0.0);
                        context.invalidate_render_cache();
                        let reference = frame(&mut context, &tree, &state, mode);
                        tree.update_render_props(inner, |p| p.opacity = 1.0);
                        context.invalidate_render_cache();
                        let page = frame(&mut context, &tree, &state, mode);
                        for y in 0..120 * scale {
                            for x in 0..160 * scale {
                                if (37 * scale..103 * scale).contains(&x)
                                    && (27 * scale..93 * scale).contains(&y)
                                {
                                    continue;
                                }
                                pixel(&page, (x, y), reference.get_pixel(x, y).0);
                            }
                        }
                    }
                }
            }
        }
    }
}
