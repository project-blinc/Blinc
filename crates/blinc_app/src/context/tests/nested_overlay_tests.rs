//! Nested overlay roots retain their late owner's isolation and foreground order.
use super::opacity_group_tests::pixel;
use super::separate_pass_flow;
use super::*;

fn state() -> RenderState {
    RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )))
}

fn scene(nested: bool, foreground: bool) -> RenderTree {
    let outer = div()
        .id("outer")
        .overlay_root()
        .absolute()
        .w(160.0)
        .h(120.0)
        .bg(Color::BLUE)
        .opacity(0.5)
        .child(super::overlay_group_tests::mixed_overlay());
    let outer = if foreground {
        outer.child(
            div()
                .absolute()
                .w(160.0)
                .h(120.0)
                .bg(Color::CYAN)
                .foreground(),
        )
    } else {
        outer
    };
    let mut tree = super::overlay_group_tests::with_plate(outer);
    tree.update_render_props(tree.query_by_id("overlay").unwrap(), |p| {
        p.is_overlay_root = nested
    });
    tree
}

fn frame(
    context: &mut RenderContext,
    tree: &RenderTree,
    state: &RenderState,
    mode: u8,
) -> image::RgbaImage {
    if mode < 3 {
        return super::gpu_pass_group_tests::frame(context, tree, state, mode);
    }
    let scale = tree.scale_factor() as u32;
    let (texture, target) = super::gpu_pass_group_tests::target(context, scale);
    context.renderer.clear_target(&target, wgpu::Color::BLUE);
    context
        .render_overlay_tree_with_motion(tree, state, 160 * scale, 120 * scale, &target)
        .unwrap();
    crate::tests::read_rendered_pixels(
        context.device(),
        context.queue(),
        &texture,
        160 * scale,
        120 * scale,
    )
}

#[test]
#[ignore] // Requires GPU
fn nested_overlay_collected_content_keeps_outer_and_inner_opacity() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        let reference = frame(&mut context, &scene(false, false), &state, mode);
        pixel(&reference, (60, 75), [128, 191, 64, 255]);
        let page = frame(&mut context, &scene(true, false), &state, mode);
        pixel(&page, (60, 75), [128, 191, 64, 255]);
        for y in 0..120 {
            for x in 0..160 {
                pixel(&page, (x, y), reference.get_pixel(x, y).0);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn nested_overlay_panel_covers_outer_foreground_inside_one_opacity_scope() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        let page = frame(&mut context, &scene(true, true), &state, mode);
        pixel(&page, (60, 75), [128, 255, 64, 255]);
        pixel(&page, (55, 45), [191, 191, 128, 255]);
        pixel(&page, (25, 105), [191, 191, 64, 255]);
        pixel(&page, (5, 5), [128, 255, 128, 255]);
    }
}

fn cyan_backdrop(
    color: [f32; 3],
    outer: f32,
    inner: f32,
    inner_brightness: f32,
    outer_brightness: f32,
) -> [u8; 4] {
    let base = [0.0, 1.0, 1.0];
    let target = [1.0, 1.0, 0.0];
    let mut result = [0; 4];
    for c in 0..3 {
        result[c] = (((color[c] * inner_brightness * inner + base[c] * (1.0 - inner))
            * outer_brightness
            * outer
            + target[c] * (1.0 - outer))
            * 255.0)
            .round() as u8;
    }
    result[3] = 255;
    result
}

#[test]
#[ignore] // Requires GPU
fn nested_overlay_mixed_stream_siblings_keep_tree_order() {
    for mode in 0..4 {
        for reverse in [false, true] {
            for first_foreground in [false, true] {
                let mut context = super::main_text_clip_tests::context(true, 1);
                let state = state();
                let first = div()
                    .overlay_root()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(100.0)
                    .h(100.0)
                    .bg(Color::RED)
                    .opacity(0.5);
                let second = div()
                    .overlay_root()
                    .absolute()
                    .left(40.0)
                    .top(30.0)
                    .w(80.0)
                    .h(80.0)
                    .bg(Color::BLUE)
                    .opacity(0.5);
                let first = if first_foreground {
                    first.foreground()
                } else {
                    first
                };
                let second = if !first_foreground {
                    second.foreground()
                } else {
                    second
                };
                let outer = div()
                    .overlay_root()
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .bg(Color::BLUE)
                    .opacity(0.5);
                let outer = if reverse {
                    outer.child(second).child(first)
                } else {
                    outer.child(first).child(second)
                };
                let tree = super::overlay_group_tests::with_plate(
                    outer.child(
                        div()
                            .absolute()
                            .w(160.0)
                            .h(120.0)
                            .bg(Color::CYAN)
                            .foreground(),
                    ),
                );
                for _ in 0..3 {
                    let page = frame(&mut context, &tree, &state, mode);
                    pixel(
                        &page,
                        (60, 50),
                        if reverse {
                            [191, 159, 64, 255]
                        } else {
                            [159, 159, 96, 255]
                        },
                    );
                    pixel(&page, (5, 5), [128, 255, 128, 255]);
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn nested_overlay_effects_endpoints_dpi_and_foreground_owner_stay_together() {
    for unified in [false, true] {
        for samples in [1, 4] {
            let mut context = super::main_text_clip_tests::context(unified, samples);
            let state = state();
            for mode in 0..4 {
                for foreground in [false, true] {
                    let mut tree = scene(true, true);
                    let outer = tree.query_by_id("outer").unwrap();
                    let inner = tree.query_by_id("overlay").unwrap();
                    tree.update_render_props(outer, |p| {
                        p.layer = if foreground {
                            RenderLayer::Foreground
                        } else {
                            RenderLayer::Background
                        }
                    });
                    for scale in [1, 2] {
                        tree.set_scale_factor(scale as f32);
                        for (o, i, ib, ob) in [
                            (0.5, 0.5, 1.0, 1.0),
                            (0.0, 0.5, 1.0, 1.0),
                            (0.5, 0.0, 1.0, 1.0),
                            (1.0, 1.0, 1.0, 1.0),
                            (0.75, 0.5, 0.5, 1.0),
                            (0.5, 0.75, 1.0, 0.5),
                            (0.5, 0.5, 1.0, 1.0),
                        ] {
                            tree.update_render_props(outer, |p| {
                                p.opacity = o;
                                p.layer_effects = vec![blinc_core::LayerEffect::brightness(ob)];
                            });
                            tree.update_render_props(inner, |p| {
                                p.opacity = i;
                                p.layer_effects = vec![blinc_core::LayerEffect::brightness(ib)];
                            });
                            context.invalidate_render_cache();
                            for _ in 0..2 {
                                let page = frame(&mut context, &tree, &state, mode);
                                for (xy, color) in [
                                    ((60, 75), [0.0, 1.0, 0.0]),
                                    ((55, 45), [1.0, 0.0, 1.0]),
                                    ((25, 105), [1.0, 0.0, 0.0]),
                                ] {
                                    pixel(
                                        &page,
                                        (xy.0 * scale, xy.1 * scale),
                                        cyan_backdrop(color, o, i, ib, ob),
                                    );
                                }
                                let expected = cyan_backdrop([1.0, 1.0, 1.0], o, i, ib, ob);
                                for (left, right) in [(25, 43), (65, 85)] {
                                    let blue = (12 * scale..30 * scale)
                                        .flat_map(|y| {
                                            (left * scale..right * scale).map(move |x| (x, y))
                                        })
                                        .map(|(x, y)| page.get_pixel(x, y).0[2])
                                        .max()
                                        .unwrap();
                                    assert!(
                                        blue.abs_diff(expected[2]) <= 5,
                                        "text coverage changed: {blue} expected {}",
                                        expected[2]
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

#[test]
#[ignore] // Requires GPU
fn nested_overlay_intermediate_effects_and_third_level_keep_ancestry_once() {
    let mut context = super::main_text_clip_tests::context(true, 4);
    let state = state();
    for mode in 0..4 {
        for deep in [false, true] {
            let outer = div()
                .id("outer")
                .overlay_root()
                .absolute()
                .w(160.0)
                .h(120.0)
                .bg(Color::BLUE)
                .opacity(0.5);
            let child = if deep {
                div()
                    .id("middle")
                    .overlay_root()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(120.0)
                    .h(100.0)
                    .bg(Color::RED)
                    .opacity(0.5)
                    .child(
                        div()
                            .id("leaf")
                            .overlay_root()
                            .absolute()
                            .left(20.0)
                            .top(20.0)
                            .w(60.0)
                            .h(60.0)
                            .bg(Color::GREEN)
                            .opacity(0.5),
                    )
                    .child(
                        div()
                            .absolute()
                            .w(120.0)
                            .h(100.0)
                            .bg(Color::CYAN)
                            .foreground(),
                    )
            } else {
                div()
                    .id("middle")
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .opacity(0.5)
                    .child(super::overlay_group_tests::mixed_overlay())
                    .child(
                        div()
                            .absolute()
                            .w(15.0)
                            .h(120.0)
                            .bg(Color::WHITE)
                            .foreground(),
                    )
            };
            let mut tree = super::overlay_group_tests::with_plate(
                outer.child(child).child(
                    div()
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .bg(Color::CYAN)
                        .foreground(),
                ),
            );
            if !deep {
                tree.update_render_props(tree.query_by_id("middle").unwrap(), |p| {
                    p.layer_effects = vec![
                        blinc_core::LayerEffect::brightness(0.5),
                        blinc_core::LayerEffect::blur(1.0),
                    ]
                });
            }
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(
                    &page,
                    (60, 75),
                    if deep {
                        [128, 255, 96, 255]
                    } else {
                        [128, 239, 96, 255]
                    },
                );
                pixel(&page, (5, 5), [128, 255, 128, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn nested_overlay_binding_and_css_updates_keep_clips_and_zero_recovery() {
    use super::opacity_group_tests::{CssSource, css_value};
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    for mode in 0..4 {
        for source in 0..5 {
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
            let inner = super::overlay_group_tests::mixed_overlay().opacity(if source == 0 {
                1.0
            } else {
                0.5
            });
            let outer = div()
                .id("outer")
                .overlay_root()
                .absolute()
                .w(160.0)
                .h(120.0)
                .bg(Color::BLUE)
                .opacity(if source == 4 { 1.0 } else { 0.5 });
            let outer = if source == 0 {
                outer.child(
                    div()
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .child(inner)
                        .motion()
                        .opacity(alpha.clone())
                        .translate_x(x.clone()),
                )
            } else {
                outer.child(inner)
            };
            let outer = outer.child(
                div()
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .bg(Color::CYAN)
                    .foreground(),
            );
            let mut tree = if source == 4 {
                super::overlay_group_tests::with_plate(
                    outer.motion().opacity(alpha.clone()).translate_x(x.clone()),
                )
            } else {
                super::overlay_group_tests::with_plate(outer)
            };
            let outer = tree.query_by_id("outer").unwrap();
            let inner = tree.query_by_id("overlay").unwrap();
            tree.update_render_props(inner, |p| {
                p.clip_path = Some(blinc_core::ClipPath::Path {
                    vertices: vec![(0.0, 0.0), (120.0, 0.0), (0.0, 100.0)],
                })
            });
            let state = RenderState::new(scheduler);
            for (opacity, offset) in [(0.5, 0), (0.0, 5), (1.0, 10), (0.75, 0), (0.5, 5)] {
                let offset = if source == 0 || source == 4 {
                    alpha.lock().unwrap().set_immediate(opacity);
                    x.lock().unwrap().set_immediate(offset as f32);
                    offset
                } else {
                    let node = if source == 3 { outer } else { inner };
                    css_value(
                        &tree,
                        node,
                        if source == 2 {
                            CssSource::Transition
                        } else {
                            CssSource::Animation
                        },
                        Some(opacity),
                    );
                    tree.apply_all_css_animation_props();
                    tree.apply_all_css_transition_props();
                    0
                };
                let (o, i) = if source >= 3 {
                    (opacity, 0.5)
                } else {
                    (0.5, opacity)
                };
                for _ in 0..2 {
                    let page = frame(&mut context, &tree, &state, mode);
                    pixel(
                        &page,
                        (60 + offset, 65),
                        cyan_backdrop([0.0, 1.0, 0.0], o, i, 1.0, 1.0),
                    );
                    pixel(
                        &page,
                        (95 + offset, 85),
                        cyan_backdrop([0.0, 1.0, 1.0], o, 0.0, 1.0, 1.0),
                    );
                    if source == 4 && offset > 0 {
                        pixel(&page, (2, 5), [255, 255, 0, 255]);
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn nested_overlay_gpu_output_keeps_late_coverage_and_runs_once() {
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
            let inner = div()
                .id("inner-gpu")
                .overlay_root()
                .overflow_clip()
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
                            ctx.run_gpu_pass(&pass, None);
                        }
                        ctx.fill_rect(
                            Rect::new(30.0, 30.0, 20.0, 20.0),
                            0.0.into(),
                            Color::GREEN.into(),
                        );
                    })
                    .absolute()
                    .w(120.0)
                    .h(100.0)
                    .foreground(),
                );
            let mut tree = super::overlay_group_tests::with_plate(
                div()
                    .id("outer")
                    .overlay_root()
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .bg(Color::BLUE)
                    .child(inner)
                    .child(
                        div()
                            .absolute()
                            .w(160.0)
                            .h(120.0)
                            .bg(Color::CYAN)
                            .foreground(),
                    ),
            );
            let outer = tree.query_by_id("outer").unwrap();
            let inner = tree.query_by_id("inner-gpu").unwrap();
            for scale in [1, 2] {
                tree.set_scale_factor(scale as f32);
                tree.update_render_props(outer, |p| p.opacity = 1.0);
                tree.update_render_props(inner, |p| p.opacity = 1.0);
                context.invalidate_render_cache();
                let baseline = frame(&mut context, &tree, &state, mode);
                let color = baseline.get_pixel(65 * scale, 65 * scale).0;
                if mesh_scene {
                    assert!(
                        color[0] > 150 && color[1] > 150,
                        "mesh output missing: {color:?}"
                    );
                } else {
                    assert_eq!(color, [255, 0, 255, 255]);
                }
                tree.update_render_props(outer, |p| p.opacity = 0.5);
                for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                    tree.update_render_props(inner, |p| p.opacity = opacity);
                    context.invalidate_render_cache();
                    for _ in 0..2 {
                        let before = calls.lock().unwrap().len();
                        let page = frame(&mut context, &tree, &state, mode);
                        pixel(
                            &page,
                            (65 * scale, 65 * scale),
                            cyan_backdrop(
                                [
                                    color[0] as f32 / 255.0,
                                    color[1] as f32 / 255.0,
                                    color[2] as f32 / 255.0,
                                ],
                                0.5,
                                opacity,
                                1.0,
                                1.0,
                            ),
                        );
                        pixel(
                            &page,
                            (60 * scale, 50 * scale),
                            cyan_backdrop([0.0, 1.0, 0.0], 0.5, opacity, 1.0, 1.0),
                        );
                        pixel(&page, (5 * scale, 5 * scale), [128, 255, 128, 255]);
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
fn nested_overlay_owner_replacement_restores_the_correct_scope() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        let mut tree = scene(true, true);
        let outer = tree.query_by_id("outer").unwrap();
        let inner = tree.query_by_id("overlay").unwrap();
        for (outer_portal, inner_portal, expected) in [
            (true, true, [128, 255, 64, 255]),
            (true, false, [128, 255, 128, 255]),
            (true, true, [128, 255, 64, 255]),
            (false, true, [191, 255, 0, 255]),
            (true, true, [128, 255, 64, 255]),
        ] {
            tree.update_render_props(outer, |p| p.is_overlay_root = outer_portal);
            tree.update_render_props(inner, |p| p.is_overlay_root = inner_portal);
            context.invalidate_render_cache();
            for _ in 0..2 {
                pixel(
                    &frame(&mut context, &tree, &state, mode),
                    (60, 75),
                    expected,
                );
            }
        }
    }
}

#[test]
fn nested_overlay_capabilities_and_unsupported_owners_remain_guarded() {
    super::opacity_group_tests::init_theme();
    for boundary in 0..7 {
        let mut tree = scene(true, true);
        let inner = tree.query_by_id("overlay").unwrap();
        if boundary >= 4 {
            tree.update_render_props(inner, |p| match boundary {
                4 => p.material = Some(blinc_layout::Material::Glass(GlassMaterial::new())),
                5 => p.rotate_y = Some(20.0),
                _ => p.flow_graph = Some(separate_pass_flow()),
            });
        }
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        if boundary != 1 {
            paint.enable_deferred_text();
        }
        if boundary != 2 {
            paint.enable_deferred_svg();
        }
        if boundary != 3 {
            paint.enable_deferred_image();
        }
        tree.render_snapshot(&mut paint);
        let bg = paint.take_batch();
        let dynamic = paint.take_dynamic_batch();
        assert!(bg.cross_stream_groups.image_nodes.is_empty());
        assert_eq!(
            !dynamic.cross_stream_groups.image_nodes.is_empty(),
            boundary == 0
        );
        assert_eq!(
            !dynamic.cross_stream_groups.text_nodes.is_empty(),
            boundary == 0
        );
        assert_eq!(
            !dynamic.cross_stream_groups.svg_nodes.is_empty(),
            boundary == 0
        );
        if boundary != 0 {
            assert!(!bg.has_cross_stream_groups());
            assert!(!dynamic.has_cross_stream_groups());
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn nested_overlay_entry_motion_and_snapshots_keep_separate_state() {
    use blinc_layout::{MotionAnimation, MotionKeyframe};
    let mut context = super::main_text_clip_tests::context(true, 1);
    for mode in 1..4 {
        for outer_motion in [false, true] {
            let mut tree = scene(true, true);
            let outer = tree.query_by_id("outer").unwrap();
            let inner = tree.query_by_id("overlay").unwrap();
            let owner = if outer_motion { outer } else { inner };
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
                let page = frame(&mut context, &tree, &state, mode);
                pixel(
                    &page,
                    (60, 75),
                    if outer_motion {
                        [191, 255, 32, 255]
                    } else {
                        [128, 255, 96, 255]
                    },
                );
                let page = frame(&mut context, &tree, &state, 0);
                pixel(
                    &page,
                    (60, 75),
                    if outer_motion {
                        [0, 255, 128, 255]
                    } else {
                        [128, 255, 0, 255]
                    },
                );
                assert_eq!(state.get_motion_values(owner).unwrap().opacity, Some(0.25));
            }
        }
    }
}
