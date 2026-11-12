//! The backdrop composite draws what its formulas say: probes match the shader's maths on the CPU
//! for the frosted path, and pinned values hold the liquid glass shading.

use crate::parity_harness::{self as harness, AuxData};

const SIZE: u32 = 160;
/// The clear colour.
const CLEAR: [u8; 4] = [64, 128, 191, 255];

/// The uniforms as the shader reads them: three vec4s, then the ABI record rows.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    screen: [f32; 4],
    source: [f32; 4],
    taps: [f32; 4],
    record: [[f32; 4]; 28],
}

/// The record rows, by what they hold.
mod row {
    pub const BOUNDS: usize = 0;
    pub const RADII: usize = 1;
    pub const FINISH: usize = 2;
    pub const MATRIX: usize = 3;
    pub const LOCAL_CLIP: usize = 6;
    pub const LOCAL_CLIP_RADII: usize = 7;
    pub const CLIP: usize = 8;
    pub const CLIP_RADII: usize = 9;
    pub const LIQUID: usize = 10;
    pub const TYPES: usize = 11;
    pub const SHAPES: usize = 12;
    pub const VIA: usize = 13;
    pub const ABERRATION: usize = 14;
    pub const AFFINE: usize = 15;
    pub const EDGE: usize = 16;
    pub const FADE_RECT: usize = 20;
    pub const FADE: usize = 21;
    pub const NOTCH: usize = 25;
}

impl Params {
    /// A box at `(x, y, w, h)` with a radius at every corner, the identity colour matrix, no clip,
    /// no tint, `taps` taps from row `first` of the auxiliary data.
    fn new(rect: [f32; 4], radius: f32, first: f32, taps: f32) -> Self {
        let mut record = [[0.0; 4]; 28];
        record[row::BOUNDS] = rect;
        record[row::RADII] = [radius; 4];
        record[row::FINISH] = [0.0, 0.0, 0.0, 1.0];
        record[row::MATRIX] = [1.0, 0.0, 0.0, 0.0];
        record[row::MATRIX + 1] = [0.0, 1.0, 0.0, 0.0];
        record[row::MATRIX + 2] = [0.0, 0.0, 1.0, 0.0];
        record[row::TYPES] = [0.0, 0.0, 0.0, 1.0];
        record[row::SHAPES] = [1.0; 4];
        record[row::AFFINE] = [1.0, 0.0, 0.0, 1.0];
        Self {
            screen: [SIZE as f32, SIZE as f32, 1.0, 0.0],
            source: [0.0, 0.0, taps, 0.0],
            taps: [first, 0.0, 0.0, 0.0],
            record,
        }
    }
}

/// The rows texture's texel at `(x, y)`: premultiplied, a little transparent at the right of every
/// eighth column so the glass shows through.
fn rows_texel(x: u32, y: u32) -> [u8; 4] {
    let alpha = 255 - (x % 8) * 20;
    let c = |v: u32| (v * alpha / 255) as u8;
    [
        c(x * 3 % 256),
        c(y * 2 % 256),
        c((x + y) % 256),
        alpha as u8,
    ]
}

/// Draw `cases` with `source` over the clear colour and read the pixels back.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    cases: &[Params],
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
                S::VERTEX | S::FRAGMENT,
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
            // Always a texture, whatever the tier.
            entry(3, S::FRAGMENT, AuxData::binding_type(true)),
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
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
    let rows_bytes: Vec<u8> = (0..SIZE)
        .flat_map(|y| (0..SIZE).flat_map(move |x| rows_texel(x, y)))
        .collect();
    let rows = harness::texture(
        device,
        queue,
        (SIZE, SIZE),
        wgpu::TextureFormat::Rgba8Unorm,
        4,
        &rows_bytes,
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let aux = AuxData::new(device, queue, true, aux);
    let prepared: Vec<_> = cases
        .iter()
        .map(|case| {
            let uniforms = harness::buffer(
                device,
                bytemuck::bytes_of(case),
                wgpu::BufferUsages::UNIFORM,
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
                        resource: wgpu::BindingResource::TextureView(&rows),
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
            (uniforms, bind)
        })
        .collect();
    harness::render(device, queue, SIZE, CLEAR, |pass| {
        pass.set_pipeline(&pipeline);
        for (_, bind) in &prepared {
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..6, 0..1);
        }
    })
}

/// A box of each kind of backdrop: frosted, tinted with grain, liquid, liquid with an edge colour,
/// clipped and faded and cut by a shape, and notched and turned.
fn scene() -> (Vec<Params>, Vec<[f32; 4]>) {
    let mut frosted = Params::new([8.0, 8.0, 48.0, 40.0], 10.0, 0.0, 5.0);
    frosted.record[row::MATRIX] = [1.1, -0.05, -0.05, 0.02];
    frosted.record[row::MATRIX + 1] = [-0.05, 0.9, -0.05, 0.0];
    frosted.record[row::MATRIX + 2] = [-0.05, -0.05, 0.6, 0.05];

    let mut tinted = Params::new([64.0, 8.0, 56.0, 40.0], 6.0, 5.0, 3.0);
    tinted.record[row::MATRIX] = [0.5, 0.0, 0.0, 0.2];
    tinted.record[row::MATRIX + 1] = [0.0, 0.8, 0.0, 0.1];
    tinted.record[row::MATRIX + 2] = [0.0, 0.0, 0.95, 0.3];
    tinted.record[row::FINISH] = [0.0, 0.0, 0.8, 0.9];

    let mut liquid = Params::new([8.0, 56.0, 56.0, 48.0], 14.0, 0.0, 5.0);
    liquid.record[row::LIQUID] = [1.0, 0.7, 5.0, 0.8];
    liquid.record[row::ABERRATION] = [0.4, 0.0, 0.0, 0.0];
    liquid.record[row::VIA] = [0.9, 0.95, 1.0, 0.1];
    liquid.record[row::MATRIX + 1][3] = 0.05;

    let mut edged = Params::new([68.0, 56.0, 52.0, 48.0], 12.0, 0.0, 5.0);
    edged.record[row::LIQUID] = [1.0, -0.6, 6.0, 2.0];
    edged.record[row::ABERRATION] = [0.2, 0.0, 0.0, 0.0];
    edged.record[row::EDGE] = [1.0, 0.8, 0.2, 0.7];
    edged.record[row::FINISH] = [0.0, 0.0, 0.5, 0.95];

    // A clip in the page frame, a clip in the box's own frame, fades on all four sides and a shape
    // clip whose header is at row 20.
    let mut clipped = Params::new([8.0, 108.0, 44.0, 44.0], 8.0, 5.0, 3.0);
    clipped.record[row::TYPES] = [0.0, 0.0, 3.0, 2.0];
    clipped.record[row::CLIP] = [12.0, 112.0, 36.0, 36.0];
    clipped.record[row::CLIP_RADII] = [14.0, 14.0, 14.0, 14.0];
    clipped.record[row::LOCAL_CLIP] = [2.0, 2.0, 38.0, 38.0];
    clipped.record[row::LOCAL_CLIP_RADII] = [5.0, 5.0, 5.0, 5.0];
    clipped.record[row::FADE_RECT] = [8.0, 108.0, 44.0, 44.0];
    clipped.record[row::FADE] = [5.0, 6.0, 7.0, 8.0];
    clipped.source[3] = 21.0;

    // Notched, and turned a little about its origin.
    let mut notched = Params::new([60.0, 112.0, 48.0, 40.0], 6.0, 0.0, 5.0);
    notched.record[row::NOTCH] = [-12.0, -8.0, 6.0, 4.0];
    notched.record[row::NOTCH + 1] = [3.0, 28.0, 8.0, 0.0];
    notched.record[row::AFFINE] = [0.985, 0.174, -0.174, 0.985];

    let aux = vec![
        // 0-4: five taps (offset, weight) summing to one.
        [-2.0, 0.1, 0.0, 0.0],
        [-1.0, 0.2, 0.0, 0.0],
        [0.0, 0.4, 0.0, 0.0],
        [1.0, 0.2, 0.0, 0.0],
        [2.0, 0.1, 0.0, 0.0],
        // 5-7: three taps from row 5.
        [-1.5, 0.25, 0.0, 0.0],
        [0.0, 0.5, 0.0, 0.0],
        [1.5, 0.25, 0.0, 0.0],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        // 20-22: a rounded rect shape clip: frame, (offset, kind 2, radius), (origin, size).
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 2.0, 4.0],
        [14.0, 114.0, 34.0, 34.0],
    ];
    (vec![frosted, tinted, liquid, edged, clipped, notched], aux)
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    harness::pixel(pixels, SIZE, x, y)
}

#[test]
fn the_composite_draws_something_in_every_box() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (cases, aux) = scene();
    let pixels = draw(
        &device,
        &queue,
        crate::sdf_russl::backdrop::WGSL,
        &cases,
        &aux,
    );
    for (i, case) in cases.iter().enumerate() {
        let [x, y, w, h] = case.record[row::BOUNDS];
        let drawn = (y as u32..(y + h) as u32)
            .flat_map(|py| (x as u32..(x + w) as u32).map(move |px| (px, py)))
            .filter(|&(px, py)| pixel(&pixels, px, py) != CLEAR)
            .count();
        assert!(drawn > 100, "case {i} drew only {drawn} pixels");
    }
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The rows texture sampled bilinearly at `(u, v)` in texels, clamped at the edges, as floats.
fn sample_rows(x: f32, y: f32) -> [f32; 4] {
    let (fx, fy) = (x - 0.5, y - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (wx, wy) = (fx - x0, fy - y0);
    let at = |x: f32, y: f32| {
        let max = (SIZE - 1) as f32;
        rows_texel(x.clamp(0.0, max) as u32, y.clamp(0.0, max) as u32).map(|c| c as f32 / 255.0)
    };
    let mut out = [0.0; 4];
    for (dx, dy, w) in [
        (0.0, 0.0, (1.0 - wx) * (1.0 - wy)),
        (1.0, 0.0, wx * (1.0 - wy)),
        (0.0, 1.0, (1.0 - wx) * wy),
        (1.0, 1.0, wx * wy),
    ] {
        let c = at(x0 + dx, y0 + dy);
        for k in 0..4 {
            out[k] += c[k] * w;
        }
    }
    out
}

/// What the frosted path draws at the pixel `(px, py)` inside a box with no clip: the taps blur the
/// rows vertically, the result is un-premultiplied, put through the colour matrix and over the
/// tint, then drawn over the clear colour at the box's opacity.
fn frosted(params: &Params, taps: &[[f32; 2]], px: u32, py: u32) -> [f32; 3] {
    let mut sum = [0.0; 4];
    for [offset, weight] in taps {
        let c = sample_rows(px as f32 + 0.5, py as f32 + 0.5 + offset);
        for k in 0..4 {
            sum[k] += c[k] * weight;
        }
    }
    let rgb = if sum[3] > 1e-4 {
        [sum[0] / sum[3], sum[1] / sum[3], sum[2] / sum[3]]
    } else {
        [0.0; 3]
    };
    let matrix = |i: usize| params.record[row::MATRIX + i];
    let dot = |m: [f32; 4]| (m[0] * rgb[0] + m[1] * rgb[1] + m[2] * rgb[2] + m[3]).clamp(0.0, 1.0);
    let colour = [dot(matrix(0)), dot(matrix(1)), dot(matrix(2))];
    let tint = [0, 1, 2].map(|i| matrix(i)[3].clamp(0.0, 1.0));
    let tint_alpha = (1.0 - (matrix(0)[0] + matrix(1)[1] + matrix(2)[2]) / 3.0).clamp(0.0, 1.0);
    let alpha = sum[3] + tint_alpha * (1.0 - sum[3]);
    let rgb = [0, 1, 2].map(|i| (colour[i] * sum[3] + tint[i] * (1.0 - sum[3])) / alpha);
    let a = alpha * params.record[row::FINISH][3];
    [0, 1, 2].map(|i| mix(CLEAR[i] as f32, rgb[i] * 255.0, a))
}

fn assert_near(pixel: [u8; 4], expected: [f32; 3], tolerance: f32, what: &str) {
    for c in 0..3 {
        assert!(
            (pixel[c] as f32 - expected[c]).abs() <= tolerance,
            "{what}: channel {c} is {} and should be {:.1}",
            pixel[c],
            expected[c]
        );
    }
}

/// Frosted boxes: the blur, the colour matrix and the tint, on the CPU. The tinted box has grain,
/// which adds up to two levels.
#[test]
fn frosted_boxes_blur_the_rows_and_tint_them_through_the_formulas() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (cases, aux) = scene();
    let pixels = draw(
        &device,
        &queue,
        crate::sdf_russl::backdrop::WGSL,
        &cases,
        &aux,
    );
    let five = [[-2.0, 0.1], [-1.0, 0.2], [0.0, 0.4], [1.0, 0.2], [2.0, 0.1]];
    for (x, y) in [(30, 28), (20, 15), (44, 22)] {
        assert_near(
            pixel(&pixels, x, y),
            frosted(&cases[0], &five, x, y),
            2.0,
            &format!("frosted box at {x},{y}"),
        );
    }
    let three = [[-1.5, 0.25], [0.0, 0.5], [1.5, 0.25]];
    for (x, y) in [(90, 28), (75, 20), (110, 35)] {
        assert_near(
            pixel(&pixels, x, y),
            frosted(&cases[1], &three, x, y),
            4.0,
            &format!("tinted box at {x},{y}"),
        );
    }
    // Outside every box the target is untouched.
    assert_eq!(pixel(&pixels, 2, 2), CLEAR);
    assert_eq!(pixel(&pixels, 124, 150), CLEAR);
    assert_eq!(pixel(&pixels, 4, 100), CLEAR);
}

/// Pixels of the liquid, clipped and notched boxes as the shader drew them when it was checked
/// against the hand-written WGSL it replaced: the rim lighting, dispersion, refraction and the
/// clips are not worth a second implementation here. A pixel can move a level or two between GPUs.
const PINS: &[(u32, u32, [u8; 3])] = &[
    // The liquid box.
    (36, 80, [106, 168, 148]),
    (12, 80, [76, 177, 156]),
    (10, 60, [64, 128, 191]),
    (30, 58, [148, 168, 155]),
    (60, 100, [64, 128, 191]),
    (24, 70, [88, 162, 110]),
    // The liquid box with an edge colour.
    (94, 80, [42, 141, 178]),
    (70, 80, [221, 170, 120]),
    (118, 100, [64, 128, 191]),
    (94, 58, [128, 160, 127]),
    (110, 66, [68, 127, 180]),
    // The clipped, faded box.
    (30, 130, [77, 62, 174]),
    (14, 130, [54, 75, 170]),
    (24, 114, [72, 228, 138]),
    (45, 145, [98, 82, 190]),
    (20, 120, [61, 204, 156]),
    // The notched box, turned.
    (72, 150, [216, 44, 222]),
    (80, 140, [240, 24, 220]),
    (70, 146, [141, 79, 204]),
    (92, 137, [33, 52, 217]),
    (88, 150, [8, 44, 238]),
    (76, 131, [64, 128, 191]),
    (96, 134, [32, 12, 230]),
    (84, 132, [64, 128, 191]),
];

#[test]
fn the_liquid_clipped_and_notched_boxes_keep_their_pinned_pixels() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (cases, aux) = scene();
    let pixels = draw(
        &device,
        &queue,
        crate::sdf_russl::backdrop::WGSL,
        &cases,
        &aux,
    );
    for &(x, y, expected) in PINS {
        assert_near(
            pixel(&pixels, x, y),
            expected.map(f32::from),
            3.0,
            &format!("pinned pixel {x},{y}"),
        );
    }
}

#[test]
fn the_cpu_params_lay_out_as_the_shader_reads_them() {
    use crate::sdf_russl::backdrop_source::Params as Shader;
    assert_eq!(std::mem::size_of::<Params>(), Shader::SIZE);
    assert_eq!(Shader::SIZE, 31 * 16);
}

/// The whole scene against this machine's golden, the output of the shader it replaced.
#[test]
fn the_scene_matches_this_machines_golden() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (cases, aux) = scene();
    harness::check_golden("backdrop_composite", SIZE, || {
        draw(
            &device,
            &queue,
            crate::sdf_russl::backdrop::WGSL,
            &cases,
            &aux,
        )
    });
}
