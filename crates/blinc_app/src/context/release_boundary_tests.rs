//! Release controls for feature output alongside collected native groups.
use super::*;
use blinc_animation::AnimationScheduler;
use blinc_gpu::custom_pass::{CustomRenderPass, GpuPass, RenderPassContext, RenderStage};
use blinc_layout::{image, svg};
use std::sync::atomic::{AtomicUsize, Ordering};

const IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFUlEQVR4nGNg+M/wH4gaGEAMBiAAAEbQBnso3jFKAAAAAElFTkSuQmCC";
#[derive(Clone, Copy, Debug)]
enum Boundary {
    Glass,
    LiquidGlass,
    ThreeD,
    FractionalThreeD,
    Overlay,
    Pass,
    Mesh,
    Legacy,
}
struct MarkerPass {
    pipeline: Option<wgpu::RenderPipeline>,
    calls: Arc<AtomicUsize>,
    viewports: Arc<Mutex<Vec<[f32; 4]>>>,
}
impl CustomRenderPass for MarkerPass {
    fn label(&self) -> &str {
        "Release boundary marker"
    }
    fn stage(&self) -> RenderStage {
        RenderStage::PostProcess
    }
    fn initialize(
        &mut self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(self.label()),
            source: wgpu::ShaderSource::Wgsl(
                r#"
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let p=array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    return vec4<f32>(p[i],0.0,1.0);
}
@fragment fn fs()->@location(0) vec4<f32> { return vec4<f32>(0.0,1.0,1.0,1.0); }
"#
                .into(),
            ),
        });
        self.pipeline = Some(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(self.label()),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            }),
        );
    }
    fn render(&mut self, ctx: &RenderPassContext) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let viewport = ctx.viewport.unwrap();
        self.viewports.lock().unwrap().push(viewport);
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(self.label()),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: ctx.target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(self.pipeline.as_ref().unwrap());
            pass.set_viewport(viewport[0], viewport[1], viewport[2], viewport[3], 0.0, 1.0);
            pass.set_scissor_rect(
                viewport[0] as u32,
                viewport[1] as u32,
                viewport[2] as u32,
                viewport[3] as u32,
            );
            pass.draw(0..3, 0..1);
        }
        ctx.queue.submit(Some(encoder.finish()));
    }
}
fn scene(
    kind: Boundary,
    pass: GpuPass,
    callbacks: Arc<AtomicUsize>,
    collected: bool,
) -> RenderTree {
    let group=div().id("capture").absolute().left(10.0).top(20.0).w(120.0).h(130.0).bg(Color::RED)
        .child(image(IMAGE).size(100.0,100.0).fade_in(std::time::Duration::ZERO))
        .child(div().absolute().left(5.0).top(25.0).child(text("M").size(24.0).color(Color::WHITE)))
        .child(div().absolute().left(75.0).top(80.0).child(svg(r##"<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15"><rect width="15" height="15" fill="#ffff00"/></svg>"##).size(15.0,15.0)));
    let group = if collected {
        group
    } else {
        div()
            .id("capture")
            .absolute()
            .left(10.0)
            .top(20.0)
            .w(120.0)
            .h(130.0)
            .bg(Color::RED)
    };
    let marker = div()
        .absolute()
        .left(5.0)
        .top(5.0)
        .w(15.0)
        .h(15.0)
        .bg(Color::GREEN);
    let owner=div().id("feature").absolute().left(160.0).top(20.0).w(65.0).h(110.0).bg(Color::rgba(0.8,0.2,0.1,1.0))
        .child(marker).child(div().absolute().top(25.0).child(text("X").size(24.0).color(Color::WHITE)))
        .child(div().absolute().left(30.0).top(5.0).child(image(IMAGE).size(25.0,25.0).fade_in(std::time::Duration::ZERO)))
        .child(div().absolute().left(35.0).top(35.0).child(svg(r##"<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15"><rect width="15" height="15" fill="#ff00ff"/></svg>"##).size(15.0,15.0)));
    let owner = if matches!(kind, Boundary::FractionalThreeD) {
        owner.left(160.25).top(20.5).w(64.5).h(109.25)
    } else {
        owner
    };
    let owner = match kind {
        Boundary::Glass | Boundary::LiquidGlass => owner.effect(
            GlassMaterial::new()
                .with_simple(matches!(kind, Boundary::Glass))
                .blur(0.0)
                .border(0.0)
                .tint(Color::rgba(0.1, 0.8, 0.3, 0.7)),
        ),
        Boundary::Overlay => owner.overlay_root(),
        Boundary::Pass => owner.child(
            canvas(move |ctx, _| {
                callbacks.fetch_add(1, Ordering::Relaxed);
                ctx.run_gpu_pass(&pass, Some(Rect::new(10.0, 60.0, 30.0, 20.0)));
            })
            .absolute()
            .w(65.0)
            .h(110.0),
        ),
        Boundary::Mesh => {
            use blinc_core::draw::{Material, MeshData, Vertex};
            let mesh = Arc::new(MeshData {
                vertices: Arc::new(vec![
                    Vertex::new([-1.0, -1.0, 0.0]),
                    Vertex::new([1.0, -1.0, 0.0]),
                    Vertex::new([0.0, 1.0, 0.0]),
                ]),
                indices: Arc::new(vec![0, 1, 2]),
                material: Material {
                    emissive: [1.0, 1.0, 0.0],
                    casts_shadows: false,
                    ..Default::default()
                },
                skin: None,
                morph_targets: Arc::new(Vec::new()),
                morph_weights: Vec::new(),
            });
            owner.child(
                canvas(move |ctx, _| {
                    callbacks.fetch_add(1, Ordering::Relaxed);
                    ctx.set_3d_viewport_bounds(65.0, 110.0);
                    ctx.draw_mesh_data(mesh.clone(), blinc_core::Mat4::IDENTITY);
                })
                .absolute()
                .w(65.0)
                .h(110.0),
            )
        }
        _ => owner,
    };
    let ui = div()
        .w(240.0)
        .h(180.0)
        .bg(Color::BLUE)
        .child(group)
        .child(owner);
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(240.0, 180.0);
    if matches!(kind, Boundary::ThreeD | Boundary::FractionalThreeD) {
        tree.update_render_props(tree.query_by_id("feature").unwrap(), |p| {
            p.rotate_y = Some(25.0);
            p.perspective = Some(500.0);
        });
    }
    tree
}
fn frame(
    context: &mut RenderContext,
    tree: &RenderTree,
    state: &RenderState,
    kind: Boundary,
    compositor: bool,
) -> ::image::RgbaImage {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Release boundary"),
        size: wgpu::Extent3d {
            width: 240,
            height: 180,
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
    if matches!(kind, Boundary::Legacy) {
        context.render_tree(tree, 240, 180, &view).unwrap();
    } else {
        context
            .render_tree_with_motion_opt(
                tree,
                state,
                240,
                180,
                &view,
                compositor.then_some(&texture),
                true,
            )
            .unwrap();
    }
    crate::tests::read_rendered_pixels(context.device(), context.queue(), &texture, 240, 180)
}
fn exercise(kind: Boundary) {
    let mut failures = Vec::new();
    for compositor in [false, true] {
        if compositor && matches!(kind, Boundary::Legacy) {
            continue;
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let callbacks = Arc::new(AtomicUsize::new(0));
        let viewports = Arc::new(Mutex::new(Vec::new()));
        let pass = GpuPass::new(MarkerPass {
            pipeline: None,
            calls: calls.clone(),
            viewports: viewports.clone(),
        });
        let mut tree = scene(kind, pass.clone(), callbacks.clone(), true);
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        let mut context = super::main_text_clip_tests::context(true, 1);
        let native_reference = if matches!(kind, Boundary::ThreeD | Boundary::FractionalThreeD) {
            let control = scene(kind, pass.clone(), callbacks.clone(), false);
            let mut reference_context = super::main_text_clip_tests::context(true, 1);
            let reference = frame(&mut reference_context, &control, &state, kind, compositor);
            let pooled = context.renderer.acquire_layer_texture((333, 199), false);
            context.renderer.release_layer_texture(pooled);
            Some(reference)
        } else {
            None
        };
        let mut previous_feature: Option<Vec<[u8; 4]>> = native_reference.map(|page| {
            (0..180)
                .flat_map(|y| (150..240).map(move |x| (x, y)))
                .map(|(x, y)| page.get_pixel(x, y).0)
                .collect()
        });
        for (stage, alpha) in [0.5_f32, 0.0, 1.0, 0.75, 0.5].into_iter().enumerate() {
            tree.update_render_props(tree.query_by_id("capture").unwrap(), |p| p.opacity = alpha);
            context.invalidate_render_cache();
            for replay in 0..2 {
                let before = calls.load(Ordering::Relaxed);
                let before_callback = callbacks.load(Ordering::Relaxed);
                let page = frame(&mut context, &tree, &state, kind, compositor);
                if matches!(kind, Boundary::Pass | Boundary::Mesh) {
                    let delta = callbacks.load(Ordering::Relaxed) - before_callback;
                    if delta != 1 {
                        failures.push(format!(
                            "comp={compositor} stage={stage} replay={replay}: {delta} canvas calls"
                        ));
                    }
                }
                if matches!(kind, Boundary::Pass) {
                    let delta = calls.load(Ordering::Relaxed) - before;
                    if delta != 1 {
                        failures.push(format!("comp={compositor} stage={stage} replay={replay}: {delta} pass dispatches"));
                    }
                    if viewports.lock().unwrap().last().copied() != Some([170.0, 80.0, 30.0, 20.0])
                    {
                        failures.push("pass viewport mismatch".into());
                    }
                    if page.get_pixel(180, 90).0 != [0, 255, 255, 255] {
                        failures.push(format!("pass pixel {:?}", page.get_pixel(180, 90)));
                    }
                }
                let feature: Vec<_> = (0..180)
                    .flat_map(|y| (150..240).map(move |x| (x, y)))
                    .map(|(x, y)| page.get_pixel(x, y).0)
                    .collect();
                if feature.iter().filter(|p| p[0] > 10 || p[1] > 10).count() <= 200 {
                    failures.push("feature disappeared".into());
                }
                // Liquid refraction can sample the neighboring group; cached replay must agree.
                if let Some(previous) = &previous_feature
                    && (!matches!(kind, Boundary::LiquidGlass) || replay > 0)
                {
                    if let Some((index, (actual, prior))) = feature
                        .iter()
                        .zip(previous)
                        .enumerate()
                        .find(|(_, (a, b))| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 2))
                    {
                        failures.push(format!("comp={compositor} stage={stage} replay={replay}: feature ({},{}) {actual:?} != {prior:?}",150+index%90,index/90));
                    }
                }
                previous_feature = Some(feature);
                if let Some(dir) = std::env::var_os("BLINC_BOUNDARY_SNAPSHOTS") {
                    let dir = std::path::PathBuf::from(dir);
                    std::fs::create_dir_all(&dir).unwrap();
                    page.save(dir.join(format!("{kind:?}-{compositor}-{stage}-{replay}.png")))
                        .unwrap();
                }
            }
        }
    }
    assert!(failures.is_empty(), "{kind:?}: {failures:?}");
}
#[test]
#[ignore] // Requires GPU
fn release_boundary_glass_with_collected_group() {
    exercise(Boundary::Glass);
}
#[test]
#[ignore] // Requires GPU
fn release_boundary_3d_with_collected_group() {
    exercise(Boundary::ThreeD);
}
#[test]
#[ignore] // Requires GPU
fn release_boundary_fractional_3d_with_collected_group() {
    exercise(Boundary::FractionalThreeD);
}
#[test]
#[ignore] // Requires GPU
fn release_boundary_liquid_glass_with_collected_group() {
    exercise(Boundary::LiquidGlass);
}
#[test]
#[ignore] // Requires GPU
fn release_boundary_overlay_with_collected_group() {
    exercise(Boundary::Overlay);
}
#[test]
#[ignore] // Requires GPU
fn release_boundary_custom_pass_with_collected_group() {
    exercise(Boundary::Pass);
}
#[test]
#[ignore] // Requires GPU
fn release_boundary_mesh_with_collected_group() {
    exercise(Boundary::Mesh);
}
#[test]
#[ignore] // Requires GPU
fn release_boundary_legacy_collection() {
    exercise(Boundary::Legacy);
}
