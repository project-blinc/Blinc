//! Canvas GPU passes participate in native visual groups.
use super::*;

#[test]
#[ignore] // Requires GPU
fn gpu_pass_group_opacity_and_paint_order() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let pass = super::release_boundary_tests::marker_pass();
    let mut tree = RenderTree::from_element(
        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(120.0)
                .h(100.0)
                .bg(Color::RED)
                .opacity(0.5)
                .child(
                    canvas(move |ctx, _| {
                        ctx.run_gpu_pass(&pass, Some(Rect::new(10.0, 10.0, 60.0, 60.0)));
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
    let (texture, view) = target(&context, 1);
    context.render_tree(&tree, 160, 120, &view).unwrap();
    let page =
        crate::tests::read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    super::opacity_group_tests::pixel(&page, (35, 25), [0, 128, 255, 255]);
    super::opacity_group_tests::pixel(&page, (55, 45), [0, 128, 127, 255]);
    super::opacity_group_tests::pixel(&page, (25, 15), [128, 0, 127, 255]);
}

pub(super) fn target(context: &RenderContext, scale: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("GPU pass groups"),
        size: wgpu::Extent3d {
            width: 160 * scale,
            height: 120 * scale,
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

pub(super) fn frame(
    context: &mut RenderContext,
    tree: &RenderTree,
    state: &RenderState,
    mode: u8,
) -> image::RgbaImage {
    let scale = tree.scale_factor() as u32;
    let (texture, view) = target(context, scale);
    let (w, h) = (160 * scale, 120 * scale);
    if mode == 0 {
        context.render_tree(tree, w, h, &view).unwrap();
    } else {
        context
            .render_tree_with_motion_opt(
                tree,
                state,
                w,
                h,
                &view,
                (mode == 2).then_some(&texture),
                true,
            )
            .unwrap();
    }
    crate::tests::read_rendered_pixels(context.device(), context.queue(), &texture, w, h)
}

#[test]
#[ignore] // Requires GPU
fn gpu_pass_group_replay_foreground_effects_and_opacity_endpoints() {
    for samples in [1, 4] {
        let mut context = super::main_text_clip_tests::context(true, samples);
        let state = RenderState::new(Arc::new(Mutex::new(
            blinc_animation::AnimationScheduler::new(),
        )));
        for mode in 0..3 {
            for foreground in [false, true] {
                let pass = super::release_boundary_tests::marker_pass();
                let leaf = canvas(move |ctx, _| {
                    ctx.run_gpu_pass(&pass, Some(Rect::new(10.0, 10.0, 60.0, 60.0)));
                    ctx.fill_rect(
                        Rect::new(30.0, 30.0, 20.0, 20.0),
                        0.0.into(),
                        Color::GREEN.into(),
                    );
                })
                .absolute()
                .w(120.0)
                .h(100.0);
                let leaf = if foreground { leaf.foreground() } else { leaf };
                let mut tree = RenderTree::from_element(
                    &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                        div()
                            .id("owner")
                            .absolute()
                            .left(20.0)
                            .top(10.0)
                            .w(120.0)
                            .h(100.0)
                            .bg(Color::RED)
                            .opacity(0.5)
                            .child(leaf),
                    ),
                );
                tree.compute_layout(160.0, 120.0);
                let owner = tree.query_by_id("owner").unwrap();
                for scale in [1, 2] {
                    tree.set_scale_factor(scale as f32);
                    for opacity in [0.5, 0.0, 1.0, 0.75, 0.5] {
                        for brightness in [1.0, 0.5] {
                            tree.update_render_props(owner, |p| {
                                p.opacity = opacity;
                                p.layer_effects =
                                    vec![blinc_core::LayerEffect::brightness(brightness)];
                            });
                            context.invalidate_render_cache();
                            for _ in 0..2 {
                                let page = frame(&mut context, &tree, &state, mode);
                                let faded = |rgb: [f32; 3]| {
                                    [
                                        (rgb[0] * opacity * brightness * 255.0).round() as u8,
                                        (rgb[1] * opacity * brightness * 255.0).round() as u8,
                                        ((rgb[2] * brightness * opacity + 1.0 - opacity) * 255.0)
                                            .round() as u8,
                                        255,
                                    ]
                                };
                                for (xy, rgb) in [
                                    ((35, 25), [0.0, 1.0, 1.0]),
                                    ((55, 45), [0.0, 1.0, 0.0]),
                                    ((25, 15), [1.0, 0.0, 0.0]),
                                ] {
                                    if page
                                        .get_pixel(xy.0 * scale, xy.1 * scale)
                                        .0
                                        .iter()
                                        .zip(faded(rgb))
                                        .any(|(a, b)| a.abs_diff(b) > 3)
                                    {
                                        eprintln!(
                                            "samples={samples} mode={mode} foreground={foreground} scale={scale} opacity={opacity} brightness={brightness}"
                                        );
                                    }
                                    super::opacity_group_tests::pixel(
                                        &page,
                                        (xy.0 * scale, xy.1 * scale),
                                        faded(rgb),
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

use blinc_gpu::custom_pass::{CustomRenderPass, GpuPass, RenderPassContext, RenderStage};

#[derive(Clone, Copy)]
enum Edit {
    Load,
    Clear(wgpu::Color),
}
struct EditPass {
    edit: Arc<Mutex<Edit>>,
    frames: Arc<Mutex<Vec<(u32, u32, f64)>>>,
}
impl CustomRenderPass for EditPass {
    fn label(&self) -> &str {
        "Group callback edits"
    }
    fn stage(&self) -> RenderStage {
        RenderStage::PostProcess
    }
    fn initialize(&mut self, _: &wgpu::Device, _: &wgpu::Queue, _: wgpu::TextureFormat) {}
    fn render(&mut self, ctx: &RenderPassContext) {
        self.frames.lock().unwrap().push((
            ctx.viewport_width,
            ctx.viewport_height,
            ctx.scale_factor,
        ));
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(self.label()),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: ctx.target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: match *self.edit.lock().unwrap() {
                            Edit::Load => wgpu::LoadOp::Load,
                            Edit::Clear(c) => wgpu::LoadOp::Clear(c),
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        ctx.queue.submit(Some(encoder.finish()));
    }
}

pub(super) fn clear_pass(color: wgpu::Color) -> (GpuPass, Arc<Mutex<Vec<(u32, u32, f64)>>>) {
    let frames = Arc::new(Mutex::new(Vec::new()));
    let pass = GpuPass::new(EditPass {
        edit: Arc::new(Mutex::new(Edit::Clear(color))),
        frames: frames.clone(),
    });
    (pass, frames)
}

#[test]
#[ignore] // Requires GPU
fn gpu_pass_group_load_clear_path_clip_and_explicit_opacity() {
    for unified in [false, true] {
        let mut context = super::main_text_clip_tests::context(unified, 4);
        let state = RenderState::new(Arc::new(Mutex::new(
            blinc_animation::AnimationScheduler::new(),
        )));
        let edit = Arc::new(Mutex::new(Edit::Load));
        let frames = Arc::new(Mutex::new(Vec::new()));
        let pass = GpuPass::new(EditPass {
            edit: edit.clone(),
            frames: frames.clone(),
        });
        let mut tree = RenderTree::from_element(
            &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(120.0)
                    .h(100.0)
                    .bg(Color::RED)
                    .opacity(0.5)
                    .child(
                        canvas(move |ctx, _| {
                            let path = blinc_core::Path::new()
                                .move_to(10.0, 10.0)
                                .line_to(90.0, 10.0)
                                .line_to(10.0, 90.0)
                                .close();
                            ctx.push_clip(blinc_core::ClipShape::path(path));
                            ctx.push_opacity(0.5);
                            ctx.run_gpu_pass(&pass, None);
                            ctx.pop_opacity();
                            ctx.pop_clip();
                        })
                        .absolute()
                        .w(120.0)
                        .h(100.0),
                    ),
            ),
        );
        tree.compute_layout(160.0, 120.0);
        for scale in [1, 2] {
            tree.set_scale_factor(scale as f32);
            for mode in 0..3 {
                for (change, expected) in [
                    (Edit::Load, [128, 0, 127, 255]),
                    (Edit::Clear(wgpu::Color::TRANSPARENT), [64, 0, 191, 255]),
                    (
                        Edit::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.5,
                            b: 0.5,
                            a: 0.5,
                        }),
                        [64, 32, 191, 255],
                    ),
                    (
                        Edit::Clear(wgpu::Color {
                            r: 0.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                        [64, 64, 191, 255],
                    ),
                    (Edit::Load, [128, 0, 127, 255]),
                ] {
                    *edit.lock().unwrap() = change;
                    for _ in 0..2 {
                        let page = frame(&mut context, &tree, &state, mode);
                        super::opacity_group_tests::pixel(
                            &page,
                            (40 * scale, 30 * scale),
                            expected,
                        );
                        super::opacity_group_tests::pixel(
                            &page,
                            (100 * scale, 90 * scale),
                            [128, 0, 127, 255],
                        );
                        super::opacity_group_tests::pixel(
                            &page,
                            (5 * scale, 5 * scale),
                            [0, 0, 255, 255],
                        );
                        assert_eq!(
                            frames.lock().unwrap().last(),
                            Some(&(160 * scale, 120 * scale, scale as f64))
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn gpu_pass_group_motion_binding_recovers_without_cache_invalidation() {
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    for compositor in [false, true] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let alpha = Arc::new(Mutex::new(AnimatedValue::with_default(
            scheduler.lock().unwrap().handle(),
            0.5,
        )));
        let pass = super::release_boundary_tests::marker_pass();
        let mut tree = RenderTree::from_element(
            &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(120.0)
                    .h(100.0)
                    .bg(Color::RED)
                    .opacity(0.99999)
                    .child(
                        canvas(move |ctx, _| {
                            ctx.run_gpu_pass(&pass, Some(Rect::new(10.0, 10.0, 60.0, 60.0)));
                        })
                        .absolute()
                        .w(120.0)
                        .h(100.0),
                    )
                    .motion()
                    .opacity(alpha.clone()),
            ),
        );
        tree.compute_layout(160.0, 120.0);
        let state = RenderState::new(scheduler);
        for opacity in [0.5_f32, 0.0, 1.0, 0.75, 0.5] {
            alpha.lock().unwrap().set_immediate(opacity);
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, if compositor { 2 } else { 1 });
                super::opacity_group_tests::pixel(
                    &page,
                    (35, 25),
                    [0, (opacity * 255.0).round() as u8, 255, 255],
                );
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn gpu_pass_group_canvas_local_layers_preserve_dpi_on_independent_replay() {
    let mut context = super::main_text_clip_tests::context(false, 4);
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let frames = Arc::new(Mutex::new(Vec::new()));
    let pass = GpuPass::new(EditPass {
        edit: Arc::new(Mutex::new(Edit::Clear(wgpu::Color {
            r: 0.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        }))),
        frames: frames.clone(),
    });
    let mut tree = RenderTree::from_element(
        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
            canvas(move |ctx, _| {
                ctx.push_layer(blinc_core::LayerConfig {
                    opacity: 0.5,
                    ..Default::default()
                });
                ctx.fill_rect(
                    Rect::new(0.0, 0.0, 160.0, 120.0),
                    0.0.into(),
                    Color::RED.into(),
                );
                ctx.run_gpu_pass(&pass, None);
                ctx.fill_rect(
                    Rect::new(40.0, 40.0, 20.0, 20.0),
                    0.0.into(),
                    Color::GREEN.into(),
                );
                ctx.pop_layer();
            })
            .absolute()
            .w(160.0)
            .h(120.0),
        ),
    );
    tree.compute_layout(160.0, 120.0);
    for scale in [1, 2] {
        tree.set_scale_factor(scale as f32);
        for mode in 0..3 {
            context.invalidate_render_cache();
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, mode);
                super::opacity_group_tests::pixel(
                    &page,
                    (10 * scale, 10 * scale),
                    [0, 128, 255, 255],
                );
                super::opacity_group_tests::pixel(
                    &page,
                    (50 * scale, 50 * scale),
                    [0, 128, 127, 255],
                );
                assert_eq!(
                    frames.lock().unwrap().last(),
                    Some(&(160 * scale, 120 * scale, scale as f64))
                );
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn gpu_pass_group_cropped_named_outputs_keep_frame_coordinates_and_alpha() {
    use blinc_core::{LayerConfig, LayerId, Point, Size};
    let mut context = super::main_text_clip_tests::context(true, 4);
    let edit = Arc::new(Mutex::new(Edit::Load));
    let frames = Arc::new(Mutex::new(Vec::new()));
    let pass = GpuPass::new(EditPass {
        edit: edit.clone(),
        frames: frames.clone(),
    });
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_layer(
        LayerConfig::new()
            .id(LayerId(55))
            .position(Point::new(8.0, 8.0))
            .size(Size::new(32.0, 32.0))
            .opacity(0.5),
    );
    paint.fill_rect(
        Rect::new(8.0, 8.0, 16.0, 32.0),
        0.0.into(),
        Color::RED.into(),
    );
    paint.run_gpu_pass(&pass, None);
    paint.pop_layer();
    paint.sample_layer(
        LayerId(55),
        Rect::new(0.0, 0.0, 32.0, 32.0),
        Rect::new(72.0, 8.0, 32.0, 32.0),
    );
    let batch = paint.take_batch();
    for (change, left, right) in [
        (Edit::Load, [128, 0, 127, 255], [0, 0, 255, 255]),
        (
            Edit::Clear(wgpu::Color::TRANSPARENT),
            [0, 0, 255, 255],
            [0, 0, 255, 255],
        ),
        (
            Edit::Clear(wgpu::Color {
                r: 0.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            }),
            [0, 128, 255, 255],
            [0, 128, 255, 255],
        ),
        (Edit::Load, [128, 0, 127, 255], [0, 0, 255, 255]),
    ] {
        *edit.lock().unwrap() = change;
        for _ in 0..2 {
            let (texture, view) = target(&context, 1);
            context.renderer.resize(160, 120);
            context
                .renderer
                .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
            context.renderer.render_overlay_msaa(&view, &batch, 4);
            let page = crate::tests::read_rendered_pixels(
                context.device(),
                context.queue(),
                &texture,
                160,
                120,
            );
            for (x, color) in [(12, left), (30, right), (76, left), (94, right)] {
                super::opacity_group_tests::pixel(&page, (x, 20), color);
            }
            assert_eq!(frames.lock().unwrap().last(), Some(&(160, 120, 1.0)));
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn gpu_pass_group_glass_backdrop_reuses_callback_output() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let frames = Arc::new(Mutex::new(Vec::new()));
    let pass = GpuPass::new(EditPass {
        edit: Arc::new(Mutex::new(Edit::Clear(wgpu::Color {
            r: 0.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        }))),
        frames: frames.clone(),
    });
    let mut tree = RenderTree::from_element(
        &div()
            .w(160.0)
            .h(120.0)
            .bg(Color::BLUE)
            .child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(10.0)
                    .w(60.0)
                    .h(60.0)
                    .opacity(0.5)
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
            ),
    );
    tree.compute_layout(160.0, 120.0);
    for mode in 0..4 {
        context.invalidate_render_cache();
        for _ in 0..2 {
            let before = frames.lock().unwrap().len();
            let page = if mode < 3 {
                frame(&mut context, &tree, &state, mode)
            } else {
                let mut paint = GpuPaintContext::new(160.0, 120.0);
                tree.render_snapshot(&mut paint);
                let batch = paint.take_batch();
                let (texture, view) = target(&context, 1);
                let backdrop = context.device().create_texture(&wgpu::TextureDescriptor {
                    label: Some("Callback downsampled backdrop"),
                    size: wgpu::Extent3d {
                        width: 80,
                        height: 60,
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
                context.renderer.resize(160, 120);
                context.renderer.render_glass_frame(
                    &view,
                    &backdrop.create_view(&Default::default()),
                    (80, 60),
                    &batch,
                    false,
                );
                crate::tests::read_rendered_pixels(
                    context.device(),
                    context.queue(),
                    &texture,
                    160,
                    120,
                )
            };
            super::opacity_group_tests::pixel(&page, (40, 30), [0, 128, 255, 255]);
            assert_eq!(
                frames.lock().unwrap().len() - before,
                1,
                "callback repeated for backdrop, mode={mode}"
            );
        }
    }
}
