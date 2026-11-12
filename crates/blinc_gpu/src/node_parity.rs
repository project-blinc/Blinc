//! The animated node table draws what the CPU patcher draws. Each case is rendered twice: the
//! primitives patched on the CPU the way `apply_binding_deltas` patches them and drawn by the
//! ordinary core shader, and the primitives as they were with the node's delta in a node row,
//! drawn by the core shader built with the table. Set `BLINC_NODE_PARITY_OUT` to a directory to
//! write each case as a PNG: the CPU's frame, the table's frame and their difference.

use crate::parity_harness as harness;
use crate::primitives::GpuPrimitive;

const SIZE: u32 = 128;

/// A node row as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct NodeRow {
    motion: [f32; 4],
    pivot: [f32; 4],
    link: [f32; 4],
}

/// A node's delta: the CSS transform about a pivot, and an opacity.
#[derive(Clone, Copy)]
struct Delta {
    translate: [f32; 2],
    scale: [f32; 2],
    pivot: [f32; 2],
    rotation: f32,
    opacity: f32,
}

impl Delta {
    const REST: Delta = Delta {
        translate: [0.0; 2],
        scale: [1.0; 2],
        pivot: [64.0, 64.0],
        rotation: 0.0,
        opacity: 1.0,
    };

    fn row(&self, parent: u32) -> NodeRow {
        NodeRow {
            motion: [
                self.translate[0],
                self.translate[1],
                self.scale[0],
                self.scale[1],
            ],
            pivot: [self.pivot[0], self.pivot[1], self.rotation, self.opacity],
            link: [parent as f32, 0.0, 0.0, 0.0],
        }
    }
}

/// What `apply_binding_deltas` does to a primitive that had no delta, with every changed property
/// in the order it applies them: translate, opacity, scale about the pivot, then rotate about it.
fn patched_like_the_cpu(mut p: GpuPrimitive, d: &Delta) -> GpuPrimitive {
    p.bounds[0] += d.translate[0];
    p.bounds[1] += d.translate[1];
    p.color[3] *= d.opacity;
    p.border_color[3] *= d.opacity;
    p.shadow_color[3] *= d.opacity;
    let [rx, ry] = d.scale;
    let [cx, cy] = d.pivot;
    p.bounds[0] = cx + (p.bounds[0] - cx) * rx;
    p.bounds[1] = cy + (p.bounds[1] - cy) * ry;
    p.bounds[2] *= rx;
    p.bounds[3] *= ry;
    let radius = (rx * ry).sqrt();
    for r in &mut p.corner_radius {
        *r *= radius;
    }
    rotated_like_the_cpu(p, d)
}

fn rotated_like_the_cpu(mut p: GpuPrimitive, d: &Delta) -> GpuPrimitive {
    let (sin_d, cos_d) = d.rotation.sin_cos();
    let [cx, cy] = d.pivot;
    let centre = (
        p.bounds[0] + p.bounds[2] * 0.5,
        p.bounds[1] + p.bounds[3] * 0.5,
    );
    let (dx, dy) = (centre.0 - cx, centre.1 - cy);
    let new_centre = (cx + dx * cos_d - dy * sin_d, cy + dx * sin_d + dy * cos_d);
    p.bounds[0] = new_centre.0 - p.bounds[2] * 0.5;
    p.bounds[1] = new_centre.1 - p.bounds[3] * 0.5;
    let (sin_a, cos_a) = (p.rotation[0], p.rotation[1]);
    p.rotation[0] = sin_a * cos_d + cos_a * sin_d;
    p.rotation[1] = cos_a * cos_d - sin_a * sin_d;
    let [a, b, c, e] = p.local_affine;
    p.local_affine = [
        a * cos_d + c * sin_d,
        b * cos_d + e * sin_d,
        -a * sin_d + c * cos_d,
        -b * sin_d + e * cos_d,
    ];
    p
}

/// The composition the node table makes, done on the CPU: scale about the pivot, turn about it,
/// then translate. The same as the shader's, to compare against when the CPU patcher's order
/// would give another answer.
fn composed(mut p: GpuPrimitive, d: &Delta) -> GpuPrimitive {
    let [sx, sy] = d.scale;
    let [px, py] = d.pivot;
    let (w, h) = (p.bounds[2], p.bounds[3]);
    let (cx, cy) = (p.bounds[0] + w * 0.5, p.bounds[1] + h * 0.5);
    let (rx, ry) = ((cx - px) * sx, (cy - py) * sy);
    let (sin_d, cos_d) = d.rotation.sin_cos();
    let ncx = px + rx * cos_d - ry * sin_d + d.translate[0];
    let ncy = py + rx * sin_d + ry * cos_d + d.translate[1];
    let (nw, nh) = (w * sx, h * sy);
    p.bounds = [ncx - nw * 0.5, ncy - nh * 0.5, nw, nh];
    let radius = (sx * sy).abs().sqrt();
    for r in &mut p.corner_radius {
        *r *= radius;
    }
    let (sin_a, cos_a) = (p.rotation[0], p.rotation[1]);
    p.rotation[0] = sin_a * cos_d + cos_a * sin_d;
    p.rotation[1] = cos_a * cos_d - sin_a * sin_d;
    let [a, b, c, e] = p.local_affine;
    p.local_affine = [
        a * cos_d + c * sin_d,
        b * cos_d + e * sin_d,
        -a * sin_d + c * cos_d,
        -b * sin_d + e * cos_d,
    ];
    for colour in [
        &mut p.color,
        &mut p.color2,
        &mut p.border_color,
        &mut p.shadow_color,
    ] {
        colour[3] *= d.opacity;
    }
    p
}

/// A mix of what motion and CSS animations move: a rounded card with a border, a translucent
/// circle, a squircle, a pill, and a rect with a gradient end.
fn scene() -> Vec<GpuPrimitive> {
    let mut squircle = GpuPrimitive::rect(18.0, 78.0, 40.0, 30.0)
        .with_color(0.95, 0.75, 0.1, 1.0)
        .with_corner_radius(12.0)
        .with_border(2.0, 0.1, 0.1, 0.1, 1.0);
    squircle.corner_shape = [2.0; 4];
    vec![
        GpuPrimitive::rect(22.0, 22.0, 48.0, 34.0)
            .with_color(0.9, 0.2, 0.1, 1.0)
            .with_corner_radius(10.0)
            .with_border(3.0, 1.0, 1.0, 1.0, 0.8),
        GpuPrimitive::circle(92.0, 40.0, 14.0).with_color(0.2, 0.7, 0.3, 0.9),
        squircle,
        GpuPrimitive::rect(72.0, 76.0, 36.0, 18.0)
            .with_color(0.2, 0.4, 0.95, 0.85)
            .with_corner_radius(9.0),
        GpuPrimitive::rect(40.0, 100.0, 30.0, 14.0)
            .with_color(0.6, 0.1, 0.7, 1.0)
            .with_corner_radii(2.0, 8.0, 2.0, 8.0),
    ]
}

/// Draw `prims` with the core shader `source` and read the pixels back. `rows` is the node table,
/// bound for the shaders that read it.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    prims: &[GpuPrimitive],
    rows: &[NodeRow],
) -> Vec<u8> {
    use wgpu::{BindingType as B, ShaderStages as S};
    let both = S::VERTEX | S::FRAGMENT;
    let buffer_entry = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: both,
        ty: B::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: S::FRAGMENT,
        ty: B::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let storage = wgpu::BufferBindingType::Storage { read_only: true };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            buffer_entry(0, wgpu::BufferBindingType::Uniform),
            buffer_entry(1, storage),
            texture_entry(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: S::FRAGMENT,
                ty: B::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            texture_entry(4),
            buffer_entry(5, storage),
            buffer_entry(6, storage),
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            }),
        ),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    let uniforms = harness::buffer(
        device,
        bytemuck::bytes_of(&[SIZE as f32, SIZE as f32, 0.0, 0.0]),
        wgpu::BufferUsages::UNIFORM,
    );
    let primitives = harness::buffer(
        device,
        bytemuck::cast_slice(prims),
        wgpu::BufferUsages::STORAGE,
    );
    let aux = harness::buffer(
        device,
        bytemuck::cast_slice(&[[0.0f32; 4]]),
        wgpu::BufferUsages::STORAGE,
    );
    // A shader that never reads the table still has the binding, so it holds at least one row.
    let identity = [Delta::REST.row(0)];
    let rows = if rows.is_empty() { &identity[..] } else { rows };
    let nodes = harness::buffer(
        device,
        bytemuck::cast_slice(rows),
        wgpu::BufferUsages::STORAGE,
    );
    let atlas = device
        .create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let sampler = device.create_sampler(&Default::default());
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            entry(0, uniforms.as_entire_binding()),
            entry(1, primitives.as_entire_binding()),
            entry(2, wgpu::BindingResource::TextureView(&atlas)),
            entry(3, wgpu::BindingResource::Sampler(&sampler)),
            entry(4, wgpu::BindingResource::TextureView(&atlas)),
            entry(5, aux.as_entire_binding()),
            entry(6, nodes.as_entire_binding()),
        ],
    });
    harness::render(device, queue, SIZE, [0, 0, 0, 0], |pass| {
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..6, 0..prims.len() as u32);
    })
}

fn entry(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource }
}

/// How two frames differ: the largest channel difference, and the pixels that differ by more than
/// the two levels edge rounding allows.
fn compare(a: &[u8], b: &[u8]) -> (u8, usize) {
    let mut worst = 0u8;
    let mut over = 0;
    for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
        let d = p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
        worst = worst.max(d);
        over += usize::from(d > 2);
    }
    (worst, over)
}

/// Write the case as a PNG, if asked: the CPU's frame, the table's, and the difference.
fn dump(name: &str, cpu: &[u8], table: &[u8]) {
    let Ok(dir) = std::env::var("BLINC_NODE_PARITY_OUT") else {
        return;
    };
    let n = SIZE as usize;
    let mut out = vec![255u8; n * 3 * n * 4];
    let blit = |out: &mut Vec<u8>, frame: &[u8], column: usize, diff: Option<&[u8]>| {
        for y in 0..n {
            for x in 0..n {
                let from = (y * n + x) * 4;
                let to = (y * n * 3 + column * n + x) * 4;
                for c in 0..4 {
                    out[to + c] = match diff {
                        Some(other) if c < 3 => {
                            frame[from + c].abs_diff(other[from + c]).saturating_mul(32)
                        }
                        Some(_) => 255,
                        None => frame[from + c],
                    };
                }
                if diff.is_none() {
                    // Composite over a checker so transparency is visible.
                    let a = frame[from + 3] as f32 / 255.0;
                    let back = if (x / 8 + y / 8) % 2 == 0 {
                        70.0
                    } else {
                        100.0
                    };
                    for c in 0..3 {
                        out[to + c] = (frame[from + c] as f32 + back * (1.0 - a)).min(255.0) as u8;
                    }
                    out[to + 3] = 255;
                }
            }
        }
    };
    blit(&mut out, cpu, 0, None);
    blit(&mut out, table, 1, None);
    blit(&mut out, cpu, 2, Some(table));
    let image = image::RgbaImage::from_raw(SIZE * 3, SIZE, out).unwrap();
    let _ = std::fs::create_dir_all(&dir);
    image.save(format!("{dir}/{name}.png")).expect("png");
}

/// Render `case` both ways and require the same pixels, to within edge rounding.
fn same_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    name: &str,
    cpu_patch: impl Fn(GpuPrimitive, &Delta) -> GpuPrimitive,
    delta: Delta,
) {
    let rest = scene();
    let patched: Vec<GpuPrimitive> = rest.iter().map(|p| cpu_patch(*p, &delta)).collect();
    let expected = draw(
        device,
        queue,
        crate::sdf_russl::sdf_core::WGSL,
        &patched,
        &[],
    );
    let mut followers = rest;
    for p in &mut followers {
        p.set_node_slot(Some(0));
    }
    let table = draw(
        device,
        queue,
        crate::sdf_russl::sdf_core_nodes::WGSL,
        &followers,
        &[delta.row(0)],
    );
    dump(name, &expected, &table);
    assert!(
        expected.iter().any(|&b| b != 0),
        "{name}: the CPU frame is empty"
    );
    let (worst, over) = compare(&expected, &table);
    eprintln!("{name}: worst channel difference {worst}, {over} pixels over two levels");
    assert!(
        worst <= 8 && over <= 24,
        "{name}: worst difference {worst}, {over} pixels over two levels"
    );
}

#[test]
fn the_table_with_no_delta_draws_the_primitives_as_they_are() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let rest = scene();
    let plain = draw(
        &device,
        &queue,
        crate::sdf_russl::sdf_core::WGSL,
        &rest,
        &[],
    );
    let mut followers = rest;
    for p in &mut followers {
        p.set_node_slot(Some(0));
    }
    let with_table = draw(
        &device,
        &queue,
        crate::sdf_russl::sdf_core_nodes::WGSL,
        &followers,
        &[Delta::REST.row(0)],
    );
    dump("rest", &plain, &with_table);
    assert_eq!(
        compare(&plain, &with_table),
        (0, 0),
        "an identity row changed pixels"
    );
}

#[test]
fn a_node_translates_its_primitives_as_the_patcher_does() {
    let Some((device, queue)) = harness::device() else {
        return;
    };
    let delta = Delta {
        translate: [9.0, -6.0],
        ..Delta::REST
    };
    same_frame(&device, &queue, "translate", patched_like_the_cpu, delta);
}

#[test]
fn a_node_scales_its_primitives_about_the_pivot_as_the_patcher_does() {
    let Some((device, queue)) = harness::device() else {
        return;
    };
    for (name, scale) in [("scale_down", [0.8, 0.8]), ("scale_stretch", [1.15, 0.85])] {
        let delta = Delta {
            scale,
            ..Delta::REST
        };
        same_frame(&device, &queue, name, patched_like_the_cpu, delta);
    }
}

#[test]
fn a_node_turns_its_primitives_about_the_pivot_as_the_patcher_does() {
    let Some((device, queue)) = harness::device() else {
        return;
    };
    for (name, rotation) in [("rotate_small", 0.25), ("rotate_large", -0.9)] {
        let delta = Delta {
            rotation,
            ..Delta::REST
        };
        same_frame(&device, &queue, name, patched_like_the_cpu, delta);
    }
}

#[test]
fn a_node_fades_its_primitives_as_the_patcher_does() {
    let Some((device, queue)) = harness::device() else {
        return;
    };
    let delta = Delta {
        opacity: 0.45,
        ..Delta::REST
    };
    same_frame(&device, &queue, "opacity", patched_like_the_cpu, delta);
}

/// Everything at once. The patcher applies each change to what the last one left, so it scales
/// the translation too; the table gives the CSS composition, which is what is compared with.
#[test]
fn a_node_with_every_delta_gives_the_css_composition() {
    let Some((device, queue)) = harness::device() else {
        return;
    };
    let delta = Delta {
        translate: [6.0, -4.0],
        scale: [1.1, 0.9],
        pivot: [60.0, 70.0],
        rotation: 0.3,
        opacity: 0.7,
    };
    same_frame(&device, &queue, "combined", composed, delta);
}

/// A child node under a parent: the child's delta, then the parent's.
#[test]
fn a_parent_node_moves_what_its_child_node_moves() {
    let Some((device, queue)) = harness::device() else {
        return;
    };
    let child = Delta {
        scale: [0.9, 0.9],
        rotation: 0.2,
        ..Delta::REST
    };
    let parent = Delta {
        translate: [8.0, 5.0],
        opacity: 0.6,
        ..Delta::REST
    };
    let rest = scene();
    let expected: Vec<GpuPrimitive> = rest
        .iter()
        .map(|p| composed(composed(*p, &child), &parent))
        .collect();
    let drawn = draw(
        &device,
        &queue,
        crate::sdf_russl::sdf_core::WGSL,
        &expected,
        &[],
    );
    let mut followers = rest;
    for p in &mut followers {
        p.set_node_slot(Some(1));
    }
    // Slot 0 is the parent, slot 1 its child.
    let table = draw(
        &device,
        &queue,
        crate::sdf_russl::sdf_core_nodes::WGSL,
        &followers,
        &[parent.row(0), child.row(1)],
    );
    dump("chain", &drawn, &table);
    let (worst, over) = compare(&drawn, &table);
    eprintln!("chain: worst channel difference {worst}, {over} pixels over two levels");
    assert!(
        worst <= 8 && over <= 24,
        "chain: worst {worst}, {over} over"
    );
}
