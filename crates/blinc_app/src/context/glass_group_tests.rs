//! Glass panels and collected children share their visual owner's opacity.
use super::opacity_group_tests::pixel;
use super::*;

#[test]
#[ignore] // Requires GPU
fn glass_group_panel_and_image_apply_parent_opacity_once() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    for mode in 0..4 {
        for on_panel in [false, true] {
            let panel = div()
                .id("panel")
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(120.0)
                .h(100.0)
                .effect(
                    GlassMaterial::new()
                        .with_simple(true)
                        .blur(0.0)
                        .border(0.0)
                        .tint(Color::rgba(1.0, 1.0, 0.0, 1.0)),
                )
                .opacity(if on_panel { 0.5 } else { 1.0 })
                .child(
                    div().absolute().left(20.0).top(20.0).w(60.0).h(60.0).child(
                        blinc_layout::image(super::overlay_group_tests::IMAGE)
                            .size(60.0, 60.0)
                            .fade_in(std::time::Duration::ZERO),
                    ),
                );
            let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
                div()
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .opacity(if on_panel { 1.0 } else { 0.5 })
                    .child(panel),
            );
            let mut tree = RenderTree::from_element(&ui);
            tree.compute_layout(160.0, 120.0);
            let page = frame(&mut context, &tree, &state, mode);
            pixel(&page, (60, 60), [0, 128, 128, 255]);
            pixel(&page, (25, 100), [64, 64, 255, 255]);
        }
    }
}

fn state() -> RenderState {
    RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )))
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

fn material(simple: bool) -> GlassMaterial {
    GlassMaterial::new()
        .with_simple(simple)
        .blur(0.0)
        .border(0.0)
        .noise(0.0)
        .tint(Color::rgba(1.0, 1.0, 0.0, 1.0))
}

fn mixed(simple: bool) -> RenderTree {
    let mut tree = RenderTree::from_element(
        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
            super::overlay_group_tests::mixed_overlay()
                .effect(material(simple))
                .opacity(1.0),
        ),
    );
    tree.compute_layout(160.0, 120.0);
    tree.update_render_props(tree.query_by_id("overlay").unwrap(), |p| {
        p.is_overlay_root = false
    });
    tree
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
fn glass_group_native_children_keep_foreground_without_an_explicit_marker() {
    super::opacity_group_tests::init_theme();
    for marker in [false, true] {
        let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .absolute()
                .w(120.0)
                .h(100.0)
                .effect(material(true))
                .child(
                    div()
                        .absolute()
                        .left(20.0)
                        .top(20.0)
                        .w(40.0)
                        .h(40.0)
                        .bg(Color::GREEN),
                ),
        );
        let ui = if marker {
            ui.child(div().foreground())
        } else {
            ui
        };
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(160.0, 120.0);
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        tree.render_snapshot(&mut paint);
        let batch = paint.take_batch();
        assert_eq!(batch.glass_count(), 1);
        assert_eq!(batch.foreground_primitives.len(), 1, "marker={marker}");
        assert_eq!(batch.foreground_primitives[0].color, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(batch.primitives.len(), 1);
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_group_mixed_content_effects_endpoints_and_frame_modes() {
    for unified in [false, true] {
        for samples in [1, 4] {
            let mut context = super::main_text_clip_tests::context(unified, samples);
            let state = state();
            for mode in 0..4 {
                for simple in [false, true] {
                    let mut tree = mixed(simple);
                    let panel = tree.query_by_id("overlay").unwrap();
                    for scale in [1, 2] {
                        tree.set_scale_factor(scale as f32);
                        for brightness in [1.0, 0.5] {
                            tree.update_render_props(panel, |p| {
                                p.opacity = 1.0;
                                p.layer_effects =
                                    vec![blinc_core::LayerEffect::brightness(brightness)];
                            });
                            context.invalidate_render_cache();
                            let baseline = frame(&mut context, &tree, &state, mode);
                            for (point, color) in [
                                ((60, 75), [0, 255, 0, 255]),
                                ((55, 45), [255, 0, 255, 255]),
                                ((105, 85), [255, 255, 0, 255]),
                            ] {
                                pixel(
                                    &baseline,
                                    (point.0 * scale, point.1 * scale),
                                    [
                                        (color[0] as f32 * brightness).round() as u8,
                                        (color[1] as f32 * brightness).round() as u8,
                                        (color[2] as f32 * brightness).round() as u8,
                                        255,
                                    ],
                                );
                            }
                            for (left, right, channel) in [(25, 43, 0), (65, 85, 1)] {
                                let max = (12 * scale..30 * scale)
                                    .flat_map(|y| {
                                        (left * scale..right * scale).map(move |x| (x, y))
                                    })
                                    .map(|(x, y)| baseline.get_pixel(x, y).0[channel])
                                    .max()
                                    .unwrap();
                                assert!(
                                    (max as f32 - 255.0 * brightness).abs() <= 5.0,
                                    "missing text: {max}"
                                );
                            }
                            for alpha in [0.5, 0.0, 1.0, 0.75, 0.5] {
                                tree.update_render_props(panel, |p| p.opacity = alpha);
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
fn glass_group_tight_backdrop_matches_legacy_screen_coordinates() {
    for simple in [false, true] {
        for blur in [0.0, 3.0] {
            for scale in [1, 2] {
                let mut context = super::main_text_clip_tests::context(true, 1);
                let state = state();
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
                                .id("glass")
                                .absolute()
                                .left(20.0)
                                .top(10.0)
                                .w(120.0)
                                .h(100.0)
                                .rounded(10.0)
                                .effect(material(simple).blur(blur).border(2.0)),
                        ),
                );
                tree.compute_layout(160.0, 120.0);
                tree.set_scale_factor(scale as f32);
                let mut paint = GpuPaintContext::new((160 * scale) as f32, (120 * scale) as f32);
                tree.render_snapshot(&mut paint);
                let legacy = paint.take_batch();
                assert!(!legacy.has_grouped_glass());
                let (texture, target) = super::gpu_pass_group_tests::target(&context, scale);
                let backdrop = context.device().create_texture(&wgpu::TextureDescriptor {
                    label: Some("Legacy glass reference"),
                    size: wgpu::Extent3d {
                        width: 80 * scale,
                        height: 60 * scale,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: context.texture_format(),
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                context.renderer.resize(160 * scale, 120 * scale);
                context.renderer.render_glass_frame(
                    &target,
                    &backdrop.create_view(&Default::default()),
                    (80 * scale, 60 * scale),
                    &legacy,
                    false,
                );
                let reference = crate::tests::read_rendered_pixels(
                    context.device(),
                    context.queue(),
                    &texture,
                    160 * scale,
                    120 * scale,
                );
                context.renderer.clear_target(&target, wgpu::Color::BLUE);
                context.renderer.render_background_pair(
                    &target,
                    (
                        &backdrop.create_view(&Default::default()),
                        (80 * scale, 60 * scale),
                    ),
                    &legacy,
                    None,
                    false,
                );
                context.renderer.render_glass(
                    &target,
                    &backdrop.create_view(&Default::default()),
                    &legacy,
                );
                let loaded = crate::tests::read_rendered_pixels(
                    context.device(),
                    context.queue(),
                    &texture,
                    160 * scale,
                    120 * scale,
                );
                for mode in 0..4 {
                    let page = frame(&mut context, &tree, &state, mode);
                    let reference = if mode == 3 { &loaded } else { &reference };
                    for y in 0..120 * scale {
                        for x in 0..160 * scale {
                            pixel(&page, (x, y), reference.get_pixel(x, y).0);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn glass_group_capabilities_and_unsupported_owners_keep_admission_guards() {
    super::opacity_group_tests::init_theme();
    for boundary in 0..9 {
        let mut tree = mixed(true);
        let panel = tree.query_by_id("overlay").unwrap();
        match boundary {
            3 => tree.update_render_props(tree.query_by_id("image-host").unwrap(), |p| {
                p.material = Some(blinc_layout::Material::Glass(material(true)))
            }),
            4 => tree.update_render_props(panel, |p| p.is_overlay_root = true),
            5 => tree.update_render_props(tree.root().unwrap(), |p| p.is_overlay_root = true),
            6 => tree.update_render_props(panel, |p| p.flow = Some("separate-pass".into())),
            7 => tree.update_render_props(panel, |p| p.rotate_y = Some(20.0)),
            8 => tree.update_render_props(tree.root().unwrap(), |p| {
                p.layer_effects = vec![blinc_core::LayerEffect::brightness(0.5)]
            }),
            _ => {}
        }
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        paint.enable_deferred_image();
        paint.enable_deferred_svg();
        if boundary != 2 {
            paint.enable_deferred_text();
        }
        if boundary != 1 {
            paint.enable_glass_layers();
        }
        tree.render_snapshot(&mut paint);
        let batch = paint.take_batch();
        assert_eq!(
            batch.has_grouped_glass(),
            boundary == 0,
            "boundary={boundary}"
        );
        assert!(!paint.take_dynamic_batch().has_grouped_glass());
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_group_live_bindings_css_clips_and_zero_recovery() {
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
            let panel = super::overlay_group_tests::mixed_overlay()
                .effect(material(true))
                .opacity(1.0);
            let mut tree = if source == 0 {
                RenderTree::from_element(
                    &div()
                        .w(160.0)
                        .h(120.0)
                        .bg(Color::BLUE)
                        .child(panel.motion().opacity(alpha.clone()).translate_x(x.clone())),
                )
            } else {
                RenderTree::from_element(&div().w(160.0).h(120.0).bg(Color::BLUE).child(panel))
            };
            tree.compute_layout(160.0, 120.0);
            let panel = tree.query_by_id("overlay").unwrap();
            tree.update_render_props(panel, |p| {
                p.is_overlay_root = false;
                p.clip_path = Some(blinc_core::ClipPath::Path {
                    vertices: vec![(0.0, 0.0), (120.0, 0.0), (0.0, 100.0)],
                });
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
                        panel,
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
                    pixel(&page, (60 + offset, 65), blend([0, 255, 0, 255], opacity));
                    pixel(&page, (110 + offset, 100), [0, 0, 255, 255]);
                    if offset > 0 {
                        pixel(&page, (20, 30), [0, 0, 255, 255]);
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_group_callbacks_and_meshes_keep_frame_coverage_and_execute_once() {
    for mesh_scene in [false, true] {
        let mut context = super::main_text_clip_tests::context(true, 4);
        let state = state();
        for mode in 0..4 {
            let (background, bg_calls) = super::gpu_pass_group_tests::clear_pass(wgpu::Color {
                r: 0.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            });
            let (pass, calls) = super::gpu_pass_group_tests::clear_pass(wgpu::Color {
                r: 1.0,
                g: 0.0,
                b: 1.0,
                a: 1.0,
            });
            let mesh = super::mesh_group_tests::mesh([1.0, 1.0, 0.0]);
            let mut tree = RenderTree::from_element(
                &div()
                    .w(160.0)
                    .h(120.0)
                    .bg(Color::BLUE)
                    .child(
                        div().id("backdrop").absolute().w(160.0).h(120.0).child(
                            canvas(move |ctx, _| ctx.run_gpu_pass(&background, None))
                                .absolute()
                                .w(160.0)
                                .h(120.0),
                        ),
                    )
                    .child(
                        div()
                            .id("glass")
                            .absolute()
                            .left(20.0)
                            .top(10.0)
                            .w(120.0)
                            .h(100.0)
                            .effect(material(true))
                            .overflow_clip()
                            .child(
                                canvas(move |ctx, _| {
                                    if mesh_scene {
                                        ctx.set_camera(&super::mesh_group_tests::camera());
                                        ctx.set_3d_viewport_bounds(80.0, 80.0);
                                        ctx.draw_mesh_data(
                                            mesh.clone(),
                                            blinc_core::Mat4::IDENTITY,
                                        );
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
                                .h(100.0),
                            ),
                    ),
            );
            tree.compute_layout(160.0, 120.0);
            let panel = tree.query_by_id("glass").unwrap();
            tree.update_render_props(tree.query_by_id("backdrop").unwrap(), |p| {
                p.layer_effects = vec![blinc_core::LayerEffect::brightness(1.0)];
            });
            for scale in [1, 2] {
                tree.set_scale_factor(scale as f32);
                tree.update_render_props(panel, |p| p.opacity = 1.0);
                context.invalidate_render_cache();
                let baseline = frame(&mut context, &tree, &state, mode);
                let color = baseline.get_pixel(65 * scale, 65 * scale).0;
                if mesh_scene {
                    assert!(color[0] > 150 && color[1] > 150, "mesh missing: {color:?}");
                } else {
                    assert_eq!(color, [255, 0, 255, 255]);
                }
                for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                    tree.update_render_props(panel, |p| p.opacity = opacity);
                    context.invalidate_render_cache();
                    for _ in 0..2 {
                        let before = calls.lock().unwrap().len();
                        let bg_before = bg_calls.lock().unwrap().len();
                        let page = frame(&mut context, &tree, &state, mode);
                        let faded = [
                            (color[0] as f32 * opacity).round() as u8,
                            (color[1] as f32 * opacity + 255.0 * (1.0 - opacity)).round() as u8,
                            (color[2] as f32 * opacity + 255.0 * (1.0 - opacity)).round() as u8,
                            255,
                        ];
                        pixel(&page, (65 * scale, 65 * scale), faded);
                        pixel(
                            &page,
                            (60 * scale, 50 * scale),
                            [0, 255, (255.0 * (1.0 - opacity)).round() as u8, 255],
                        );
                        pixel(&page, (5 * scale, 5 * scale), [0, 255, 255, 255]);
                        assert_eq!(bg_calls.lock().unwrap().len(), bg_before + 1);
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
fn glass_group_entry_motion_and_owner_replacement_keep_snapshot_state() {
    use blinc_layout::{MotionAnimation, MotionKeyframe};
    let mut context = super::main_text_clip_tests::context(true, 1);
    for mode in 1..4 {
        let mut tree = mixed(true);
        let panel = tree.query_by_id("overlay").unwrap();
        let animation = MotionAnimation {
            enter_from: Some(MotionKeyframe::new().opacity(0.25)),
            enter_duration_ms: 1000,
            ..Default::default()
        };
        tree.update_render_props(panel, |p| p.motion = Some(animation.clone()));
        let mut state = state();
        state.start_enter_motion(panel, animation);
        for _ in 0..3 {
            pixel(
                &frame(&mut context, &tree, &state, mode),
                (60, 75),
                blend([0, 255, 0, 255], 0.25),
            );
            pixel(
                &frame(&mut context, &tree, &state, 0),
                (60, 75),
                [0, 255, 0, 255],
            );
            assert_eq!(state.get_motion_values(panel).unwrap().opacity, Some(0.25));
        }
        tree.update_render_props(panel, |p| {
            p.motion = None;
            p.opacity = 0.5;
        });
        for glass in [false, true, false, true] {
            tree.update_render_props(panel, |p| {
                p.material = glass.then(|| blinc_layout::Material::Glass(material(true)))
            });
            context.invalidate_render_cache();
            for _ in 0..2 {
                pixel(
                    &frame(&mut context, &tree, &state, 0),
                    (60, 75),
                    blend([0, 255, 0, 255], 0.5),
                );
            }
        }
    }
}

#[test]
fn glass_group_marker_keeps_small_surfaces_and_owned_clip_rows() {
    super::opacity_group_tests::init_theme();
    let mut tree = RenderTree::from_element(
        &div().w(3840.0).h(2160.0).bg(Color::BLUE).child(
            div()
                .id("glass")
                .absolute()
                .left(100.0)
                .top(80.0)
                .w(80.0)
                .h(50.0)
                .effect(material(true))
                .opacity(0.5),
        ),
    );
    tree.compute_layout(3840.0, 2160.0);
    tree.update_render_props(tree.query_by_id("glass").unwrap(), |p| {
        p.clip_path = Some(blinc_core::ClipPath::Path {
            vertices: vec![(0.0, 0.0), (80.0, 0.0), (0.0, 50.0)],
        })
    });
    let mut paint = GpuPaintContext::new(3840.0, 2160.0);
    paint.enable_glass_layers();
    tree.render_snapshot(&mut paint);
    let mut batch = paint.take_batch();
    batch.aux_data.clear();
    let glass = batch
        .foreground_layer_commands
        .iter()
        .find_map(|cmd| match &cmd.command {
            blinc_gpu::primitives::LayerCommand::Glass(glass) => Some(glass),
            _ => None,
        })
        .unwrap();
    assert_eq!(glass.frame_size, (3840, 2160));
    assert_eq!(glass.paint.dest, Rect::new(98.0, 78.0, 84.0, 54.0));
    assert!(!glass.paint.clip_aux.is_empty());
    assert_eq!(glass.paint.opacity, 1.0);
}

#[test]
#[ignore] // Requires GPU
fn glass_group_backdrop_refreshes_after_background_removal() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    for mode in 0..4 {
        let mut tree = RenderTree::from_element(
            &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                div()
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
        for (background, mut expected) in [
            (Some(Color::BLUE), [64, 64, 255, 255]),
            (Some(Color::RED), [255, 64, 0, 255]),
            (None, [64, 64, 0, 255]),
            (Some(Color::GREEN), [64, 255, 0, 255]),
        ] {
            tree.update_render_props(tree.root().unwrap(), |p| {
                p.background = background.map(Brush::Solid)
            });
            context.invalidate_render_cache();
            if background.is_none() && mode == 3 {
                expected[2] = 128;
            }
            for _ in 0..2 {
                pixel(
                    &frame(&mut context, &tree, &state, mode),
                    (60, 60),
                    expected,
                );
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_group_opaque_panel_keeps_the_direct_glass_surface_path() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let mut tree = RenderTree::from_element(
        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(80.0)
                .h(60.0)
                .effect(material(true))
                .child(
                    div()
                        .absolute()
                        .left(20.0)
                        .top(20.0)
                        .w(30.0)
                        .h(30.0)
                        .bg(Color::GREEN),
                ),
        ),
    );
    tree.compute_layout(160.0, 120.0);
    let state = state();
    assert_eq!(context.renderer.layer_texture_cache_mut().pool_size(), 0);
    for mode in 0..4 {
        let page = frame(&mut context, &tree, &state, mode);
        pixel(&page, (25, 15), [128, 128, 255, 255]);
        pixel(&page, (50, 40), [0, 255, 0, 255]);
        assert_eq!(
            context.renderer.layer_texture_cache_mut().pool_size(),
            if mode == 3 { 2 } else { 0 },
            "mode={mode}"
        );
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_group_spatial_effects_preserve_ancestor_clip_frames() {
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
                                .id("glass")
                                .absolute()
                                .left(-10.0)
                                .top(-10.0)
                                .w(120.0)
                                .h(100.0)
                                .effect(material(true))
                                .opacity(0.5)
                                .child(
                                    blinc_layout::image(super::overlay_group_tests::IMAGE)
                                        .size(120.0, 100.0)
                                        .fade_in(std::time::Duration::ZERO),
                                ),
                        ),
                ),
            );
            tree.compute_layout(160.0, 120.0);
            tree.set_scale_factor(scale as f32);
            tree.update_render_props(tree.query_by_id("glass").unwrap(), |p| {
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
