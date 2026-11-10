//! Mesh scenes retain their canvas group's paint position and compositing state.
use super::gpu_pass_group_tests::{frame, target};
use super::*;
use blinc_core::{
    Camera, Mat4, Vec3,
    draw::{Material, MeshData, Vertex},
};

pub(super) fn mesh(color: [f32; 3]) -> Arc<MeshData> {
    Arc::new(MeshData {
        vertices: Arc::new(vec![
            Vertex::new([-1.0, -1.0, 0.0]),
            Vertex::new([1.0, -1.0, 0.0]),
            Vertex::new([0.0, 1.0, 0.0]),
        ]),
        indices: Arc::new(vec![0, 1, 2]),
        material: Material {
            base_color: [0.0, 0.0, 0.0, 1.0],
            emissive: color,
            casts_shadows: false,
            ..Default::default()
        },
        skin: None,
        morph_targets: Arc::new(Vec::new()),
        morph_weights: Vec::new(),
    })
}

fn scene() -> RenderTree {
    let triangle = mesh([1.0, 1.0, 0.0]);
    let mut tree = RenderTree::from_element(
        &div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .id("group")
                .absolute()
                .left(20.0)
                .top(10.0)
                .w(120.0)
                .h(100.0)
                .bg(Color::RED)
                .child(
                    canvas(move |ctx, _| {
                        ctx.set_camera(&Camera::orthographic(
                            Vec3::new(0.0, 0.0, 3.0),
                            Vec3::new(0.0, 0.0, 0.0),
                            1.5,
                        ));
                        ctx.set_3d_viewport_bounds(80.0, 80.0);
                        ctx.draw_mesh_data(triangle.clone(), Mat4::IDENTITY);
                        ctx.fill_rect(
                            Rect::new(20.0, 20.0, 20.0, 20.0),
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
    tree.update_render_props(tree.query_by_id("group").unwrap(), |p| {
        p.layer_effects = vec![blinc_core::LayerEffect::brightness(1.0)]
    });
    tree
}

#[test]
#[ignore] // Requires GPU
fn mesh_group_opacity_and_later_canvas_draws_share_paint_order() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let mut tree = scene();
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let opaque = frame(&mut context, &tree, &state, 0);
    let color = opaque.get_pixel(65, 50).0;
    assert!(
        color[0] > 200 && color[1] > 200 && color[0] > color[2] + 20,
        "mesh geometry missing: {color:?}"
    );
    tree.update_render_props(tree.query_by_id("group").unwrap(), |p| p.opacity = 0.5);
    let page = frame(&mut context, &tree, &state, 0);
    super::opacity_group_tests::pixel(
        &page,
        (65, 50),
        [
            color[0] / 2,
            color[1] / 2,
            ((color[2] as u16 + 255) / 2) as u8,
            255,
        ],
    );
    super::opacity_group_tests::pixel(&page, (55, 40), [0, 128, 127, 255]);
}

fn faded(color: [u8; 4], opacity: f32, brightness: f32) -> [u8; 4] {
    [
        (color[0] as f32 * opacity * brightness).round() as u8,
        (color[1] as f32 * opacity * brightness).round() as u8,
        (color[2] as f32 * opacity * brightness + 255.0 * (1.0 - opacity)).round() as u8,
        255,
    ]
}

#[test]
#[ignore] // Requires GPU
fn mesh_group_replay_foreground_effects_dpi_and_opacity_endpoints() {
    for samples in [1, 4] {
        let mut context = super::main_text_clip_tests::context(true, samples);
        let state = RenderState::new(Arc::new(Mutex::new(
            blinc_animation::AnimationScheduler::new(),
        )));
        for mode in 0..3 {
            for foreground in [false, true] {
                let mut tree = scene();
                let group = tree.query_by_id("group").unwrap();
                if foreground {
                    tree.update_render_props(group, |p| p.layer = RenderLayer::Foreground);
                }
                for scale in [1, 2] {
                    tree.set_scale_factor(scale as f32);
                    tree.update_render_props(group, |p| {
                        p.opacity = 1.0;
                        p.layer_effects = vec![blinc_core::LayerEffect::brightness(1.0)];
                    });
                    context.invalidate_render_cache();
                    let reference = frame(&mut context, &tree, &state, mode);
                    let source = reference.get_pixel(65 * scale, 50 * scale).0;
                    assert!(
                        source[0] > 200 && source[1] > 200,
                        "mesh missing: {source:?}"
                    );
                    for (opacity, brightness) in
                        [(0.5, 1.0), (0.0, 1.0), (1.0, 1.0), (0.75, 0.5), (0.5, 1.0)]
                    {
                        tree.update_render_props(group, |p| {
                            p.opacity = opacity;
                            p.layer_effects = vec![blinc_core::LayerEffect::brightness(brightness)];
                        });
                        context.invalidate_render_cache();
                        for _ in 0..2 {
                            let page = frame(&mut context, &tree, &state, mode);
                            super::opacity_group_tests::pixel(
                                &page,
                                (65 * scale, 50 * scale),
                                faded(source, opacity, brightness),
                            );
                            super::opacity_group_tests::pixel(
                                &page,
                                (55 * scale, 40 * scale),
                                faded([0, 255, 0, 255], opacity, brightness),
                            );
                            super::opacity_group_tests::pixel(
                                &page,
                                (25 * scale, 100 * scale),
                                faded([255, 0, 0, 255], opacity, brightness),
                            );
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn camera() -> Camera {
    Camera::orthographic(Vec3::new(0.0, 0.0, 3.0), Vec3::new(0.0, 0.0, 0.0), 1.5)
}
fn raw_frame(context: &mut RenderContext, batch: &PrimitiveBatch) -> image::RgbaImage {
    let (texture, view) = target(context, 1);
    context.renderer.resize(160, 120);
    context
        .renderer
        .render_with_clear(&view, batch, [0.0, 0.0, 1.0, 1.0]);
    crate::tests::read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120)
}
fn mesh_scene(draws: &[(Arc<MeshData>, Mat4)], alpha: f32) -> PrimitiveBatch {
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.set_camera(&camera());
    paint.push_layer(blinc_core::LayerConfig::new().opacity(alpha));
    for (mesh, transform) in draws {
        paint.draw_mesh_data(mesh.clone(), *transform);
    }
    paint.pop_layer();
    assert!(paint.take_pending_meshes().is_empty());
    paint.take_batch()
}

#[test]
#[ignore] // Requires GPU
fn mesh_group_consecutive_draws_keep_shared_depth_shadows_and_transparency() {
    use blinc_core::draw::AlphaMode;
    let mut context = super::main_text_clip_tests::context(true, 1);
    let mut front = mesh([0.0, 1.0, 0.0]);
    Arc::make_mut(&mut front).material.casts_shadows = true;
    let mut back = mesh([1.0, 0.0, 0.0]);
    Arc::make_mut(&mut back).material.casts_shadows = true;
    let near = Mat4::translation(0.0, 0.0, 0.5);
    let reference = raw_frame(&mut context, &mesh_scene(&[(front.clone(), near)], 1.0));
    let expected = reference.get_pixel(80, 60).0;
    for draws in [
        vec![(front.clone(), near), (back.clone(), Mat4::IDENTITY)],
        vec![(back.clone(), Mat4::IDENTITY), (front.clone(), near)],
    ] {
        let page = raw_frame(&mut context, &mesh_scene(&draws, 1.0));
        super::opacity_group_tests::pixel(&page, (80, 60), expected);
    }
    let mut blend = front.clone();
    let material = &mut Arc::make_mut(&mut blend).material;
    material.alpha_mode = AlphaMode::Blend;
    material.base_color[3] = 0.4;
    let first = raw_frame(
        &mut context,
        &mesh_scene(
            &[(blend.clone(), near), (back.clone(), Mat4::IDENTITY)],
            1.0,
        ),
    );
    let second = raw_frame(
        &mut context,
        &mesh_scene(&[(back, Mat4::IDENTITY), (blend, near)], 1.0),
    );
    super::opacity_group_tests::pixel(&second, (80, 60), first.get_pixel(80, 60).0);
    assert_ne!(
        first.get_pixel(80, 60).0,
        expected,
        "transparent scene did not include the back mesh"
    );
}

#[test]
#[ignore] // Requires GPU
fn mesh_group_paths_and_explicit_opacity_cover_the_scene_result() {
    let mut context = super::main_text_clip_tests::context(false, 4);
    let triangle = mesh([1.0, 1.0, 0.0]);
    let reference = raw_frame(
        &mut context,
        &mesh_scene(&[(triangle.clone(), Mat4::IDENTITY)], 1.0),
    );
    let color = reference.get_pixel(65, 50).0;
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.set_camera(&camera());
    paint.push_layer(blinc_core::LayerConfig::new().opacity(0.5));
    paint.fill_rect(
        Rect::new(0.0, 0.0, 160.0, 120.0),
        0.0.into(),
        Color::RED.into(),
    );
    paint.push_clip(blinc_core::ClipShape::path(
        blinc_core::Path::new()
            .move_to(10.0, 10.0)
            .line_to(150.0, 10.0)
            .line_to(10.0, 110.0)
            .close(),
    ));
    paint.push_opacity(0.5);
    paint.draw_mesh_data(triangle, Mat4::IDENTITY);
    paint.pop_opacity();
    paint.pop_clip();
    paint.pop_layer();
    for _ in 0..2 {
        let page = raw_frame(&mut context, &paint.batch().clone());
        super::opacity_group_tests::pixel(
            &page,
            (65, 50),
            [
                (63.75 + color[0] as f32 * 0.25).round() as u8,
                (color[1] as f32 * 0.25).round() as u8,
                (127.5 + color[2] as f32 * 0.25).round() as u8,
                255,
            ],
        );
        super::opacity_group_tests::pixel(&page, (130, 95), [128, 0, 127, 255]);
    }
}

#[test]
#[ignore] // Requires GPU
fn mesh_group_rgba_shapes_and_callbacks_split_mesh_scenes_in_paint_order() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let front = mesh([0.0, 1.0, 0.0]);
    let back = mesh([1.0, 0.0, 0.0]);
    let reference = raw_frame(
        &mut context,
        &mesh_scene(&[(back.clone(), Mat4::IDENTITY)], 1.0),
    );
    let expected = reference.get_pixel(80, 60).0;
    for barrier in 0..3 {
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        paint.set_camera(&camera());
        paint.push_layer(blinc_core::LayerConfig::new().opacity(1.0));
        paint.draw_mesh_data(front.clone(), Mat4::translation(0.0, 0.0, 0.5));
        let rect = Rect::new(40.0, 30.0, 80.0, 60.0);
        match barrier {
            0 => paint.fill_rect(rect, 0.0.into(), Color::CYAN.into()),
            1 => paint.draw_rgba_pixels(&[0, 255, 255, 255], 1, 1, rect),
            _ => paint.run_gpu_pass(&super::release_boundary_tests::marker_pass(), Some(rect)),
        }
        paint.draw_mesh_data(back.clone(), Mat4::IDENTITY);
        paint.pop_layer();
        let batch = paint.take_batch();
        let page = raw_frame(&mut context, &batch);
        super::opacity_group_tests::pixel(&page, (80, 60), expected);
    }
}

#[test]
#[ignore] // Requires GPU
fn mesh_group_independent_scenes_and_empty_draws_preserve_tonemap_boundaries() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let triangle = mesh([1.0, 1.0, 0.0]);
    let mut empty = triangle.clone();
    Arc::make_mut(&mut empty).indices = Arc::new(Vec::new());
    let independent = |empty_at_ends: bool| {
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        paint.set_camera(&camera());
        paint.set_3d_viewport_bounds(80.0, 80.0);
        if empty_at_ends {
            paint.draw_mesh_data(empty.clone(), Mat4::IDENTITY);
        }
        paint.draw_mesh_data(triangle.clone(), Mat4::IDENTITY);
        if empty_at_ends {
            paint.draw_mesh_data(empty.clone(), Mat4::IDENTITY);
        }
        paint.take_pending_meshes()
    };
    let mut pages = Vec::new();
    for pending in [independent(false), independent(true)] {
        let (texture, view) = target(&context, 1);
        context.renderer.resize(160, 120);
        context.renderer.clear_target(&view, wgpu::Color::BLUE);
        context
            .renderer
            .render_pending_meshes(&view, 160, 120, &pending);
        pages.push(crate::tests::read_rendered_pixels(
            context.device(),
            context.queue(),
            &texture,
            160,
            120,
        ));
    }
    let expected = pages[0].get_pixel(40, 40).0;
    super::opacity_group_tests::pixel(&pages[1], (40, 40), expected);
    let mut pending = independent(false);
    let mut right = pending[0].clone();
    right.viewport = Some([80.0, 0.0, 80.0, 80.0]);
    pending.push(right);
    let (texture, view) = target(&context, 1);
    context.renderer.clear_target(&view, wgpu::Color::BLUE);
    context
        .renderer
        .render_pending_meshes(&view, 160, 120, &pending);
    let page =
        crate::tests::read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    super::opacity_group_tests::pixel(&page, (40, 40), expected);
    super::opacity_group_tests::pixel(&page, (120, 40), expected);
    super::opacity_group_tests::pixel(&page, (40, 100), [0, 0, 255, 255]);
}

#[test]
#[ignore] // Requires GPU
fn mesh_group_live_bindings_recover_from_zero_without_cache_invalidation() {
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    for mode in [1, 2] {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let opacity = Arc::new(Mutex::new(AnimatedValue::with_default(
            scheduler.lock().unwrap().handle(),
            1.0,
        )));
        let triangle = mesh([1.0, 1.0, 0.0]);
        let mut tree = RenderTree::from_element(
            &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                div()
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .child(
                        canvas(move |ctx, _| {
                            ctx.set_camera(&camera());
                            ctx.draw_mesh_data(triangle.clone(), Mat4::IDENTITY);
                        })
                        .absolute()
                        .w(160.0)
                        .h(120.0),
                    )
                    .motion()
                    .opacity(opacity.clone()),
            ),
        );
        tree.compute_layout(160.0, 120.0);
        let state = RenderState::new(scheduler);
        let reference = frame(&mut context, &tree, &state, mode).get_pixel(80, 60).0;
        assert!(reference[0] > 200 && reference[1] > 200);
        for alpha in [0.5, 0.0, 1.0, 0.75, 0.5] {
            opacity.lock().unwrap().set_immediate(alpha);
            for _ in 0..2 {
                let page = frame(&mut context, &tree, &state, mode);
                super::opacity_group_tests::pixel(&page, (80, 60), faded(reference, alpha, 1.0));
            }
        }
    }
}

struct SceneCounter(Arc<std::sync::atomic::AtomicUsize>);
impl blinc_gpu::custom_pass::CustomRenderPass for SceneCounter {
    fn label(&self) -> &str {
        "Mesh scene count"
    }
    fn stage(&self) -> blinc_gpu::custom_pass::RenderStage {
        blinc_gpu::custom_pass::RenderStage::Scene3D
    }
    fn initialize(&mut self, _: &wgpu::Device, _: &wgpu::Queue, _: wgpu::TextureFormat) {}
    fn render(&mut self, context: &blinc_gpu::custom_pass::RenderPassContext) {
        assert_eq!(
            (context.viewport_width, context.viewport_height),
            (160, 120)
        );
        assert!(context.view_proj.is_some() && context.camera_pos.is_some());
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

#[test]
#[ignore] // Requires GPU
fn mesh_group_glass_backdrops_and_multiple_meshes_execute_scene_hooks_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut context = super::main_text_clip_tests::context(true, 1);
    let calls = Arc::new(AtomicUsize::new(0));
    context
        .renderer
        .register_custom_pass(Box::new(SceneCounter(calls.clone())));
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let front = mesh([0.0, 1.0, 0.0]);
    let back = mesh([1.0, 0.0, 0.0]);
    let mut tree = RenderTree::from_element(
        &div()
            .w(160.0)
            .h(120.0)
            .bg(Color::BLUE)
            .child(
                div().opacity(0.5).absolute().w(100.0).h(100.0).child(
                    canvas(move |ctx, _| {
                        ctx.set_camera(&camera());
                        ctx.set_3d_viewport_bounds(80.0, 80.0);
                        ctx.draw_mesh_data(front.clone(), Mat4::translation(0.0, 0.0, 0.5));
                        ctx.draw_mesh_data(back.clone(), Mat4::IDENTITY);
                    })
                    .absolute()
                    .w(100.0)
                    .h(100.0),
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
    for mode in 0..3 {
        context.invalidate_render_cache();
        for _ in 0..2 {
            let before = calls.load(Ordering::Relaxed);
            let page = frame(&mut context, &tree, &state, mode);
            assert!(page.get_pixel(40, 40).0[1] > 50);
            assert_eq!(
                calls.load(Ordering::Relaxed) - before,
                1,
                "duplicate scene, mode={mode}"
            );
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn mesh_group_named_crops_keep_camera_frame_and_scene_opacity() {
    use blinc_core::{LayerConfig, LayerId, Point, Size, Transform};
    let mut context = super::main_text_clip_tests::context(true, 4);
    let triangle = mesh([1.0, 1.0, 0.0]);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.set_camera(&camera());
    paint.push_layer(
        LayerConfig::new()
            .id(LayerId(99))
            .position(Point::new(20.0, 10.0))
            .size(Size::new(80.0, 80.0))
            .opacity(0.5),
    );
    paint.push_transform(Transform::translate(20.0, 10.0));
    paint.set_3d_viewport_bounds(80.0, 80.0);
    paint.draw_mesh_data(triangle, Mat4::IDENTITY);
    paint.pop_transform();
    paint.pop_layer();
    paint.sample_layer(
        LayerId(99),
        Rect::new(20.0, 20.0, 40.0, 40.0),
        Rect::new(100.0, 40.0, 40.0, 40.0),
    );
    let batch = paint.take_batch();
    for _ in 0..2 {
        let page = raw_frame(&mut context, &batch);
        let source = page.get_pixel(60, 50).0;
        assert!(source[0] > 80 && source[0] < 150);
        super::opacity_group_tests::pixel(&page, (120, 60), source);
    }
}

#[test]
#[ignore] // Requires GPU
fn mesh_group_perspective_camera_lights_and_environment_match_direct_output() {
    use blinc_core::{CubemapData, Light};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut context = super::main_text_clip_tests::context(true, 1);
    let calls = Arc::new(AtomicUsize::new(0));
    context
        .renderer
        .register_custom_pass(Box::new(SceneCounter(calls.clone())));
    let triangle = mesh([1.0, 1.0, 0.0]);
    let perspective = Camera::perspective(
        Vec3::new(0.0, 0.0, 3.0),
        Vec3::new(0.0, 0.0, 0.0),
        std::f32::consts::FRAC_PI_3,
    );
    let environment = Arc::new(CubemapData {
        faces: vec![vec![0, 0x38, 0, 0x38, 0, 0x38, 0, 0x3c]; 6],
        size: 1,
        mip_count: 1,
    });
    let light = Light::Directional {
        direction: Vec3::new(0.0, -1.0, -1.0),
        color: Color::WHITE,
        intensity: 0.8,
        cast_shadows: true,
    };
    let mut independent = GpuPaintContext::new(160.0, 120.0);
    independent.set_camera(&perspective);
    independent.add_light(light.clone());
    independent.set_environment_cubemap(environment.clone());
    independent.draw_mesh_data(triangle.clone(), Mat4::translation(0.1, 0.0, 0.0));
    let pending = independent.take_pending_meshes();
    let (texture, view) = target(&context, 1);
    context.renderer.resize(160, 120);
    context.renderer.clear_target(&view, wgpu::Color::BLUE);
    context
        .renderer
        .render_pending_meshes(&view, 160, 120, &pending);
    let expected =
        crate::tests::read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120)
            .get_pixel(80, 60)
            .0;
    assert!(
        expected[0] > 100 && expected[1] > 100,
        "perspective mesh missing: {expected:?}"
    );
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.set_camera(&camera());
    paint.add_light(light);
    paint.set_environment_cubemap(environment);
    paint.push_layer(blinc_core::LayerConfig::new().opacity(0.5));
    paint.draw_mesh_data(mesh([0.0, 1.0, 0.0]), Mat4::IDENTITY);
    paint.set_camera(&perspective);
    paint.draw_mesh_data(triangle, Mat4::translation(0.1, 0.0, 0.0));
    paint.pop_layer();
    let before = calls.load(Ordering::Relaxed);
    let page = raw_frame(&mut context, &paint.take_batch());
    assert_eq!(
        calls.load(Ordering::Relaxed) - before,
        2,
        "camera changes must separate scene runs"
    );
    super::opacity_group_tests::pixel(&page, (80, 60), faded(expected, 0.5, 1.0));
}

#[test]
#[ignore] // Requires GPU
fn a_mesh_draws_the_same_after_its_targets_are_released() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let tree = scene();
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let before = frame(&mut context, &tree, &state, 0);
    let drawn = before.get_pixel(65, 50).0;
    assert!(
        drawn[0] > 200 && drawn[1] > 200,
        "the mesh was not drawn: {drawn:?}"
    );
    // The release drops the mesh pass's window-sized targets; the next frame makes them again.
    context.release_idle_memory();
    let after = frame(&mut context, &tree, &state, 0);
    assert_eq!(before.get_pixel(65, 50), after.get_pixel(65, 50));
}
