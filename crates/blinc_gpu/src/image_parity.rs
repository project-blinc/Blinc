//! The image shader draws what its formulas say, on the storage tier and the data texture tier:
//! the same instances give the same pixels on both, and probes match the shader's maths on the CPU.

use crate::image::GpuImageInstance;
use crate::parity_harness::{self as harness, AuxData};

const SIZE: u32 = 128;
const TEXTURE: u32 = 16;
/// The clear colour, as the 8-bit pixel it reads back as.
const CLEAR: [u8; 4] = [64, 128, 191, 255];

/// The image's texel at `(x, y)`: every channel varies, and the alpha dips every fourth column.
fn texel(x: u32, y: u32) -> [u8; 4] {
    [
        (x * 16) as u8,
        (y * 16) as u8,
        ((x + y) * 8) as u8,
        255 - (x % 4) as u8 * 40,
    ]
}

/// The vertex attributes of an instance: thirteen vec4s.
fn attributes() -> Vec<wgpu::VertexAttribute> {
    (0..13)
        .map(|i| wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: i as u64 * 16,
            shader_location: i,
        })
        .collect()
}

/// Draw `instances` with `source` and read the pixels back. `data_texture` is the tier without
/// storage buffers: the auxiliary data is an `RGBA32F` texture.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    data_texture: bool,
    instances: &[GpuImageInstance],
    aux: &[[f32; 4]],
) -> Vec<u8> {
    use wgpu::{BindingType as B, ShaderStages as S};
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
                S::VERTEX,
                B::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
            entry(
                1,
                S::FRAGMENT,
                B::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            entry(
                2,
                S::FRAGMENT,
                B::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
            entry(3, S::FRAGMENT, AuxData::binding_type(data_texture)),
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let attributes = attributes();
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
                array_stride: std::mem::size_of::<GpuImageInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &attributes,
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
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
    let instance_buffer = harness::buffer(
        device,
        bytemuck::cast_slice(instances),
        wgpu::BufferUsages::VERTEX,
    );
    let aux = AuxData::new(device, queue, data_texture, aux);
    let image_bytes: Vec<u8> = (0..TEXTURE)
        .flat_map(|y| (0..TEXTURE).flat_map(move |x| texel(x, y)))
        .collect();
    let image = harness::texture(
        device,
        queue,
        (TEXTURE, TEXTURE),
        wgpu::TextureFormat::Rgba8Unorm,
        4,
        &image_bytes,
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
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
                resource: wgpu::BindingResource::TextureView(&image),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: aux.resource(),
            },
        ],
    });

    harness::render(device, queue, SIZE, CLEAR, |pass| {
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.set_vertex_buffer(0, instance_buffer.slice(..));
        pass.draw(0..6, 0..instances.len() as u32);
    })
}

/// Instances that cover each feature of the shader.
fn scene() -> (Vec<GpuImageInstance>, Vec<[f32; 4]>) {
    let mut abi = GpuImageInstance::new(4.0, 92.0, 40.0, 30.0)
        .with_clip_rounded_rect(6.0, 94.0, 36.0, 26.0, 10.0);
    // A clip header at row 0: local clip, its radii, bounds, fades and the clip's own info,
    // which points at a rounded rect shape clip at row 8.
    abi.abi_clip = [1.0, 0.0, 0.0, 0.0];
    // An alpha mask with a clip header at row 16, in a wide clip whose corner is on the image.
    let mut mask_only = GpuImageInstance::new(52.0, 92.0, 24.0, 24.0)
        .with_tint(1.0, 0.4, 0.1, 0.9)
        .with_clip_rounded_rect(-100.0, 92.0, 168.0, 20.0, 8.0);
    mask_only.abi_clip = [17.0, 1.0, 0.0, 0.0];
    // A mapping header at row 12 repeating in both directions, one at row 14 repeating in x.
    let mut mapped = GpuImageInstance::new(80.0, 92.0, 20.0, 24.0);
    mapped.abi_clip = [0.0, 0.0, 13.0, 0.0];
    let mut mapped_x =
        GpuImageInstance::new(104.0, 92.0, 20.0, 24.0).with_clip_rect(-100.0, 96.0, 400.0, 14.0);
    mapped_x.abi_clip = [0.0, 0.0, 15.0, 0.0];
    let mut linear = GpuImageInstance::new(60.0, 44.0, 30.0, 30.0);
    linear.mask_info = [1.0, 1.0, 0.2, 0.0];
    linear.mask_params = [0.0, 0.0, 1.0, 1.0];
    let mut radial = GpuImageInstance::new(96.0, 44.0, 24.0, 24.0);
    radial.mask_info = [2.0, 1.0, 0.0, 0.0];
    radial.mask_params = [0.5, 0.5, 0.5, 0.0];
    let instances = vec![
        GpuImageInstance::new(4.0, 4.0, 40.0, 30.0).with_border_radius(6.0),
        GpuImageInstance::new(52.0, 4.0, 40.0, 30.0)
            .with_filter([0.5, 0.2, 0.3, 0.7], [1.2, 1.1, 0.8, 0.0]),
        GpuImageInstance::new(96.0, 10.0, 24.0, 24.0)
            .with_src_uv(0.25, 0.25, 0.75, 0.75)
            .with_transform(0.866, 0.5, -0.5, 0.866),
        // A tint, opacity, rounded corners and a border.
        GpuImageInstance::new(4.0, 44.0, 50.0, 40.0)
            .with_tint(0.9, 0.8, 1.0, 1.0)
            .with_opacity(0.7)
            .with_border_radius(10.0)
            .with_image_border(3.0, 0.25, 0.5, 0.0, 1.0)
            .with_clip_rounded_rect_corners(8.0, 46.0, 44.0, 36.0, 8.0, 2.0, 8.0, 2.0)
            .with_clip2_rect(6.0, 46.0, 40.0, 30.0),
        linear,
        radial,
        abi,
        mask_only,
        mapped,
        mapped_x,
    ];
    let aux = vec![
        // 0: local clip (x, y, width, height).
        [2.0, 2.0, 30.0, 26.0],
        // 1: its corner radii.
        [6.0, 6.0, 6.0, 6.0],
        // 2: fade bounds (x, y, width, height).
        [4.0, 92.0, 30.0, 30.0],
        // 3: fades (top, right, bottom, left).
        [4.0, 5.0, 6.0, 7.0],
        // 4: (corner shape, local edge width, corner shapes row pointer, shape clip pointer).
        [4.0, 1.0, 6.0, 9.0],
        // 5: the clip's corner shape at each corner.
        [2.0, 1.0, 0.5, 3.0],
        [0.0; 4],
        [0.0; 4],
        // 8-10: a rounded rect shape clip: frame, (offset, kind 2, radius), (origin, size).
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 2.0, 5.0],
        [0.0, 88.0, 40.0, 30.0],
        [0.0; 4],
        // 12-13: mapping (origin, size), then (repeat flags, edge width): repeat both.
        [2.4, 2.4, 8.0, 8.0],
        [3.0, 1.0, 0.0, 0.0],
        // 14-15: repeat in x only.
        [2.4, 2.0, 8.0, 8.0],
        [1.0, 1.0, 0.0, 0.0],
        // 16-20: a clip header with no local clip, no fades and the corner shape 3.
        [0.0; 4],
        [0.0; 4],
        [52.0, 92.0, 24.0, 24.0],
        [0.0; 4],
        [3.0, 1.0, 0.0, 0.0],
    ];
    (instances, aux)
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    harness::pixel(pixels, SIZE, x, y)
}

#[test]
fn the_data_texture_tier_draws_the_pixels_the_storage_tier_draws() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (instances, aux) = scene();
    let storage = draw(
        &device,
        &queue,
        crate::sdf_russl::image::WGSL,
        false,
        &instances,
        &aux,
    );
    let texture = draw(
        &device,
        &queue,
        crate::sdf_russl::image_dt::WGSL,
        true,
        &instances,
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

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The channel `colour` at `alpha`, premultiplied, leaves over the clear colour, in 8-bit levels.
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

fn texel_rgba(x: u32, y: u32) -> (Rgb, f32) {
    let t = texel(x, y);
    ([0, 1, 2].map(|c| t[c] as f32 / 255.0), t[3] as f32 / 255.0)
}

/// The image sampled bilinearly at `(u, v)` with clamp-to-edge, as the shader's sampler does.
fn sample(u: f32, v: f32) -> (Rgb, f32) {
    let (fx, fy) = (u * TEXTURE as f32 - 0.5, v * TEXTURE as f32 - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (wx, wy) = (fx - x0, fy - y0);
    let at = |x: f32, y: f32| {
        let max = (TEXTURE - 1) as f32;
        texel_rgba(x.clamp(0.0, max) as u32, y.clamp(0.0, max) as u32)
    };
    let mut rgb = [0.0; 3];
    let mut alpha = 0.0;
    for (dx, dy, w) in [
        (0.0, 0.0, (1.0 - wx) * (1.0 - wy)),
        (1.0, 0.0, wx * (1.0 - wy)),
        (0.0, 1.0, (1.0 - wx) * wy),
        (1.0, 1.0, wx * wy),
    ] {
        let (c, a) = at(x0 + dx, y0 + dy);
        for k in 0..3 {
            rgb[k] += c[k] * w;
        }
        alpha += a * w;
    }
    (rgb, alpha)
}

fn dot3(a: Rgb, b: Rgb) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The shader's CSS filters in the shader's order.
fn filter(mut rgb: Rgb, fa: [f32; 4], fb: [f32; 4]) -> Rgb {
    let w = [0.2126, 0.7152, 0.0722];
    if fa[0] > 0.0 {
        let lum = dot3(rgb, w);
        rgb = rgb.map(|c| mix(c, lum, fa[0]));
    }
    if fa[2] > 0.0 {
        let sepia = [
            dot3(rgb, [0.393, 0.769, 0.189]),
            dot3(rgb, [0.349, 0.686, 0.168]),
            dot3(rgb, [0.272, 0.534, 0.131]),
        ];
        rgb = [0, 1, 2].map(|c| mix(rgb[c], sepia[c], fa[2]));
    }
    if fa[1] > 0.0 {
        rgb = rgb.map(|c| mix(c, 1.0 - c, fa[1]));
    }
    if fa[3].abs() > 0.001 {
        let (cos_h, sin_h) = (fa[3].cos(), fa[3].sin());
        let r = [
            cos_h + (1.0 - cos_h) * w[0],
            (1.0 - cos_h) * w[0] * w[1] - sin_h * w[2],
            (1.0 - cos_h) * w[0] * w[2] + sin_h * w[1],
        ];
        let g = [
            (1.0 - cos_h) * w[0] * w[1] + sin_h * w[2],
            cos_h + (1.0 - cos_h) * w[1],
            (1.0 - cos_h) * w[1] * w[2] - sin_h * w[0],
        ];
        let b = [
            (1.0 - cos_h) * w[0] * w[2] - sin_h * w[1],
            (1.0 - cos_h) * w[1] * w[2] + sin_h * w[0],
            cos_h + (1.0 - cos_h) * w[2],
        ];
        rgb = [dot3(rgb, r), dot3(rgb, g), dot3(rgb, b)];
    }
    rgb = rgb.map(|c| c * fb[0]);
    rgb = rgb.map(|c| (c - 0.5) * fb[1] + 0.5);
    if (fb[2] - 1.0).abs() > 0.001 {
        let lum = dot3(rgb, w);
        rgb = rgb.map(|c| mix(lum, c, fb[2]));
    }
    rgb.map(|c| c.clamp(0.0, 1.0))
}

/// Signed distance to a rounded rect with the same radius at every corner.
fn rounded_rect_sdf(p: [f32; 2], origin: [f32; 2], size: [f32; 2], radius: f32) -> f32 {
    let half = [size[0] * 0.5, size[1] * 0.5];
    let rel = [p[0] - origin[0] - half[0], p[1] - origin[1] - half[1]];
    let r = radius.min(half[0].min(half[1]));
    let q = [rel[0].abs() - half[0] + r, rel[1].abs() - half[1] + r];
    q[0].max(q[1]).min(0.0) + q[0].max(0.0).hypot(q[1].max(0.0)) - r
}

/// One 16 by 16 instance per feature, drawn one for one so each pixel samples a texel centre.
fn probe_scene() -> (Vec<GpuImageInstance>, Vec<[f32; 4]>) {
    let at = |x: f32, y: f32| GpuImageInstance::new(x, y, 16.0, 16.0);
    let mut linear = at(96.0, 0.0);
    linear.mask_info = [1.0, 1.0, 0.0, 0.0];
    linear.mask_params = [0.0, 0.0, 1.0, 0.0];
    let mut radial = at(0.0, 24.0);
    radial.mask_info = [2.0, 1.0, 0.0, 0.0];
    radial.mask_params = [0.5, 0.5, 0.5, 0.0];
    let mut alpha_mask = at(24.0, 24.0).with_tint(1.0, 0.4, 0.1, 0.9);
    alpha_mask.abi_clip = [0.0, 1.0, 0.0, 0.0];
    let mut mapped = at(96.0, 24.0);
    mapped.abi_clip = [0.0, 0.0, 1.0, 0.0];
    let mut faded = at(0.0, 48.0);
    faded.abi_clip = [3.0, 0.0, 0.0, 0.0];
    let instances = vec![
        at(0.0, 0.0),
        at(24.0, 0.0)
            .with_tint(0.5, 1.0, 0.25, 0.8)
            .with_opacity(0.5),
        at(48.0, 0.0).with_filter([0.5, 0.25, 0.3, 0.6], [1.2, 1.1, 0.7, 0.0]),
        at(72.0, 0.0)
            .with_border_radius(6.0)
            .with_image_border(2.0, 0.25, 0.5, 0.0, 1.0),
        linear,
        radial,
        alpha_mask,
        at(48.0, 24.0).with_clip2_rect(62.0, 28.0, 8.0, 8.0),
        at(72.0, 24.0).with_clip_rounded_rect(74.0, 26.0, 12.0, 12.0, 5.0),
        mapped,
        faded,
        // Turned a quarter about its centre, and flipped.
        at(24.0, 48.0).with_transform(0.0, 1.0, -1.0, 0.0),
        at(48.0, 48.0).with_transform(1.0, 0.0, 0.0, -1.0),
        // Sepia alone, so its matrix shows undiluted.
        at(72.0, 48.0).with_filter([0.0, 0.0, 1.0, 0.0], [1.0, 1.0, 1.0, 0.0]),
    ];
    let aux = vec![
        // 0-1: a mapping (origin, size), then (repeat both ways, edge width).
        [0.0, 0.0, 8.0, 8.0],
        [3.0, 1.0, 0.0, 0.0],
        // 2-6: a clip header at row 2: no local clip, radii, fade bounds, fades, info.
        [0.0; 4],
        [0.0; 4],
        [0.0, 48.0, 16.0, 16.0],
        [4.0, 5.0, 6.0, 7.0],
        [1.0, 1.0, 0.0, 0.0],
    ];
    (instances, aux)
}

/// The pixel at local `(lx, ly)` of the probe instance at `(x, y)`.
fn probe(pixels: &[u8], x: u32, y: u32, lx: u32, ly: u32) -> [u8; 4] {
    pixel(pixels, x + lx, y + ly)
}

#[test]
fn images_draw_the_texel_through_the_formulas() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (instances, aux) = probe_scene();
    let pixels = draw(
        &device,
        &queue,
        crate::sdf_russl::image::WGSL,
        false,
        &instances,
        &aux,
    );
    let (rgb, a) = texel_rgba(5, 7);
    assert_near(probe(&pixels, 0, 0, 5, 7), over_clear(rgb, a), "plain");

    // Tint, then opacity.
    let (rgb, a) = texel_rgba(9, 3);
    assert_near(
        probe(&pixels, 24, 0, 9, 3),
        over_clear([rgb[0] * 0.5, rgb[1], rgb[2] * 0.25], a * 0.8 * 0.5),
        "tint and opacity",
    );

    // Filters.
    for (lx, ly) in [(4, 5), (14, 12)] {
        let (rgb, a) = texel_rgba(lx, ly);
        let rgb = filter(rgb, [0.5, 0.25, 0.3, 0.6], [1.2, 1.1, 0.7, 0.0]);
        assert_near(probe(&pixels, 48, 0, lx, ly), over_clear(rgb, a), "filters");
    }

    // Sepia: each output channel is a weighted sum of the three input channels. The first texel
    // keeps every channel below one; the second saturates the red.
    for (lx, ly) in [(6, 4), (14, 12)] {
        let (rgb, a) = texel_rgba(lx, ly);
        assert_near(
            probe(&pixels, 72, 48, lx, ly),
            over_clear(
                [
                    dot3(rgb, [0.393, 0.769, 0.189]),
                    dot3(rgb, [0.349, 0.686, 0.168]),
                    dot3(rgb, [0.272, 0.534, 0.131]),
                ]
                .map(|c| c.clamp(0.0, 1.0)),
                a,
            ),
            "sepia",
        );
    }

    // A rounded corner, and a border that mixes in from the inside.
    let border = [64.0 / 255.0, 128.0 / 255.0, 0.0];
    for (lx, ly) in [(8, 1), (8, 8), (1, 1), (0, 8)] {
        let sdf = rounded_rect_sdf([lx as f32 + 0.5, ly as f32 + 0.5], [0.0; 2], [16.0; 2], 6.0);
        let outer = 1.0 - smoothstep(-1.0, 1.0, sdf);
        let factor = smoothstep(-2.0 - 1.0, -2.0 + 1.0, sdf);
        let (rgb, a) = texel_rgba(lx, ly);
        let rgb = [0, 1, 2].map(|c| mix(rgb[c], border[c], factor));
        let a = mix(a, 1.0, factor) * outer;
        assert_near(
            probe(&pixels, 72, 0, lx, ly),
            over_clear(rgb, a),
            &format!("border at {lx},{ly}"),
        );
    }

    // Mask gradients scale colour and alpha.
    for lx in [10, 15] {
        let (rgb, a) = texel_rgba(lx, 6);
        let m = mix(1.0, 0.0, ((lx as f32 + 0.5) / 16.0).clamp(0.0, 1.0));
        assert_near(
            probe(&pixels, 96, 0, lx, 6),
            over_clear(rgb.map(|c| c * m), a * m),
            "linear mask",
        );
    }
    let (rgb, a) = texel_rgba(12, 4);
    let t = ((12.5_f32 / 16.0 - 0.5).hypot(4.5 / 16.0 - 0.5) / 0.5).clamp(0.0, 1.0);
    let m = mix(1.0, 0.0, t);
    assert_near(
        probe(&pixels, 0, 24, 12, 4),
        over_clear(rgb.map(|c| c * m), a * m),
        "radial mask",
    );

    // An alpha mask takes the tint's colour and the texel's alpha.
    let (_, a) = texel_rgba(6, 6);
    assert_near(
        probe(&pixels, 24, 24, 6, 6),
        over_clear([1.0, 0.4, 0.1], a * 0.9),
        "alpha mask",
    );

    // The second clip is a sharp rect: outside it nothing is drawn, and its edge is soft.
    assert_eq!(pixel(&pixels, 55, 31), CLEAR, "outside the second clip");
    let (rgb, a) = texel_rgba(15, 7);
    assert_near(
        pixel(&pixels, 63, 31),
        over_clear(rgb, a),
        "inside the second clip",
    );
    let d = rounded_rect_sdf([62.5, 31.5], [62.0, 28.0], [8.0; 2], 0.0);
    let (rgb, a) = texel_rgba(14, 7);
    assert_near(
        pixel(&pixels, 62, 31),
        over_clear(rgb, a * (1.0 - smoothstep(-0.75, 0.75, d))),
        "the second clip's edge",
    );

    // A rounded rect clip's corner.
    for (lx, ly) in [(2, 2), (3, 3), (8, 8), (2, 6)] {
        let d = rounded_rect_sdf(
            [72.0 + lx as f32 + 0.5, 24.0 + ly as f32 + 0.5],
            [74.0, 26.0],
            [12.0; 2],
            5.0,
        );
        let clip = 1.0 - smoothstep(-0.75, 0.75, d);
        let (rgb, a) = texel_rgba(lx, ly);
        assert_near(
            probe(&pixels, 72, 24, lx, ly),
            over_clear(rgb, a * clip),
            "rounded clip",
        );
    }

    // An ABI mapping repeating twice across the image: a sample between four texels.
    let (rgb, a) = sample((11.5_f32 / 8.0).fract(), (13.5_f32 / 8.0).fract());
    assert_near(
        probe(&pixels, 96, 24, 11, 13),
        over_clear(rgb, a),
        "repeating mapping",
    );

    // ABI fades from each edge, one at a time.
    for (lx, ly, fade) in [
        (8, 1, 1.5 / 4.0),
        (14, 8, 1.5 / 5.0),
        (8, 14, 1.5 / 6.0),
        (1, 8, 1.5 / 7.0),
    ] {
        let (rgb, a) = texel_rgba(lx, ly);
        assert_near(
            probe(&pixels, 0, 48, lx, ly),
            over_clear(rgb, a * fade),
            &format!("fade at {lx},{ly}"),
        );
    }

    // A flip shows the texel at (x, 15 - y).
    let (rgb, a) = texel_rgba(3, 10);
    assert_near(probe(&pixels, 48, 48, 3, 5), over_clear(rgb, a), "flip");

    // A quarter turn about the centre: the pixel at (x, y) shows the texel at (y, 15 - x).
    let (rgb, a) = texel_rgba(5, 12);
    assert_near(
        probe(&pixels, 24, 48, 3, 5),
        over_clear(rgb, a),
        "quarter turn",
    );
}
