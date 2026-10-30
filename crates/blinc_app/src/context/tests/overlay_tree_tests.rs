//! Dedicated overlay trees load the destination and retain ordered visual groups.
use super::opacity_group_tests::pixel;
use super::*;

fn scene() -> RenderTree {
    let mut tree = RenderTree::from_element(
        &div()
            .w(160.0)
            .h(120.0)
            .child(super::overlay_group_tests::mixed_overlay()),
    );
    tree.compute_layout(160.0, 120.0);
    tree
}

fn overlay(
    context: &mut RenderContext,
    tree: &RenderTree,
    state: &RenderState,
) -> image::RgbaImage {
    let width = (160.0 * tree.scale_factor()) as u32;
    let height = (120.0 * tree.scale_factor()) as u32;
    let (texture, target) = crate::tests::create_test_texture(
        context.device(),
        context.texture_format(),
        width,
        height,
    );
    context.renderer.clear_target(&target, wgpu::Color::BLUE);
    context
        .render_overlay_tree_with_motion(tree, state, width, height, &target)
        .unwrap();
    crate::tests::read_rendered_pixels(context.device(), context.queue(), &texture, width, height)
}

#[test]
#[ignore] // Requires GPU
fn overlay_tree_keeps_dynamic_groups_and_loads_existing_pixels() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let tree = scene();
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let page = overlay(&mut context, &tree, &state);
    pixel(&page, (25, 105), [128, 0, 128, 255]);
    pixel(&page, (60, 75), [0, 128, 128, 255]);
    pixel(&page, (105, 85), [128, 128, 128, 255]);
    pixel(&page, (55, 45), [128, 0, 255, 255]);
    pixel(&page, (5, 5), [0, 0, 255, 255]);
}

fn state() -> RenderState {
    RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )))
}

fn blue_blend(color: [f32; 3], opacity: f32, brightness: f32) -> [u8; 4] {
    [
        (color[0] * opacity * brightness * 255.0).round() as u8,
        (color[1] * opacity * brightness * 255.0).round() as u8,
        ((color[2] * brightness * opacity + 1.0 - opacity) * 255.0).round() as u8,
        255,
    ]
}

#[test]
#[ignore] // Requires GPU
fn overlay_tree_static_dynamic_and_foreground_groups_apply_effects_once() {
    for unified in [false, true] {
        for samples in [1, 4] {
            let mut context = super::main_text_clip_tests::context(unified, samples);
            let state = state();
            for dynamic in [false, true] {
                for foreground in [false, true] {
                    let mut tree = scene();
                    let owner = tree.query_by_id("overlay").unwrap();
                    tree.update_render_props(owner, |p| {
                        p.is_overlay_root = dynamic;
                        p.layer = if foreground {
                            RenderLayer::Foreground
                        } else {
                            RenderLayer::Background
                        };
                    });
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
                            for _ in 0..2 {
                                let page = overlay(&mut context, &tree, &state);
                                for (xy, color) in [
                                    ((25, 105), [1.0, 0.0, 0.0]),
                                    ((60, 75), [0.0, 1.0, 0.0]),
                                    ((105, 85), [1.0, 1.0, 0.0]),
                                    ((55, 45), [1.0, 0.0, 1.0]),
                                ] {
                                    pixel(
                                        &page,
                                        (xy.0 * scale, xy.1 * scale),
                                        blue_blend(color, opacity, brightness),
                                    );
                                }
                                for (left, right) in [(25, 43), (65, 85)] {
                                    let green = (12 * scale..30 * scale)
                                        .flat_map(|y| {
                                            (left * scale..right * scale).map(move |x| (x, y))
                                        })
                                        .map(|(x, y)| page.get_pixel(x, y).0[1])
                                        .max()
                                        .unwrap();
                                    assert!(
                                        (green as f32 - opacity * brightness * 255.0).abs() < 5.0,
                                        "text missing or double alpha: {green}"
                                    );
                                }
                                pixel(&page, (5 * scale, 5 * scale), [0, 0, 255, 255]);
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
fn overlay_tree_live_motion_and_css_keep_zero_recovery_and_clip_frames() {
    use super::opacity_group_tests::{CssSource, css_value};
    use blinc_animation::{AnimatedValue, AnimationScheduler};
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
        let root = super::overlay_group_tests::mixed_overlay().opacity(1.0);
        let mut tree = if source == 0 {
            let mut tree = RenderTree::from_element(
                &div()
                    .w(160.0)
                    .h(120.0)
                    .child(root.motion().opacity(alpha.clone()).translate_x(x.clone())),
            );
            tree.compute_layout(160.0, 120.0);
            tree
        } else {
            scene()
        };
        let owner = tree.query_by_id("overlay").unwrap();
        tree.update_render_props(owner, |p| {
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
                    owner,
                    if source == 1 {
                        CssSource::Animation
                    } else {
                        CssSource::Transition
                    },
                    Some(opacity),
                );
                tree.update_render_props(owner, |p| p.opacity = opacity);
                0
            };
            for _ in 0..2 {
                let page = overlay(&mut context, &tree, &state);
                pixel(
                    &page,
                    (60 + offset, 65),
                    blue_blend([0.0, 1.0, 0.0], opacity, 1.0),
                );
                pixel(&page, (95 + offset, 85), [0, 0, 255, 255]);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_tree_preserves_main_replay_and_promoted_texture_ownership() {
    use super::gpu_pass_group_tests::frame;
    use super::opacity_group_tests::{CssSource, css_fixture};
    for mode in [1, 2] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let state = state();
        let overlay_tree = scene();
        overlay(&mut context, &overlay_tree, &state);
        let (mut main, owner) = css_fixture(0.5, CssSource::Animation);
        {
            let store = main.css_anim_store();
            let mut store = store.lock().unwrap();
            store
                .animations
                .get_mut(&main.stable_id(owner).unwrap())
                .unwrap()
                .current_properties
                .background_color = None;
        }
        main.apply_all_css_animation_props();
        let mut baseline = frame(&mut context, &main, &state, mode);
        for _ in 0..2 {
            baseline = frame(&mut context, &main, &state, mode);
        }
        assert!(
            !context.css_composited_textures.is_empty(),
            "main fixture must have a promoted texture"
        );
        let scope = context.cached_tree_scope;
        let primitives = context
            .cached_bg_batch
            .as_ref()
            .unwrap()
            .primitives
            .as_ptr();
        let hashes = context.css_composited_scratch_hash.clone();
        let owners = context.css_cache_owners.clone();
        let views: Vec<_> = context
            .css_composited_textures
            .iter()
            .map(|(key, (texture, _))| (*key, &texture.view as *const _ as usize))
            .collect();
        let valid = context.renderer.static_layer_valid();
        for _ in 0..3 {
            let page = overlay(&mut context, &overlay_tree, &state);
            pixel(&page, (25, 105), [128, 0, 128, 255]);
            assert_eq!(context.cached_tree_scope, scope);
            assert_eq!(
                context
                    .cached_bg_batch
                    .as_ref()
                    .unwrap()
                    .primitives
                    .as_ptr(),
                primitives
            );
            assert_eq!(context.css_composited_scratch_hash, hashes);
            assert!(context.css_cache_owners == owners);
            assert_eq!(context.renderer.static_layer_valid(), valid);
            for (key, view) in &views {
                assert_eq!(
                    &context.css_composited_textures[key].0.view as *const _ as usize,
                    *view
                );
            }
        }
        let page = frame(&mut context, &main, &state, mode);
        assert_eq!(page, baseline);
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_tree_paths_raw_images_and_stacks_preserve_loaded_alpha() {
    use blinc_core::{Path, Transform};
    let mut context = super::main_text_clip_tests::context(true, 4);
    let state = state();
    let mut tree = RenderTree::from_element(
        &div()
            .w(160.0)
            .h(120.0)
            .child(
                canvas(|ctx, _| {
                    ctx.fill_path(
                        &Path::rect(Rect::new(20.0, 20.0, 30.0, 30.0)),
                        Color::rgba(0.0, 1.0, 0.0, 0.5).into(),
                    );
                    ctx.draw_rgba_pixels(
                        &[255, 0, 0, 128],
                        1,
                        1,
                        Rect::new(60.0, 20.0, 30.0, 30.0),
                    );
                    ctx.push_transform(Transform::translate(100.0, 20.0));
                    ctx.fill_rect(
                        Rect::new(0.0, 0.0, 30.0, 30.0),
                        0.0.into(),
                        Color::rgba(1.0, 1.0, 0.0, 0.5).into(),
                    );
                    ctx.pop_transform();
                })
                .absolute()
                .w(160.0)
                .h(120.0),
            )
            .child(
                div()
                    .stack_layer()
                    .absolute()
                    .left(20.0)
                    .top(70.0)
                    .w(30.0)
                    .h(30.0)
                    .bg(Color::rgba(1.0, 0.0, 0.0, 0.5)),
            ),
    );
    tree.compute_layout(160.0, 120.0);
    for _ in 0..2 {
        let page = overlay(&mut context, &tree, &state);
        for (xy, color) in [
            ((30, 30), [0, 128, 128, 255]),
            ((70, 30), [128, 0, 127, 255]),
            ((110, 30), [128, 128, 128, 255]),
            ((30, 80), [128, 0, 128, 255]),
            ((5, 5), [0, 0, 255, 255]),
        ] {
            pixel(&page, xy, color);
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_tree_gpu_output_keeps_group_coverage_order_and_display_scale() {
    for mesh_scene in [false, true] {
        for grouped in [false, true] {
            let mut context = super::main_text_clip_tests::context(true, 4);
            let state = state();
            let (pass, frames) = super::gpu_pass_group_tests::clear_pass(wgpu::Color {
                r: 0.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            });
            let mesh = super::mesh_group_tests::mesh([1.0, 1.0, 0.0]);
            let mut tree = RenderTree::from_element(
                &div().w(160.0).h(120.0).child(
                    div()
                        .id("owner")
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
                                    ctx.run_gpu_pass(
                                        &pass,
                                        Some(Rect::new(20.0, 20.0, 60.0, 60.0)),
                                    );
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
            let owner = tree.query_by_id("owner").unwrap();
            tree.update_render_props(owner, |p| {
                p.is_overlay_root = grouped;
                if grouped {
                    p.layer_effects = vec![blinc_core::LayerEffect::brightness(1.0)];
                }
            });
            for scale in [1, 2] {
                tree.set_scale_factor(scale as f32);
                let page = overlay(&mut context, &tree, &state);
                let color = page.get_pixel(65 * scale, 65 * scale).0;
                if mesh_scene {
                    assert!(color[0] > 150 && color[1] > 150, "mesh missing: {color:?}");
                } else {
                    pixel(&page, (65 * scale, 65 * scale), [0, 255, 255, 255]);
                }
                if grouped {
                    for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                        tree.update_render_props(owner, |p| p.opacity = opacity);
                        let before = frames.lock().unwrap().len();
                        let page = overlay(&mut context, &tree, &state);
                        pixel(
                            &page,
                            (65 * scale, 65 * scale),
                            blue_blend(
                                [
                                    color[0] as f32 / 255.0,
                                    color[1] as f32 / 255.0,
                                    color[2] as f32 / 255.0,
                                ],
                                opacity,
                                1.0,
                            ),
                        );
                        pixel(
                            &page,
                            (60 * scale, 50 * scale),
                            blue_blend([0.0, 1.0, 0.0], opacity, 1.0),
                        );
                        if !mesh_scene && opacity > 0.0 {
                            assert_eq!(frames.lock().unwrap().len(), before + 1);
                        }
                        pixel(&page, (5 * scale, 5 * scale), [0, 0, 255, 255]);
                    }
                    tree.update_render_props(owner, |p| p.opacity = 1.0);
                }
                if !mesh_scene {
                    let frames = frames.lock().unwrap();
                    assert_eq!(
                        *frames.last().unwrap(),
                        (160 * scale, 120 * scale, scale as f64)
                    );
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_tree_entry_motion_keeps_state_and_does_not_duplicate_global_cursors() {
    use blinc_layout::{MotionAnimation, MotionKeyframe};
    let mut context = super::main_text_clip_tests::context(true, 1);
    let mut tree = scene();
    let owner = tree.query_by_id("overlay").unwrap();
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
    state.add_cursor(0.0, 0.0, 10.0, 10.0, Color::GREEN);
    for _ in 0..3 {
        let page = overlay(&mut context, &tree, &state);
        pixel(&page, (25, 105), blue_blend([1.0, 0.0, 0.0], 0.25, 1.0));
        pixel(&page, (60, 75), blue_blend([0.0, 1.0, 0.0], 0.25, 1.0));
        pixel(&page, (5, 5), [0, 0, 255, 255]);
        assert_eq!(state.get_motion_values(owner).unwrap().opacity, Some(0.25));
        assert_eq!(state.overlays().len(), 1);
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_tree_transparent_destination_and_repeated_loads_keep_alpha() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let tree = scene();
    let state = state();
    let (texture, target) =
        crate::tests::create_test_texture(context.device(), context.texture_format(), 160, 120);
    context
        .renderer
        .clear_target(&target, wgpu::Color::TRANSPARENT);
    for alpha in [128, 191] {
        context
            .render_overlay_tree_with_motion(&tree, &state, 160, 120, &target)
            .unwrap();
        let page = crate::tests::read_rendered_pixels(
            context.device(),
            context.queue(),
            &texture,
            160,
            120,
        );
        pixel(&page, (60, 75), [0, alpha, 0, alpha]);
        pixel(&page, (25, 105), [alpha, 0, 0, alpha]);
        pixel(&page, (5, 5), [0, 0, 0, 0]);
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_tree_glass_keeps_independent_backdrops_and_single_image_alpha() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    let (pass, calls) = super::gpu_pass_group_tests::clear_pass(wgpu::Color {
        r: 0.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    });
    let mut tree = RenderTree::from_element(
        &div()
            .w(160.0)
            .h(120.0)
            .child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(60.0)
                    .h(60.0)
                    .opacity(0.5)
                    .overflow_clip()
                    .bg(Color::RED)
                    .child(
                        canvas(move |ctx, _| ctx.run_gpu_pass(&pass, None))
                            .absolute()
                            .w(60.0)
                            .h(60.0),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(100.0)
                    .top(10.0)
                    .w(40.0)
                    .h(40.0)
                    .effect(
                        GlassMaterial::new()
                            .with_simple(true)
                            .blur(0.0)
                            .border(0.0)
                            .tint(Color::rgba(0.1, 0.2, 0.3, 0.5)),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(100.0)
                    .top(70.0)
                    .w(40.0)
                    .h(40.0)
                    .child(
                        blinc_layout::image("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNg+M/QAAADggGA8sg/+gAAAABJRU5ErkJggg==")
                            .size(40.0, 40.0)
                            .fade_in(std::time::Duration::ZERO),
                    ),
            ),
    );
    tree.compute_layout(160.0, 120.0);
    let main = super::main_text_clip_tests::tree(false, false);
    // The main frame's glass backdrop must survive the independent overlay's paint.
    let mut main = main;
    main.update_render_props(main.root().unwrap(), |p| {
        p.material = Some(blinc_layout::Material::Glass(
            GlassMaterial::new().with_simple(true).blur(0.0).border(0.0),
        ))
    });
    let (_texture, target) =
        crate::tests::create_test_texture(context.device(), context.texture_format(), 240, 180);
    context
        .render_tree_with_motion(&main, &state, 240, 180, &target)
        .unwrap();
    assert!(context.backdrop_texture.is_some());
    let main_size = (
        context.backdrop_texture.as_ref().unwrap().width,
        context.backdrop_texture.as_ref().unwrap().height,
    );
    for _ in 0..2 {
        let before = calls.lock().unwrap().len();
        let page = overlay(&mut context, &tree, &state);
        pixel(&page, (30, 30), [0, 128, 255, 255]);
        pixel(&page, (120, 90), [0, 128, 128, 255]);
        pixel(&page, (5, 5), [0, 0, 255, 255]);
        assert_eq!(calls.lock().unwrap().len(), before + 1);
        assert_eq!(
            (
                context.backdrop_texture.as_ref().unwrap().width,
                context.backdrop_texture.as_ref().unwrap().height
            ),
            main_size
        );
        assert!(context.overlay_backdrop_texture.is_some());
    }
}

#[test]
#[ignore] // Requires GPU
fn overlay_tree_css_candidates_paint_fresh_and_invalidate_reused_tree_records() {
    use super::opacity_group_tests::{CssSource, css_fixture, css_value};
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = state();
    let (mut tree, owner) = css_fixture(0.5, CssSource::Animation);
    for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
        css_value(&tree, owner, CssSource::Animation, Some(opacity));
        {
            let store = tree.css_anim_store();
            let mut store = store.lock().unwrap();
            store
                .animations
                .get_mut(&tree.stable_id(owner).unwrap())
                .unwrap()
                .current_properties
                .background_color = None;
        }
        tree.apply_all_css_animation_props();
        let page = overlay(&mut context, &tree, &state);
        pixel(&page, (55, 55), blue_blend([0.0, 1.0, 0.0], opacity, 1.0));
        assert!(!context.has_render_cache());
        assert!(context.cached_tree_scope.is_none());
        assert!(context.css_composited_textures.is_empty());
        assert!(context.motion_subtree_textures.is_empty());
        if opacity > 0.0 {
            assert!(tree.composite_promotion().contains(&owner));
        }
    }
    super::gpu_pass_group_tests::frame(&mut context, &tree, &state, 2);
    assert!(context.has_render_cache());
    overlay(&mut context, &tree, &state);
    assert!(
        !context.has_render_cache(),
        "same-tree overlay replaced the main tree's paint records"
    );
}

#[test]
#[ignore] // Requires GPU
fn overlay_tree_atlas_reclamation_invalidates_main_glyph_replay() {
    use super::gpu_pass_group_tests::frame;
    let mut context = super::main_text_clip_tests::context(true, 1);
    for step in 0..96 {
        if context.text_ctx.atlas_utilization() >= ATLAS_GC_PRESSURE {
            break;
        }
        let _ = context.text_ctx.prepare_text(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
            0.0,
            0.0,
            64.0 + step as f32 * 3.0,
            [1.0; 4],
        );
    }
    assert!(
        context.text_ctx.atlas_utilization() >= ATLAS_GC_PRESSURE,
        "fixture must reach atlas reclamation pressure"
    );
    let mut main = RenderTree::from_element(
        &div()
            .w(160.0)
            .h(120.0)
            .bg(Color::BLUE)
            .child(text("CACHE").size(24.0).color(Color::WHITE)),
    );
    main.compute_layout(160.0, 120.0);
    let state = state();
    frame(&mut context, &main, &state, 2);
    let baseline = frame(&mut context, &main, &state, 2);
    assert!(context.renderer.static_layer_valid());
    let glyphs = context.text_ctx.atlas().glyph_count();
    // Model a long-lived main cache while independent overlays advance atlas epochs.
    for _ in 0..ATLAS_GC_MAX_AGE + 2 {
        context.text_ctx.begin_atlas_epoch();
    }
    let mut empty = RenderTree::from_element(&div().w(160.0).h(120.0));
    empty.compute_layout(160.0, 120.0);
    overlay(&mut context, &empty, &state);
    assert!(context.text_ctx.atlas().glyph_count() < glyphs);
    assert!(!context.has_render_cache());
    assert!(!context.renderer.static_layer_valid());
    assert_eq!(frame(&mut context, &main, &state, 2), baseline);
}
