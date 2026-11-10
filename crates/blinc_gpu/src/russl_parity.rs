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

/// How a tier gets its primitives to the shader.
#[derive(Clone, Copy, PartialEq)]
enum Tier {
    /// Storage buffers in every stage.
    Storage,
    /// The vertex stage reads a vertex buffer; the fragment stage reads storage buffers.
    VertexBuffer,
    /// The vertex stage reads a vertex buffer; the fragment stage reads data textures.
    DataTexture,
}

fn layout(device: &wgpu::Device, tier: Tier) -> wgpu::BindGroupLayout {
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
    let texture = |binding, filterable| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let storage = wgpu::BufferBindingType::Storage { read_only: true };
    let data = tier == Tier::DataTexture;
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            buffer(0, wgpu::BufferBindingType::Uniform),
            if data {
                texture(1, false)
            } else {
                buffer(1, storage)
            },
            texture(2, true),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            texture(4, true),
            if data {
                texture(5, false)
            } else {
                buffer(5, storage)
            },
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
    draw_with_aux(device, queue, source, constants, prims, &[[0.0; 4]])
}

/// `draw` with the auxiliary data buffer `aux` bound.
fn draw_with_aux(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    constants: &[(&str, f64)],
    prims: &[GpuPrimitive],
    aux: &[[f32; 4]],
) -> Vec<u8> {
    draw_in_tier(device, queue, Tier::Storage, source, constants, prims, aux)
}

/// An `RGBA32F` texture of `width` by `height` texels holding `bytes`, row by row.
fn data_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    bytes: &[u8],
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 16),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&Default::default())
}

/// `draw` for a tier: how the primitives and the auxiliary data reach the shader.
fn draw_in_tier(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    tier: Tier,
    source: &str,
    constants: &[(&str, f64)],
    prims: &[GpuPrimitive],
    aux: &[[f32; 4]],
) -> Vec<u8> {
    use crate::primitives::SdfVertexInstance;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let bind_layout = layout(device, tier);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let options = wgpu::PipelineCompilationOptions {
        constants,
        ..Default::default()
    };
    let vertex_buffers: &[wgpu::VertexBufferLayout] = if tier == Tier::Storage {
        &[]
    } else {
        &[SdfVertexInstance::LAYOUT]
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
            buffers: vertex_buffers,
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
    let instances: Vec<SdfVertexInstance> = prims
        .iter()
        .map(SdfVertexInstance::from_primitive)
        .collect();
    let instance_buffer = buffer(bytemuck::cast_slice(&instances), wgpu::BufferUsages::VERTEX);
    // The primitives and the auxiliary data, as the tier binds them.
    let primitives_buffer;
    let aux_buffer;
    let primitives_view;
    let aux_view;
    let (primitives, aux_binding) = if tier == Tier::DataTexture {
        primitives_view = data_texture(
            device,
            queue,
            GpuPrimitive::DATA_TEXTURE_TEXELS,
            prims.len() as u32,
            bytemuck::cast_slice(prims),
        );
        // The auxiliary texture is 1024 texels wide and as many rows as it needs.
        let rows = aux.len().div_ceil(1024).max(1);
        let mut padded = aux.to_vec();
        padded.resize(rows * 1024, [0.0; 4]);
        aux_view = data_texture(
            device,
            queue,
            1024,
            rows as u32,
            bytemuck::cast_slice(&padded),
        );
        (
            wgpu::BindingResource::TextureView(&primitives_view),
            wgpu::BindingResource::TextureView(&aux_view),
        )
    } else {
        primitives_buffer = buffer(bytemuck::cast_slice(prims), wgpu::BufferUsages::STORAGE);
        aux_buffer = buffer(bytemuck::cast_slice(aux), wgpu::BufferUsages::STORAGE);
        (
            primitives_buffer.as_entire_binding(),
            aux_buffer.as_entire_binding(),
        )
    };
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
                resource: primitives,
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
                resource: aux_binding,
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
        if tier != Tier::Storage {
            pass.set_vertex_buffer(0, instance_buffer.slice(..));
        }
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
    aux: &[[f32; 4]],
    a: &str,
    b: &str,
) {
    let a = draw_with_aux(device, queue, a, &[], prims, aux);
    let b = draw_with_aux(device, queue, b, &[], prims, aux);
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
        &[[0.0; 4]],
        crate::sdf_russl::storage::sdf_core::WGSL,
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
fn the_shadow_fallback_tiers_draw_what_the_storage_tier_draws() {
    use crate::primitives::PrimitiveType::*;
    let Some((device, queue)) = device() else {
        eprintln!("no adapter with vertex storage: skipped");
        return;
    };
    let shadow = |kind: crate::primitives::PrimitiveType, x, y, w, h, params: [f32; 4]| {
        let mut p = GpuPrimitive::rect(x, y, w, h).with_corner_radius(8.0);
        p.type_info[0] = kind as u32;
        p.shadow = params;
        p.shadow_color = [0.0, 0.0, 0.0, 0.7];
        p
    };
    let mut prims = vec![
        shadow(Shadow, 14.0, 14.0, 40.0, 30.0, [3.0, 4.0, 8.0, 2.0]),
        shadow(Shadow, 70.0, 14.0, 40.0, 30.0, [0.0; 4]),
        shadow(InnerShadow, 14.0, 70.0, 40.0, 40.0, [2.0, 3.0, 6.0, 1.0]),
        shadow(InnerShadow, 70.0, 70.0, 40.0, 40.0, [0.0, 0.0, 0.0, 2.0]),
        shadow(CircleShadow, 40.0, 40.0, 24.0, 24.0, [4.0, 4.0, 10.0, 3.0]),
        shadow(
            CircleInnerShadow,
            90.0,
            90.0,
            30.0,
            30.0,
            [2.0, -2.0, 5.0, 1.0],
        ),
        shadow(Shadow, 30.0, 60.0, 50.0, 30.0, [1.0, 2.0, 6.0, 0.0])
            .with_clip_rounded_rect(30.0, 60.0, 30.0, 20.0, 6.0, 6.0, 6.0, 6.0),
    ];
    // A shadow clipped to a triangle whose vertices are read from the auxiliary data: the data
    // texture tier reads them from a texture.
    let mut clipped = shadow(Shadow, 50.0, 20.0, 40.0, 30.0, [0.0, 0.0, 6.0, 2.0]);
    clipped.type_info[2] = 4;
    clipped.clip_bounds = [0.0, 0.0, 128.0, 128.0];
    // The vertices sit in the second 1024-texel row of the auxiliary texture, past its middle.
    clipped.clip_radius = [0.0, 0.0, 3.0, 1600.0];
    prims.push(clipped);
    let mut aux = vec![[0.0; 4]; 1600];
    aux.extend([[-10.0, -10.0, 60.0, -10.0], [25.0, 50.0, 0.0, 0.0]]);

    let storage = draw_in_tier(
        &device,
        &queue,
        Tier::Storage,
        crate::sdf_russl::sdf_shadow::WGSL,
        &[],
        &prims,
        &aux,
    );
    assert!(
        storage.iter().any(|&p| p != 0),
        "the storage tier drew nothing"
    );
    for (tier, source) in [
        (Tier::VertexBuffer, crate::sdf_russl::sdf_shadow_vb::WGSL),
        (Tier::DataTexture, crate::sdf_russl::sdf_shadow_dt::WGSL),
    ] {
        let other = draw_in_tier(&device, &queue, tier, source, &[], &prims, &aux);
        let differing = storage
            .as_chunks::<4>()
            .0
            .iter()
            .zip(other.as_chunks::<4>().0)
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(differing, 0, "{differing} of {} pixels differ", SIZE * SIZE);
    }
}

/// A scene of 3D shapes and a group, as a test draws it: the group's shapes are read from the
/// auxiliary data at index 1600, in the second row of the data texture tier's aux texture.
fn scene_3d() -> (Vec<GpuPrimitive>, Vec<[f32; 4]>) {
    let shape = |kind: f32, x: f32, y: f32| {
        GpuPrimitive::rect(x, y, 36.0, 36.0)
            .with_color(0.8, 0.5, 0.3, 1.0)
            .with_corner_radius(6.0)
            .with_3d_rotation(0.5, 0.6, 300.0)
            .with_3d_shape(kind, 24.0)
            .with_light([0.3, -0.5, 0.8], 0.9)
            .with_3d_material(0.2, 32.0)
    };
    let mut prims = vec![
        shape(1.0, 4.0, 4.0),
        shape(2.0, 46.0, 4.0),
        shape(3.0, 88.0, 4.0),
        shape(4.0, 4.0, 46.0),
        shape(5.0, 46.0, 46.0),
        shape(1.0, 88.0, 46.0)
            .with_linear_gradient(1.0, 0.1, 0.1, 1.0, 0.1, 0.1, 1.0, 1.0)
            .with_clip_rounded_rect(88.0, 46.0, 30.0, 30.0, 8.0, 8.0, 8.0, 8.0),
        shape(2.0, 4.0, 88.0)
            .with_radial_gradient(1.0, 1.0, 0.2, 1.0, 0.2, 0.1, 0.6, 1.0)
            .with_clip_circle(22.0, 106.0, 14.0),
    ];
    prims[5].gradient_params = [88.0, 46.0, 118.0, 76.0];
    prims[6].gradient_params = [22.0, 106.0, 20.0, 0.0];
    let mut group = GpuPrimitive::rect(46.0, 88.0, 70.0, 36.0)
        .with_3d_rotation(0.3, 0.4, 300.0)
        .with_3d_shape(6.0, 24.0)
        .with_light([0.3, -0.5, 0.8], 0.9)
        .with_3d_material(0.2, 32.0);
    group.border = [0.0, 2.0, 1600.0, 24.0];
    prims.push(group);
    let mut aux = vec![[0.0; 4]; 1600];
    aux.extend([
        [-12.0, 0.0, 0.0, 4.0],
        [1.0, 24.0, 0.0, 0.0],
        [14.0, 14.0, 12.0, 0.0],
        [0.9, 0.3, 0.2, 1.0],
        [14.0, 0.0, 0.0, 0.0],
        [2.0, 24.0, 3.0, 6.0],
        [14.0, 14.0, 12.0, 0.0],
        [0.2, 0.4, 0.9, 1.0],
    ]);
    (prims, aux)
}

#[test]
fn the_3d_fallback_tiers_draw_what_the_storage_tier_draws() {
    let Some((device, queue)) = device() else {
        eprintln!("no adapter with vertex storage: skipped");
        return;
    };
    let (prims, aux) = scene_3d();
    let storage = draw_in_tier(
        &device,
        &queue,
        Tier::Storage,
        crate::sdf_russl::sdf_3d::WGSL,
        &[],
        &prims,
        &aux,
    );
    assert!(
        storage.iter().any(|&p| p != 0),
        "the storage tier drew nothing"
    );
    for (tier, source) in [
        (Tier::VertexBuffer, crate::sdf_russl::sdf_3d_vb::WGSL),
        (Tier::DataTexture, crate::sdf_russl::sdf_3d_dt::WGSL),
    ] {
        let other = draw_in_tier(&device, &queue, tier, source, &[], &prims, &aux);
        let differing = storage
            .as_chunks::<4>()
            .0
            .iter()
            .zip(other.as_chunks::<4>().0)
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(differing, 0, "{differing} of {} pixels differ", SIZE * SIZE);
    }
}

#[test]
fn the_3d_pipeline_raymarches_shapes_and_groups() {
    let Some((device, queue)) = device() else {
        eprintln!("no adapter with vertex storage: skipped");
        return;
    };
    let shape = |kind: f32, x: f32, y: f32| {
        GpuPrimitive::rect(x, y, 36.0, 36.0)
            .with_color(0.8, 0.5, 0.3, 1.0)
            .with_corner_radius(6.0)
            .with_3d_rotation(0.5, 0.6, 300.0)
            .with_3d_shape(kind, 24.0)
            .with_light([0.3, -0.5, 0.8], 0.9)
            .with_3d_material(0.2, 32.0)
    };
    // Each shape draws something and leaves the corner of the target untouched.
    for (kind, name) in [
        (1.0, "box"),
        (2.0, "sphere"),
        (3.0, "cylinder"),
        (4.0, "torus"),
        (5.0, "capsule"),
    ] {
        let pixels = draw(
            &device,
            &queue,
            crate::sdf_russl::sdf_3d::WGSL,
            &[],
            &[shape(kind, 46.0, 46.0)],
        );
        let any_drawn = pixels.as_chunks::<4>().0.iter().any(|px| px[3] > 0);
        assert!(any_drawn, "the {name} drew nothing");
        assert_eq!(alpha(&pixels, 2, 2), 0, "the {name} drew past its bounds");
    }

    // A group reads its shapes from the aux buffer: with them it draws, with zeros it does not.
    let mut group = GpuPrimitive::rect(30.0, 44.0, 70.0, 36.0)
        .with_3d_rotation(0.3, 0.4, 300.0)
        .with_3d_shape(6.0, 24.0)
        .with_light([0.3, -0.5, 0.8], 0.9)
        .with_3d_material(0.2, 32.0);
    group.border = [0.0, 2.0, 0.0, 24.0];
    let aux = [
        [-12.0, 0.0, 0.0, 4.0],
        [1.0, 24.0, 0.0, 0.0],
        [14.0, 14.0, 12.0, 0.0],
        [0.9, 0.3, 0.2, 1.0],
        [14.0, 0.0, 0.0, 0.0],
        [2.0, 24.0, 3.0, 6.0],
        [14.0, 14.0, 12.0, 0.0],
        [0.2, 0.4, 0.9, 1.0],
    ];
    let wgsl = crate::sdf_russl::sdf_3d::WGSL;
    let grouped = draw_with_aux(&device, &queue, wgsl, &[], &[group], &aux);
    let empty = draw_with_aux(&device, &queue, wgsl, &[], &[group], &[[0.0; 4]]);
    assert!(
        grouped.as_chunks::<4>().0.iter().any(|px| px[3] > 0),
        "the group drew nothing"
    );
    assert_ne!(grouped, empty, "the group does not read its shapes");
}

#[test]
fn the_notch_pipeline_fills_the_top_slot_for_bulges_and_peaks_and_hollows_it_for_scoops_and_cuts() {
    let Some((device, queue)) = device() else {
        eprintln!("no adapter with vertex storage: skipped");
        return;
    };
    // A bulge or a peak sits in the height the body is inset by; a scoop or a cut is carved out
    // of the body. Probe the middle of the top edge, just below the outer bounds.
    let notch = |kind: f32| {
        let mut p = GpuPrimitive::rect(40.0, 50.0, 48.0, 40.0).with_color(0.2, 0.6, 0.8, 1.0);
        p.type_info[0] = 8;
        p.perspective = [kind, 20.0, 10.0, 0.0];
        p
    };
    let wgsl = crate::sdf_russl::storage::sdf_notch::WGSL;
    for (kind, name, filled) in [
        (1.0, "scoop", false),
        (2.0, "bulge", true),
        (3.0, "cut", false),
        (4.0, "peak", true),
    ] {
        let pixels = draw(&device, &queue, wgsl, &[], &[notch(kind)]);
        assert!(
            alpha(&pixels, 64, 80) > 250,
            "the {name} left the body empty"
        );
        let slot = alpha(&pixels, 64, 53);
        if filled {
            assert!(slot > 0, "the {name} does not fill the top slot");
        } else {
            assert_eq!(slot, 0, "the {name} does not hollow the top slot");
        }
    }
}

#[test]
fn the_cpu_primitive_lays_out_as_the_shader_reads_it() {
    use crate::sdf_russl::storage::primitive::Primitive;
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
