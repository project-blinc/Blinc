//! Each shader draws what its formulas say: known inputs go through it and the pixels are compared
//! with the same maths on the CPU.

use wgpu::util::DeviceExt;

pub(super) const SIZE: u32 = 64;

fn adapter() -> Option<wgpu::Adapter> {
    let instance = wgpu::Instance::default();
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok()
}

pub(super) fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let adapter = adapter()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// Whether a depth texture can be copied to a buffer; a GL adapter cannot.
pub(super) fn can_read_depth() -> bool {
    adapter().is_some_and(|a| {
        a.get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::DEPTH_TEXTURE_AND_BUFFER_COPIES)
    })
}

/// What a pass binds, by binding number in group 0.
pub(super) enum Input<'a> {
    Uniform(&'a [u8]),
    Texture(&'a wgpu::TextureView),
    Sampler(&'a wgpu::Sampler),
}

pub(super) struct Pass<'a> {
    pub(super) vertex: &'a str,
    pub(super) fragment: &'a str,
    pub(super) inputs: &'a [(u32, Input<'a>)],
    pub(super) vertices: u32,
}

/// A texture whose texels follow `texel(x, y)`.
pub(super) fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
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
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes.len() as u32 / height),
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

/// The texels of a `SIZE` square RGBA8 gradient, offset by `seed`.
pub(super) fn gradient_bytes(seed: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            bytes.extend([
                ((x * 4 + seed * 17) % 256) as u8,
                ((y * 4 + seed * 29) % 256) as u8,
                (((x + y) * 2 + seed * 41) % 256) as u8,
                255,
            ]);
        }
    }
    bytes
}

pub(super) fn gradient(device: &wgpu::Device, queue: &wgpu::Queue, seed: u32) -> wgpu::TextureView {
    texture(
        device,
        queue,
        SIZE,
        SIZE,
        wgpu::TextureFormat::Rgba8Unorm,
        &gradient_bytes(seed),
    )
}

/// Draw `pass` with `source` over a cleared target and read the pixels back.
pub(super) fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    pass: &Pass,
) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some(pass.vertex),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(pass.fragment),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
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
    let buffers: Vec<Option<wgpu::Buffer>> = pass
        .inputs
        .iter()
        .map(|(_, input)| match input {
            Input::Uniform(bytes) => Some(device.create_buffer_init(
                &wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytes,
                    usage: wgpu::BufferUsages::UNIFORM,
                },
            )),
            _ => None,
        })
        .collect();
    let entries: Vec<wgpu::BindGroupEntry> = pass
        .inputs
        .iter()
        .zip(&buffers)
        .map(|((binding, input), buffer)| wgpu::BindGroupEntry {
            binding: *binding,
            resource: match input {
                Input::Uniform(_) => buffer.as_ref().expect("a buffer").as_entire_binding(),
                Input::Texture(view) => wgpu::BindingResource::TextureView(view),
                Input::Sampler(sampler) => wgpu::BindingResource::Sampler(sampler),
            },
        })
        .collect();
    let bind = (!entries.is_empty()).then(|| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        })
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
        let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // A colour no pass writes, so a discarded pixel shows.
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.25,
                        g: 0.5,
                        b: 0.75,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        render.set_pipeline(&pipeline);
        if let Some(bind) = &bind {
            render.set_bind_group(0, bind, &[]);
        }
        render.draw(0..pass.vertices, 0..1);
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

pub(super) fn floats(values: &[f32]) -> Vec<u8> {
    bytemuck::cast_slice(values).to_vec()
}

macro_rules! need_device {
    () => {
        match $crate::shaders_russl::harness::device() {
            Some(d) => d,
            None => {
                eprintln!("no adapter: skipped");
                return;
            }
        }
    };
}
pub(super) use need_device;

pub(super) const CLEARED: [u8; 4] = [64, 128, 191, 255];

pub(super) fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

/// A texel of a gradient as 0..1 channels, the edge repeated outside the texture.
pub(super) fn texel(data: &[u8], x: i32, y: i32) -> [f32; 4] {
    let (x, y) = (x.clamp(0, SIZE as i32 - 1), y.clamp(0, SIZE as i32 - 1));
    let at = ((y as u32 * SIZE + x as u32) * 4) as usize;
    [0, 1, 2, 3].map(|c| data[at + c] as f32 / 255.0)
}

/// The pixel is within two levels of `want`, a colour in 0..1.
pub(super) fn is(got: [u8; 4], want: [f32; 4], what: &str) {
    for c in 0..4 {
        let want = (want[c].clamp(0.0, 1.0) * 255.0).round() as i32;
        assert!(
            (got[c] as i32 - want).abs() <= 2,
            "{what}: channel {c} is {} and should be {want} ({got:?})",
            got[c]
        );
    }
}

pub(super) fn aces(x: f32) -> f32 {
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

pub(super) const POINTS: [(u32, u32); 5] = [(0, 0), (63, 0), (31, 31), (5, 58), (60, 40)];
