//! The path shader draws what its formulas say, on the storage tier and the data texture tier: the
//! same paths give the same pixels on both, and probes match the shader's maths on the CPU.

use crate::parity_harness::{self as harness, AuxData};
use crate::path::PathVertex;
use crate::primitives::PathUniforms;

const SIZE: u32 = 192;
/// The clear colour.
const CLEAR: [u8; 4] = [64, 128, 191, 255];
/// A cell is 40 pixels square; four to a row, 48 apart.
const CELL: f32 = 40.0;
const IMAGE: u32 = 16;
const BACKDROP: u32 = 32;
const RAMP: u32 = 8;

/// A vertex of a path at `position` with the UV and colour given, and everything else off.
fn vertex(position: [f32; 2], uv: [f32; 2], color: [f32; 4]) -> PathVertex {
    PathVertex {
        position,
        color,
        end_color: color,
        uv,
        gradient_params: [0.0; 4],
        gradient_type: 0,
        edge_distance: 0.0,
        clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
        clip_radius: [0.0; 4],
        clip_type: 0,
        clip_shape: 0,
        _padding: [0; 2],
    }
}

/// The cell in column `col`, row `row`.
fn origin(col: u32, row: u32) -> [f32; 2] {
    [4.0 + 48.0 * col as f32, 4.0 + 48.0 * row as f32]
}

/// A cell as two triangles, its UV running 0 to 1 across it. `shape` sets what each vertex
/// carries beyond position and UV.
fn quad(col: u32, row: u32, color: [f32; 4], shape: impl Fn(&mut PathVertex)) -> Vec<PathVertex> {
    let [x, y] = origin(col, row);
    let corner = |u: f32, v: f32| {
        let mut v = vertex([x + u * CELL, y + v * CELL], [u, v], color);
        shape(&mut v);
        v
    };
    vec![
        corner(0.0, 0.0),
        corner(1.0, 0.0),
        corner(0.0, 1.0),
        corner(1.0, 0.0),
        corner(1.0, 1.0),
        corner(0.0, 1.0),
    ]
}

/// One draw: its uniforms and its vertices.
struct Draw {
    uniforms: PathUniforms,
    vertices: Vec<PathVertex>,
}

fn uniforms() -> PathUniforms {
    PathUniforms {
        viewport_size: [SIZE as f32; 2],
        ..Default::default()
    }
}

/// The ramp texture's texel `i`: a multi-stop gradient.
fn ramp(i: u32) -> [u8; 4] {
    [
        (i * 36) as u8,
        255 - (i * 30) as u8,
        if i.is_multiple_of(2) { 220 } else { 40 },
        255,
    ]
}

/// The image brush's texel at `(x, y)`.
fn image_texel(x: u32, y: u32) -> [u8; 4] {
    [(x * 16) as u8, (y * 16) as u8, ((x + y) * 8) as u8, 255]
}

/// The backdrop's texel at `(x, y)`: red jumps from texel to texel, so the width of a blur shows;
/// green and blue are smooth and far from gray, so the saturation does.
fn backdrop_texel(x: u32, y: u32) -> [u8; 4] {
    [
        ((x * x * 3 + y * 7) % 256) as u8,
        (y * y / 4) as u8,
        (x * y / 5 + 30) as u8,
        255,
    ]
}

/// Draw `draws` with `source` over the clear colour and read the pixels back. `data_texture` is the
/// tier without storage buffers: the auxiliary data is an `RGBA32F` texture.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    data_texture: bool,
    draws: &[Draw],
    aux: &[[f32; 4]],
) -> Vec<u8> {
    use wgpu::{BindingType as B, ShaderStages as S};
    let texture = |dimension| B::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: true },
        view_dimension: dimension,
        multisampled: false,
    };
    let sampler = B::Sampler(wgpu::SamplerBindingType::Filtering);
    let entry = |binding, stage, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stage,
        ty,
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            entry(
                0,
                S::VERTEX | S::FRAGMENT,
                B::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
            entry(1, S::FRAGMENT, texture(wgpu::TextureViewDimension::D1)),
            entry(2, S::FRAGMENT, sampler),
            entry(3, S::FRAGMENT, texture(wgpu::TextureViewDimension::D2)),
            entry(4, S::FRAGMENT, sampler),
            entry(5, S::FRAGMENT, texture(wgpu::TextureViewDimension::D2)),
            entry(6, S::FRAGMENT, sampler),
            entry(7, S::FRAGMENT, AuxData::binding_type(data_texture)),
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    // The production vertex layout, one attribute per field of `PathVertex`.
    let attributes: Vec<wgpu::VertexAttribute> = [
        (wgpu::VertexFormat::Float32x2, 0),
        (wgpu::VertexFormat::Float32x4, 8),
        (wgpu::VertexFormat::Float32x4, 24),
        (wgpu::VertexFormat::Float32x2, 40),
        (wgpu::VertexFormat::Float32x4, 48),
        (wgpu::VertexFormat::Uint32, 64),
        (wgpu::VertexFormat::Float32, 68),
        (wgpu::VertexFormat::Float32x4, 72),
        (wgpu::VertexFormat::Float32x4, 88),
        (wgpu::VertexFormat::Uint32, 104),
        (wgpu::VertexFormat::Uint32, 108),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (format, offset))| wgpu::VertexAttribute {
        format,
        offset,
        shader_location: i as u32,
    })
    .collect();
    let blend = wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    };
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
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<PathVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(blend),
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

    // The ramp is a 1D texture.
    let ramp_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: RAMP,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D1,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let ramp_bytes: Vec<u8> = (0..RAMP).flat_map(ramp).collect();
    queue.write_texture(
        ramp_texture.as_image_copy(),
        &ramp_bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(RAMP * 4),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width: RAMP,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let ramp_view = ramp_texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D1),
        ..Default::default()
    });
    let image_bytes: Vec<u8> = (0..IMAGE)
        .flat_map(|y| (0..IMAGE).flat_map(move |x| image_texel(x, y)))
        .collect();
    let image = harness::texture(
        device,
        queue,
        (IMAGE, IMAGE),
        wgpu::TextureFormat::Rgba8Unorm,
        4,
        &image_bytes,
    );
    let backdrop_bytes: Vec<u8> = (0..BACKDROP)
        .flat_map(|y| (0..BACKDROP).flat_map(move |x| backdrop_texel(x, y)))
        .collect();
    let backdrop = harness::texture(
        device,
        queue,
        (BACKDROP, BACKDROP),
        wgpu::TextureFormat::Rgba8Unorm,
        4,
        &backdrop_bytes,
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let aux = AuxData::new(device, queue, data_texture, aux);

    // A uniform buffer, a bind group and a vertex buffer for each draw.
    let prepared: Vec<_> = draws
        .iter()
        .map(|d| {
            let uniforms = harness::buffer(
                device,
                bytemuck::bytes_of(&d.uniforms),
                wgpu::BufferUsages::UNIFORM,
            );
            let vertices = harness::buffer(
                device,
                bytemuck::cast_slice(&d.vertices),
                wgpu::BufferUsages::VERTEX,
            );
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&ramp_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&image),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&backdrop),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: aux.resource(),
                    },
                ],
            });
            (uniforms, vertices, bind, d.vertices.len() as u32)
        })
        .collect();
    harness::render(device, queue, SIZE, CLEAR, |pass| {
        pass.set_pipeline(&pipeline);
        for (_, vertices, bind, count) in &prepared {
            pass.set_bind_group(0, bind, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.draw(0..*count, 0..1);
        }
    })
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    harness::pixel(pixels, SIZE, x, y)
}

/// The paths: each feature in a cell of its own.
fn scene() -> (Vec<Draw>, Vec<[f32; 4]>) {
    let red = [0.9, 0.2, 0.2, 1.0];
    let shapes = [
        // A rounded rect clip with a radius a corner.
        quad(0, 0, red, |v| {
            v.clip_type = 1;
            v.clip_bounds = [8.0, 8.0, 32.0, 32.0];
            v.clip_radius = [8.0, 2.0, 8.0, 2.0];
        }),
        // A circle clip: the scissor, then (centre, radius).
        quad(1, 0, [0.2, 0.9, 0.3, 1.0], |v| {
            v.clip_type = 2;
            v.clip_bounds = [52.0, 4.0, 40.0, 40.0];
            v.clip_radius = [72.0, 24.0, 16.0, 0.0];
        }),
        // An ellipse clip: the scissor, then (centre, radii).
        quad(2, 0, [0.2, 0.3, 0.9, 1.0], |v| {
            v.clip_type = 3;
            v.clip_bounds = [100.0, 4.0, 40.0, 40.0];
            v.clip_radius = [120.0, 24.0, 18.0, 12.0];
        }),
        // A polygon clip: the scissor, then the vertex count and the first auxiliary row.
        quad(3, 0, [0.9, 0.8, 0.1, 1.0], |v| {
            v.clip_type = 4;
            v.clip_bounds = [148.0, 4.0, 40.0, 40.0];
            v.clip_radius = [0.0, 0.0, 5.0, 0.0];
        }),
        // An ABI shape clip whose header is at row 3.
        quad(0, 1, [0.7, 0.2, 0.8, 1.0], |v| v.clip_shape = 4),
        quad(1, 1, [0.1, 0.7, 0.7, 0.7], |_| {}),
    ];
    let gradients = [
        // Linear and radial, the two stops mixed from the vertex colours.
        quad(2, 1, [1.0, 0.0, 0.0, 1.0], |v| {
            v.end_color = [0.0, 0.0, 1.0, 1.0];
            v.gradient_type = 1;
            v.gradient_params = [0.0, 0.0, 0.6, 0.6];
        }),
        quad(3, 1, [1.0, 0.9, 0.0, 1.0], |v| {
            v.end_color = [0.0, 0.3, 0.6, 0.5];
            v.gradient_type = 2;
            v.gradient_params = [0.5, 0.5, 0.5, 0.4];
        }),
    ];
    let ramped = [
        // The same through the ramp texture.
        quad(0, 2, [1.0; 4], |v| {
            v.gradient_type = 1;
            v.gradient_params = [0.0, 0.0, 1.0, 0.0];
        }),
        quad(1, 2, [1.0; 4], |v| {
            v.gradient_type = 2;
            v.gradient_params = [0.5, 0.5, 0.5, 0.5];
        }),
    ];
    let mut by_ramp = uniforms();
    by_ramp.use_gradient_texture = 1;
    let mut imaged = uniforms();
    imaged.use_image_texture = 1;
    imaged.image_uv_bounds = [0.25, 0.25, 1.0, 0.75];
    let mut glassy = uniforms();
    glassy.use_glass_effect = 1;
    glassy.glass_params = [20.0, 1.5, 0.4, 0.8];
    glassy.glass_tint = [0.2, 0.5, 1.0, 0.5];
    let mut moved = uniforms();
    moved.opacity = 0.6;
    moved.transform = [
        [0.5, 0.0, 20.0, 0.0],
        [0.0, 0.5, 76.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ];
    let draws = vec![
        Draw {
            uniforms: uniforms(),
            vertices: shapes.concat(),
        },
        Draw {
            uniforms: uniforms(),
            vertices: gradients.concat(),
        },
        Draw {
            uniforms: by_ramp,
            vertices: ramped.concat(),
        },
        Draw {
            uniforms: imaged,
            vertices: quad(2, 2, [1.0, 0.8, 0.8, 0.9], |_| {}),
        },
        Draw {
            uniforms: glassy,
            vertices: quad(3, 2, [1.0; 4], |_| {}),
        },
        Draw {
            uniforms: moved,
            vertices: quad(0, 3, [0.2, 0.8, 0.4, 1.0], |_| {}),
        },
    ];
    let aux = vec![
        // 0-2: a pentagon, two vertices to a row.
        [168.0, 6.0, 186.0, 20.0],
        [178.0, 42.0, 158.0, 42.0],
        [150.0, 20.0, 0.0, 0.0],
        // 3-5: an ABI shape clip: frame, (offset, kind 2, radius), (origin, size).
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 2.0, 6.0],
        [10.0, 58.0, 28.0, 28.0],
    ];
    (draws, aux)
}

#[test]
fn the_data_texture_tier_draws_the_pixels_the_storage_tier_draws() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (draws, aux) = scene();
    let storage = draw(
        &device,
        &queue,
        crate::sdf_russl::path::WGSL,
        false,
        &draws,
        &aux,
    );
    let texture = draw(
        &device,
        &queue,
        crate::sdf_russl::path_dt::WGSL,
        true,
        &draws,
        &aux,
    );
    assert!(
        storage.as_chunks::<4>().0.iter().any(|p| *p != CLEAR),
        "nothing was drawn"
    );
    assert!(storage == texture, "the tiers draw different pixels");
}

type Rgb = [f32; 3];

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn mix_rgb(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [0, 1, 2].map(|c| mix(a[c], b[c], t))
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `colour` (0 to 1) drawn at `alpha` over the clear colour, in 8-bit levels.
fn over_clear(colour: Rgb, alpha: f32) -> Rgb {
    [0, 1, 2].map(|c| (colour[c] * alpha + CLEAR[c] as f32 / 255.0 * (1.0 - alpha)) * 255.0)
}

fn assert_near(pixel: [u8; 4], expected: Rgb, what: &str) {
    for c in 0..3 {
        assert!(
            (pixel[c] as f32 - expected[c]).abs() <= 2.0,
            "{what}: channel {c} is {} and should be {:.1}",
            pixel[c],
            expected[c]
        );
    }
}

fn rgb_of(texel: [u8; 4]) -> Rgb {
    [0, 1, 2].map(|c| texel[c] as f32 / 255.0)
}

/// A bilinear sample of a `size` square texture at `(u, v)`, clamped at the edges.
fn sample(texel: impl Fn(u32, u32) -> [u8; 4], size: u32, u: f32, v: f32) -> Rgb {
    let (fx, fy) = (u * size as f32 - 0.5, v * size as f32 - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (wx, wy) = (fx - x0, fy - y0);
    let at = |x: f32, y: f32| {
        let max = (size - 1) as f32;
        rgb_of(texel(x.clamp(0.0, max) as u32, y.clamp(0.0, max) as u32))
    };
    let mut rgb = [0.0; 3];
    for (dx, dy, w) in [
        (0.0, 0.0, (1.0 - wx) * (1.0 - wy)),
        (1.0, 0.0, wx * (1.0 - wy)),
        (0.0, 1.0, (1.0 - wx) * wy),
        (1.0, 1.0, wx * wy),
    ] {
        let c = at(x0 + dx, y0 + dy);
        for k in 0..3 {
            rgb[k] += c[k] * w;
        }
    }
    rgb
}

/// A linear sample of the ramp at `t`, clamped at the ends.
fn sample_ramp(t: f32) -> Rgb {
    let f = t * RAMP as f32 - 0.5;
    let (i0, w) = (f.floor(), f - f.floor());
    let at = |i: f32| rgb_of(ramp(i.clamp(0.0, (RAMP - 1) as f32) as u32));
    mix_rgb(at(i0), at(i0 + 1.0), w)
}

/// Signed distance to a rounded rect with a radius for each corner (top-left, top-right,
/// bottom-right, bottom-left).
fn rounded_rect_sdf(p: [f32; 2], origin: [f32; 2], size: [f32; 2], radius: [f32; 4]) -> f32 {
    let half = [size[0] * 0.5, size[1] * 0.5];
    let rel = [p[0] - origin[0] - half[0], p[1] - origin[1] - half[1]];
    let r = match (rel[1] < 0.0, rel[0] > 0.0) {
        (true, true) => radius[1],
        (true, false) => radius[0],
        (false, true) => radius[2],
        (false, false) => radius[3],
    }
    .min(half[0].min(half[1]));
    let q = [rel[0].abs() - half[0] + r, rel[1].abs() - half[1] + r];
    q[0].max(0.0).hypot(q[1].max(0.0)) + q[0].max(q[1]).min(0.0) - r
}

/// The pixel at local `(lx, ly)` of the cell `(col, row)`, and the UV there.
fn cell(pixels: &[u8], col: u32, row: u32, lx: u32, ly: u32) -> ([u8; 4], [f32; 2]) {
    let [x, y] = origin(col, row);
    (
        pixel(pixels, x as u32 + lx, y as u32 + ly),
        [(lx as f32 + 0.5) / CELL, (ly as f32 + 0.5) / CELL],
    )
}

fn centre(col: u32, row: u32, lx: u32, ly: u32) -> [f32; 2] {
    let [x, y] = origin(col, row);
    [x + lx as f32 + 0.5, y + ly as f32 + 0.5]
}

#[test]
fn paths_draw_the_brush_and_the_clip_through_the_formulas() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (draws, aux) = scene();
    let pixels = draw(
        &device,
        &queue,
        crate::sdf_russl::path::WGSL,
        false,
        &draws,
        &aux,
    );
    let red = [0.9, 0.2, 0.2];

    // A rounded rect clip: a pixel inside, one cut by the scissor, the edge and the corners.
    assert_near(
        cell(&pixels, 0, 0, 20, 20).0,
        over_clear(red, 1.0),
        "inside the rect clip",
    );
    assert_eq!(cell(&pixels, 0, 0, 2, 20).0, CLEAR, "outside the rect clip");
    for (lx, ly) in [(4, 20), (4, 4), (35, 4), (35, 35), (4, 35)] {
        let d = rounded_rect_sdf(
            centre(0, 0, lx, ly),
            [8.0, 8.0],
            [32.0, 32.0],
            [8.0, 2.0, 8.0, 2.0],
        );
        assert_near(
            cell(&pixels, 0, 0, lx, ly).0,
            over_clear(red, 1.0 - smoothstep(-0.75, 0.75, d)),
            &format!("rect clip at {lx},{ly}"),
        );
    }

    // A circle clip, in the scissor and by distance from the centre.
    let green = [0.2, 0.9, 0.3];
    for (lx, ly) in [(20, 20), (35, 20), (16, 4), (2, 2)] {
        let p = centre(1, 0, lx, ly);
        let d = (p[0] - 72.0).hypot(p[1] - 24.0) - 16.0;
        assert_near(
            cell(&pixels, 1, 0, lx, ly).0,
            over_clear(green, 1.0 - smoothstep(-0.75, 0.75, d)),
            &format!("circle clip at {lx},{ly}"),
        );
    }

    // An ellipse clip: normalised distance, scaled by the smaller radius.
    let blue = [0.2, 0.3, 0.9];
    for (lx, ly) in [(20, 20), (36, 20), (20, 8), (3, 3)] {
        let p = centre(2, 0, lx, ly);
        let n = ((p[0] - 120.0) / 18.0).hypot((p[1] - 24.0) / 12.0);
        let d = (n - 1.0) * 12.0;
        assert_near(
            cell(&pixels, 2, 0, lx, ly).0,
            over_clear(blue, 1.0 - smoothstep(-0.75, 0.75, d)),
            &format!("ellipse clip at {lx},{ly}"),
        );
    }

    // A polygon clip: inside the pentagon and outside it, both in the scissor.
    let yellow = [0.9, 0.8, 0.1];
    assert_near(
        cell(&pixels, 3, 0, 20, 22).0,
        over_clear(yellow, 1.0),
        "inside the polygon",
    );
    assert_near(
        cell(&pixels, 3, 0, 4, 16).0,
        over_clear(yellow, 1.0),
        "inside the polygon, by its fifth vertex",
    );
    assert_eq!(cell(&pixels, 3, 0, 2, 2).0, CLEAR, "outside the polygon");
    assert_eq!(
        cell(&pixels, 3, 0, 37, 37).0,
        CLEAR,
        "outside the polygon, low"
    );

    // An ABI shape clip: a rounded rect inside the cell.
    let purple = [0.7, 0.2, 0.8];
    assert_near(
        cell(&pixels, 0, 1, 20, 20).0,
        over_clear(purple, 1.0),
        "inside the shape clip",
    );
    assert_eq!(cell(&pixels, 0, 1, 3, 3).0, CLEAR, "outside the shape clip");

    // A translucent vertex colour.
    assert_near(
        cell(&pixels, 1, 1, 20, 20).0,
        over_clear([0.1, 0.7, 0.7], 0.7),
        "translucent colour",
    );

    // Two-stop gradients mix the vertex colours: along (0,0)-(1,1), and by distance from a centre.
    for (lx, ly) in [(10, 30), (30, 5)] {
        let (pix, uv) = cell(&pixels, 2, 1, lx, ly);
        // The gradient line runs (0, 0) to (0.6, 0.6): t is the projection over its squared length.
        let t = ((uv[0] * 0.6 + uv[1] * 0.6) / 0.72).clamp(0.0, 1.0);
        assert_near(
            pix,
            over_clear(mix_rgb([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], t), 1.0),
            &format!("linear gradient at {lx},{ly}"),
        );
    }
    for (lx, ly) in [(20, 20), (30, 12), (4, 36)] {
        let (pix, uv) = cell(&pixels, 3, 1, lx, ly);
        let d = [(uv[0] - 0.5) / 0.5, (uv[1] - 0.5) / 0.4];
        let t = d[0].hypot(d[1]).clamp(0.0, 1.0);
        let alpha = mix(1.0, 0.5, t);
        assert_near(
            pix,
            over_clear(mix_rgb([1.0, 0.9, 0.0], [0.0, 0.3, 0.6], t), alpha),
            &format!("radial gradient at {lx},{ly}"),
        );
    }

    // The same through the ramp: a linear sample at `t`.
    for (lx, ly) in [(5, 10), (20, 30), (36, 4)] {
        let (pix, uv) = cell(&pixels, 0, 2, lx, ly);
        assert_near(
            pix,
            over_clear(sample_ramp(uv[0].clamp(0.0, 1.0)), 1.0),
            &format!("linear ramp at {lx},{ly}"),
        );
    }
    for (lx, ly) in [(20, 20), (30, 12)] {
        let (pix, uv) = cell(&pixels, 1, 2, lx, ly);
        let t = ((uv[0] - 0.5).hypot(uv[1] - 0.5) / 0.5).clamp(0.0, 1.0);
        assert_near(
            pix,
            over_clear(sample_ramp(t), 1.0),
            &format!("radial ramp at {lx},{ly}"),
        );
    }

    // An image brush: the UV mapped into the image's bounds, multiplied by the vertex colour.
    for (lx, ly) in [(8, 8), (30, 25)] {
        let (pix, uv) = cell(&pixels, 2, 2, lx, ly);
        let image_uv = [0.25 + uv[0] * 0.75, 0.25 + uv[1] * 0.5];
        let texel = sample(image_texel, IMAGE, image_uv[0], image_uv[1]);
        let tint = [1.0, 0.8, 0.8];
        assert_near(
            pix,
            over_clear([0, 1, 2].map(|c| texel[c] * tint[c]), 0.9),
            &format!("image brush at {lx},{ly}"),
        );
    }

    // Glass: a box blur of the backdrop, its saturation raised, tinted, at the glass opacity.
    for (lx, ly) in [(20, 20), (6, 30)] {
        let (pix, _) = cell(&pixels, 3, 2, lx, ly);
        let p = centre(3, 2, lx, ly);
        let uv = [p[0] / SIZE as f32, p[1] / SIZE as f32];
        let mut total = [0.0; 3];
        for x in -2..=2 {
            for y in -2..=2 {
                let offset = [x as f32 * 10.0 / SIZE as f32, y as f32 * 10.0 / SIZE as f32];
                let c = sample(
                    backdrop_texel,
                    BACKDROP,
                    (uv[0] + offset[0]).clamp(0.0, 1.0),
                    (uv[1] + offset[1]).clamp(0.0, 1.0),
                );
                for k in 0..3 {
                    total[k] += c[k] / 25.0;
                }
            }
        }
        let gray = total[0] * 0.299 + total[1] * 0.587 + total[2] * 0.114;
        let saturated = [0, 1, 2].map(|c| mix(gray, total[c], 1.5));
        let tinted = mix_rgb(saturated, [0.2, 0.5, 1.0], 0.4 * 0.5);
        assert_near(pix, over_clear(tinted, 0.8), &format!("glass at {lx},{ly}"));
    }

    // A transform and the path's opacity.
    assert_near(
        pixel(&pixels, 32, 160),
        over_clear([0.2, 0.8, 0.4], 0.6),
        "transformed, translucent",
    );
    assert_eq!(
        pixel(&pixels, 12, 160),
        CLEAR,
        "outside the transformed path"
    );
}

#[test]
fn the_cpu_path_uniforms_lay_out_as_the_shader_reads_them() {
    use crate::sdf_russl::path_source::PathUniforms as Shader;
    use std::mem::{offset_of, size_of};
    assert_eq!(size_of::<PathUniforms>(), Shader::SIZE);
    macro_rules! same_offsets {
        ($(($cpu:ident, $shader:ident)),* $(,)?) => {
            $(
                assert_eq!(
                    offset_of!(PathUniforms, $cpu),
                    offset_of!(Shader, $shader),
                    concat!("`", stringify!($cpu), "` sits elsewhere in the shader"),
                );
            )*
        };
    }
    same_offsets!(
        (viewport_size, viewport_size),
        (opacity, opacity),
        (transform, transform_row0),
        (clip_bounds, clip_bounds),
        (clip_radius, clip_radius),
        (clip_type, clip_type),
        (use_gradient_texture, use_gradient_texture),
        (use_image_texture, use_image_texture),
        (use_glass_effect, use_glass_effect),
        (image_uv_bounds, image_uv_bounds),
        (glass_params, glass_params),
        (glass_tint, glass_tint),
    );
    assert_eq!(
        offset_of!(Shader, transform_row2) - offset_of!(Shader, transform_row0),
        2 * size_of::<[f32; 4]>(),
        "the transform rows are not contiguous"
    );
}
