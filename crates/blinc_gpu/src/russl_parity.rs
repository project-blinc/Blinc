//! The simple core pipeline (the `FEAT_*` blocks off) draws the same pixels as the full one for
//! the primitives `GpuPrimitive::is_simple_core` sends it. The CPU's `GpuPrimitive` must hold the
//! bytes the shader's `Primitive` reads, which the layout test checks.

use crate::primitives::GpuPrimitive;
use wgpu::util::DeviceExt;

const SIZE: u32 = 128;

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok()?;
    if adapter
        .get_downlevel_capabilities()
        .flags
        .contains(wgpu::DownlevelFlags::VERTEX_STORAGE)
    {
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    } else {
        None
    }
}

fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let both = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
    let buffer = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: both,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let texture = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let storage = wgpu::BufferBindingType::Storage { read_only: true };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            buffer(0, wgpu::BufferBindingType::Uniform),
            buffer(1, storage),
            texture(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            texture(4),
            buffer(5, storage),
        ],
    })
}

/// Draw `prims` with `source` (specialised by wgpu with `constants`) and read the pixels back.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    constants: &[(&str, f64)],
    prims: &[GpuPrimitive],
) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let bind_layout = layout(device);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let options = wgpu::PipelineCompilationOptions {
        constants,
        ..Default::default()
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_layout],
                push_constant_ranges: &[],
            }),
        ),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: options.clone(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: options,
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    let uniforms: [f32; 4] = [SIZE as f32, SIZE as f32, 0.0, 0.0];
    let buffer = |contents: &[u8], usage| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents,
            usage,
        })
    };
    let uniforms = buffer(bytemuck::bytes_of(&uniforms), wgpu::BufferUsages::UNIFORM);
    let primitives = buffer(bytemuck::cast_slice(prims), wgpu::BufferUsages::STORAGE);
    let aux = buffer(&[0u8; 16], wgpu::BufferUsages::STORAGE);
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
        layout: &bind_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: primitives.as_entire_binding(),
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
                resource: wgpu::BindingResource::TextureView(&atlas),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: aux.as_entire_binding(),
            },
        ],
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
        format,
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
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..6, 0..prims.len() as u32);
    }
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
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
    slice.get_mapped_range().to_vec()
}

#[test]
fn the_simple_pipeline_draws_what_the_full_one_draws_for_simple_primitives() {
    let Some((device, queue)) = device() else {
        eprintln!("no adapter with vertex storage: skipped");
        return;
    };
    let prims = [
        GpuPrimitive::rect(8.0, 8.0, 52.0, 40.0)
            .with_color(0.9, 0.2, 0.1, 1.0)
            .with_corner_radius(10.0),
        GpuPrimitive::rect(68.0, 8.0, 52.0, 40.0)
            .with_color(0.1, 0.4, 0.9, 0.8)
            .with_border(3.0, 0.0, 0.0, 0.0, 1.0),
        GpuPrimitive::circle(34.0, 90.0, 26.0).with_color(0.2, 0.8, 0.3, 1.0),
        GpuPrimitive::rect(64.0, 60.0, 60.0, 60.0)
            .with_color(0.6, 0.1, 0.7, 1.0)
            .with_corner_radii(4.0, 16.0, 4.0, 16.0)
            .with_clip_rounded_rect(70.0, 66.0, 48.0, 48.0, 12.0, 12.0, 12.0, 12.0),
    ];
    for p in &prims {
        assert!(p.is_simple_core(), "{p:?} is not a simple core primitive");
    }
    same_pixels(&device, &queue, &prims);
}

/// `a` and `b` draw `prims` to the same pixels.
fn assert_same_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    prims: &[GpuPrimitive],
    a: &str,
    b: &str,
) {
    let a = draw(device, queue, a, &[], prims);
    let b = draw(device, queue, b, &[], prims);
    assert!(a.iter().any(|&p| p != 0), "the first pipeline drew nothing");
    let differing = a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(differing, 0, "{differing} of {} pixels differ", SIZE * SIZE);
}

/// The simple pipeline and the full one draw `prims` to the same pixels.
fn same_pixels(device: &wgpu::Device, queue: &wgpu::Queue, prims: &[GpuPrimitive]) {
    assert_same_pixels(
        device,
        queue,
        prims,
        crate::sdf_russl::sdf_core::WGSL,
        crate::sdf_russl::simple::WGSL,
    );
}

/// The alpha of the pixel at `(x, y)` in `draw`'s output.
fn alpha(pixels: &[u8], x: u32, y: u32) -> u8 {
    pixels[((y * SIZE + x) * 4 + 3) as usize]
}

fn outer_shadow(rect: [f32; 4], radius: f32, shadow: [f32; 4]) -> GpuPrimitive {
    let mut p = GpuPrimitive::rect(rect[0], rect[1], rect[2], rect[3]).with_corner_radius(radius);
    p.type_info[0] = crate::primitives::PrimitiveType::Shadow as u32;
    p.shadow = shadow;
    p.shadow_color = [0.0, 0.0, 0.0, 1.0];
    p
}

#[test]
fn the_shadow_pipeline_grows_corners_and_edges_the_way_css_does() {
    let Some((device, queue)) = device() else {
        eprintln!("no adapter with vertex storage: skipped");
        return;
    };
    let draw_one = |p: GpuPrimitive| {
        draw(
            &device,
            &queue,
            crate::sdf_russl::sdf_shadow::WGSL,
            &[],
            &[p],
        )
    };

    // The quad grows by the spread, and a square corner stays square: the grown corner pixel is
    // covered, the pixel past the grown edge is not.
    let grown = draw_one(outer_shadow(
        [40.0, 40.0, 48.0, 48.0],
        0.0,
        [0.0, 0.0, 0.0, 8.0],
    ));
    assert!(alpha(&grown, 33, 33) > 250, "the square corner is rounded");
    assert!(alpha(&grown, 34, 60) > 250, "the spread is not drawn");
    assert_eq!(
        alpha(&grown, 30, 60),
        0,
        "the shadow reaches past the spread"
    );
    assert_eq!(
        alpha(&grown, 60, 60),
        0,
        "the shadow is drawn under the shape"
    );

    // An outer shadow that has only an offset is drawn.
    let moved = draw_one(outer_shadow(
        [40.0, 40.0, 48.0, 48.0],
        0.0,
        [10.0, 0.0, 0.0, 0.0],
    ));
    assert!(
        alpha(&moved, 92, 60) > 250,
        "an offset-only shadow is skipped"
    );

    // An unblurred edge is antialiased over a pixel: the edge at x = 31.5 splits pixel 31.
    let edge = draw_one(outer_shadow(
        [40.0, 40.0, 48.0, 48.0],
        0.0,
        [0.0, 0.0, 0.0, 8.5],
    ));
    let a = alpha(&edge, 31, 60);
    assert!(
        (100..160).contains(&a),
        "the unblurred edge is not antialiased: {a}"
    );
}

#[test]
fn the_cpu_primitive_lays_out_as_the_shader_reads_it() {
    use crate::sdf_russl::abi::Primitive;
    use std::mem::{offset_of, size_of};
    assert_eq!(size_of::<GpuPrimitive>(), Primitive::SIZE);
    macro_rules! same_offsets {
        ($($field:ident),* $(,)?) => {
            $(
                assert_eq!(
                    offset_of!(GpuPrimitive, $field),
                    offset_of!(Primitive, $field),
                    concat!("`", stringify!($field), "` sits elsewhere in the shader"),
                );
            )*
        };
    }
    same_offsets!(
        bounds,
        corner_radius,
        color,
        color2,
        border,
        border_color,
        shadow,
        shadow_color,
        clip_bounds,
        clip_radius,
        clip_corner_shape,
        gradient_params,
        rotation,
        local_affine,
        perspective,
        sdf_3d,
        light,
        filter_a,
        filter_b,
        mask_params,
        mask_info,
        corner_shape,
        clip_fade,
        type_info,
        side_color_top,
        side_color_right,
        side_color_bottom,
        side_color_left,
        gradient_table,
        local_clip_bounds,
        local_clip_radius,
        clip_fade_bounds,
        notch_info,
        clip_shape,
    );
}
