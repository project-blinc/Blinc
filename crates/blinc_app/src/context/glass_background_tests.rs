//! Background glass groups retain their own paint order and local backdrop.
use super::glass_group_tests::{frame, material, state};
use super::opacity_group_tests::pixel;
use super::*;

fn scene() -> RenderTree {
    let mut tree = RenderTree::from_element(
        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .id("owner")
                .absolute()
                .w(160.0)
                .h(120.0)
                .bg(Color::RED)
                .opacity(0.5)
                .child(
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
                        ),
                ),
        ),
    );
    tree.compute_layout(160.0, 120.0);
    tree
}

#[test]
#[ignore] // Requires GPU
fn glass_background_paint_and_panel_apply_joint_opacity() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        let page = frame(&mut context, &scene(), &state, mode);
        pixel(&page, (60, 60), [0, 128, 128, 255]);
        pixel(&page, (10, 5), [128, 0, 128, 255]);
        pixel(&page, (25, 100), [128, 64, 128, 255]);
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
fn glass_background_mixed_content_effects_endpoints_and_replay() {
    for unified in [false, true] {
        for samples in [1, 4] {
            let mut context = super::main_text_clip_tests::context(unified, samples);
            let state = state();
            for mode in 0..4 {
                for simple in [false, true] {
                    let mut tree = RenderTree::from_element(
                        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                            div()
                                .id("owner")
                                .absolute()
                                .w(160.0)
                                .h(120.0)
                                .bg(Color::RED)
                                .child(div().absolute().w(15.0).h(120.0).bg(Color::CYAN))
                                .child(
                                    super::overlay_group_tests::mixed_overlay()
                                        .effect(material(simple)),
                                )
                                .child(
                                    div()
                                        .foreground()
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
fn glass_background_order_stays_at_the_background_anchor_with_motion() {
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    for mode in 0..4 {
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
            .absolute()
            .w(160.0)
            .h(120.0)
            .bg(Color::RED)
            .child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(120.0)
                    .h(100.0)
                    .effect(material(true))
                    .child(
                        blinc_layout::image(super::overlay_group_tests::IMAGE)
                            .size(120.0, 100.0)
                            .fade_in(std::time::Duration::ZERO),
                    ),
            );
        let mut card = RenderTree::from_element(
            &div()
                .w(160.0)
                .h(120.0)
                .bg(Color::BLUE)
                .child(owner.motion().opacity(alpha.clone()).translate_x(x.clone()))
                .child(
                    div()
                        .absolute()
                        .left(55.0)
                        .top(55.0)
                        .w(10.0)
                        .h(10.0)
                        .bg(Color::YELLOW),
                )
                .child(
                    div()
                        .foreground()
                        .absolute()
                        .left(75.0)
                        .top(55.0)
                        .w(10.0)
                        .h(10.0)
                        .bg(Color::CYAN),
                ),
        );
        card.compute_layout(160.0, 120.0);
        let state = RenderState::new(scheduler);
        for (value, offset) in [(0.5, 0), (0.0, 5), (1.0, 10), (0.75, 0), (0.5, 5)] {
            alpha.lock().unwrap().set_immediate(value);
            x.lock().unwrap().set_immediate(offset as f32);
            for _ in 0..2 {
                let page = frame(&mut context, &card, &state, mode);
                pixel(&page, (45 + offset, 45), blend([0, 255, 0, 255], value));
                pixel(&page, (56, 56), [255, 255, 0, 255]);
                pixel(&page, (76, 56), [0, 255, 255, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_background_nested_isolation_samples_unattenuated_prior_paint() {
    let mut context = super::main_text_clip_tests::context(true, 4);
    let state = state();
    for mode in 0..4 {
        for scale in [1, 2] {
            let mut tree = RenderTree::from_element(
                &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                    div()
                        .absolute()
                        .left(10.0)
                        .top(5.0)
                        .w(140.0)
                        .h(110.0)
                        .bg(Color::RED)
                        .opacity(0.5)
                        .child(
                            div()
                                .absolute()
                                .left(10.0)
                                .top(5.0)
                                .w(120.0)
                                .h(100.0)
                                .bg(Color::WHITE)
                                .opacity(0.5)
                                .child(
                                    div()
                                        .absolute()
                                        .w(120.0)
                                        .h(100.0)
                                        .effect(material(true).tint(Color::TRANSPARENT))
                                        .child(
                                            div()
                                                .absolute()
                                                .left(20.0)
                                                .top(20.0)
                                                .w(60.0)
                                                .h(60.0)
                                                .bg(Color::GREEN),
                                        ),
                                ),
                        ),
                ),
            );
            tree.compute_layout(160.0, 120.0);
            tree.set_scale_factor(scale as f32);
            for _ in 0..3 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(&page, (25 * scale, 100 * scale), [128, 64, 191, 255]);
                pixel(&page, (60 * scale, 60 * scale), [64, 64, 128, 255]);
                pixel(&page, (5 * scale, 5 * scale), [0, 0, 255, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_background_external_images_and_transparent_paint_join_the_backdrop() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        for scale in [1, 2] {
            let mut tree = RenderTree::from_element(
                &div()
                    .w(160.0)
                    .h(120.0)
                    .bg(Color::BLUE)
                    .child(
                        div().absolute().w(160.0).h(120.0).child(
                            blinc_layout::image(super::overlay_group_tests::IMAGE)
                                .size(160.0, 120.0)
                                .fade_in(std::time::Duration::ZERO),
                        ),
                    )
                    .child(
                        div()
                            .id("owner")
                            .absolute()
                            .left(20.0)
                            .top(10.0)
                            .w(120.0)
                            .h(100.0)
                            .bg(Color::rgba(1.0, 0.0, 0.0, 0.5))
                            .opacity(0.5)
                            .child(
                                div()
                                    .absolute()
                                    .w(120.0)
                                    .h(100.0)
                                    .effect(material(true).tint(Color::TRANSPARENT)),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(145.0)
                            .top(5.0)
                            .w(10.0)
                            .h(10.0)
                            .effect(material(true).tint(Color::TRANSPARENT)),
                    ),
            );
            tree.compute_layout(160.0, 120.0);
            tree.set_scale_factor(scale as f32);
            for _ in 0..3 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(&page, (60 * scale, 60 * scale), [64, 191, 0, 255]);
                pixel(&page, (5 * scale, 5 * scale), [0, 255, 0, 255]);
                pixel(&page, (150 * scale, 10 * scale), [0, 255, 0, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_background_gpu_prefix_is_sampled_without_replaying_callbacks() {
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
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .bg(Color::RED)
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
                        )
                        .child(
                            div()
                                .absolute()
                                .left(20.0)
                                .top(10.0)
                                .w(120.0)
                                .h(100.0)
                                .effect(material(true).tint(Color::TRANSPARENT)),
                        ),
                ),
            );
            tree.compute_layout(160.0, 120.0);
            let owner = tree.query_by_id("owner").unwrap();
            for scale in [1, 2] {
                tree.set_scale_factor(scale as f32);
                tree.update_render_props(owner, |p| {
                    p.opacity = 1.0;
                    p.layer_effects = vec![blinc_core::LayerEffect::brightness(1.0)];
                });
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
fn glass_background_spatial_effects_keep_outer_clip_coordinates() {
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
                                .absolute()
                                .left(-30.0)
                                .top(-20.0)
                                .w(160.0)
                                .h(120.0)
                                .bg(Color::RED)
                                .opacity(0.5)
                                .child(
                                    div()
                                        .absolute()
                                        .left(20.0)
                                        .top(10.0)
                                        .w(120.0)
                                        .h(100.0)
                                        .effect(material(true))
                                        .child(
                                            div()
                                                .absolute()
                                                .left(20.0)
                                                .top(20.0)
                                                .w(60.0)
                                                .h(60.0)
                                                .bg(Color::GREEN),
                                        ),
                                ),
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

#[test]
#[ignore] // Requires GPU
fn glass_background_css_endpoints_recover_without_changing_background_order() {
    use super::opacity_group_tests::{CssSource, css_value};
    for source in [CssSource::Animation, CssSource::Transition] {
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
                            .absolute()
                            .w(160.0)
                            .h(120.0)
                            .bg(Color::RED)
                            .child(
                                div()
                                    .absolute()
                                    .left(20.0)
                                    .top(10.0)
                                    .w(120.0)
                                    .h(100.0)
                                    .effect(material(true))
                                    .child(
                                        blinc_layout::image(super::overlay_group_tests::IMAGE)
                                            .size(120.0, 100.0)
                                            .fade_in(std::time::Duration::ZERO),
                                    ),
                            ),
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
            let owner = tree.query_by_id("owner").unwrap();
            for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                css_value(&tree, owner, source, Some(opacity));
                tree.apply_all_css_animation_props();
                tree.apply_all_css_transition_props();
                for _ in 0..2 {
                    let page = frame(&mut context, &tree, &state, mode);
                    pixel(&page, (45, 45), blend([0, 255, 0, 255], opacity));
                    pixel(&page, (56, 56), [255, 255, 0, 255]);
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_background_root_effects_and_material_replacement_keep_content() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        let mut tree = scene();
        let owner = tree.query_by_id("owner").unwrap();
        let panel = tree.query_by_id("panel").unwrap();
        tree.update_render_props(owner, |p| p.opacity = 1.0);
        tree.update_render_props(tree.root().unwrap(), |p| {
            p.layer_effects = vec![blinc_core::LayerEffect::brightness(0.5)]
        });
        for glass in [true, false, true, false, true] {
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
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(&page, (60, 60), [0, 128, 0, 255]);
                pixel(&page, (10, 5), [128, 0, 0, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_background_native_css_zero_recovery_keeps_the_background_anchor() {
    use super::opacity_group_tests::{CssSource, css_value};
    for source in [CssSource::Animation, CssSource::Transition] {
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
                            .absolute()
                            .w(160.0)
                            .h(120.0)
                            .bg(Color::RED)
                            .child(
                                div()
                                    .absolute()
                                    .left(20.0)
                                    .top(10.0)
                                    .w(120.0)
                                    .h(100.0)
                                    .effect(material(true))
                                    .child(div().absolute().w(120.0).h(100.0).bg(Color::GREEN)),
                            ),
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
            let owner = tree.query_by_id("owner").unwrap();
            for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                css_value(&tree, owner, source, Some(opacity));
                tree.apply_all_css_animation_props();
                tree.apply_all_css_transition_props();
                for _ in 0..2 {
                    let page = frame(&mut context, &tree, &state, mode);
                    pixel(&page, (45, 45), blend([0, 255, 0, 255], opacity));
                    pixel(&page, (56, 56), [255, 255, 0, 255]);
                }
            }
        }
    }
}
