//! Image node opacity composes once with intrinsic pixels and surrounding groups.
use super::opacity_group_tests::pixel;
use super::*;

const GREEN: &str = super::overlay_group_tests::IMAGE;

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

fn scene(opacity: f32) -> RenderTree {
    let mut tree = RenderTree::from_element(
        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .id("host")
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(100.0)
                .h(100.0)
                .child(
                    blinc_layout::image(GREEN)
                        .size(60.0, 60.0)
                        .opacity(opacity)
                        .fade_in(std::time::Duration::ZERO),
                ),
        ),
    );
    tree.compute_layout(160.0, 120.0);
    tree
}

fn blend(alpha: f32) -> [u8; 4] {
    [
        0,
        (alpha * 255.0).round() as u8,
        ((1.0 - alpha) * 255.0).round() as u8,
        255,
    ]
}

fn image_node(tree: &RenderTree) -> LayoutNodeId {
    tree.layout_tree.children(tree.query_by_id("host").unwrap())[0]
}

#[test]
#[ignore] // Requires GPU
fn image_opacity_builder_half_is_not_squared() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let tree = scene(0.5);
    let state = state();
    for mode in 0..4 {
        for _ in 0..2 {
            pixel(
                &frame(&mut context, &tree, &state, mode),
                (40, 30),
                blend(0.5),
            );
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn image_opacity_parent_foreground_and_endpoints_compose_once() {
    for samples in [1, 4] {
        let mut context = super::main_text_clip_tests::context(true, samples);
        let state = state();
        for mode in 0..4 {
            for foreground in [false, true] {
                for scale in [1, 2] {
                    for opacity in [0.5, 0.0, 1.0, 0.25, 0.75, 0.5] {
                        let mut tree = scene(opacity);
                        tree.set_scale_factor(scale as f32);
                        let node = image_node(&tree);
                        tree.update_render_props(node, |p| {
                            p.layer = if foreground {
                                RenderLayer::Foreground
                            } else {
                                RenderLayer::Background
                            }
                        });
                        tree.update_render_props(tree.query_by_id("host").unwrap(), |p| {
                            p.opacity = 0.5
                        });
                        for _ in 0..2 {
                            let page = frame(&mut context, &tree, &state, mode);
                            pixel(&page, (40 * scale, 30 * scale), blend(opacity * 0.5));
                            pixel(&page, (5 * scale, 5 * scale), [0, 0, 255, 255]);
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn image_opacity_stylesheet_and_animated_values_replace_the_builder_value() {
    use super::opacity_group_tests::{CssSource, css_value};
    for mode in 0..4 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let state = state();
        for opacity in [0.0, 0.25, 0.75, 1.0, 0.5] {
            let mut tree = scene(0.5);
            tree.set_stylesheet(
                blinc_layout::css_parser::Stylesheet::parse(&format!(
                    "img {{ opacity: {opacity}; }}"
                ))
                .unwrap(),
            );
            tree.apply_stylesheet_base_styles();
            let node = image_node(&tree);
            assert_eq!(
                tree.get_render_node(node).unwrap().props.opacity,
                opacity,
                "stylesheet must match the image node"
            );
            for _ in 0..2 {
                pixel(
                    &frame(&mut context, &tree, &state, mode),
                    (40, 30),
                    blend(opacity),
                );
            }
        }
        if mode > 0 {
            for source in [CssSource::Animation, CssSource::Transition] {
                let mut tree = scene(0.5);
                let node = image_node(&tree);
                for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                    css_value(&tree, node, source, Some(opacity));
                    tree.apply_all_css_animation_props();
                    tree.apply_all_css_transition_props();
                    for _ in 0..2 {
                        pixel(
                            &frame(&mut context, &tree, &state, mode),
                            (40, 30),
                            blend(opacity),
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn image_opacity_authored_alpha_tint_and_load_fade_remain_independent() {
    const HALF_GREEN: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNg+M/QAAADggGA8sg/+gAAAABJRU5ErkJggg==";
    for mode in 0..4 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let state = state();
        let mut tree = RenderTree::from_element(
            &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(100.0)
                    .h(100.0)
                    .opacity(0.75)
                    .child(
                        blinc_layout::image(HALF_GREEN)
                            .size(60.0, 60.0)
                            .opacity(0.5)
                            .tint_rgba(1.0, 1.0, 1.0, 0.5)
                            .fade_in(std::time::Duration::from_millis(100_000)),
                    ),
            ),
        );
        tree.compute_layout(160.0, 120.0);
        frame(&mut context, &tree, &state, mode);
        for fade in [0.25_f32, 0.75, 1.0, 0.5] {
            context.image_load_times.insert(
                HALF_GREEN.into(),
                web_time::Instant::now()
                    - std::time::Duration::from_millis((fade * 100_000.0) as u64),
            );
            context.image_fade_deadlines.insert(
                HALF_GREEN.into(),
                web_time::Instant::now() + std::time::Duration::from_millis(100_000),
            );
            context.invalidate_render_cache();
            let page = frame(&mut context, &tree, &state, mode);
            pixel(
                &page,
                (40, 30),
                blend(0.75 * 0.5 * (128.0 / 255.0) * 0.5 * fade),
            );
        }
    }
}

struct IntrinsicImage {
    image: blinc_layout::Image,
    alpha: f32,
}
impl blinc_layout::ElementBuilder for IntrinsicImage {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> LayoutNodeId {
        self.image.build(tree)
    }
    fn render_props(&self) -> blinc_layout::RenderProps {
        self.image.render_props()
    }
    fn children_builders(&self) -> &[Box<dyn blinc_layout::ElementBuilder>] {
        self.image.children_builders()
    }
    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        blinc_layout::div::ElementTypeId::Image
    }
    fn image_render_info(&self) -> Option<blinc_layout::ImageRenderInfo> {
        let mut info = self.image.image_render_info().unwrap();
        info.opacity = self.alpha;
        Some(info)
    }
}

#[test]
#[ignore] // Requires GPU
fn image_opacity_custom_content_alpha_and_image_brush_alpha_are_preserved() {
    for mode in 0..4 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let state = state();
        for custom in [false, true] {
            let parent = div()
                .id("host")
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(100.0)
                .h(100.0)
                .opacity(0.5);
            let parent = if custom {
                parent.child(IntrinsicImage {
                    image: blinc_layout::image(GREEN)
                        .size(60.0, 60.0)
                        .opacity(0.5)
                        .no_fade(),
                    alpha: 0.4,
                })
            } else {
                parent.child(
                    div()
                        .w(60.0)
                        .h(60.0)
                        .opacity(0.5)
                        .background(blinc_core::ImageBrush::new(GREEN).opacity(0.4)),
                )
            };
            let mut tree =
                RenderTree::from_element(&div().w(160.0).h(120.0).bg(Color::BLUE).child(parent));
            tree.compute_layout(160.0, 120.0);
            for opacity in [0.5, 0.75, 0.4, 0.0, 1.0, 0.5] {
                tree.update_render_props(image_node(&tree), |p| p.opacity = opacity);
                context.invalidate_render_cache();
                pixel(
                    &frame(&mut context, &tree, &state, mode),
                    (40, 30),
                    blend(0.5 * opacity * 0.4),
                );
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn image_opacity_placeholder_and_loaded_source_keep_the_same_node_alpha() {
    for mode in 0..4 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let state = state();
        let path = std::env::temp_dir().join(format!(
            "blinc-image-opacity-{}-{mode}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut tree = RenderTree::from_element(
            &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                blinc_layout::image(path.to_str().unwrap())
                    .size(60.0, 60.0)
                    .opacity(0.5)
                    .placeholder_color(Color::CYAN)
                    .no_fade(),
            ),
        );
        tree.compute_layout(160.0, 120.0);
        pixel(
            &frame(&mut context, &tree, &state, mode),
            (30, 30),
            [0, 128, 255, 255],
        );
        image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255]))
            .save(&path)
            .unwrap();
        context.invalidate_render_cache();
        pixel(
            &frame(&mut context, &tree, &state, mode),
            (30, 30),
            blend(0.5),
        );
        std::fs::remove_file(&path).unwrap();
        context.invalidate_asset_path(&path);
        pixel(
            &frame(&mut context, &tree, &state, mode),
            (30, 30),
            [0, 128, 255, 255],
        );
    }
}

#[test]
#[ignore] // Requires GPU
fn image_opacity_incremental_and_full_rebuilds_refresh_the_node_value() {
    fn ui(opacity: f32) -> Div {
        div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .id("host")
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(100.0)
                .h(100.0)
                .child(
                    blinc_layout::image(GREEN)
                        .size(60.0, 60.0)
                        .opacity(opacity)
                        .no_fade(),
                ),
        )
    }
    for mode in 0..4 {
        for incremental in [false, true] {
            let mut context = super::main_text_clip_tests::context(true, 1);
            let state = state();
            let mut tree = RenderTree::from_element(&ui(0.5));
            tree.compute_layout(160.0, 120.0);
            frame(&mut context, &tree, &state, mode);
            for opacity in [0.0, 1.0, 0.25, 0.75, 0.5] {
                if incremental {
                    tree.incremental_update(&ui(opacity));
                } else {
                    assert!(tree.update_if_changed(&ui(opacity)));
                }
                tree.compute_layout(160.0, 120.0);
                context.invalidate_render_cache();
                for _ in 0..2 {
                    pixel(
                        &frame(&mut context, &tree, &state, mode),
                        (40, 30),
                        blend(opacity),
                    );
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn image_opacity_live_parent_binding_keeps_builder_alpha_and_zero_recovery() {
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
        let mut tree = RenderTree::from_element(
            &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(100.0)
                    .h(100.0)
                    .child(
                        blinc_layout::image(GREEN)
                            .size(60.0, 60.0)
                            .opacity(0.5)
                            .no_fade(),
                    )
                    .motion()
                    .opacity(alpha.clone())
                    .translate_x(x.clone()),
            ),
        );
        tree.compute_layout(160.0, 120.0);
        let state = RenderState::new(scheduler);
        for (opacity, offset) in [(0.5, 0), (0.0, 5), (1.0, 10), (0.75, 0), (0.5, 5)] {
            alpha.lock().unwrap().set_immediate(opacity);
            x.lock().unwrap().set_immediate(offset as f32);
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, mode);
                pixel(&page, (40 + offset, 30), blend(0.5 * opacity));
                if offset > 0 {
                    pixel(&page, (22, 30), [0, 0, 255, 255]);
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn image_opacity_independent_collection_uses_the_resolved_node_value() {
    let mut context = super::main_text_clip_tests::context(false, 1);
    let mut tree = scene(0.5);
    tree.update_render_props(tree.query_by_id("host").unwrap(), |p| p.opacity = 0.5);
    let node = image_node(&tree);
    for opacity in [0.5, 0.25, 0.0, 1.0, 0.5] {
        tree.update_render_props(node, |p| p.opacity = opacity);
        let (texts, svgs, images) = context.collect_render_elements(&tree);
        let (texture, target) = super::gpu_pass_group_tests::target(&context, 1);
        context.renderer.resize(160, 120);
        context.renderer.clear_target(&target, wgpu::Color::BLUE);
        context.preload_images(&images, 160.0, 120.0);
        context.render_images(&target, &images, 160.0, 120.0, 1.0);
        let page = crate::tests::read_rendered_pixels(
            context.device(),
            context.queue(),
            &texture,
            160,
            120,
        );
        pixel(&page, (40, 30), blend(opacity * 0.5));
        context.return_scratch_elements(texts, svgs, images);
    }
}
