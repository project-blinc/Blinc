//! What the shader parity tests share: a device, textures and buffers from bytes, and a render pass
//! over a cleared target that reads the pixels back.

use wgpu::util::DeviceExt;

/// A device and queue on the default adapter, or none where the machine has no adapter.
pub(crate) fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// A 2D texture of `size` holding `bytes`, row by row, `texel_bytes` to a texel.
pub(crate) fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: (u32, u32),
    format: wgpu::TextureFormat,
    texel_bytes: u32,
    bytes: &[u8],
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
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
            bytes_per_row: Some(size.0 * texel_bytes),
            rows_per_image: Some(size.1),
        },
        wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&Default::default())
}

/// A buffer holding `contents`.
pub(crate) fn buffer(
    device: &wgpu::Device,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents,
        usage,
    })
}

/// The auxiliary data bound the way a tier binds it: a storage buffer, or on the data texture tier
/// an `RGBA32F` texture 1024 texels wide.
pub(crate) enum AuxData {
    Storage(wgpu::Buffer),
    Texture(wgpu::TextureView),
}

impl AuxData {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data_texture: bool,
        aux: &[[f32; 4]],
    ) -> Self {
        let mut aux = aux.to_vec();
        if aux.is_empty() {
            aux.push([0.0; 4]);
        }
        if data_texture {
            let rows = aux.len().div_ceil(1024);
            aux.resize(rows * 1024, [0.0; 4]);
            Self::Texture(texture(
                device,
                queue,
                (1024, rows as u32),
                wgpu::TextureFormat::Rgba32Float,
                16,
                bytemuck::cast_slice(&aux),
            ))
        } else {
            Self::Storage(buffer(
                device,
                bytemuck::cast_slice(&aux),
                wgpu::BufferUsages::STORAGE,
            ))
        }
    }

    pub(crate) fn resource(&self) -> wgpu::BindingResource<'_> {
        match self {
            Self::Storage(buffer) => buffer.as_entire_binding(),
            Self::Texture(view) => wgpu::BindingResource::TextureView(view),
        }
    }

    /// The layout entry type of the binding.
    pub(crate) fn binding_type(data_texture: bool) -> wgpu::BindingType {
        if data_texture {
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            }
        } else {
            wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            }
        }
    }
}

/// Clear a `size` square `Rgba8Unorm` target to `clear`, let `draw` record into it, and read the
/// pixels back, four bytes each.
pub(crate) fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: u32,
    clear: [u8; 4],
    draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
) -> Vec<u8> {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (size * size * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let view = target.create_view(&Default::default());
        let [r, g, b, a] = clear.map(|c| c as f64 / 255.0);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 4),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: size,
            height: size,
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

/// The pixel at `(x, y)` of `pixels`, rows `size` wide.
pub(crate) fn pixel(pixels: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * size + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}
