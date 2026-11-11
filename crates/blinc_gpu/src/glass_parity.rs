//! The glass shaders draw what their formulas say, on the storage tier and the data texture tier:
//! the same primitives give the same pixels on both, the simple frosted glass matches its maths on
//! the CPU, and each scene matches a golden per machine.

use crate::parity_harness::{self as harness, AuxData};
use crate::primitives::{GlassType, GlassUniforms, GpuGlassPrimitive};

const SIZE: u32 = 192;
const BACKDROP: u32 = 192;
/// The clear colour.
const CLEAR: [u8; 4] = [64, 128, 191, 255];

/// The backdrop's texel at `(x, y)`: a rough red, a ramp for green and a curve for blue, so a blur
/// shows and so does the saturation.
fn backdrop_texel(x: u32, y: u32) -> [u8; 4] {
    [
        ((x * x * 3 + y * 7) % 256) as u8,
        ((x + y) * 3 / 4) as u8,
        (x * y / 150 + 30) as u8,
        255,
    ]
}

/// A draw: its uniforms and its primitives.
struct Group {
    uniforms: GlassUniforms,
    prims: Vec<GpuGlassPrimitive>,
}

fn uniforms() -> GlassUniforms {
    GlassUniforms {
        viewport_size: [SIZE as f32; 2],
        time: 0.0,
        clip_in_image: 0.0,
        sample_size: [BACKDROP as f32; 2],
        origin: [0.0; 2],
    }
}

/// Draw `groups` with `source` over the clear colour and read the pixels back. `data_texture` is
/// the tier without storage buffers: the primitives and the auxiliary data are `RGBA32F` textures.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    data_texture: bool,
    groups: &[Group],
    aux: &[[f32; 4]],
) -> Vec<u8> {
    use wgpu::{BindingType as B, ShaderStages as S};
    let entry = |binding, stage, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stage,
        ty,
        count: None,
    };
    let prims_type = if data_texture {
        B::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        }
    } else {
        B::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        }
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
            entry(1, S::VERTEX | S::FRAGMENT, prims_type),
            entry(
                2,
                S::FRAGMENT,
                B::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            entry(
                3,
                S::FRAGMENT,
                B::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
            entry(4, S::FRAGMENT, AuxData::binding_type(data_texture)),
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
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
    let prepared: Vec<_> = groups
        .iter()
        .map(|g| {
            let uniforms = harness::buffer(
                device,
                bytemuck::bytes_of(&g.uniforms),
                wgpu::BufferUsages::UNIFORM,
            );
            let prim_bytes: &[u8] = bytemuck::cast_slice(&g.prims);
            // A row of nine RGBA32F texels for each primitive on the data texture tier.
            let (prims_buffer, prims_view) = if data_texture {
                let view = harness::texture(
                    device,
                    queue,
                    (9, g.prims.len() as u32),
                    wgpu::TextureFormat::Rgba32Float,
                    16,
                    prim_bytes,
                );
                (None, Some(view))
            } else {
                let buffer = harness::buffer(device, prim_bytes, wgpu::BufferUsages::STORAGE);
                (Some(buffer), None)
            };
            let prims = match (&prims_buffer, &prims_view) {
                (Some(buffer), _) => buffer.as_entire_binding(),
                (_, Some(view)) => wgpu::BindingResource::TextureView(view),
                _ => unreachable!(),
            };
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
                        resource: prims,
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&backdrop),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: aux.resource(),
                    },
                ],
            });
            (
                uniforms,
                prims_buffer,
                prims_view,
                bind,
                g.prims.len() as u32,
            )
        })
        .collect();
    harness::render(device, queue, SIZE, CLEAR, |pass| {
        pass.set_pipeline(&pipeline);
        for (_, _, _, bind, count) in &prepared {
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..6, 0..*count);
        }
    })
}

/// A glass primitive of `glass_type` in the cell `(col, row)`: 48 pixels square, 64 apart.
fn cell(col: u32, row: u32, glass_type: GlassType) -> GpuGlassPrimitive {
    GpuGlassPrimitive::new(8.0 + 64.0 * col as f32, 8.0 + 64.0 * row as f32, 48.0, 48.0)
        .with_glass_type(glass_type)
}

/// Each kind of glass and each feature in a cell of its own: a tint, a shadow, a border colour,
/// refraction, a rect clip, a rounded clip, an ABI shape clip and a circle.
fn scene() -> (Vec<Group>, Vec<[f32; 4]>) {
    let ox = |col: u32| 8.0 + 64.0 * col as f32;
    let mut shaped = cell(1, 2, GlassType::Regular).with_blur(5.0);
    // A clip header at row 0 of the auxiliary data, named by a negative `clip_radius.w`.
    shaped.clip_radius = [1.0, 0.0, 0.0, -1.0];
    let prims = vec![
        cell(0, 0, GlassType::Regular)
            .with_corner_radius(10.0)
            .with_blur(4.0)
            .with_saturation(1.2)
            .with_brightness(1.05)
            .with_noise(0.6)
            .with_border_thickness(2.0)
            .with_light_angle(0.8)
            .with_tint(0.9, 0.95, 1.0, 0.5)
            .with_shadow_offset(6.0, 0.4, 2.0, 3.0),
        cell(1, 0, GlassType::UltraThin)
            .with_corner_radius(8.0)
            .with_blur(2.0)
            .with_border_thickness(3.0)
            .with_border_color(1.0, 0.8, 0.2, 0.8),
        cell(2, 0, GlassType::Thick)
            .with_corner_radius(12.0)
            .with_blur(6.0)
            .with_refraction(0.5)
            .with_light_angle(2.5)
            .with_tint(1.0, 0.5, 0.5, 0.6),
        cell(0, 1, GlassType::Chrome)
            .with_corner_radius(14.0)
            .with_blur(3.0)
            .flat()
            .with_shadow(4.0, 0.5),
        cell(1, 1, GlassType::Simple)
            .with_corner_radius(8.0)
            .with_blur(4.0)
            .with_noise(0.5)
            .with_tint(0.2, 0.4, 0.9, 0.5)
            .with_shadow(5.0, 0.4)
            .with_clip_rounded_rect_per_corner(ox(1) + 4.0, 72.0, 40.0, 40.0, 6.0, 6.0, 6.0, 0.0),
        cell(2, 1, GlassType::Regular)
            .with_corner_radius(10.0)
            .with_blur(8.0)
            .with_clip_rect(ox(2) + 2.0, 74.0, 44.0, 44.0),
        cell(0, 2, GlassType::Thin)
            .with_corner_radius(6.0)
            .with_blur(0.2)
            .with_clip_rect(-100.0, 139.0, 150.0, 40.0),
        shaped,
        GpuGlassPrimitive::circle(ox(2) + 24.0, 160.0, 24.0)
            .with_blur(5.0)
            .with_shadow(5.0, 0.3),
    ];
    let mut in_image = uniforms();
    in_image.clip_in_image = 1.0;
    let aux = vec![
        // 0-2: an ABI rounded rect shape clip: frame, (offset, kind 2, radius), (origin, size).
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 2.0, 6.0],
        [ox(1) + 6.0, 142.0, 36.0, 36.0],
    ];
    (
        vec![
            Group {
                uniforms: uniforms(),
                prims,
            },
            // The same clips applied in the retained image instead: the shader skips them.
            Group {
                uniforms: in_image,
                prims: vec![
                    cell(2, 2, GlassType::Regular)
                        .with_blur(3.0)
                        .with_clip_rect(ox(2) + 20.0, 156.0, 20.0, 20.0),
                ],
            },
        ],
        aux,
    )
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    harness::pixel(pixels, SIZE, x, y)
}

type Rgb = [f32; 3];

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The backdrop sampled bilinearly at `(u, v)`, clamped at the edges.
fn sample(u: f32, v: f32) -> Rgb {
    let (fx, fy) = (u * BACKDROP as f32 - 0.5, v * BACKDROP as f32 - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (wx, wy) = (fx - x0, fy - y0);
    let at = |x: f32, y: f32| {
        let max = (BACKDROP - 1) as f32;
        let t = backdrop_texel(x.clamp(0.0, max) as u32, y.clamp(0.0, max) as u32);
        [0, 1, 2].map(|c| t[c] as f32 / 255.0)
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

/// The backdrop blurred around `uv`: 6 rings of 12 samples on a golden-angle spiral out to 2.5
/// sigma, weighted as a Gaussian and clamped to the clip. The liquid shader clamps a blur under
/// half a pixel to the clip too; the simple one does not.
fn blur(uv: [f32; 2], radius: f32, clip: [f32; 4], clamp_small: bool) -> Rgb {
    let has_clip = clip[0] > -5000.0;
    let size = BACKDROP as f32;
    let (low, high) = (
        [clip[0] / size, clip[1] / size],
        [(clip[0] + clip[2]) / size, (clip[1] + clip[3]) / size],
    );
    let clamped = |p: [f32; 2]| {
        if has_clip {
            [p[0].clamp(low[0], high[0]), p[1].clamp(low[1], high[1])]
        } else {
            p
        }
    };
    if radius < 0.5 {
        let at = if clamp_small { clamped(uv) } else { uv };
        return sample(at[0], at[1]);
    }
    let centre = clamped(uv);
    let mut sum = sample(centre[0], centre[1]);
    let mut total = 1.0;
    for ring in 1..=6 {
        let ring_radius = radius * 2.5 * (ring as f32 / 6.0);
        let offset = ring_radius / size;
        for i in 0..12 {
            let angle = i as f32 * (std::f32::consts::TAU / 12.0) + ring as f32 * 2.399_963_2;
            let at = clamped([uv[0] + angle.cos() * offset, uv[1] + angle.sin() * offset]);
            let weight = (-(ring_radius * ring_radius) / (2.0 * radius * radius)).exp();
            let c = sample(at[0], at[1]);
            for k in 0..3 {
                sum[k] += c[k] * weight;
            }
            total += weight;
        }
    }
    sum.map(|c| c / total)
}

/// Signed distance to a rounded rect with the same radius at every corner.
fn rounded_rect_sdf(p: [f32; 2], origin: [f32; 2], size: [f32; 2], radius: f32) -> f32 {
    let half = [size[0] * 0.5, size[1] * 0.5];
    let rel = [p[0] - origin[0] - half[0], p[1] - origin[1] - half[1]];
    let r = radius.min(half[0].min(half[1]));
    let q = [rel[0].abs() - half[0] + r, rel[1].abs() - half[1] + r];
    q[0].max(0.0).hypot(q[1].max(0.0)) + q[0].max(q[1]).min(0.0) - r
}

/// What simple frosted glass draws at the pixel `(px, py)` for a primitive with no noise and no
/// shadow: the blurred backdrop, its saturation and brightness, a light additive tint and a border
/// colour along the rim, over the clear colour at the mask.
fn simple_glass(prim: &GpuGlassPrimitive, clamp_small: bool, px: u32, py: u32) -> Rgb {
    let p = [px as f32 + 0.5, py as f32 + 0.5];
    let size = BACKDROP as f32;
    let [x, y, w, h] = prim.bounds;
    let d = rounded_rect_sdf(p, [x, y], [w, h], prim.corner_radius[0]);
    let mask = 1.0 - smoothstep(-1.5, 1.5, d);
    let [blur_radius, saturation, brightness, _] = prim.params;
    let backdrop = blur(
        [p[0] / size, p[1] / size],
        blur_radius,
        prim.clip_bounds,
        clamp_small,
    );
    let gray = backdrop[0] * 0.299 + backdrop[1] * 0.587 + backdrop[2] * 0.114;
    let mut rgb = [0, 1, 2].map(|c| mix(gray, backdrop[c], saturation) * brightness);
    let tint = prim.tint_color;
    if tint[3] > 0.001 {
        for c in 0..3 {
            rgb[c] += tint[c] * tint[3] * 0.5;
        }
    }
    let bc = prim.border_color;
    if bc[3] > 0.001 {
        let width = prim.params2[0];
        let inner = -d;
        let band =
            smoothstep(0.0, width * 0.3, inner) * (1.0 - smoothstep(width, width * 1.5, inner));
        for c in 0..3 {
            rgb[c] = mix(rgb[c], bc[c], band * bc[3] * mask);
        }
    }
    [0, 1, 2].map(|c| mix(CLEAR[c] as f32, rgb[c].clamp(0.0, 1.0) * 255.0, mask))
}

fn assert_near(pixel: [u8; 4], expected: Rgb, tolerance: f32, what: &str) {
    for c in 0..3 {
        assert!(
            (pixel[c] as f32 - expected[c]).abs() <= tolerance,
            "{what}: channel {c} is {} and should be {:.1}",
            pixel[c],
            expected[c]
        );
    }
}

/// Four frosted primitives with no noise and no shadow: a tint with saturation and brightness, a
/// blur under half a pixel inside a clip, a blur held inside a clip, and a border colour.
fn probe_prims() -> Vec<GpuGlassPrimitive> {
    vec![
        cell(0, 0, GlassType::Simple)
            .with_corner_radius(10.0)
            .with_blur(4.0)
            .with_saturation(1.3)
            .with_brightness(0.9)
            .with_tint(0.2, 0.4, 0.9, 0.5),
        cell(1, 0, GlassType::Simple)
            .with_corner_radius(10.0)
            .with_blur(0.2)
            .with_clip_rect(76.0, 12.0, 40.0, 40.0),
        cell(0, 1, GlassType::Simple)
            .with_corner_radius(10.0)
            .with_blur(6.0)
            .with_clip_rect(12.0, 76.0, 36.0, 36.0),
        cell(1, 1, GlassType::Simple)
            .with_corner_radius(8.0)
            .with_blur(3.0)
            .with_border_thickness(4.0)
            .with_border_color(1.0, 0.5, 0.1, 1.0),
    ]
}

/// The frosted glass of both shaders, the simple one and the liquid one's simple type, against the
/// CPU: blur, saturation, brightness, tint, a clip, and a border colour.
#[test]
fn frosted_glass_draws_through_the_formulas() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let prims = probe_prims();
    let groups = [Group {
        uniforms: uniforms(),
        prims: prims.clone(),
    }];
    // Pixels well inside, and by a clip's edge; the border's edge is the rim at 2.5 and 0.5 pixels.
    let probes: &[(usize, u32, u32)] = &[
        (0, 32, 32),
        (0, 20, 40),
        (1, 96, 32),
        (1, 80, 20),
        (2, 16, 100),
        (2, 30, 80),
        (3, 74, 96),
        (3, 72, 96),
        (3, 96, 100),
    ];
    // The liquid shader's simple type has no border colour and clamps a small blur to the clip.
    for (source, liquid, name) in [
        (crate::sdf_russl::simple_glass::WGSL, false, "simple glass"),
        (
            crate::sdf_russl::glass::WGSL,
            true,
            "liquid glass, simple type",
        ),
    ] {
        let pixels = draw(&device, &queue, source, false, &groups, &[]);
        for &(i, x, y) in probes.iter().filter(|(i, ..)| !(liquid && *i == 3)) {
            assert_near(
                pixel(&pixels, x, y),
                simple_glass(&prims[i], liquid, x, y),
                3.0,
                &format!("{name}, primitive {i} at {x},{y}"),
            );
        }
    }
}

#[test]
fn the_data_texture_tier_draws_the_pixels_the_storage_tier_draws() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (groups, aux) = scene();
    for (storage, texture, name) in [
        (
            crate::sdf_russl::glass::WGSL,
            crate::sdf_russl::glass_dt::WGSL,
            "glass",
        ),
        (
            crate::sdf_russl::simple_glass::WGSL,
            crate::sdf_russl::simple_glass_dt::WGSL,
            "simple glass",
        ),
    ] {
        let a = draw(&device, &queue, storage, false, &groups, &aux);
        let b = draw(&device, &queue, texture, true, &groups, &aux);
        assert!(
            a.as_chunks::<4>().0.iter().any(|p| *p != CLEAR),
            "{name}: nothing was drawn"
        );
        assert!(a == b, "{name}: the tiers draw different pixels");
    }
}

/// Each shader's scene against this machine's golden, the output of the shader it replaced.
#[test]
fn the_scenes_match_this_machines_goldens() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (groups, aux) = scene();
    for (source, name) in [
        (crate::sdf_russl::glass::WGSL, "glass"),
        (crate::sdf_russl::simple_glass::WGSL, "simple_glass"),
    ] {
        harness::check_golden(name, SIZE, || {
            draw(&device, &queue, source, false, &groups, &aux)
        });
    }
}

#[test]
fn the_cpu_glass_types_lay_out_as_the_shader_reads_them() {
    use crate::sdf_russl::glass_data::{GlassPrimitive, GlassUniforms as Uniforms};
    use std::mem::{offset_of, size_of};
    assert_eq!(size_of::<GpuGlassPrimitive>(), GlassPrimitive::SIZE);
    assert_eq!(size_of::<GlassUniforms>(), Uniforms::SIZE);
    macro_rules! same_offsets {
        ($ty:ty, $shader:ty, $($field:ident),* $(,)?) => {
            $(
                assert_eq!(
                    offset_of!($ty, $field),
                    offset_of!($shader, $field),
                    concat!("`", stringify!($field), "` sits elsewhere in the shader"),
                );
            )*
        };
    }
    same_offsets!(
        GpuGlassPrimitive,
        GlassPrimitive,
        bounds,
        corner_radius,
        tint_color,
        params,
        params2,
        type_info,
        clip_bounds,
        clip_radius,
        border_color,
    );
    same_offsets!(
        GlassUniforms,
        Uniforms,
        viewport_size,
        time,
        clip_in_image,
        sample_size,
        origin,
    );
}

/// Pixels of the liquid glass scene as the shader drew them when it was checked against the
/// hand-written WGSL it replaced: four to a cell (centre, left, top, right), where the bevel,
/// refraction, highlight and clips differ. A pixel can move a level or two between GPUs.
const PINS: &[(u32, u32, [u8; 3])] = &[
    // Regular with a tint, shadow and noise.
    (32, 32, [141, 45, 31]),
    (12, 32, [242, 23, 31]),
    (32, 12, [133, 28, 37]),
    (52, 38, [155, 93, 50]),
    // Ultra thin with a border colour.
    (96, 32, [131, 89, 44]),
    (76, 32, [117, 69, 43]),
    (96, 12, [95, 73, 31]),
    (116, 38, [179, 163, 60]),
    // Thick, refracting, tinted.
    (160, 32, [136, 143, 67]),
    (140, 32, [139, 117, 55]),
    (160, 12, [124, 117, 33]),
    (180, 38, [134, 166, 77]),
    // Chrome, flat, with a shadow.
    (32, 96, [147, 112, 72]),
    (12, 96, [129, 93, 55]),
    (32, 76, [133, 93, 62]),
    (52, 102, [136, 127, 83]),
    // Simple type with a rounded clip.
    (96, 96, [143, 170, 149]),
    (76, 96, [128, 155, 137]),
    (96, 76, [139, 153, 135]),
    (116, 102, [64, 128, 191]),
    // Regular in a rect clip.
    (160, 96, [128, 192, 133]),
    (140, 96, [156, 169, 113]),
    (160, 76, [141, 169, 103]),
    (180, 102, [157, 210, 151]),
    // Thin, a small blur in a wide clip.
    (32, 160, [97, 145, 66]),
    (12, 160, [91, 115, 26]),
    (32, 140, [196, 122, 54]),
    (52, 166, [64, 128, 191]),
    // Regular in an abi shape clip.
    (96, 160, [136, 192, 133]),
    (76, 160, [64, 128, 191]),
    (96, 140, [64, 128, 191]),
    (116, 166, [64, 128, 191]),
    // A circle with a shadow.
    (160, 160, [128, 240, 201]),
    (140, 160, [169, 230, 190]),
    (160, 140, [98, 230, 190]),
    (180, 166, [122, 126, 221]),
];

#[test]
fn the_liquid_glass_scene_keeps_its_pinned_pixels() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let (groups, aux) = scene();
    let pixels = draw(
        &device,
        &queue,
        crate::sdf_russl::glass::WGSL,
        false,
        &groups,
        &aux,
    );
    for &(x, y, expected) in PINS {
        assert_near(
            pixel(&pixels, x, y),
            expected.map(f32::from),
            4.0,
            &format!("pinned pixel {x},{y}"),
        );
    }
}
