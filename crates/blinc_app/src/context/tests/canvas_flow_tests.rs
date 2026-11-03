//! A canvas draws its flow shader over its own bounds.
use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::read_rendered_pixels;
use blinc_animation::AnimationScheduler;

fn pixel(page: &image::RgbaImage, xy: (u32, u32), expected: [u8; 4]) {
    let actual = page.get_pixel(xy.0, xy.1).0;
    assert!(
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 3),
        "pixel {xy:?} is {actual:?}, expected {expected:?}"
    );
}

fn render(ui: &Div, width: u32, height: u32) -> image::RgbaImage {
    render_with_cursor(ui, width, height, [0.0, 0.0])
}

fn render_with_cursor(ui: &Div, width: u32, height: u32, cursor: [f32; 2]) -> image::RgbaImage {
    let mut app = BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required");
    app.set_cursor_position(cursor[0], cursor[1]);
    let mut tree = RenderTree::from_element(ui);
    tree.compute_layout(width as f32, height as f32);
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    let texture = app.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Canvas flow surface"),
        size: wgpu::Extent3d {
            width,
            height,
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
    });
    let view = texture.create_view(&Default::default());
    app.render_tree_with_motion_opt(&tree, &state, &view, None, width, height, false)
        .unwrap();
    read_rendered_pixels(app.device(), app.queue(), &texture, width, height)
}

#[test]
#[ignore] // Requires GPU
fn a_canvas_flow_stops_at_its_ancestors_clip_and_keeps_its_uv() {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    // Red from the canvas's horizontal midpoint on. The canvas is 100 wide and its parent shows
    // the first 60.
    let shader = blinc_layout::flow_shader(
        "@flow half { target: fragment; input uv: builtin(uv); \
         output color = vec4(step(0.5, uv.x), 0.0, 0.0, 1.0); }",
    )
    .expect("flow parses");
    let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
        div().w(60.0).h(60.0).overflow_clip().child(
            canvas(|_, _| {})
                .absolute()
                .left(0.0)
                .top(0.0)
                .w(100.0)
                .h(60.0)
                .flow(shader),
        ),
    );
    let page = render(&ui, 160, 120);
    pixel(&page, (40, 30), [0, 0, 0, 255]);
    pixel(&page, (55, 30), [255, 0, 0, 255]);
    pixel(&page, (80, 30), [0, 0, 255, 255]);
    pixel(&page, (30, 90), [0, 0, 255, 255]);
}

#[test]
#[ignore] // Requires GPU
fn a_canvas_flow_covers_the_canvas_and_nothing_else() {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let mut app = BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required");
    let shader = blinc_layout::flow_shader(
        "@flow solid { target: fragment; output color = vec4(1.0, 0.0, 0.0, 1.0); }",
    )
    .expect("flow parses");
    let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
        canvas(|_, _| {})
            .absolute()
            .left(20.0)
            .top(20.0)
            .w(60.0)
            .h(60.0)
            .flow(shader),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    let texture = app.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Canvas flow surface"),
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
    });
    let view = texture.create_view(&Default::default());
    app.render_tree_with_motion_opt(&tree, &state, &view, None, 160, 120, false)
        .unwrap();
    let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
    pixel(&page, (50, 50), [255, 0, 0, 255]);
    pixel(&page, (10, 10), [0, 0, 255, 255]);
    pixel(&page, (100, 50), [0, 0, 255, 255]);
}

#[test]
#[ignore] // Requires GPU
fn a_canvas_flow_reads_the_cursor_as_its_pointer() {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    // Red where `uv` is left of the pointer, green where it is right.
    let shader = blinc_layout::flow_shader(
        "@flow follow { target: fragment; input uv: builtin(uv); input pointer: builtin(pointer); \
         node left = step(uv.x, pointer.x); \
         output color = vec4(left, 1.0 - left, 0.0, 1.0); }",
    )
    .expect("flow compiles");
    let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
        canvas(|_, _| {})
            .absolute()
            .left(20.0)
            .top(20.0)
            .w(100.0)
            .h(60.0)
            .flow(shader),
    );
    // The cursor is a quarter across the canvas.
    let page = render_with_cursor(&ui, 160, 120, [45.0, 50.0]);
    pixel(&page, (30, 50), [255, 0, 0, 255]);
    pixel(&page, (100, 50), [0, 255, 0, 255]);
    pixel(&page, (140, 50), [0, 0, 255, 255]);
}
