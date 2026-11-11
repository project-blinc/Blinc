//! What the shader parity tests share: a device, textures and buffers from bytes, and a render pass
//! over a cleared target that reads the pixels back.

use wgpu::util::DeviceExt;

/// A device and queue on the default adapter, or none where the machine has no adapter.
fn adapter() -> Option<wgpu::Adapter> {
    let instance = wgpu::Instance::default();
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok()
}

pub(crate) fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let adapter = adapter()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// Whether the adapter is the GL backend, which emulates what the others do natively (a 1D texture
/// is a 2D one of one row).
pub(crate) fn on_gl() -> bool {
    adapter().is_some_and(|a| a.get_info().backend == wgpu::Backend::Gl)
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
    let clear = clear.map(|c| c as f64 / 255.0);
    render_targets(
        device,
        queue,
        size,
        &[(wgpu::TextureFormat::Rgba8Unorm, 4, clear)],
        draw,
    )
    .remove(0)
}

/// Clear one `size` square target for each of `targets` (its format, its bytes a pixel and its
/// clear colour), let `draw` record into them, and read each back with its rows unpadded.
pub(crate) fn render_targets(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: u32,
    targets: &[(wgpu::TextureFormat, u32, [f64; 4])],
    draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
) -> Vec<Vec<u8>> {
    let textures: Vec<wgpu::Texture> = targets
        .iter()
        .map(|(format, _, _)| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: *format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        })
        .collect();
    // A copy's rows are padded to the alignment the API requires.
    let padded: Vec<u32> = targets
        .iter()
        .map(|(_, bytes, _)| (size * bytes).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT))
        .collect();
    let readbacks: Vec<wgpu::Buffer> = padded
        .iter()
        .map(|padded| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: (padded * size) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        })
        .collect();
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let views: Vec<wgpu::TextureView> = textures
            .iter()
            .map(|t| t.create_view(&Default::default()))
            .collect();
        let attachments: Vec<Option<wgpu::RenderPassColorAttachment>> = views
            .iter()
            .zip(targets)
            .map(|(view, (_, _, [r, g, b, a]))| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: *r,
                            g: *g,
                            b: *b,
                            a: *a,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })
            })
            .collect();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        draw(&mut pass);
    }
    for ((texture, readback), padded) in textures.iter().zip(&readbacks).zip(&padded) {
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(*padded),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
        );
    }
    queue.submit([encoder.finish()]);
    for readback in &readbacks {
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    }
    device
        .poll(wgpu::PollType::Wait)
        .expect("the copies finish");
    readbacks
        .iter()
        .zip(targets)
        .zip(&padded)
        .map(|((readback, (_, bytes, _)), padded)| {
            let mapped = readback.slice(..).get_mapped_range();
            mapped
                .chunks_exact(*padded as usize)
                .flat_map(|row| &row[..(size * bytes) as usize])
                .copied()
                .collect()
        })
        .collect()
}

/// The pixel at `(x, y)` of `pixels`, rows `size` wide.
pub(crate) fn pixel(pixels: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * size + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// The directory of this machine's goldens, `.blinc/snapshots/blinc_gpu/<os>` at the workspace
/// root (`BLINC_SNAPSHOT_DIR` replaces `.blinc/snapshots`), as the recorder's snapshots keep
/// theirs: per machine and untracked, because adapters differ in how they round.
pub(crate) fn golden_dir() -> std::path::PathBuf {
    let root = std::env::var_os("BLINC_SNAPSHOT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .expect("the workspace root")
                .join(".blinc/snapshots")
        });
    root.join("blinc_gpu").join(std::env::consts::OS)
}

/// `render`'s pixels, `size` square, against the golden `name` of this machine: every channel
/// within two levels, on all but a few pixels. `BLINC_SNAPSHOT=bless` records the golden; without
/// goldens on this machine nothing is compared.
pub(crate) fn check_golden(name: &str, size: u32, render: impl FnOnce() -> Vec<u8>) {
    let dir = golden_dir();
    let path = dir.join(format!("{name}.png"));
    let bless = std::env::var("BLINC_SNAPSHOT").is_ok_and(|v| v == "bless");
    let recorded = std::fs::read_dir(&dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.path().extension().is_some_and(|x| x == "png"))
    });
    if !bless && !recorded {
        eprintln!("golden `{name}` skipped: no goldens for this platform");
        return;
    }
    let got = render();
    if bless {
        std::fs::create_dir_all(&dir).unwrap();
        image::save_buffer(&path, &got, size, size, image::ColorType::Rgba8).unwrap();
        return;
    }
    let want = image::open(&path)
        .unwrap_or_else(|e| panic!("golden {name}: {e}"))
        .into_rgba8()
        .into_raw();
    assert_eq!(got.len(), want.len(), "the golden {name} has another size");
    let differing = got
        .as_chunks::<4>()
        .0
        .iter()
        .zip(want.as_chunks::<4>().0)
        .filter(|(g, w)| g.iter().zip(w.iter()).any(|(g, w)| g.abs_diff(*w) > 2))
        .count();
    assert!(
        differing <= 8,
        "{name}: {differing} of {} pixels differ from the golden",
        size * size
    );
}
