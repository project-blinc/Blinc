use super::grouped_text_tests::frame;
use super::opacity_group_tests::{CssSource, css_value, pixel};
use super::*;
use blinc_animation::{AnimatedValue, AnimationScheduler, SharedAnimatedValue};
use blinc_core::{BlendMode, LayerEffect, MaskMode};
use blinc_gpu::primitives::LayerCommand;
use blinc_layout::{image, rich_text, svg};

const IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEElEQVR4nGNg+M/QAMYwBgA07AX9m+NLtAAAAABJRU5ErkJggg==";

const OWNER_ALPHA: f32 = 0.999991;
const INNER_ALPHA: f32 = 0.713;
const MOTION_ALPHA: f32 = 0.999983;

#[derive(Clone, Copy, Debug)]
enum Source {
    Static,
    Binding,
    Css(CssSource),
}
#[derive(Clone, Copy, Debug)]
enum Fx {
    Swap,
    Blur,
    Shadow,
    Glow,
    Mask(bool),
    Blend(BlendMode),
    Chain,
}
#[derive(Clone, Copy, Debug)]
struct Scene {
    content_fg: bool,
    owner_fg: bool,
    clipped: bool,
    nested: bool,
    dpi: f32,
    source: Source,
}
impl Default for Scene {
    fn default() -> Self {
        Self {
            content_fg: false,
            owner_fg: false,
            clipped: false,
            nested: false,
            dpi: 1.0,
            source: Source::Static,
        }
    }
}
fn effects(fx: Fx, dpi: f32) -> (Vec<LayerEffect>, BlendMode) {
    let swap = LayerEffect::ColorMatrix {
        matrix: [
            0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0,
        ],
    };
    let blur = LayerEffect::blur(3.0 * dpi);
    match fx {
        Fx::Swap => (vec![swap], BlendMode::Normal),
        Fx::Blur => (vec![blur], BlendMode::Normal),
        Fx::Shadow => (
            vec![LayerEffect::drop_shadow(
                6.0 * dpi,
                4.0 * dpi,
                3.0 * dpi,
                Color::WHITE,
            )],
            BlendMode::Normal,
        ),
        Fx::Glow => (
            vec![LayerEffect::glow(Color::WHITE, 3.0, 2.0, 0.7)],
            BlendMode::Normal,
        ),
        Fx::Mask(luminance) => (
            vec![LayerEffect::MaskImage {
                image_url: IMAGE.into(),
                mask_mode: if luminance {
                    MaskMode::Luminance
                } else {
                    MaskMode::Alpha
                },
            }],
            BlendMode::Normal,
        ),
        Fx::Blend(mode) => (Vec::new(), mode),
        Fx::Chain => (vec![blur, swap], BlendMode::Screen),
    }
}
fn scene(opts: Scene, alpha: Option<SharedAnimatedValue>) -> RenderTree {
    let content=div().id("inner").w(140.0).h(130.0).opacity(if opts.nested {INNER_ALPHA} else {1.0})
        .child(image(IMAGE).size(100.0,100.0).fade_in(std::time::Duration::ZERO))
        .child(div().absolute().left(65.0).top(5.0).child(svg(r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><rect width="40" height="40" fill="#00ff00" opacity="0.7"/></svg>"##).size(40.0,40.0)))
        .child(div().absolute().left(5.0).top(40.0).child(text("MM").size(24.0).color(Color::GREEN)))
        .child(div().absolute().left(5.0).top(70.0).child(rich_text("<span color=\"#00ff00\"><b>M</b></span><span color=\"#00ffff\"><u>M</u></span>").size(24.0)))
        .child(canvas(|ctx,_|ctx.draw_rgba_pixels(&[0,255,0,160],1,1,Rect::new(90.0,80.0,30.0,30.0))).absolute().w(140.0).h(130.0));
    let content = if opts.content_fg {
        content.foreground()
    } else {
        content
    };
    let owner = div()
        .id("group")
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(140.0)
        .h(130.0)
        .bg(Color::RED)
        .opacity(OWNER_ALPHA)
        .child(content);
    let owner = if opts.owner_fg {
        owner.foreground()
    } else {
        owner
    };
    let ui = div().w(240.0).h(180.0).bg(Color::BLUE);
    let ui = if let Some(alpha) = alpha {
        ui.child(owner.motion().opacity(alpha))
    } else {
        ui.child(owner)
    };
    let ui = ui
        .child(
            div()
                .absolute()
                .left(42.0)
                .top(42.0)
                .w(5.0)
                .h(5.0)
                .bg(Color::YELLOW),
        )
        .child(
            div()
                .absolute()
                .left(200.0)
                .top(20.0)
                .w(10.0)
                .h(10.0)
                .bg(Color::WHITE)
                .foreground(),
        );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(240.0, 180.0);
    tree.set_scale_factor(opts.dpi);
    if matches!(opts.source, Source::Binding) {
        let motion = *tree.motion_bindings_map().keys().next().unwrap();
        tree.update_render_props(motion, |p| p.opacity = MOTION_ALPHA);
    }
    if opts.clipped {
        tree.update_render_props(tree.query_by_id("group").unwrap(), |p| {
            p.transform = Some(blinc_core::Transform::scale(0.9, 1.05));
            p.transform_origin = Some([0.0, 0.0]);
            p.clip_path = Some(blinc_core::ClipPath::Path {
                vertices: vec![(0.0, 0.0), (140.0, 0.0), (120.0, 120.0), (0.0, 120.0)],
            });
        });
    }
    tree
}
fn with_config(batch: &PrimitiveBatch, fx: Fx, alpha: f32, opts: Scene) -> PrimitiveBatch {
    let mut batch = batch.clone();
    let mut found = 0;
    let mut inner = 0;
    let mut motion = 0;
    for command in batch
        .layer_commands
        .iter_mut()
        .chain(&mut batch.foreground_layer_commands)
    {
        if let LayerCommand::Push { config } = &mut command.command {
            if config.opacity == OWNER_ALPHA {
                let (effects, blend) = effects(fx, opts.dpi);
                config.effects = effects;
                config.blend_mode = blend;
                config.opacity = if matches!(opts.source, Source::Binding) {
                    1.0
                } else {
                    alpha
                };
                found += 1;
            }
            if matches!(opts.source, Source::Binding) && config.opacity == MOTION_ALPHA {
                config.opacity = alpha;
                motion += 1;
            }
            if opts.nested && config.opacity == INNER_ALPHA {
                config.effects = vec![LayerEffect::brightness(0.6)];
                inner += 1;
            }
        }
    }
    assert_eq!(found, 1);
    assert_eq!(inner, usize::from(opts.nested));
    assert_eq!(motion, usize::from(matches!(opts.source, Source::Binding)));
    batch
}
fn reference(context: &mut RenderContext, batch: &PrimitiveBatch) -> ::image::RgbaImage {
    let (texture, view) =
        crate::tests::create_test_texture(context.device(), context.texture_format(), 240, 180);
    context.preload_mask_images(batch);
    context
        .renderer
        .render_with_clear(&view, batch, [0.0, 0.0, 0.0, 1.0]);
    context.renderer.render_foreground(&view, batch);
    crate::tests::read_rendered_pixels(context.device(), context.queue(), &texture, 240, 180)
}
fn exercise(fx: Fx, opts: Scene) {
    for compositor in [false, true] {
        let mut expected_context = super::main_text_clip_tests::context(true, 1);
        let mut context = super::main_text_clip_tests::context(true, 1);
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let alpha = Arc::new(Mutex::new(AnimatedValue::with_default(
            scheduler.lock().unwrap().handle(),
            1.0,
        )));
        let mut tree = scene(
            opts,
            matches!(opts.source, Source::Binding).then(|| alpha.clone()),
        );
        let group = tree.query_by_id("group").unwrap();
        let state = RenderState::new(scheduler);
        frame(&mut expected_context, &tree, &state, false);
        let baseline = expected_context.cached_bg_batch.clone().unwrap();
        let (fxs, blend) = effects(fx, opts.dpi);
        // The walker scales blur and shadow lengths from layout pixels.
        let (authored, _) = effects(fx, 1.0);
        tree.update_render_props(group, |p| {
            p.layer_effects = authored;
            p.mix_blend_mode = Some(blend);
        });
        if matches!(opts.source, Source::Binding) {
            let motion = *tree.motion_bindings_map().keys().next().unwrap();
            tree.update_render_props(motion, |p| p.opacity = 1.0);
        }
        if opts.nested {
            tree.update_render_props(tree.query_by_id("inner").unwrap(), |p| {
                p.layer_effects = vec![LayerEffect::brightness(0.6)]
            });
        }
        for value in [0.5_f32, 1.0, 0.0, 0.75, 0.5] {
            match opts.source {
                Source::Static => {
                    tree.update_render_props(group, |p| p.opacity = value);
                    context.invalidate_render_cache();
                }
                Source::Binding => {
                    tree.update_render_props(group, |p| p.opacity = 1.0);
                    alpha.lock().unwrap().set_immediate(value);
                }
                Source::Css(source) => {
                    css_value(&tree, group, source, Some(value));
                    tree.update_render_props(group, |p| p.opacity = value);
                }
            }
            let batch = with_config(&baseline, fx, value, opts);
            let expected = reference(&mut expected_context, &batch);
            eprintln!("fx={fx:?} opts={opts:?} compositor={compositor} value={value}");
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, compositor);
                for y in 0..180 {
                    for x in 0..240 {
                        pixel(&page, (x, y), expected.get_pixel(x, y).0);
                    }
                }
                let cached = context.cached_bg_batch.as_ref().unwrap();
                assert!(!cached.cross_stream_groups.text_nodes.is_empty());
                assert_eq!(cached.cross_stream_groups.svg_nodes.len(), 1);
                assert_eq!(cached.cross_stream_groups.image_nodes.len(), 1);
                if value > 0.0 {
                    let config = cached
                        .layer_commands
                        .iter()
                        .chain(&cached.foreground_layer_commands)
                        .find_map(|entry| match &entry.command {
                            LayerCommand::Push { config }
                                if config.effects == fxs && config.blend_mode == blend =>
                            {
                                Some(config)
                            }
                            _ => None,
                        })
                        .unwrap();
                    assert_eq!(
                        config.opacity,
                        if matches!(opts.source, Source::Binding) {
                            1.0
                        } else {
                            value
                        }
                    );
                    if matches!(fx, Fx::Mask(_)) {
                        assert!(context.renderer.has_mask_image(IMAGE));
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn effect_group_color_matrix_reaches_images_and_text() {
    for content_fg in [false, true] {
        exercise(
            Fx::Swap,
            Scene {
                content_fg,
                ..Default::default()
            },
        );
    }
}
#[test]
#[ignore] // Requires GPU
fn effect_group_spatial_effects_keep_both_streams_and_expanded_bounds() {
    for fx in [Fx::Blur, Fx::Shadow, Fx::Glow, Fx::Chain] {
        for content_fg in [false, true] {
            exercise(
                fx,
                Scene {
                    content_fg,
                    ..Default::default()
                },
            );
        }
    }
}
#[test]
#[ignore] // Requires GPU
fn effect_group_blends_join_content_at_its_native_anchor() {
    for mode in [
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Difference,
    ] {
        for owner_fg in [false, true] {
            exercise(
                Fx::Blend(mode),
                Scene {
                    content_fg: !owner_fg,
                    owner_fg,
                    ..Default::default()
                },
            );
        }
    }
}
#[test]
#[ignore] // Requires GPU
fn effect_group_binding_and_css_endpoints_recover_with_nested_clips() {
    for source in [
        Source::Binding,
        Source::Css(CssSource::Animation),
        Source::Css(CssSource::Transition),
    ] {
        exercise(
            Fx::Chain,
            Scene {
                source,
                content_fg: true,
                nested: true,
                clipped: true,
                ..Default::default()
            },
        );
    }
}
#[test]
#[ignore] // Requires GPU
fn effect_group_image_masks_preload_background_and_foreground_owners() {
    for owner_fg in [false, true] {
        for luminance in [false, true] {
            exercise(
                Fx::Mask(luminance),
                Scene {
                    owner_fg,
                    content_fg: !owner_fg,
                    ..Default::default()
                },
            );
        }
    }
}
#[test]
#[ignore] // Requires GPU
fn effect_group_dpi_and_nested_effects_keep_cropped_clip_frames() {
    exercise(
        Fx::Shadow,
        Scene {
            dpi: 1.25,
            content_fg: true,
            nested: true,
            clipped: true,
            ..Default::default()
        },
    );
}
#[test]
fn effect_group_capabilities_preserve_owner_and_unsupported_fallback() {
    super::opacity_group_tests::init_theme();
    for missing in 0..4 {
        for boundary in 0..5 {
            let mut tree = scene(
                Scene {
                    content_fg: true,
                    ..Default::default()
                },
                None,
            );
            tree.update_render_props(tree.query_by_id("group").unwrap(), |p| {
                p.opacity = 1.0;
                p.layer_effects = vec![LayerEffect::brightness(0.5)];
                p.mix_blend_mode = Some(BlendMode::Screen);
                match boundary {
                    1 => p.flow = Some("separate-pass".into()),
                    2 => p.is_overlay_root = true,
                    3 => p.shape_3d = Some(1.0),
                    4 => p.layer = RenderLayer::Glass,
                    _ => {}
                }
            });
            let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
            let mut paint = GpuPaintContext::new(240.0, 180.0);
            if missing != 1 {
                paint.enable_deferred_text();
            }
            if missing != 2 {
                paint.enable_deferred_svg();
            }
            if missing != 3 {
                paint.enable_deferred_image();
            }
            tree.render_with_motion(&mut paint, &state);
            let batch = paint.take_batch();
            let dynamic = paint.take_dynamic_batch();
            let captured = missing == 0 && boundary == 0;
            assert_eq!(!batch.cross_stream_groups.text_nodes.is_empty(), captured);
            assert_eq!(!batch.cross_stream_groups.svg_nodes.is_empty(), captured);
            assert_eq!(!batch.cross_stream_groups.image_nodes.is_empty(), captured);
            let late = missing == 0 && boundary == 2;
            assert_eq!(!dynamic.cross_stream_groups.text_nodes.is_empty(), late);
            assert_eq!(!dynamic.cross_stream_groups.svg_nodes.is_empty(), late);
            assert_eq!(!dynamic.cross_stream_groups.image_nodes.is_empty(), late);
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn effect_group_replacement_and_removal_refresh_cached_content() {
    for compositor in [false, true] {
        let opts = Scene {
            content_fg: true,
            ..Default::default()
        };
        let mut expected_context = super::main_text_clip_tests::context(true, 1);
        let mut context = super::main_text_clip_tests::context(true, 1);
        let mut tree = scene(opts, None);
        let group = tree.query_by_id("group").unwrap();
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        frame(&mut expected_context, &tree, &state, false);
        let baseline = expected_context.cached_bg_batch.clone().unwrap();
        for fx in [
            Fx::Swap,
            Fx::Blur,
            Fx::Mask(false),
            Fx::Blend(BlendMode::Normal),
            Fx::Chain,
        ] {
            let batch = with_config(&baseline, fx, 0.5, opts);
            let expected = reference(&mut expected_context, &batch);
            let (effects, blend) = effects(fx, 1.0);
            tree.update_render_props(group, |p| {
                p.opacity = 0.5;
                p.layer_effects = effects;
                p.mix_blend_mode = Some(blend);
            });
            context.invalidate_render_cache();
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, compositor);
                for y in 0..180 {
                    for x in 0..240 {
                        pixel(&page, (x, y), expected.get_pixel(x, y).0);
                    }
                }
            }
        }
    }
}

#[test]
fn effect_group_native_only_does_not_require_content_capabilities() {
    use blinc_core::DrawContext;
    super::opacity_group_tests::init_theme();
    let ui = div()
        .id("group")
        .w(100.0)
        .h(100.0)
        .bg(Color::RED)
        .child(div().w(40.0).h(40.0).bg(Color::GREEN).foreground());
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(100.0, 100.0);
    tree.update_render_props(tree.root().unwrap(), |p| {
        p.layer_effects = vec![LayerEffect::brightness(0.6)]
    });
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    let mut paint = GpuPaintContext::new(100.0, 100.0);
    assert!(!paint.supports_deferred_text());
    tree.render_with_motion(&mut paint, &state);
    let mut batch = paint.take_batch();
    batch.resolve_cross_stream_groups();
    assert!(batch.has_cross_stream_groups());
    assert!(batch.foreground_primitives.is_empty());
    assert_eq!(batch.primitives.len(), 2);
    assert!(batch.layer_commands.iter().any(|entry|matches!(&entry.command,LayerCommand::Push{config} if config.effects==vec![LayerEffect::brightness(0.6)])));
}

#[test]
fn effect_group_unsupported_effect_owner_keeps_nested_native_fallback() {
    super::opacity_group_tests::init_theme();
    let ui = div()
        .w(240.0)
        .h(180.0)
        .layer_effect(LayerEffect::brightness(0.6))
        .child(
            div()
                .w(100.0)
                .h(100.0)
                .opacity(0.5)
                .child(div().w(40.0).h(40.0).bg(Color::GREEN).foreground()),
        )
        .child(text("unresolved collected content"));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(240.0, 180.0);
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    let mut paint = GpuPaintContext::new(240.0, 180.0);
    tree.render_with_motion(&mut paint, &state);
    let batch = paint.take_batch();
    assert!(!batch.has_cross_stream_groups());
    assert_eq!(batch.foreground_primitives[0].color[3], 0.5);
}
