//! Image pipelines and drawing images, with the blend and auxiliary data variants.

use super::image_blend::ImageBlend;
use super::{GpuRenderer, ImagePipeline};
use crate::image::GpuImageInstance;
use crate::primitives::Uniforms;
use crate::shaders::{IMAGE_DT_SHADER, IMAGE_SHADER};

impl GpuRenderer {
    /// Create the image rendering pipeline (lazily initialized)
    pub(super) fn ensure_image_pipeline(&mut self) {
        if self.image_pipeline.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Image Shader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                    if self.has_storage_buffers {
                        IMAGE_SHADER
                    } else {
                        IMAGE_DT_SHADER
                    },
                )),
            });

        // Bind group layout: uniforms, texture, sampler
        let bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Image Bind Group Layout"),
                    entries: &[
                        // Uniforms (viewport size)
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Image texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // Sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: if self.has_storage_buffers {
                                wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                                    has_dynamic_offset: false,
                                    min_binding_size: None,
                                }
                            } else {
                                wgpu::BindingType::Texture {
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: false,
                                    },
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                    multisampled: false,
                                }
                            },
                            count: None,
                        },
                    ],
                });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Image Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        // Blending for premultiplied alpha
        let blend_state = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let pipeline = self.create_image_render_pipeline(&shader, &pipeline_layout, blend_state);

        // Create instance buffer (max 1000 images per batch)
        let instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Image Instance Buffer"),
            size: (std::mem::size_of::<GpuImageInstance>() * 1000) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create sampler
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Image Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        self.image_pipeline = Some(ImagePipeline {
            pipeline,
            replacement: None,
            bind_group_layout,
            instance_buffer,
            sampler,
        });
    }

    /// Render images to a texture view
    ///
    /// # Arguments
    /// * `target` - The target texture view to render to
    /// * `image_view` - The image texture view to sample from
    /// * `instances` - The image instances to render
    pub fn render_images(
        &mut self,
        target: &wgpu::TextureView,
        image_view: &wgpu::TextureView,
        instances: &[GpuImageInstance],
    ) {
        self.render_images_with_aux(target, image_view, instances, &[]);
    }

    /// Draw images with the auxiliary clip rows addressed by their instances.
    pub fn render_images_with_aux(
        &mut self,
        target: &wgpu::TextureView,
        image_view: &wgpu::TextureView,
        instances: &[GpuImageInstance],
        aux: &[[f32; 4]],
    ) {
        self.render_images_with_aux_blend(target, image_view, instances, aux, ImageBlend::Over);
    }

    pub(super) fn render_images_with_aux_blend(
        &mut self,
        target: &wgpu::TextureView,
        image_view: &wgpu::TextureView,
        instances: &[GpuImageInstance],
        aux: &[[f32; 4]],
        blend: ImageBlend,
    ) {
        if instances.is_empty() {
            return;
        }
        self.ensure_image_pipeline();
        if !matches!(blend, ImageBlend::Over) {
            self.ensure_image_replacement_pipelines();
        }
        self.update_aux_data_slice(aux);

        let image_pipeline = self.image_pipeline.as_ref().unwrap();

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update instance buffer
        self.queue.write_buffer(
            &image_pipeline.instance_buffer,
            0,
            bytemuck::cast_slice(instances),
        );

        // Create bind group for this image
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Image Bind Group"),
            layout: &image_pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffers.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(image_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&image_pipeline.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: if self.has_storage_buffers {
                        self.buffers.aux_data.as_entire_binding()
                    } else {
                        wgpu::BindingResource::TextureView(
                            self.buffers
                                .aux_data_view
                                .as_ref()
                                .expect("image DT auxiliary texture"),
                        )
                    },
                },
            ],
        });

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Image Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Image Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Preserve existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(match blend {
                ImageBlend::Over => &image_pipeline.pipeline,
                ImageBlend::Erase => &image_pipeline.replacement.as_ref().unwrap()[0],
                ImageBlend::Add => &image_pipeline.replacement.as_ref().unwrap()[1],
            });
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.set_vertex_buffer(0, image_pipeline.instance_buffer.slice(..));
            if let Some((sx, sy, sw, sh)) = self.pending_scissor {
                render_pass.set_scissor_rect(sx, sy, sw, sh);
            }
            render_pass.draw(0..6, 0..instances.len() as u32);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Layer Texture Cache Accessors
    // ─────────────────────────────────────────────────────────────────────────

    // ─── Custom Render Pass API ────────────────────────────────────────────
}
