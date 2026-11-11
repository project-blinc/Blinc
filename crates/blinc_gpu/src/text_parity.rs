//! The glyph shader draws what its formulas say, on the storage tier and the data texture tier:
//! the same glyphs give the same pixels on both, and probes match the shader's maths on the CPU.

use crate::parity_harness::{self as harness, AuxData};

const SIZE: u32 = 128;
const GLYPH_ATLAS: u32 = 64;
const COLOR_ATLAS: u32 = 32;
/// The clear colour, as the 8-bit pixel it reads back as.
const CLEAR: [u8; 4] = [64, 128, 191, 255];
const NO_CLIP: [f32; 4] = [-10000.0, -10000.0, 20000.0, 20000.0];

/// One glyph as the shader reads it: six vec4s.
struct Glyph {
    bounds: [f32; 4],
    uv_bounds: [f32; 4],
    color: [f32; 4],
    clip_bounds: [f32; 4],
    clip_fade: [f32; 4],
    flags: [f32; 4],
}

impl Glyph {
    fn new(bounds: [f32; 4], uv_bounds: [f32; 4], color: [f32; 4]) -> Self {
        Self {
            bounds,
            uv_bounds,
            color,
            clip_bounds: NO_CLIP,
            clip_fade: [0.0; 4],
            flags: [0.0; 4],
        }
    }

    fn emoji(mut self) -> Self {
        self.flags[0] = 1.0;
        self
    }

    fn clipped(mut self, bounds: [f32; 4], fade: [f32; 4]) -> Self {
        self.clip_bounds = bounds;
        self.clip_fade = fade;
        self
    }

    /// Clip to the shape whose descriptor starts at `aux[base]`.
    fn shaped(mut self, base: u32) -> Self {
        self.flags[1] = (base + 1) as f32;
        self
    }

    fn floats(&self) -> [f32; 24] {
        let mut out = [0.0; 24];
        for (i, v) in [
            self.bounds,
            self.uv_bounds,
            self.color,
            self.clip_bounds,
            self.clip_fade,
            self.flags,
        ]
        .iter()
        .enumerate()
        {
            out[i * 4..i * 4 + 4].copy_from_slice(v);
        }
        out
    }
}

/// The glyph atlas coverage at texel `(x, y)`.
fn coverage(x: u32, y: u32) -> u8 {
    ((x * 5 + y * 3) % 256) as u8
}

/// The colour atlas texel at `(x, y)`, as sRGB bytes.
fn emoji_texel(x: u32, y: u32) -> [u8; 4] {
    [
        (x * 8) as u8,
        (y * 8) as u8,
        ((x + y) * 4) as u8,
        255 - (x % 8) as u8 * 10,
    ]
}

/// Binding `binding` of the text bind group, visible to `stage`.
fn entry(
    binding: u32,
    stage: wgpu::ShaderStages,
    ty: wgpu::BindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stage,
        ty,
        count: None,
    }
}

/// Draw `glyphs` with `source` and read the pixels back. `data_texture` is the tier without
/// storage buffers: the glyphs and the auxiliary data are `RGBA32F` textures.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    data_texture: bool,
    glyphs: &[Glyph],
    aux: &[[f32; 4]],
) -> Vec<u8> {
    use wgpu::{BindingType as B, ShaderStages as S};
    let float = |filterable| B::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable },
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: false,
    };
    let storage = B::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: None,
    };
    let data = if data_texture { float(false) } else { storage };
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
            entry(1, S::VERTEX, data),
            entry(2, S::FRAGMENT, float(true)),
            entry(
                3,
                S::FRAGMENT,
                B::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
            entry(4, S::FRAGMENT, float(true)),
            entry(5, S::FRAGMENT, AuxData::binding_type(data_texture)),
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let format = wgpu::TextureFormat::Rgba8Unorm;
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
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
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

    let uniforms = harness::buffer(
        device,
        bytemuck::bytes_of(&[SIZE as f32, SIZE as f32, 0.0, 0.0]),
        wgpu::BufferUsages::UNIFORM,
    );
    let glyph_floats: Vec<f32> = glyphs.iter().flat_map(|g| g.floats()).collect();
    let glyph_buffer;
    let glyph_view;
    let glyph_binding = if data_texture {
        glyph_view = harness::texture(
            device,
            queue,
            (6, glyphs.len() as u32),
            wgpu::TextureFormat::Rgba32Float,
            16,
            bytemuck::cast_slice(&glyph_floats),
        );
        wgpu::BindingResource::TextureView(&glyph_view)
    } else {
        glyph_buffer = harness::buffer(
            device,
            bytemuck::cast_slice(&glyph_floats),
            wgpu::BufferUsages::STORAGE,
        );
        glyph_buffer.as_entire_binding()
    };
    let aux = AuxData::new(device, queue, data_texture, aux);
    let atlas_bytes: Vec<u8> = (0..GLYPH_ATLAS)
        .flat_map(|y| (0..GLYPH_ATLAS).map(move |x| coverage(x, y)))
        .collect();
    let atlas = harness::texture(
        device,
        queue,
        (GLYPH_ATLAS, GLYPH_ATLAS),
        wgpu::TextureFormat::R8Unorm,
        1,
        &atlas_bytes,
    );
    let emoji_bytes: Vec<u8> = (0..COLOR_ATLAS)
        .flat_map(|y| (0..COLOR_ATLAS).flat_map(move |x| emoji_texel(x, y)))
        .collect();
    let emoji = harness::texture(
        device,
        queue,
        (COLOR_ATLAS, COLOR_ATLAS),
        wgpu::TextureFormat::Rgba8UnormSrgb,
        4,
        &emoji_bytes,
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
                resource: glyph_binding,
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&atlas),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&emoji),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: aux.resource(),
            },
        ],
    });

    harness::render(device, queue, SIZE, CLEAR, |pass| {
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..6, 0..glyphs.len() as u32);
    })
}

/// Grayscale and colour glyphs, a rectangular clip with fades, and rounded rect and ellipse
/// shape clips.
fn scene() -> (Vec<Glyph>, Vec<[f32; 4]>) {
    let white = [1.0; 4];
    let glyphs = vec![
        // Scaled, so each pixel samples between texels.
        Glyph::new(
            [6.0, 6.0, 40.0, 30.0],
            [4.0, 4.0, 36.0, 28.0],
            [1.0, 0.5, 0.2, 1.0],
        ),
        Glyph::new([60.0, 6.0, 32.0, 32.0], [0.0, 0.0, 32.0, 32.0], white).emoji(),
        // Fades at the top and bottom, and a clip edge between pixels.
        Glyph::new(
            [6.0, 50.0, 50.0, 40.0],
            [0.0, 0.0, 50.0, 40.0],
            [0.2, 0.9, 0.4, 0.8],
        )
        .clipped([14.25, 56.0, 30.0, 28.0], [4.0, 0.0, 6.0, 0.0]),
        Glyph::new(
            [64.0, 50.0, 50.0, 40.0],
            [8.0, 8.0, 58.0, 48.0],
            [0.9, 0.9, 0.2, 1.0],
        )
        .shaped(0),
        Glyph::new([8.0, 96.0, 50.0, 28.0], [0.0, 0.0, 30.0, 16.0], white)
            .emoji()
            .shaped(3),
        // Fades at the right and left.
        Glyph::new(
            [70.0, 100.0, 32.0, 20.0],
            [0.0, 0.0, 32.0, 20.0],
            [1.0, 1.0, 1.0, 0.7],
        )
        .emoji()
        .clipped([74.0, 102.0, 30.0, 20.0], [0.0, 5.0, 0.0, 5.0]),
        // Atlas pixels start at (10, 20), one for one, in a clip that starts left of zero.
        Glyph::new(
            [96.0, 6.0, 28.0, 24.0],
            [10.0, 20.0, 38.0, 44.0],
            [0.3, 0.6, 1.0, 1.0],
        )
        .clipped([-200.0, 10.0, 400.0, 14.0], [0.0; 4]),
    ];
    let aux = vec![
        // A rounded rect: frame, (offset, kind 2, radius), (origin, size).
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 2.0, 8.0],
        [70.0, 56.0, 40.0, 36.0],
        // An ellipse: frame, (offset, kind 1), (centre, radii).
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 0.0],
        [30.0, 108.0, 20.0, 12.0],
    ];
    (glyphs, aux)
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
    let (glyphs, aux) = scene();
    let storage = draw(
        &device,
        &queue,
        crate::sdf_russl::text::WGSL,
        false,
        &glyphs,
        &aux,
    );
    let texture = draw(
        &device,
        &queue,
        crate::sdf_russl::text_dt::WGSL,
        true,
        &glyphs,
        &aux,
    );
    assert!(
        storage.as_chunks::<4>().0.iter().any(|p| *p != CLEAR),
        "nothing was drawn"
    );
    assert!(storage == texture, "the tiers draw different pixels");
}

/// `colour` (0 to 1) at `alpha` over the clear colour, as the 8-bit channel the blend stores.
fn over_clear(colour: f32, alpha: f32, channel: usize) -> f32 {
    (colour * alpha + CLEAR[channel] as f32 / 255.0 * (1.0 - alpha)) * 255.0
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn assert_near(pixel: [u8; 4], expected: [f32; 3], what: &str) {
    for c in 0..3 {
        assert!(
            (pixel[c] as f32 - expected[c]).abs() <= 2.0,
            "{what}: channel {c} is {} and should be {:.1}",
            pixel[c],
            expected[c]
        );
    }
}

/// The pixel of a grayscale glyph at `at` whose atlas texel is `texel`: the tint at
/// `coverage ^ 0.7` times the glyph's alpha and the clip's.
fn assert_gray(pixels: &[u8], at: (u32, u32), texel: (u32, u32), tint: [f32; 4], clip: f32) {
    let a = (coverage(texel.0, texel.1) as f32 / 255.0).powf(0.7) * tint[3] * clip;
    assert_near(
        pixel(pixels, at.0, at.1),
        [0, 1, 2].map(|c| over_clear(tint[c], a, c)),
        &format!("grayscale glyph at {at:?}"),
    );
}

/// The pixel of a colour glyph at `at` whose atlas texel is `texel`: the sRGB texel decoded, at its
/// own alpha times the clip's.
fn assert_emoji(pixels: &[u8], at: (u32, u32), texel: (u32, u32), clip: f32) {
    let t = emoji_texel(texel.0, texel.1);
    let a = t[3] as f32 / 255.0 * clip;
    assert_near(
        pixel(pixels, at.0, at.1),
        [0, 1, 2].map(|c| over_clear(srgb_to_linear(t[c] as f32 / 255.0), a, c)),
        &format!("emoji glyph at {at:?}"),
    );
}

/// Glyphs whose atlas pixels land on texel centres, so the sample is the texel itself and the
/// formulas can be evaluated on the CPU.
#[test]
fn glyphs_draw_the_atlas_texel_through_the_formulas() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (glyphs, aux) = scene();
    let pixels = draw(
        &device,
        &queue,
        crate::sdf_russl::text::WGSL,
        false,
        &glyphs,
        &aux,
    );

    assert_gray(&pixels, (100, 14), (14, 28), [0.3, 0.6, 1.0, 1.0], 1.0);
    assert_eq!(pixel(&pixels, 100, 8), CLEAR, "above the clip");
    assert_emoji(&pixels, (70, 18), (10, 12), 1.0);

    // Top and bottom fades, and a clip edge a quarter of a pixel into its pixel.
    let tint = [0.2, 0.9, 0.4, 0.8];
    assert_gray(&pixels, (20, 57), (14, 7), tint, 0.375);
    assert_gray(&pixels, (25, 82), (19, 32), tint, 0.25);
    assert_gray(&pixels, (14, 70), (8, 20), tint, 0.75);
    // Right and left fades of a colour glyph.
    assert_emoji(&pixels, (100, 110), (30, 10), 0.7);
    assert_emoji(&pixels, (76, 110), (6, 10), 0.5);

    // A rounded rect shape clip: a pixel inside its corner arc is clipped, one well inside is not.
    assert_eq!(pixel(&pixels, 70, 56), CLEAR, "outside the rounded corner");
    assert_ne!(pixel(&pixels, 90, 70), CLEAR, "inside the rounded rect");
    // An ellipse shape clip: the corner of the glyph's bounds is outside it.
    assert_eq!(pixel(&pixels, 9, 97), CLEAR, "outside the ellipse");
    assert_ne!(pixel(&pixels, 30, 108), CLEAR, "inside the ellipse");
}
