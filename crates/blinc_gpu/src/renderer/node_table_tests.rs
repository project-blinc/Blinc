//! The renderer with the animated node table: off by default, drawing what it drew when no
//! primitive names a row, and moving the primitives that do through the core and shadow
//! pipelines.

use super::{GpuRenderer, RendererConfig};
use crate::node_table::{NODE_TABLE_CAPACITY, NodeRow};
use crate::primitives::{GpuPrimitive, PrimitiveBatch, PrimitiveType};

const SIZE: u32 = 128;

fn renderer(node_table: bool) -> Option<GpuRenderer> {
    let mut r = pollster::block_on(GpuRenderer::new(RendererConfig {
        node_table,
        texture_format: Some(wgpu::TextureFormat::Rgba8Unorm),
        ..RendererConfig::default()
    }))
    .ok()?;
    r.resize(SIZE, SIZE);
    Some(r)
}

/// A card with a border, a circle and a shadow, all at rest.
fn scene() -> Vec<GpuPrimitive> {
    let mut shadow = GpuPrimitive::rect(30.0, 30.0, 46.0, 32.0).with_corner_radius(8.0);
    shadow.type_info[0] = PrimitiveType::Shadow as u32;
    shadow.shadow = [0.0, 6.0, 6.0, 0.0];
    shadow.shadow_color = [0.0, 0.0, 0.0, 0.6];
    vec![
        shadow,
        GpuPrimitive::rect(30.0, 30.0, 46.0, 32.0)
            .with_color(0.9, 0.3, 0.1, 1.0)
            .with_corner_radius(8.0)
            .with_border(2.0, 1.0, 1.0, 1.0, 0.9),
        GpuPrimitive::circle(84.0, 84.0, 14.0).with_color(0.2, 0.6, 0.9, 0.9),
    ]
}

fn draw(r: &mut GpuRenderer, primitives: Vec<GpuPrimitive>) -> Vec<u8> {
    let device = r.device_arc();
    let queue = r.queue_arc();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("node table test target"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut batch = PrimitiveBatch::new();
    batch.primitives = primitives;
    r.render(&view, &batch);

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (SIZE * SIZE * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 4),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::Wait).ok();
    let pixels = readback.slice(..).get_mapped_range().to_vec();
    readback.unmap();
    pixels
}

/// The most any channel differs, and how many pixels differ by more than two levels.
fn compare(a: &[u8], b: &[u8]) -> (u8, usize) {
    assert_eq!(a.len(), b.len());
    let mut worst = 0u8;
    let mut over = 0;
    for (pa, pb) in a.chunks(4).zip(b.chunks(4)) {
        let d = pa
            .iter()
            .zip(pb)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        worst = worst.max(d);
        over += usize::from(d > 2);
    }
    (worst, over)
}

#[test]
fn the_table_is_off_unless_asked_for() {
    let Some(r) = renderer(false) else { return };
    assert!(!r.node_table_enabled());
    assert!(!r.set_node_rows(&[NodeRow::REST]));
}

#[test]
fn rows_past_the_capacity_are_refused() {
    let Some(r) = renderer(true) else { return };
    if !r.node_table_enabled() {
        return;
    }
    assert!(r.set_node_rows(&vec![NodeRow::REST; NODE_TABLE_CAPACITY]));
    assert!(!r.set_node_rows(&vec![NodeRow::REST; NODE_TABLE_CAPACITY + 1]));
}

#[test]
fn primitives_that_name_no_row_draw_as_they_did() {
    let (Some(mut off), Some(mut on)) = (renderer(false), renderer(true)) else {
        return;
    };
    if !on.node_table_enabled() {
        return;
    }
    assert!(on.set_node_rows(&[NodeRow::new(
        [40.0, 40.0],
        [2.0, 2.0],
        [0.0, 0.0],
        1.0,
        0.2,
        None
    )]));
    let before = draw(&mut off, scene());
    let after = draw(&mut on, scene());
    assert!(before.iter().any(|&b| b != 0), "the scene drew nothing");
    assert_eq!(
        compare(&before, &after),
        (0, 0),
        "a row nobody names changed pixels"
    );
}

#[test]
fn a_row_moves_and_fades_what_names_it() {
    let (Some(mut off), Some(mut on)) = (renderer(false), renderer(true)) else {
        return;
    };
    if !on.node_table_enabled() {
        return;
    }
    let (dx, dy, opacity) = (9.0, -6.0, 0.5);

    // What the CPU patcher does: move the bounds and scale the alphas.
    let patched: Vec<GpuPrimitive> = scene()
        .into_iter()
        .map(|mut p| {
            p.bounds[0] += dx;
            p.bounds[1] += dy;
            p.color[3] *= opacity;
            p.border_color[3] *= opacity;
            p.shadow_color[3] *= opacity;
            p
        })
        .collect();
    let expected = draw(&mut off, patched);

    let mut followers = scene();
    for p in &mut followers {
        p.set_node_slot(Some(0));
    }
    assert!(on.set_node_rows(&[NodeRow::new(
        [dx, dy],
        [1.0, 1.0],
        [64.0, 64.0],
        0.0,
        opacity,
        None
    )]));
    let actual = draw(&mut on, followers);

    assert!(
        expected.iter().any(|&b| b != 0),
        "the patched frame is empty"
    );
    let (worst, over) = compare(&expected, &actual);
    assert!(
        worst <= 3 && over == 0,
        "the table frame differs from the patched one: worst {worst}, {over} pixels over two levels"
    );
    let at_rest = draw(&mut off, scene());
    assert_ne!(compare(&at_rest, &actual), (0, 0), "the row moved nothing");
}
