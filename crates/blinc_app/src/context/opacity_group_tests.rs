use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::read_rendered_pixels;
use blinc_animation::AnimationScheduler;
use blinc_gpu::primitives::LayerCommand;

#[derive(Clone, Copy, Debug)]
enum Content {
    SingleChild,
    OwnBackground,
    Canvas,
    NestedOpacity,
}

const CASES: [Content; 4] = [
    Content::SingleChild,
    Content::OwnBackground,
    Content::Canvas,
    Content::NestedOpacity,
];

fn rectangles() -> Div {
    div()
        .absolute()
        .w(160.0)
        .h(120.0)
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
                .left(40.0)
                .top(40.0)
                .w(60.0)
                .h(60.0)
                .bg(Color::GREEN),
        )
}

fn fixture(content: Content, foreground: bool) -> RenderTree {
    let ui = div().w(160.0).h(120.0).bg(Color::BLUE);
    let ui = match content {
        Content::Canvas => {
            let leaf = canvas(|ctx, _| {
                ctx.fill_rect(
                    Rect::new(20.0, 20.0, 60.0, 60.0),
                    0.0.into(),
                    Color::RED.into(),
                );
                ctx.draw_rgba_pixels(&[0, 255, 0, 255], 1, 1, Rect::new(40.0, 40.0, 60.0, 60.0));
                ctx.push_opacity(0.5);
                ctx.fill_rect(
                    Rect::new(130.0, 5.0, 10.0, 10.0),
                    0.0.into(),
                    Color::WHITE.into(),
                );
                ctx.pop_opacity();
            })
            .absolute()
            .w(160.0)
            .h(120.0)
            .opacity(0.5);
            ui.child(if foreground { leaf.foreground() } else { leaf })
        }
        _ => {
            let owner = match content {
                Content::OwnBackground => div()
                    .absolute()
                    .left(20.0)
                    .top(20.0)
                    .w(60.0)
                    .h(60.0)
                    .bg(Color::RED)
                    .child(
                        div()
                            .absolute()
                            .left(20.0)
                            .top(20.0)
                            .w(60.0)
                            .h(60.0)
                            .bg(Color::GREEN),
                    ),
                Content::SingleChild => div().absolute().w(160.0).h(120.0).child(rectangles()),
                Content::NestedOpacity => div()
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .child(rectangles().opacity(0.5)),
                Content::Canvas => unreachable!(),
            }
            .opacity(0.5);
            ui.child(if foreground {
                owner.foreground()
            } else {
                owner
            })
        }
    };
    let ui = ui.child(
        div()
            .absolute()
            .left(130.0)
            .top(100.0)
            .w(10.0)
            .h(10.0)
            .bg(Color::rgba(1.0, 1.0, 1.0, 0.5)),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    tree
}

fn init_theme() {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
}

#[test]
fn opacity_groups_single_child_and_leaf_output_keep_commands_with_their_owner() {
    init_theme();
    for content in CASES {
        for (foreground, route) in [
            (false, 0),
            (true, 0),
            (false, 1),
            (true, 1),
            (false, 2),
            (true, 2),
        ] {
            let tree = fixture(content, foreground);
            let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
            let mut paint = GpuPaintContext::new(160.0, 120.0);
            if route == 1 {
                paint.push_motion_subtree();
            }
            if route == 2 {
                paint.push_composite_layer(91);
            }
            tree.render_with_motion(&mut paint, &state);
            if route == 1 {
                paint.pop_motion_subtree();
            }
            if route == 2 {
                paint.pop_composite_layer();
            }
            let batch = match route {
                0 => paint.take_batch(),
                1 => paint.take_dynamic_batch(),
                _ => paint.take_composite_layer_batches().remove(&91).unwrap(),
            };
            let commands = if foreground && route != 2 {
                &batch.foreground_layer_commands
            } else {
                &batch.layer_commands
            };
            let groups = if matches!(content, Content::NestedOpacity) {
                2
            } else {
                1
            };
            assert_eq!(
                commands.len(),
                groups * 2,
                "{content:?} foreground={foreground} route={route}"
            );
            for command in commands.iter().take(groups) {
                assert!(
                    matches!(&command.command, LayerCommand::Push { config } if config.opacity == 0.5)
                );
            }
            let sentinel = batch
                .primitives
                .iter()
                .find(|p| p.bounds == [130.0, 100.0, 10.0, 10.0])
                .unwrap();
            assert_eq!(
                sentinel.color[3], 0.5,
                "{content:?} foreground={foreground} route={route}"
            );
            if matches!(content, Content::Canvas) {
                assert!(tree.had_retained_canvas_painted());
                assert!(tree.canvas_paint_records().is_empty());
                let images = if foreground && route != 2 {
                    &batch.foreground_dynamic_images
                } else {
                    &batch.dynamic_images
                };
                assert_eq!(images.len(), 1);
                assert_eq!(images[0].opacity, 1.0);
            }
        }
    }
}

fn pixel(page: &image::RgbaImage, xy: (u32, u32), expected: [u8; 4]) {
    let actual = page.get_pixel(xy.0, xy.1).0;
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
        "{xy:?}: {actual:?}, expected {expected:?}"
    );
}

#[test]
#[ignore] // Requires GPU
fn opacity_groups_foreign_pass_keeps_inherited_fallback() {
    init_theme();
    for compositor in [false, true] {
        let mut app = BlincApp::with_config(BlincConfig {
            sample_count: 1,
            ..Default::default()
        })
        .expect("offscreen GPU required");
        for child_opacity in [1.0_f32, 0.5] {
            let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
                div().absolute().w(160.0).h(120.0).opacity(0.5).child(
                    div()
                        .absolute()
                        .left(20.0)
                        .top(20.0)
                        .w(60.0)
                        .h(60.0)
                        .foreground()
                        .opacity(child_opacity)
                        .bg(Color::GREEN),
                ),
            );
            let mut tree = RenderTree::from_element(&ui);
            tree.compute_layout(160.0, 120.0);
            let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
            app.context().invalidate_render_cache();
            for replay in [false, true] {
                let texture = surface(&app);
                let view = texture.create_view(&Default::default());
                app.render_tree_with_motion_opt(
                    &tree,
                    &state,
                    &view,
                    compositor.then_some(&texture),
                    160,
                    120,
                    replay,
                )
                .unwrap();
                let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
                let alpha = 0.5 * child_opacity;
                pixel(
                    &page,
                    (55, 55),
                    [
                        0,
                        (255.0 * alpha).round() as u8,
                        (255.0 * (1.0 - alpha)).round() as u8,
                        255,
                    ],
                );
            }
        }
    }
}

fn surface(app: &BlincApp) -> wgpu::Texture {
    app.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Opacity group surface"),
        size: wgpu::Extent3d {
            width: 160,
            height: 120,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: app.texture_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

#[test]
#[ignore] // Requires GPU
fn opacity_groups_single_child_and_leaf_frames_preserve_overlap_and_recovery() {
    init_theme();
    for content in CASES {
        for (foreground, compositor) in [(false, false), (false, true), (true, false), (true, true)]
        {
            let mut app = BlincApp::with_config(BlincConfig {
                sample_count: 1,
                ..Default::default()
            })
            .expect("offscreen GPU required");
            let mut tree = fixture(content, foreground);
            let owner = tree.layout_tree.children(tree.root().unwrap())[0];
            let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
            for opacity in [0.5_f32, 1.0, 0.0, 0.75, 0.5] {
                tree.update_render_props(owner, |p| p.opacity = opacity);
                app.context().invalidate_render_cache();
                for replay in [false, true] {
                    let texture = surface(&app);
                    let view = texture.create_view(&Default::default());
                    app.render_tree_with_motion_opt(
                        &tree,
                        &state,
                        &view,
                        compositor.then_some(&texture),
                        160,
                        120,
                        replay,
                    )
                    .unwrap();
                    let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
                    eprintln!(
                        "{content:?} foreground={foreground} compositor={compositor} opacity={opacity} replay={replay}"
                    );
                    let alpha = opacity
                        * if matches!(content, Content::NestedOpacity) {
                            0.5
                        } else {
                            1.0
                        };
                    pixel(
                        &page,
                        (55, 55),
                        [
                            0,
                            (255.0 * alpha).round() as u8,
                            (255.0 * (1.0 - alpha)).round() as u8,
                            255,
                        ],
                    );
                    pixel(
                        &page,
                        (25, 25),
                        [
                            (255.0 * alpha).round() as u8,
                            0,
                            (255.0 * (1.0 - alpha)).round() as u8,
                            255,
                        ],
                    );
                    pixel(&page, (5, 5), [0, 0, 255, 255]);
                    pixel(&page, (135, 105), [128, 128, 255, 255]);
                    if matches!(content, Content::Canvas) {
                        pixel(
                            &page,
                            (135, 10),
                            [
                                (127.5 * opacity).round() as u8,
                                (127.5 * opacity).round() as u8,
                                255,
                                255,
                            ],
                        );
                    }
                }
            }
        }
    }
}
