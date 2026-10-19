//! Ordered ABI backdrops, sampled from the current layer before they are painted.

use super::{Kind, RECORD_FLOATS, Record, Unsupported, row};
use crate::renderer::LayerTexture;

const SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/backdrop.wgsl"));
const ROWS_SHADER: &str = include_str!("../shaders/backdrop_rows.wgsl");

/// A backdrop's logical ABI rows and the shape points they address.
#[derive(Clone, Debug)]
pub struct Backdrop {
    pub rows: [[f32; 4]; RECORD_FLOATS / 4],
    /// Device pixels per logical pixel. The blur row already uses device pixels.
    pub scale: f32,
    /// Element-frame shape header and optional polygon points.
    pub aux: Vec<[f32; 4]>,
}

impl Record<'_> {
    /// Read a backdrop, retaining its logical geometry and target-pixel blur.
    pub fn backdrop(&self, scale: f32, list: &[f32]) -> Result<Backdrop, Unsupported> {
        if self.kind() != Kind::Backdrop {
            return Err(Unsupported::Kind(self.row(row::TYPES)[0]));
        }
        if let Some(why) = self.unsupported() {
            return Err(why);
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Unsupported::Degenerate);
        }
        let rows: [[f32; 4]; RECORD_FLOATS / 4] = std::array::from_fn(|i| self.row(i));
        if rows.iter().flatten().any(|v| !v.is_finite())
            || rows[row::FILL][0] < 0.0
            || rows[row::FILL][1] < 0.0
        {
            return Err(Unsupported::BackdropEffect);
        }
        Ok(Backdrop {
            rows,
            scale,
            aux: self.clip_rows(scale, list)?,
        })
    }
}

impl Backdrop {
    pub(super) fn moved(&self, dx: f32, dy: f32) -> Self {
        let mut draw = self.clone();
        for at in [row::BOUNDS, row::CLIP, row::FADE_RECT] {
            draw.rows[at][0] += dx / self.scale;
            draw.rows[at][1] += dy / self.scale;
        }
        if !draw.aux.is_empty() {
            super::clip::translate(&mut draw.aux, dx, dy);
        }
        draw
    }

    /// The transformed box's bounds, widened by a physical antialiasing fringe.
    fn region(&self, size: (u32, u32), margin: f32) -> Option<[u32; 4]> {
        let [x, y, w, h] = self.rows[row::BOUNDS];
        let [a, b, c, d] = self.rows[row::AFFINE];
        let gx = 1.5 / a.hypot(b).max(0.01);
        let gy = 1.5 / c.hypot(d).max(0.01);
        let points = [[-gx, -gy], [w + gx, -gy], [-gx, h + gy], [w + gx, h + gy]].map(|[u, v]| {
            [
                (x + a * u + c * v) * self.scale,
                (y + b * u + d * v) * self.scale,
            ]
        });
        let pad = margin + 2.0;
        let (mut low, mut high) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for p in points {
            for axis in 0..2 {
                low[axis] = low[axis].min(p[axis]);
                high[axis] = high[axis].max(p[axis]);
            }
        }
        let x0 = (low[0] - pad).floor().max(0.0).min(size.0 as f32) as u32;
        let y0 = (low[1] - pad).floor().max(0.0).min(size.1 as f32) as u32;
        let x1 = (high[0] + pad).ceil().max(0.0).min(size.0 as f32) as u32;
        let y1 = (high[1] + pad).ceil().max(0.0).min(size.1 as f32) as u32;
        (x1 > x0 && y1 > y0).then_some([x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)])
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    screen: [f32; 4],
    source: [f32; 4],
    taps: [f32; 4],
    record: [[f32; 4]; RECORD_FLOATS / 4],
}

/// Pipelines and scratch textures retained across ordered backdrop draws.
pub(super) struct BackdropPass {
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    sampler: wgpu::Sampler,
    rows: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    copy: wgpu::RenderPipeline,
    pool: Vec<LayerTexture>,
    aux: wgpu::Texture,
    aux_height: u32,
    format: wgpu::TextureFormat,
}

impl BackdropPass {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let uniform = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Display List Backdrop"),
            entries: &[
                uniform,
                texture(1, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(3, false),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Display List Backdrop"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = |source: &'static str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Display List Backdrop"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            })
        };
        let module = shader(SHADER);
        let rows_module = shader(ROWS_SHADER);
        let pipeline = |module: &wgpu::ShaderModule, entry: &str, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Display List Backdrop"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Display List Backdrop"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            rows: pipeline(&rows_module, "fs_rows", None),
            composite: pipeline(&module, "fs_main", Some(wgpu::BlendState::ALPHA_BLENDING)),
            copy: pipeline(&rows_module, "fs_copy", None),
            layout,
            uniforms,
            sampler,
            pool: Vec::new(),
            aux: Self::aux_texture(device, 1),
            aux_height: 1,
            format,
        }
    }

    fn aux_texture(device: &wgpu::Device, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Display List Backdrop Auxiliary"),
            size: wgpu::Extent3d {
                width: 1024,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    fn take(&mut self, device: &wgpu::Device, size: (u32, u32)) -> LayerTexture {
        match self.pool.iter().position(|t| t.size == size) {
            Some(at) => self.pool.swap_remove(at),
            None => LayerTexture::new(device, size, self.format, false),
        }
    }

    fn group(&self, device: &wgpu::Device, source: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Display List Backdrop"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        &self.aux.create_view(&Default::default()),
                    ),
                },
            ],
        })
    }

    fn pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        source: &wgpu::BindGroup,
        pipeline: &wgpu::RenderPipeline,
        region: [u32; 4],
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Display List Backdrop"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_scissor_rect(region[0], region[1], region[2], region[3]);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, source, &[]);
        pass.draw(0..6, 0..1);
    }

    /// Copy a completed premultiplied frame to a target requiring only render usage.
    pub(super) fn copy(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Display List Frame Copy"),
        });
        self.pass(
            &mut encoder,
            target,
            &self.group(device, source),
            &self.copy,
            [0, 0, size.0, size.1],
        );
        queue.submit([encoder.finish()]);
    }

    /// Snapshot first, blur its rows, then sample those rows while painting the box.
    pub(super) fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: (u32, u32),
        draw: &Backdrop,
    ) {
        let Some(region) = draw.region(size, 0.0) else {
            return;
        };
        let margin = draw.rows[row::FILL][1].max(0.0) * draw.scale;
        let read = draw
            .region(size, margin)
            .expect("the visible box has a read region");
        let source = self.take(device, (read[2], read[3]));
        let rows = self.take(device, source.size);
        let mut aux = draw.aux.clone();
        let first = aux.len();
        let taps = super::gaussian::taps(draw.rows[row::FILL][0]);
        aux.extend(taps.iter().map(|p| [p[0], p[1], 0.0, 0.0]));
        let height = aux.len().div_ceil(1024).max(1) as u32;
        if height > self.aux_height {
            self.aux = Self::aux_texture(device, height);
            self.aux_height = height;
        }
        aux.resize(height as usize * 1024, [0.0; 4]);
        queue.write_texture(
            self.aux.as_image_copy(),
            bytemuck::cast_slice(&aux),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024 * 16),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width: 1024,
                height,
                depth_or_array_layers: 1,
            },
        );
        let params = Params {
            screen: [size.0 as f32, size.1 as f32, draw.scale, 0.0],
            source: [
                read[0] as f32,
                read[1] as f32,
                taps.len() as f32,
                if draw.aux.is_empty() { 0.0 } else { 1.0 },
            ],
            taps: [first as f32, 0.0, 0.0, 0.0],
            record: draw.rows,
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&params));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Display List Backdrop"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: target.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: read[0],
                    y: read[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            source.texture.as_image_copy(),
            wgpu::Extent3d {
                width: read[2],
                height: read[3],
                depth_or_array_layers: 1,
            },
        );
        self.pass(
            &mut encoder,
            &rows.view,
            &self.group(device, &source.view),
            &self.rows,
            [0, 0, read[2], read[3]],
        );
        self.pass(
            &mut encoder,
            target,
            &self.group(device, &rows.view),
            &self.composite,
            region,
        );
        queue.submit([encoder.finish()]);
        self.pool.extend([source, rows]);
        self.pool.drain(..self.pool.len().saturating_sub(6));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backdrop_shaders_validate() {
        for source in [SHADER, ROWS_SHADER] {
            let module = naga::front::wgsl::parse_str(source)
                .map_err(|e| e.emit_to_string(source))
                .unwrap();
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
        assert_eq!(std::mem::size_of::<Params>(), 31 * 16);
    }
}
