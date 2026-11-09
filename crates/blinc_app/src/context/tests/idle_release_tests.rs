//! Releasing the between-frame GPU memory while idle: the static layer goes, and the next frame,
//! fast paint or not, draws the scene from scratch.
use super::*;
use crate::tests::read_rendered_pixels;

const W: u32 = 200;
const H: u32 = 120;

/// A context for a scene without text, so the test needs a GPU but no system font.
fn context() -> RenderContext {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let renderer = pollster::block_on(GpuRenderer::new(blinc_gpu::RendererConfig {
        sample_count: 1,
        ..Default::default()
    }))
    .expect("offscreen GPU required");
    let device = renderer.device_arc();
    let queue = renderer.queue_arc();
    let text = TextRenderingContext::new(device.clone(), queue.clone());
    RenderContext::new(renderer, text, device, queue, 1)
}

fn surface(c: &RenderContext) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = c.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Idle surface"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: c.texture_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    (texture, view)
}

#[test]
#[ignore] // Requires GPU
fn a_frame_after_an_idle_release_draws_the_whole_scene() {
    let mut c = context();
    let state = blinc_layout::RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let mut tree = RenderTree::from_element(
        &div()
            .w(W as f32)
            .h(H as f32)
            .bg(Color::BLUE)
            .child(div().w(50.0).h(50.0).bg(Color::RED)),
    );
    tree.compute_layout(W as f32, H as f32);
    let frame = |c: &mut RenderContext, fast: bool| {
        let (texture, view) = surface(c);
        c.render_tree_with_motion_opt(&tree, &state, W, H, &view, Some(&texture), fast)
            .expect("render");
        let page = read_rendered_pixels(c.device(), c.queue(), &texture, W, H);
        (page.get_pixel(25, 25).0, page.get_pixel(150, 80).0)
    };
    let drawn = ([255, 0, 0, 255], [0, 0, 255, 255]);
    assert_eq!(frame(&mut c, false), drawn, "the first frame");
    assert_eq!(
        frame(&mut c, true),
        drawn,
        "a fast paint from the static layer"
    );

    c.release_idle_memory();
    assert!(
        c.renderer.static_layer_view().is_none(),
        "the static layer was kept"
    );
    assert_eq!(
        frame(&mut c, true),
        drawn,
        "the fast paint after the release"
    );
    assert_eq!(frame(&mut c, true), drawn, "the fast paint after that");
}
