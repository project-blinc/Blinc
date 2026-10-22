//! Upload native glass instances and bind their current backdrop/clip resources.
use super::{CachedGlassResources, GpuRenderer};
use crate::primitives::{GlassUniforms, GpuGlassPrimitive};

pub(super) struct GlassDataTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    height: u32,
}

impl GpuRenderer {
    pub(super) fn prepare_glass_draw(
        &mut self,
        backdrop: &wgpu::TextureView,
        primitives: &[GpuGlassPrimitive],
    ) {
        if self.cached_glass.is_none() {
            self.cached_glass = Some(CachedGlassResources {
                sampler: self.device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("Glass backdrop sampler"),
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                }),
                bind_group: None,
                data: None,
            });
        }
        let uniforms = GlassUniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            time: self.time,
            _padding: 0.0,
        };
        self.queue.write_buffer(
            &self.buffers.glass_uniforms,
            0,
            bytemuck::bytes_of(&uniforms),
        );
        let cached = self.cached_glass.as_mut().unwrap();
        if self.has_storage_buffers {
            if !primitives.is_empty() {
                self.queue.write_buffer(
                    &self.buffers.glass_primitives,
                    0,
                    bytemuck::cast_slice(primitives),
                );
            }
        } else {
            let height = (primitives.len() as u32).max(1);
            if cached.data.as_ref().is_none_or(|data| data.height < height) {
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Glass instance data"),
                    size: wgpu::Extent3d {
                        width: 9,
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
                let view = texture.create_view(&Default::default());
                cached.data = Some(GlassDataTexture {
                    _texture: texture,
                    view,
                    height,
                });
            }
            if !primitives.is_empty() {
                self.queue.write_texture(
                    cached.data.as_ref().unwrap()._texture.as_image_copy(),
                    bytemuck::cast_slice(primitives),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(144),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width: 9,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        // Backdrops and auxiliary allocations can change without a viewport resize.
        cached.bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Glass draw bindings"),
            layout: &self.bind_group_layouts.glass,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffers.glass_uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: if self.has_storage_buffers {
                        self.buffers.glass_primitives.as_entire_binding()
                    } else {
                        wgpu::BindingResource::TextureView(&cached.data.as_ref().unwrap().view)
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&cached.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: if self.has_storage_buffers {
                        self.buffers.aux_data.as_entire_binding()
                    } else {
                        wgpu::BindingResource::TextureView(
                            self.buffers
                                .aux_data_view
                                .as_ref()
                                .expect("DT auxiliary texture"),
                        )
                    },
                },
            ],
        }));
    }
}
