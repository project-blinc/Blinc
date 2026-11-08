//! The surface texture is the size every cached copy into it must follow.
//!
//! During a window resize the size the app hands to the renderer (taken from the window) can be a frame
//! ahead of the swapchain texture, which still has the size of the last `Resized` event. The static layer
//! used to be sized from the former, so copying it into the narrower surface was a wgpu validation error
//! ("Copy of X 0..1278 would end up overrunning the bounds of the Destination texture of X size 1276").
use super::*;
use crate::tests::read_rendered_pixels;

const H: u32 = 180;

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

fn surface(c: &RenderContext, width: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = c.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Resize surface"),
        size: wgpu::Extent3d {
            width,
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
fn compositor_follows_a_surface_narrower_than_the_size_it_was_given() {
    let mut c = context();
    let state = blinc_layout::RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    let mut tree = RenderTree::from_element(&div().w(240.0).h(H as f32).bg(Color::BLUE));
    tree.compute_layout(240.0, H as f32);
    // First frame: the surface has the full size and fills the static layer.
    // Second frame: the surface is two pixels narrower while the caller still says 240.
    for surface_width in [240, 238] {
        let (texture, view) = surface(&c, surface_width);
        c.render_tree_with_motion_opt(&tree, &state, 240, H, &view, Some(&texture), false)
            .expect("render");
        let page = read_rendered_pixels(c.device(), c.queue(), &texture, surface_width, H);
        assert_eq!(page.get_pixel(surface_width / 2, H / 2).0, [0, 0, 255, 255]);
    }
}
