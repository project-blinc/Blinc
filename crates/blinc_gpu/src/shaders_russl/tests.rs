//! Each shader draws what its formulas say: known inputs go through it and the pixels are compared
//! with the same maths on the CPU.

use super::harness::{
    CLEARED, Input, POINTS, Pass, SIZE, aces, floats, gradient_bytes, is, need_device, pixel,
    render, texel, texture,
};
use wgpu::util::DeviceExt;

#[test]
fn clear_quad_clears_to_opaque_black() {
    let (device, queue) = need_device!();
    let pass = Pass {
        vertex: "vs_main",
        fragment: "fs_main",
        inputs: &[],
        vertices: 3,
    };
    let pixels = render(&device, &queue, super::clear_quad::WGSL, &pass);
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 255])
    );
}

#[test]
fn skybox_draws_its_gradient() {
    let (device, queue) = need_device!();
    let pass = Pass {
        vertex: "vs_main",
        fragment: "fs_main",
        inputs: &[],
        vertices: 3,
    };
    let pixels = render(&device, &queue, super::skybox::WGSL, &pass);
    for (x, y) in POINTS {
        let (u, v) = ((x as f32 + 0.5) / 64.0, (y as f32 + 0.5) / 64.0);
        let vert = 1.0 - 4.0 * (v - 0.5) * (v - 0.5);
        let horiz = 1.0 - 2.0 * (u - 0.5) * (u - 0.5);
        let b = 0.08 + 0.12 * vert * horiz;
        is(pixel(&pixels, x, y), [b * 0.9, b * 0.92, b, 1.0], "skybox");
    }
}

#[test]
fn tonemap_maps_the_hdr_and_adds_the_bloom() {
    let (device, queue) = need_device!();
    let (hdr_data, bloom_data) = (gradient_bytes(1), gradient_bytes(5));
    let hdr = texture(
        &device,
        &queue,
        SIZE,
        SIZE,
        wgpu::TextureFormat::Rgba8Unorm,
        &hdr_data,
    );
    let bloom = texture(
        &device,
        &queue,
        SIZE,
        SIZE,
        wgpu::TextureFormat::Rgba8Unorm,
        &bloom_data,
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let pass = Pass {
        vertex: "vs_main",
        fragment: "fs_main",
        inputs: &[
            (0, Input::Texture(&hdr)),
            (1, Input::Sampler(&sampler)),
            (2, Input::Texture(&bloom)),
        ],
        vertices: 3,
    };
    let pixels = render(&device, &queue, super::tonemap::WGSL, &pass);
    for (x, y) in POINTS {
        let (h, b) = (
            texel(&hdr_data, x as i32, y as i32),
            texel(&bloom_data, x as i32, y as i32),
        );
        let want = [0, 1, 2].map(|c| aces(h[c] + b[c] * 0.4));
        is(
            pixel(&pixels, x, y),
            [want[0], want[1], want[2], 1.0],
            "tonemap",
        );
    }
}

#[test]
fn bloom_extracts_bright_pixels_and_blurs() {
    let (device, queue) = need_device!();
    let data = gradient_bytes(3);
    let source = texture(
        &device,
        &queue,
        SIZE,
        SIZE,
        wgpu::TextureFormat::Rgba8Unorm,
        &data,
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let threshold = 0.35;
    for mode in [0.0, 1.0] {
        let uniforms = floats(&[1.0 / SIZE as f32, 1.0 / SIZE as f32, threshold, mode]);
        let pass = Pass {
            vertex: "vs_main",
            fragment: "fs_main",
            inputs: &[
                (0, Input::Uniform(&uniforms)),
                (1, Input::Texture(&source)),
                (2, Input::Sampler(&sampler)),
            ],
            vertices: 3,
        };
        let pixels = render(&device, &queue, super::bloom::WGSL, &pass);
        for (x, y) in POINTS {
            let (x, y) = (x as i32, y as i32);
            let c = texel(&data, x, y);
            let want = if mode < 0.5 {
                let brightness = c[0].max(c[1]).max(c[2]);
                let contribution = (brightness - threshold).clamp(0.0, 1.0) / brightness.max(0.001);
                [
                    c[0] * contribution,
                    c[1] * contribution,
                    c[2] * contribution,
                ]
            } else {
                let taps = [(-1, -1), (1, -1), (-1, 1), (1, 1)]
                    .map(|(dx, dy)| texel(&data, x + dx, y + dy));
                [0, 1, 2].map(|k| (c[k] * 4.0 + taps.iter().map(|t| t[k]).sum::<f32>()) / 8.0)
            };
            is(
                pixel(&pixels, x as u32, y as u32),
                [want[0], want[1], want[2], 1.0],
                if mode < 0.5 {
                    "bloom threshold"
                } else {
                    "bloom blur"
                },
            );
        }
    }
}

#[test]
fn oit_composite_skips_uncovered_pixels_and_averages_the_rest() {
    let (device, queue) = need_device!();
    // An accumulated alpha below one, so the average has to divide it out.
    let mut accum_data = gradient_bytes(2);
    for texel in accum_data.chunks_exact_mut(4) {
        texel[3] = 200;
    }
    let accum = texture(
        &device,
        &queue,
        SIZE,
        SIZE,
        wgpu::TextureFormat::Rgba8Unorm,
        &accum_data,
    );
    // Reveal is 1 (nothing covers the pixel) in the left half and falls off to the right.
    let reveal_at = |x: u32| -> u8 {
        if x < SIZE / 2 {
            255
        } else {
            (255 - (x - SIZE / 2) * 7) as u8
        }
    };
    let mut bytes = Vec::new();
    for _y in 0..SIZE {
        for x in 0..SIZE {
            bytes.extend([reveal_at(x), 0, 0, 255]);
        }
    }
    let reveal = texture(
        &device,
        &queue,
        SIZE,
        SIZE,
        wgpu::TextureFormat::Rgba8Unorm,
        &bytes,
    );
    let sampler = device.create_sampler(&Default::default());
    let pass = Pass {
        vertex: "vs_main",
        fragment: "fs_main",
        inputs: &[
            (0, Input::Texture(&accum)),
            (1, Input::Texture(&reveal)),
            (2, Input::Sampler(&sampler)),
        ],
        vertices: 3,
    };
    let pixels = render(&device, &queue, super::oit_composite::WGSL, &pass);
    // Two pixels with a little coverage, which still composite, besides the usual probes.
    let probes = POINTS.into_iter().chain([(33, 10), (40, 20)]);
    for (x, y) in probes {
        let got = pixel(&pixels, x, y);
        if x < SIZE / 2 {
            assert_eq!(got, CLEARED, "an uncovered pixel is left alone");
        } else {
            let a = texel(&accum_data, x as i32, y as i32);
            let coverage = 1.0 - reveal_at(x) as f32 / 255.0;
            is(
                got,
                [a[0] / a[3], a[1] / a[3], a[2] / a[3], coverage],
                "oit",
            );
        }
    }
}

#[test]
fn backdrop_rows_blur_by_their_taps_and_copy_raw() {
    let (device, queue) = need_device!();
    let data = gradient_bytes(4);
    let source = texture(
        &device,
        &queue,
        SIZE,
        SIZE,
        wgpu::TextureFormat::Rgba8Unorm,
        &data,
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // Three taps (offset in pixels, weight) in the second row of the auxiliary texture, past its
    // middle, where the shader starts reading at `params.taps.x`.
    let taps: [[f32; 2]; 3] = [[-3.0, 0.25], [0.0, 0.5], [3.0, 0.25]];
    let mut aux = vec![[0.0f32; 4]; 2048];
    for (i, tap) in taps.iter().enumerate() {
        aux[1600 + i] = [tap[0], tap[1], 0.0, 0.0];
    }
    let aux = texture(
        &device,
        &queue,
        1024,
        2,
        wgpu::TextureFormat::Rgba32Float,
        bytemuck::cast_slice(&aux),
    );
    // Params: screen, source (z is the tap count), taps (x is where they start), then the record.
    let mut params = vec![0.0f32; 4 * (3 + 28)];
    params[4..8].copy_from_slice(&[SIZE as f32, SIZE as f32, 3.0, 0.0]);
    params[8..12].copy_from_slice(&[1600.0, 0.0, 0.0, 0.0]);
    let params = floats(&params);
    let rows = Pass {
        vertex: "vs_main",
        fragment: "fs_rows",
        inputs: &[
            (0, Input::Uniform(&params)),
            (1, Input::Texture(&source)),
            (2, Input::Sampler(&sampler)),
            (3, Input::Texture(&aux)),
        ],
        vertices: 6,
    };
    let blurred = render(&device, &queue, super::backdrop_rows::WGSL, &rows);
    let copy = Pass {
        vertex: "vs_main",
        fragment: "fs_copy",
        inputs: &[(1, Input::Texture(&source))],
        vertices: 6,
    };
    let copied = render(&device, &queue, super::backdrop_rows::WGSL, &copy);
    for (x, y) in POINTS {
        let want: Vec<f32> = (0..4)
            .map(|c| {
                taps.iter()
                    .map(|t| texel(&data, x as i32 + t[0] as i32, y as i32)[c] * t[1])
                    .sum()
            })
            .collect();
        is(
            pixel(&blurred, x, y),
            [want[0], want[1], want[2], want[3]],
            "rows",
        );
        is(
            pixel(&copied, x, y),
            texel(&data, x as i32, y as i32),
            "copy",
        );
    }
}

#[test]
fn shadow_depth_writes_the_depth_of_the_transformed_triangle() {
    let (device, queue) = need_device!();
    // Column-major matrices: a model that moves right a little, a light that halves depth.
    #[rustfmt::skip]
    let uniforms = floats(&[
        1.0, 0.0, 0.0, 0.0,  0.0, 1.0, 0.0, 0.0,  0.0, 0.0, 0.5, 0.0,  0.0, 0.0, 0.25, 1.0,
        1.0, 0.0, 0.0, 0.0,  0.0, 1.0, 0.0, 0.0,  0.0, 0.0, 1.0, 0.0,  0.1, 0.0, 0.0, 1.0,
    ]);
    let triangle = [[-0.6f32, -0.6, 0.2], [0.7, -0.5, 0.6], [0.0, 0.7, 0.9]];
    let vertices = floats(&triangle.concat());
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(super::shadow_depth::WGSL.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 12,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                }],
            }],
            compilation_options: Default::default(),
        },
        fragment: None,
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &uniforms,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &vertices,
        usage: wgpu::BufferUsages::VERTEX,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (SIZE * SIZE * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let view = target.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 4),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    device
        .poll(wgpu::PollType::Wait)
        .expect("the copy finishes");
    let depth = |x: u32, y: u32| -> f32 {
        let at = ((y * SIZE + x) * 4) as usize;
        f32::from_le_bytes(slice.get_mapped_range()[at..at + 4].try_into().unwrap())
    };
    // The model moves x by 0.1 and the light maps z to z / 2 + 0.25. Depth is linear across the
    // triangle, so a pixel's depth is the barycentric blend of the three corners' depths.
    let corner = |v: [f32; 3]| [v[0] + 0.1, v[1], v[2] * 0.5 + 0.25];
    let [a, b, c] = triangle.map(corner);
    let expected = |x: u32, y: u32| -> Option<f32> {
        let (px, py) = ((x as f32 + 0.5) / 32.0 - 1.0, 1.0 - (y as f32 + 0.5) / 32.0);
        let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        let l1 = ((b[1] - c[1]) * (px - c[0]) + (c[0] - b[0]) * (py - c[1])) / det;
        let l2 = ((c[1] - a[1]) * (px - c[0]) + (a[0] - c[0]) * (py - c[1])) / det;
        let l3 = 1.0 - l1 - l2;
        (l1 > 0.05 && l2 > 0.05 && l3 > 0.05).then(|| l1 * a[2] + l2 * b[2] + l3 * c[2])
    };
    let mut inside = 0;
    for y in (0..SIZE).step_by(3) {
        for x in (0..SIZE).step_by(3) {
            match expected(x, y) {
                Some(want) => {
                    inside += 1;
                    assert!((depth(x, y) - want).abs() < 2e-4, "depth at ({x}, {y})");
                }
                None if x < 2 || y < 2 => assert_eq!(depth(x, y), 1.0, "outside at ({x}, {y})"),
                None => {}
            }
        }
    }
    assert!(
        inside > 40,
        "the triangle covers few probe pixels: {inside}"
    );
}
