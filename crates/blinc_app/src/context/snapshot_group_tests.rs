//! Stateless tree frames retain the same native group boundaries as animated frames.
use super::*;
use blinc_core::{DrawContext, Rect};

const IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNg+M/wHwAEAQH/cetH5QAAAABJRU5ErkJggg==";

fn target(context: &RenderContext, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Snapshot group test"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: context.texture_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    (texture, view)
}

fn scene() -> RenderTree {
    let mut tree = RenderTree::from_element(
        &div().w(240.0).h(180.0).bg(Color::BLUE).child(
            div()
                .id("group")
                .absolute()
                .left(20.0)
                .top(20.0)
                .w(140.0)
                .h(130.0)
                .bg(Color::RED)
                .opacity(0.5)
                .child(
                    canvas(|ctx, _| {
                        ctx.fill_rect(
                            Rect::new(20.0, 20.0, 60.0, 60.0),
                            0.0.into(),
                            Color::GREEN.into(),
                        )
                    })
                    .absolute()
                    .w(140.0)
                    .h(130.0),
                )
                .child(
                    div()
                        .absolute()
                        .left(25.0)
                        .top(25.0)
                        .child(text("M").size(24.0).color(Color::WHITE)),
                ),
        ),
    );
    tree.compute_layout(240.0, 180.0);
    tree
}

fn frame(context: &mut RenderContext, tree: &RenderTree) -> image::RgbaImage {
    let scale = tree.scale_factor() as u32;
    let (w, h) = (240 * scale, 180 * scale);
    let (target, view) = target(context, w, h);
    context.render_tree(tree, w, h, &view).unwrap();
    assert!(!context.has_render_cache());
    assert!(context.css_composited_textures.is_empty());
    assert!(context.motion_subtree_textures.is_empty());
    crate::tests::read_rendered_pixels(context.device(), context.queue(), &target, w, h)
}

fn pixel(page: &image::RgbaImage, scale: u32, point: (u32, u32), color: [u8; 4]) {
    super::opacity_group_tests::pixel(page, (point.0 * scale, point.1 * scale), color);
}

fn faded(color: [u8; 3], opacity: f32) -> [u8; 4] {
    [
        (color[0] as f32 * opacity).round() as u8,
        (color[1] as f32 * opacity).round() as u8,
        (color[2] as f32 * opacity + 255.0 * (1.0 - opacity)).round() as u8,
        255,
    ]
}

#[test]
#[ignore] // Requires GPU
fn snapshot_group_opacity_applies_once_to_overlapping_canvas_and_text() {
    for unified in [false, true] {
        for samples in [1, 4] {
            let mut context = super::main_text_clip_tests::context(unified, samples);
            for scale in [1, 2] {
                let mut tree = scene();
                tree.set_scale_factor(scale as f32);
                let group = tree.query_by_id("group").unwrap();
                for opacity in [0.5_f32, 0.0, 1.0, 0.75, 0.5] {
                    tree.update_render_props(group, |p| p.opacity = opacity);
                    for _ in 0..2 {
                        let page = frame(&mut context, &tree);
                        pixel(&page, scale, (90, 90), faded([0, 255, 0], opacity));
                        pixel(&page, scale, (25, 25), faded([255, 0, 0], opacity));
                        if opacity > 0.0 {
                            let white = (45 * scale..78 * scale)
                                .flat_map(|y| (45 * scale..75 * scale).map(move |x| (x, y)))
                                .map(|(x, y)| page.get_pixel(x, y).0[0])
                                .max()
                                .unwrap();
                            assert!(white as f32 >= opacity * 250.0, "text missing at {opacity}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn snapshot_group_images_svg_styled_text_and_foreground_share_effects() {
    for samples in [1, 4] {
        let content=div().id("group").absolute().left(20.0).top(20.0).w(140.0).h(130.0).bg(Color::RED)
            .child(div().absolute().left(20.0).top(80.0).child(blinc_layout::image(IMAGE).size(20.0,20.0).fade_in(std::time::Duration::ZERO)))
            .child(div().absolute().left(50.0).top(80.0).w(20.0).h(20.0).background(blinc_core::ImageBrush::new(IMAGE)))
            .child(div().absolute().left(85.0).top(80.0).child(blinc_layout::svg(r##"<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15"><rect width="15" height="15" fill="#ffff00"/></svg>"##).size(15.0,15.0)))
            .child(div().absolute().left(5.0).top(5.0).child(blinc_layout::rich_text("<span color=\"#00ffff\"><b>MM</b></span>").size(18.0).no_wrap()))
            .child(div().absolute().left(100.0).top(20.0).w(20.0).h(20.0).bg(Color::YELLOW).foreground());
        let mut tree =
            RenderTree::from_element(&div().w(240.0).h(180.0).bg(Color::BLUE).child(content));
        tree.compute_layout(240.0, 180.0);
        let group = tree.query_by_id("group").unwrap();
        let mut context = super::main_text_clip_tests::context(true, samples);
        for brightness in [1.0_f32, 0.5] {
            tree.update_render_props(group, |p| {
                p.layer_effects = if brightness == 1.0 {
                    vec![]
                } else {
                    vec![blinc_core::LayerEffect::brightness(brightness)]
                }
            });
            for opacity in [0.5_f32, 0.0, 1.0, 0.75, 0.5] {
                tree.update_render_props(group, |p| p.opacity = opacity);
                for _ in 0..2 {
                    let page = frame(&mut context, &tree);
                    let primary = (255.0 * brightness).round() as u8;
                    pixel(&page, 1, (25, 90), faded([primary, 0, 0], opacity));
                    pixel(&page, 1, (50, 110), faded([0, primary, 0], opacity));
                    pixel(&page, 1, (80, 110), faded([0, primary, 0], opacity));
                    pixel(&page, 1, (110, 110), faded([primary, primary, 0], opacity));
                    pixel(&page, 1, (130, 50), faded([primary, primary, 0], opacity));
                    let green = (25..48)
                        .flat_map(|y| (25..65).map(move |x| (x, y)))
                        .map(|(x, y)| page.get_pixel(x, y).0[1])
                        .max()
                        .unwrap();
                    assert!(
                        (green as f32 - opacity * brightness * 255.0).abs() <= 3.0,
                        "styled text opacity/effect mismatch: {green}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn snapshot_group_nested_foreground_opacity_respects_parent_clips() {
    let mut tree = RenderTree::from_element(
        &div().w(240.0).h(180.0).bg(Color::BLUE).child(
            div()
                .id("group")
                .absolute()
                .left(20.0)
                .top(20.0)
                .w(140.0)
                .h(130.0)
                .bg(Color::RED)
                .opacity(0.5)
                .child(
                    div()
                        .opacity(0.5)
                        .child(
                            div()
                                .absolute()
                                .left(20.0)
                                .top(20.0)
                                .w(60.0)
                                .h(60.0)
                                .bg(Color::GREEN),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(40.0)
                                .top(40.0)
                                .w(60.0)
                                .h(60.0)
                                .bg(Color::GREEN)
                                .foreground(),
                        ),
                )
                .child(text("M").color(Color::WHITE)),
        ),
    );
    tree.compute_layout(240.0, 180.0);
    let owner = tree.query_by_id("group").unwrap();
    tree.update_render_props(owner, |p| {
        p.clip_path = Some(blinc_core::ClipPath::Inset {
            top: blinc_core::ClipLength::Px(0.0),
            right: blinc_core::ClipLength::Px(50.0),
            bottom: blinc_core::ClipLength::Px(0.0),
            left: blinc_core::ClipLength::Px(0.0),
            round: None,
        })
    });
    let mut context = super::main_text_clip_tests::context(true, 1);
    let page = frame(&mut context, &tree);
    pixel(&page, 1, (70, 70), [64, 64, 127, 255]);
    pixel(&page, 1, (115, 100), [0, 0, 255, 255]);
}

#[test]
#[ignore] // Requires GPU
fn snapshot_group_interleaved_stateful_frames_preserve_entry_motion() {
    use blinc_animation::{AnimationScheduler, Spring, SpringConfig};
    use blinc_layout::element::{MotionAnimation, MotionKeyframe};
    for compositor in [false, true] {
        let mut tree = RenderTree::from_element(
            &div().w(240.0).h(180.0).bg(Color::BLUE).child(
                div()
                    .id("group")
                    .absolute()
                    .left(20.0)
                    .top(20.0)
                    .w(140.0)
                    .h(130.0)
                    .bg(Color::RED),
            ),
        );
        tree.compute_layout(240.0, 180.0);
        let group = tree.query_by_id("group").unwrap();
        let motion = MotionAnimation {
            enter_from: Some(MotionKeyframe::new().opacity(0.25)),
            enter_duration_ms: 1000,
            ..Default::default()
        };
        tree.update_render_props(group, |p| p.motion = Some(motion.clone()));
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let sentinel = scheduler
            .lock()
            .unwrap()
            .add_spring(Spring::new(SpringConfig::stiff(), 0.42));
        let mut state = RenderState::new(scheduler);
        state.start_enter_motion(group, motion);
        let mut context = super::main_text_clip_tests::context(true, 1);
        let (target, view) = target(&context, 240, 180);
        for _ in 0..3 {
            context
                .render_tree_with_motion_opt(
                    &tree,
                    &state,
                    240,
                    180,
                    &view,
                    compositor.then_some(&target),
                    true,
                )
                .unwrap();
            let page = crate::tests::read_rendered_pixels(
                context.device(),
                context.queue(),
                &target,
                240,
                180,
            );
            pixel(&page, 1, (25, 25), [64, 0, 191, 255]);
            let page = frame(&mut context, &tree);
            pixel(&page, 1, (25, 25), [255, 0, 0, 255]);
            assert_eq!(
                blinc_layout::render_state::get_global_scheduler()
                    .unwrap()
                    .get_spring_value(sentinel),
                Some(0.42)
            );
        }
        assert_eq!(state.get_motion_values(group).unwrap().opacity, Some(0.25));
    }
}
