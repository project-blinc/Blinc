//! Visual tests for blinc_app API
//!
//! Tests render to PNG files in test_output/blinc_app/ for visual verification.

use crate::app::BlincConfig;
use crate::prelude::*;
use image::{ImageBuffer, Rgba, RgbaImage};
use std::path::Path;

/// Test output directory
const OUTPUT_DIR: &str = "test_output/blinc_app";

/// Create test app with MSAA enabled for smooth SVG edges
fn create_test_app() -> BlincApp {
    BlincApp::with_config(BlincConfig {
        sample_count: 4, // 4x MSAA for smooth edges
        ..Default::default()
    })
    .expect("Failed to create test app")
}

/// Create a test texture for rendering (must match renderer's format)
fn create_test_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Test Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Padded bytes per row for wgpu buffer alignment
fn padded_bytes_per_row(width: u32) -> u32 {
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    ((unpadded + align - 1) / align) * align
}

/// Save a rendered texture to PNG
fn save_to_png(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    path: &Path,
) {
    let bytes_per_row = padded_bytes_per_row(width);
    let buffer_size = (bytes_per_row * height) as u64;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Readback Buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Copy Encoder"),
    });

    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(std::iter::once(encoder.finish()));

    let buffer_slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().expect("Failed to map buffer");

    let data = buffer_slice.get_mapped_range();

    // Create image (convert BGRA to RGBA)
    let mut img: RgbaImage = ImageBuffer::new(width, height);
    for y in 0..height {
        let row_start = (y * bytes_per_row) as usize;
        let row_end = row_start + (width * 4) as usize;
        let row_data = &data[row_start..row_end];

        for x in 0..width {
            let i = (x * 4) as usize;
            // BGRA -> RGBA
            img.put_pixel(
                x,
                y,
                Rgba([row_data[i + 2], row_data[i + 1], row_data[i], row_data[i + 3]]),
            );
        }
    }

    drop(data);
    buffer.unmap();

    // Ensure output directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    img.save(path).expect("Failed to save PNG");
}

/// Render a UI element and save to PNG
fn render_to_png(app: &mut BlincApp, name: &str, ui: &impl ElementBuilder, width: u32, height: u32) {
    let (texture, view) = create_test_texture(app.device(), width, height);
    app.render(ui, &view, width as f32, height as f32)
        .expect("Render failed");

    let path = Path::new(OUTPUT_DIR).join(format!("{}.png", name));
    save_to_png(app.device(), app.queue(), &texture, width, height, &path);
    println!("Saved: {:?}", path);
}

#[test]
fn test_simple_red_box() {
    let mut app = create_test_app();
    let ui = div().w(200.0).h(200.0).bg(Color::RED);
    render_to_png(&mut app, "simple_red_box", &ui, 200, 200);
}

#[test]
fn test_nested_boxes() {
    let mut app = create_test_app();

    let ui = div()
        .w(400.0)
        .h(300.0)
        .flex_col()
        .gap(4.0)
        .p(4.0)
        .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
        .child(div().h(80.0).w_full().rounded(8.0).bg(Color::RED))
        .child(div().flex_grow().w_full().rounded(8.0).bg(Color::GREEN))
        .child(div().h(80.0).w_full().rounded(8.0).bg(Color::BLUE));

    render_to_png(&mut app, "nested_boxes", &ui, 400, 300);
}

#[test]
fn test_text_element() {
    let mut app = create_test_app();

    let ui = div()
        .w(400.0)
        .h(200.0)
        .flex_col()
        .items_center()
        .justify_center()
        .bg(Color::WHITE)
        .child(text("Hello Blinc!").size(32.0).color(Color::BLACK));

    render_to_png(&mut app, "text_element", &ui, 400, 200);
}

#[test]
fn test_svg_icon() {
    let mut app = create_test_app();

    let svg_source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#3B82F6"/></svg>"##;

    let ui = div()
        .w(200.0)
        .h(200.0)
        .flex_col()
        .items_center()
        .justify_center()
        .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
        .child(svg(svg_source).size(100.0, 100.0));

    render_to_png(&mut app, "svg_icon", &ui, 200, 200);
}

#[test]
fn test_glass_panel() {
    let mut app = create_test_app();

    let ui = div()
        .w(400.0)
        .h(300.0)
        .bg(Color::rgba(0.2, 0.1, 0.4, 1.0))
        // Background blob
        .child(
            div()
                .absolute()
                .w(150.0)
                .h(150.0)
                .rounded(75.0)
                .bg(Color::rgba(0.95, 0.3, 0.5, 1.0)),
        )
        // Another background blob
        .child(
            div()
                .absolute()
                .mt(4.0)
                .ml(50.0)
                .w(120.0)
                .h(120.0)
                .rounded(60.0)
                .bg(Color::rgba(0.3, 0.8, 0.6, 1.0)),
        )
        // Glass card
        .child(
            div()
                .w(280.0)
                .h(180.0)
                .m(4.0)
                .rounded(20.0)
                .p(4.0)
                .flex_col()
                .gap(2.0)
                .effect(
                    GlassMaterial::new()
                        .blur(25.0)
                        .tint_rgba(0.95, 0.95, 0.98, 0.5)
                        .border(1.0),
                )
                .child(
                    div()
                        .w(200.0)
                        .h(20.0)
                        .rounded(4.0)
                        .bg(Color::rgba(1.0, 1.0, 1.0, 0.8)),
                )
                .child(
                    div()
                        .w(140.0)
                        .h(14.0)
                        .rounded(3.0)
                        .bg(Color::rgba(1.0, 1.0, 1.0, 0.5)),
                )
                .child(
                    div()
                        .flex_grow()
                        .w_full()
                        .rounded(8.0)
                        .bg(Color::rgba(1.0, 1.0, 1.0, 0.15)),
                ),
        );

    render_to_png(&mut app, "glass_panel", &ui, 400, 300);
}

#[test]
fn test_flex_row_justify() {
    let mut app = create_test_app();

    let ui = div()
        .w(400.0)
        .h(100.0)
        .flex_row()
        .justify_between()
        .items_center()
        .p(4.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .child(div().w(60.0).h(60.0).rounded(8.0).bg(Color::RED))
        .child(div().w(60.0).h(60.0).rounded(8.0).bg(Color::GREEN))
        .child(div().w(60.0).h(60.0).rounded(8.0).bg(Color::BLUE));

    render_to_png(&mut app, "flex_row_justify", &ui, 400, 100);
}

#[test]
fn test_card_component() {
    let mut app = create_test_app();

    let card = div()
        .w(300.0)
        .h(200.0)
        .p(4.0)
        .rounded(16.0)
        .bg(Color::WHITE)
        .flex_col()
        .gap(3.0)
        // Header row
        .child(
            div()
                .w_full()
                .h(48.0)
                .flex_row()
                .gap(3.0)
                .items_center()
                // Avatar
                .child(
                    div()
                        .w(48.0)
                        .h(48.0)
                        .rounded(24.0)
                        .bg(Color::rgba(0.3, 0.5, 0.9, 1.0)),
                )
                // Title area
                .child(
                    div()
                        .flex_grow()
                        .h(48.0)
                        .flex_col()
                        .gap(1.0)
                        .justify_center()
                        .child(
                            div()
                                .w(120.0)
                                .h(14.0)
                                .rounded(3.0)
                                .bg(Color::rgba(0.2, 0.2, 0.25, 1.0)),
                        )
                        .child(
                            div()
                                .w(80.0)
                                .h(10.0)
                                .rounded(2.0)
                                .bg(Color::rgba(0.6, 0.6, 0.65, 1.0)),
                        ),
                ),
        )
        // Content area
        .child(
            div()
                .w_full()
                .flex_grow()
                .rounded(8.0)
                .bg(Color::rgba(0.95, 0.95, 0.97, 1.0)),
        )
        // Button row
        .child(
            div()
                .w_full()
                .h(36.0)
                .flex_row()
                .justify_end()
                .gap(2.0)
                .child(
                    div()
                        .w(80.0)
                        .h(36.0)
                        .rounded(8.0)
                        .bg(Color::rgba(0.9, 0.9, 0.92, 1.0)),
                )
                .child(
                    div()
                        .w(80.0)
                        .h(36.0)
                        .rounded(8.0)
                        .bg(Color::rgba(0.3, 0.5, 0.9, 1.0)),
                ),
        );

    render_to_png(&mut app, "card_component", &card, 300, 200);
}

#[test]
fn test_music_player() {
    let mut app = create_test_app();
    let scale = 2.0;

    // SVG icons
    let rewind_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640"><path d="M236.3 107.1C247.9 96 265 92.9 279.7 99.2C294.4 105.5 304 120 304 136L304 272.3L476.3 107.2C487.9 96 505 92.9 519.7 99.2C534.4 105.5 544 120 544 136L544 504C544 520 534.4 534.5 519.7 540.8C505 547.1 487.9 544 476.3 532.9L304 367.7L304 504C304 520 294.4 534.5 279.7 540.8C265 547.1 247.9 544 236.3 532.9L44.3 348.9C36.4 341.4 32 330.9 32 320C32 309.1 36.5 298.7 44.3 291.1L236.3 107.1z" fill="white"/></svg>"#;
    let pause_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640"><path d="M176 96C149.5 96 128 117.5 128 144L128 496C128 522.5 149.5 544 176 544L240 544C266.5 544 288 522.5 288 496L288 144C288 117.5 266.5 96 240 96L176 96zM400 96C373.5 96 352 117.5 352 144L352 496C352 522.5 373.5 544 400 544L464 544C490.5 544 512 522.5 512 496L512 144C512 117.5 490.5 96 464 96L400 96z" fill="white"/></svg>"#;
    let forward_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640"><path d="M403.7 107.1C392.1 96 375 92.9 360.3 99.2C345.6 105.5 336 120 336 136L336 272.3L163.7 107.2C152.1 96 135 92.9 120.3 99.2C105.6 105.5 96 120 96 136L96 504C96 520 105.6 534.5 120.3 540.8C135 547.1 152.1 544 163.7 532.9L336 367.7L336 504C336 520 345.6 534.5 360.3 540.8C375 547.1 392.1 544 403.7 532.9L595.7 348.9C603.6 341.4 608 330.9 608 320C608 309.1 603.5 298.7 595.7 291.1L403.7 107.1z" fill="white"/></svg>"#;

    let bar_h = 7.0 * scale;
    let icon_size = 32.0 * scale;

    let ui = div()
        .w(400.0 * scale)
        .h(300.0 * scale)
        .bg(Color::rgba(0.4, 0.2, 0.6, 1.0))
        // Background blobs
        .child(
            div()
                .absolute()
                .w(200.0 * scale)
                .h(200.0 * scale)
                .rounded(100.0 * scale)
                .bg(Color::rgba(0.95, 0.3, 0.5, 1.0)),
        )
        .child(
            div()
                .absolute()
                .ml(50.0)
                .mt(30.0)
                .w(180.0 * scale)
                .h(180.0 * scale)
                .rounded(90.0 * scale)
                .bg(Color::rgba(0.2, 0.8, 0.85, 1.0)),
        )
        // Player card
        .child(
            div()
                .w(340.0 * scale)
                .h(140.0 * scale)
                .m(7.0)
                .rounded(28.0 * scale)
                .flex_col()
                .p(5.0)
                .gap(2.0)
                .effect(
                    GlassMaterial::new()
                        .blur(30.0 * scale)
                        .tint_rgba(0.12, 0.12, 0.14, 0.55)
                        .saturation(0.85)
                        .border(0.6 * scale),
                )
                // Title
                .child(
                    div()
                        .w_full()
                        .h(20.0 * scale)
                        .flex_row()
                        .justify_center()
                        .items_center()
                        .child(
                            text("Blinc UI 0.1.0")
                                .size(14.0 * scale)
                                .color(Color::rgba(1.0, 1.0, 1.0, 0.95)),
                        ),
                )
                // Progress bar
                .child(
                    div()
                        .w_full()
                        .h(bar_h + 8.0 * scale)
                        .flex_row()
                        .items_center()
                        .gap(2.0)
                        .child(
                            div()
                                .w(35.0 * scale)
                                .flex_row()
                                .justify_end()
                                .items_center()
                                .child(
                                    text("0:10")
                                        .size(11.0 * scale)
                                        .color(Color::rgba(1.0, 1.0, 1.0, 0.85)),
                                ),
                        )
                        .child(
                            div()
                                .flex_grow()
                                .h(bar_h)
                                .rounded(bar_h / 2.0)
                                .effect(
                                    GlassMaterial::new()
                                        .blur(25.0 * scale)
                                        .tint_rgba(1.0, 1.0, 1.0, 0.65)
                                        .border(0.0),
                                )
                                .child(
                                    div()
                                        .w(20.0 * scale)
                                        .h_full()
                                        .rounded(bar_h / 2.0)
                                        .bg(Color::WHITE),
                                ),
                        )
                        .child(
                            div()
                                .w(40.0 * scale)
                                .flex_row()
                                .justify_start()
                                .items_center()
                                .child(
                                    text("-3:24")
                                        .size(11.0 * scale)
                                        .color(Color::rgba(1.0, 1.0, 1.0, 0.85)),
                                ),
                        ),
                )
                // Controls
                .child(
                    div()
                        .w_full()
                        .flex_grow()
                        .flex_row()
                        .justify_center()
                        .items_center()
                        .gap(10.0)
                        .child(svg(rewind_svg).square(icon_size))
                        .child(svg(pause_svg).square(icon_size))
                        .child(svg(forward_svg).square(icon_size)),
                ),
        );

    render_to_png(
        &mut app,
        "music_player",
        &ui,
        (400.0 * scale) as u32,
        (300.0 * scale) as u32,
    );
}

#[test]
fn test_render_tree_reuse() {
    let mut app = create_test_app();

    let ui = div()
        .w(200.0)
        .h(200.0)
        .flex_col()
        .gap(2.0)
        .p(2.0)
        .bg(Color::WHITE)
        .child(div().flex_grow().w_full().rounded(8.0).bg(Color::RED))
        .child(div().flex_grow().w_full().rounded(8.0).bg(Color::GREEN))
        .child(div().flex_grow().w_full().rounded(8.0).bg(Color::BLUE));

    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 200.0);

    let (texture, view) = create_test_texture(app.device(), 200, 200);

    // Render the same tree 3 times
    for i in 0..3 {
        app.render_tree(&tree, &view, 200, 200)
            .expect("Render failed");
    }

    let path = Path::new(OUTPUT_DIR).join("render_tree_reuse.png");
    save_to_png(app.device(), app.queue(), &texture, 200, 200, &path);
    println!("Saved: {:?}", path);
}
