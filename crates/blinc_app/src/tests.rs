//! Visual tests for blinc_app API
//!
//! Tests render to PNG files in test_output/blinc_app/ for visual verification.
//! These tests require a GPU and will be skipped in CI environments without one.

use crate::app::BlincConfig;
use crate::prelude::*;
use image::{ImageBuffer, Rgba, RgbaImage};
use std::path::Path;

/// Test output directory
const OUTPUT_DIR: &str = "test_output/blinc_app";

/// Create test app for rendering tests
/// Returns None if no GPU adapter is available (e.g., in CI without GPU)
fn create_test_app() -> Option<BlincApp> {
    match BlincApp::with_config(BlincConfig {
        sample_count: 1, // SDF handles AA for most elements
        ..Default::default()
    }) {
        Ok(app) => Some(app),
        Err(e) => {
            eprintln!("Skipping test: no GPU available ({e})");
            None
        }
    }
}

/// Macro to skip test if no GPU is available
macro_rules! require_gpu {
    ($app:ident) => {
        let Some(mut $app) = create_test_app() else {
            return; // Skip test if no GPU
        };
    };
}

/// Create a test texture for rendering.
///
/// Takes the format from the renderer: a pipeline and its render pass
/// must agree, and a hardcoded copy here silently broke every pixel test
/// the moment the renderer's default changed.
fn create_test_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
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
        format,
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
    #[allow(clippy::manual_div_ceil)]
    {
        ((unpadded + align - 1) / align) * align
    }
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
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
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
    let _ = device.poll(wgpu::PollType::Wait);
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
                Rgba([
                    row_data[i + 2],
                    row_data[i + 1],
                    row_data[i],
                    row_data[i + 3],
                ]),
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
fn render_to_png(
    app: &mut BlincApp,
    name: &str,
    ui: &impl ElementBuilder,
    width: u32,
    height: u32,
) {
    let (texture, view) = create_test_texture(app.device(), app.texture_format(), width, height);
    app.render(ui, &view, width as f32, height as f32)
        .expect("Render failed");

    let path = Path::new(OUTPUT_DIR).join(format!("{}.png", name));
    save_to_png(app.device(), app.queue(), &texture, width, height, &path);
    println!("Saved: {:?}", path);
}

#[test]
fn test_simple_red_box() {
    require_gpu!(app);
    let ui = div().w(200.0).h(200.0).bg(Color::RED);
    render_to_png(&mut app, "simple_red_box", &ui, 200, 200);
}

#[test]
fn test_nested_boxes() {
    require_gpu!(app);

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
    require_gpu!(app);

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
    require_gpu!(app);

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
    require_gpu!(app);

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
    require_gpu!(app);

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
    require_gpu!(app);

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
    require_gpu!(app);
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

/// Read the rendered texture back as an `RgbaImage`, mirroring the
/// inner half of `save_to_png`. Lets pixel-asserting tests inspect
/// the GPU output directly without round-tripping through the
/// filesystem.
fn read_rendered_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> RgbaImage {
    let bytes_per_row = padded_bytes_per_row(width);
    let buffer_size = (bytes_per_row * height) as u64;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Pixel Readback Buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Pixel Readback Encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
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
    let _ = device.poll(wgpu::PollType::Wait);
    rx.recv().unwrap().expect("Failed to map buffer");

    let data = buffer_slice.get_mapped_range();
    let mut img: RgbaImage = ImageBuffer::new(width, height);
    for y in 0..height {
        let row_start = (y * bytes_per_row) as usize;
        let row_end = row_start + (width * 4) as usize;
        let row = &data[row_start..row_end];
        for x in 0..width {
            let i = (x * 4) as usize;
            // Convert BGRA → RGBA. `Bgra8UnormSrgb` is the test
            // texture's format (see `create_test_texture`).
            img.put_pixel(x, y, Rgba([row[i + 2], row[i + 1], row[i], row[i + 3]]));
        }
    }
    drop(data);
    buffer.unmap();
    img
}

/// Regression test for the no-glass MSAA layer-effect bug.
///
/// Pre-fix: `RenderContext::render_tree_with_motion`'s no-glass
/// fast path dispatched to `render_overlay_msaa` whenever
/// `sample_count > 1`. `render_overlay_msaa` doesn't walk
/// `batch.layer_commands`, so `LayerEffect::Blur` (and friends)
/// pushed via `Div::blur` were silently dropped on the default
/// 4× MSAA configuration. The blur radius reached the renderer
/// in `batch.layer_commands` but never reached
/// `apply_layer_effects`, so the rect rendered as if no filter
/// were set — sharp edges instead of softened ones.
///
/// Post-fix (commits `da29258b` + `8d92007c`): the no-glass fast
/// path always routes through `render_with_clear`, which honours
/// the layer stack regardless of MSAA. Path overlay MSAA still
/// runs after to recover path-edge AA.
///
/// Test asserts that a pixel just outside the inner rect's
/// authored boundary picks up colour from the rect (i.e. the
/// blur softened the edge into the surrounding white). Without
/// the fix, pixels outside the sharp boundary stay pure white
/// because the blur was dropped.
#[test]
fn test_layer_blur_visible_on_msaa_path() {
    let Ok(mut app) = BlincApp::with_config(BlincConfig {
        sample_count: 4, // The configuration that exposed the bug.
        ..Default::default()
    }) else {
        eprintln!("Skipping test_layer_blur_visible_on_msaa_path: no GPU");
        return;
    };

    let canvas = 200.0_f32;
    let inner_size = 100.0_f32;
    let blur_radius = 8.0_f32;
    let inset = (canvas - inner_size) / 2.0; // 50.0 — inner rect at (50, 50)..(150, 150)

    let ui = div().w(canvas).h(canvas).bg(Color::WHITE).child(
        div()
            .w(inner_size)
            .h(inner_size)
            .absolute()
            .left(inset)
            .top(inset)
            .blur(blur_radius)
            .bg(Color::RED),
    );

    // The bug lives in `render_tree_with_motion`'s no-glass fast
    // path — `app.render` / `RenderContext::render_tree` skip layer
    // effects entirely, so reproducing through that path would
    // pass even with the bug regressed. Build the motion tree +
    // empty render state by hand and route through the right
    // entry point.
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(canvas, canvas);
    let scheduler = std::sync::Arc::new(std::sync::Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    ));
    let render_state = blinc_layout::RenderState::new(scheduler);

    let (texture, view) = create_test_texture(
        app.device(),
        app.texture_format(),
        canvas as u32,
        canvas as u32,
    );
    app.render_tree_with_motion(&tree, &render_state, &view, canvas as u32, canvas as u32)
        .expect("Render failed");

    let img = read_rendered_pixels(
        app.device(),
        app.queue(),
        &texture,
        canvas as u32,
        canvas as u32,
    );

    // Save for visual debugging — expected output is a soft-edged
    // red blob in the centre of a white square.
    let path = Path::new(OUTPUT_DIR).join("layer_blur_visible_on_msaa_path.png");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    img.save(&path).expect("Failed to save PNG");

    // Sample a pixel `expand` pixels outside the inner rect's
    // authored sharp edge along the horizontal centerline. With
    // an 8 px blur the edge falloff extends well past 4 px, so
    // this pixel should pick up significant red and reduced
    // green/blue. Without the fix it would be pure white
    // (the rect's primitives never made it onto the target
    // because `render_overlay_msaa` swallowed the layer push,
    // so the surrounding white shows through unmodified).
    let expand = 4_u32;
    let probe_x = (inset as u32) - expand; // 46
    let probe_y = (canvas as u32) / 2; // 100
    let p = img.get_pixel(probe_x, probe_y);
    let red = p[0] as i32;
    let green = p[1] as i32;
    let blue = p[2] as i32;

    // Pure-white sentinel: r=g=b=255. If the bug regressed, all
    // three channels would be saturated. The blurred-rect signal
    // is "red elevated relative to the others" — at least 25 over
    // the green/blue channels at this probe point on the fixed
    // implementation.
    assert!(
        red - green > 25 && red - blue > 25,
        "Layer blur not visible on MSAA path: probe at ({probe_x}, {probe_y}) = \
         (r={red}, g={green}, b={blue}); expected red elevated by >25 vs green/blue. \
         Saved at {path:?}.",
    );

    // Sanity: a pixel deep inside the rect should be near-pure red.
    let mid = canvas as u32 / 2;
    let inside = img.get_pixel(mid, mid);
    assert!(
        inside[0] > 200 && inside[1] < 100 && inside[2] < 100,
        "Inner rect not red at centre: ({}, {}, {}) at ({mid}, {mid}). \
         Saved at {path:?}.",
        inside[0],
        inside[1],
        inside[2],
    );

    // Sanity: a pixel in the corner should be near-pure white
    // (well outside the blur falloff).
    let far = img.get_pixel(5, 5);
    assert!(
        far[0] > 240 && far[1] > 240 && far[2] > 240,
        "Corner pixel not white: ({}, {}, {}). Saved at {path:?}.",
        far[0],
        far[1],
        far[2],
    );
}

/// Companion to `test_layer_blur_visible_on_msaa_path` — same
/// blur scenario, but routed through `app.render`. That entry
/// point goes through `RenderContext::render_tree` →
/// `tree.render_to_layer` → `render_layer`, which historically
/// dropped layer effects entirely (the simpler render path
/// didn't push a layer for nodes carrying `props.layer_effects`).
/// Now that `render_layer` mirrors `render_layer_with_motion`'s
/// layer-effect push, the convenience entry point produces the
/// same blurred output as the motion path. Without the fix the
/// rect renders as a sharp square and the probe pixel sits at
/// pure white.
#[test]
fn test_layer_blur_visible_via_app_render() {
    let Ok(mut app) = BlincApp::with_config(BlincConfig {
        sample_count: 4,
        ..Default::default()
    }) else {
        eprintln!("Skipping test_layer_blur_visible_via_app_render: no GPU");
        return;
    };

    let canvas = 200.0_f32;
    let inner_size = 100.0_f32;
    let blur_radius = 8.0_f32;
    let inset = (canvas - inner_size) / 2.0;

    let ui = div().w(canvas).h(canvas).bg(Color::WHITE).child(
        div()
            .w(inner_size)
            .h(inner_size)
            .absolute()
            .left(inset)
            .top(inset)
            .blur(blur_radius)
            .bg(Color::RED),
    );

    let (texture, view) = create_test_texture(
        app.device(),
        app.texture_format(),
        canvas as u32,
        canvas as u32,
    );
    app.render(&ui, &view, canvas, canvas)
        .expect("Render failed");

    let img = read_rendered_pixels(
        app.device(),
        app.queue(),
        &texture,
        canvas as u32,
        canvas as u32,
    );

    let path = Path::new(OUTPUT_DIR).join("layer_blur_visible_via_app_render.png");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    img.save(&path).expect("Failed to save PNG");

    let probe_x = (inset as u32) - 4;
    let probe_y = (canvas as u32) / 2;
    let p = img.get_pixel(probe_x, probe_y);
    let red = p[0] as i32;
    let green = p[1] as i32;
    let blue = p[2] as i32;
    assert!(
        red - green > 25 && red - blue > 25,
        "Layer blur not visible via app.render: probe at ({probe_x}, {probe_y}) = \
         (r={red}, g={green}, b={blue}); expected red elevated by >25 vs green/blue. \
         Saved at {path:?}.",
    );
}

#[test]
fn test_render_tree_reuse() {
    require_gpu!(app);

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

    let (texture, view) = create_test_texture(app.device(), app.texture_format(), 200, 200);

    // Render the same tree 3 times
    for i in 0..3 {
        app.render_tree(&tree, &view, 200, 200)
            .expect("Render failed");
    }

    let path = Path::new(OUTPUT_DIR).join("render_tree_reuse.png");
    save_to_png(app.device(), app.queue(), &texture, 200, 200, &path);
    println!("Saved: {:?}", path);
}

#[test]
fn render_to_rgba8_with_motion_routes_through_motion_path() {
    require_gpu!(app);

    // Solid blue 8x8. The motion path *must* end up with the same
    // pixels as the static path for a static tree — the regression
    // this guards is "render_to_texture_with_motion calls the right
    // renderer entry point (render_tree_with_motion_opt) and doesn't
    // silently fall back to a no-op or skip dispatch when the tree
    // has no motion containers".
    let ui = div().w(8.0).h(8.0).bg(Color::BLUE);

    let mut tree = blinc_layout::RenderTree::from_element(&ui);
    tree.compute_layout(8.0, 8.0);

    let scheduler = std::sync::Arc::new(std::sync::Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    ));
    let mut state = blinc_layout::RenderState::new(scheduler);
    state.set_viewport(0.0, 0.0, 8.0, 8.0);

    let pixels = app
        .render_to_rgba8_with_motion(&tree, &state, 8, 8)
        .expect("render_to_rgba8_with_motion should succeed");

    assert_eq!(pixels.len(), 8 * 8 * 4);
    let i = (4 * 8 + 4) * 4;
    assert!(
        pixels[i + 2] > 200,
        "expected blue dominant in centre pixel, got RGBA = ({}, {}, {}, {})",
        pixels[i],
        pixels[i + 1],
        pixels[i + 2],
        pixels[i + 3]
    );
    assert!(pixels[i] < 80, "red should be ~0 for solid blue");
    assert!(pixels[i + 1] < 80, "green should be ~0 for solid blue");
    assert_eq!(pixels[i + 3], 255, "alpha should be opaque");
}

#[test]
fn render_to_rgba8_returns_packed_pixels() {
    require_gpu!(app);

    // Solid red over the full surface. This test is about packing and
    // row padding, not colour; see
    // `render_to_rgba8_does_not_double_encode_gamma` for the gamma path.
    // Pure red says nothing about it either way, since 255 is a fixed
    // point of the sRGB curve.
    let ui = div().w(8.0).h(8.0).bg(Color::RED);

    let pixels = app
        .render_to_rgba8(&ui, 8, 8)
        .expect("render_to_rgba8 should succeed");

    // Right size, right padding-stripping.
    assert_eq!(pixels.len(), 8 * 8 * 4);

    // Every pixel should be the same colour — there's no padding left
    // over from `bytes_per_row` alignment if this passes.
    let probe = [pixels[0], pixels[1], pixels[2], pixels[3]];
    for chunk in pixels.chunks_exact(4) {
        assert_eq!(chunk, probe, "padding bytes leaked into row N+1");
    }

    // Centre pixel should be red after BGRA -> RGBA swizzle.
    let row = 4;
    let col = 4;
    let i = (row * 8 + col) * 4;
    assert!(
        pixels[i] > 200,
        "expected red dominant in centre pixel, got RGBA = ({}, {}, {}, {})",
        pixels[i],
        pixels[i + 1],
        pixels[i + 2],
        pixels[i + 3]
    );
    assert!(pixels[i + 1] < 80, "green should be ~0 for solid red");
    assert!(pixels[i + 2] < 80, "blue should be ~0 for solid red");
    assert_eq!(pixels[i + 3], 255, "alpha should be opaque");
}

/// `render_to_rgba8` must give back the colour that went in.
///
/// Windowed surfaces deliberately pick a NON-sRGB format, because Blinc's
/// shaders bake the gamma transfer at the source (see the rationale in
/// `renderer.rs`). The headless default was `Bgra8UnormSrgb`, which treats
/// those already-encoded values as linear and encodes them again, so every
/// offscreen capture came out lighter than the same UI on screen.
///
/// Pure red hides this: 255 is a fixed point of the sRGB curve, which is
/// why `render_to_rgba8_returns_packed_pixels` never caught it. A mid-tone
/// does not.
#[test]
fn render_to_rgba8_does_not_double_encode_gamma() {
    require_gpu!(app);

    // Hybrid's primary. Mid-tones are where a stray transfer shows.
    let want: [u8; 3] = [0x2A, 0x63, 0xE9];
    let ui = div().w(8.0).h(8.0).bg(Color::from_hex(0x2A63E9));

    let pixels = app
        .render_to_rgba8(&ui, 8, 8)
        .expect("render_to_rgba8 should succeed");

    let got = [pixels[0], pixels[1], pixels[2]];
    eprintln!("wanted {want:?}, got {got:?}");
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        let diff = (*g as i32 - *w as i32).abs();
        assert!(
            diff <= 2,
            "channel {i}: got {g}, want {w} (off by {diff}). \
             A second sRGB encode would read about {:?}.",
            [113, 167, 245]
        );
    }
}

/// A `display: none` subtree draws nothing, text included.
///
/// Taffy gives a hidden node and everything under it a zero-size layout, but
/// text drawn into a zero-width box still lands on screen, one letter per
/// line at the corner. Painting has to stop at the hidden node.
#[test]
fn a_display_none_subtree_draws_nothing() {
    require_gpu!(app);
    blinc_layout::text_measurer::init_text_measurer();

    let scene = |with_hidden: bool| {
        let mut column = div()
            .w(160.0)
            .h(80.0)
            .bg(Color::BLACK)
            .flex_col()
            .child(text("one").size(20.0).color(Color::WHITE));
        if with_hidden {
            column = column.child(
                div()
                    .hidden()
                    .bg(Color::RED)
                    .child(text("GONE text").size(20.0).color(Color::WHITE)),
            );
        }
        column.child(text("two").size(20.0).color(Color::WHITE))
    };

    let without = app.render_to_rgba8(&scene(false), 160, 80).expect("render");
    let with = app.render_to_rgba8(&scene(true), 160, 80).expect("render");

    let differing = without
        .chunks_exact(4)
        .zip(with.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 0,
        "a hidden subtree changed {differing} pixels: its content was drawn"
    );
}
