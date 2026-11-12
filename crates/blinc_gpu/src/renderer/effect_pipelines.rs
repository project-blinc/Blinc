//! The layer effect pipelines (blur, colour matrix, drop shadow, glow, mask image) and clearing a target.

use super::GpuRenderer;
use crate::primitives::{
    BlurUniforms, ColorMatrixUniforms, DropShadowUniforms, GlowUniforms, Uniforms,
};
use crate::shaders::{
    BLUR_SHADER, COLOR_MATRIX_SHADER, DROP_SHADOW_SHADER, GLOW_SHADER, MASK_IMAGE_SHADER,
};

impl GpuRenderer {
    /// Lazily create the blur effect pipeline and its uniform buffers
    pub(super) fn ensure_blur_pipeline(&mut self) {
        if self.effect_pipelines.blur.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Blur Effect Shader"),
                source: wgpu::ShaderSource::Wgsl(BLUR_SHADER.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Blur Effect Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.blur],
                push_constant_ranges: &[],
            });

        let targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.blur = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Blur Effect Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_kawase_blur"),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        // Also create the 8 uniform buffers for multi-pass blur
        if self.buffers.blur_uniforms_pool.is_none() {
            self.buffers.blur_uniforms_pool = Some(
                (0..8)
                    .map(|i| {
                        self.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some(&format!("Blur Uniforms Pass {i}")),
                            size: std::mem::size_of::<BlurUniforms>() as u64,
                            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        })
                    })
                    .collect(),
            );
        }
    }

    /// Lazily create the color matrix effect pipeline and its uniform buffer
    pub(super) fn ensure_color_matrix_pipeline(&mut self) {
        if self.effect_pipelines.color_matrix.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Color Matrix Effect Shader"),
                source: wgpu::ShaderSource::Wgsl(COLOR_MATRIX_SHADER.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Color Matrix Effect Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.color_matrix],
                push_constant_ranges: &[],
            });

        let targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.color_matrix = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Color Matrix Effect Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_color_matrix"),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        if self.buffers.color_matrix_uniforms.is_none() {
            self.buffers.color_matrix_uniforms =
                Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Color Matrix Uniforms Buffer"),
                    size: std::mem::size_of::<ColorMatrixUniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
        }
    }

    /// Lazily create the drop shadow effect pipeline and its uniform buffer
    pub(super) fn ensure_drop_shadow_pipeline(&mut self) {
        if self.effect_pipelines.drop_shadow.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Drop Shadow Effect Shader"),
                source: wgpu::ShaderSource::Wgsl(DROP_SHADOW_SHADER.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Drop Shadow Effect Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.drop_shadow],
                push_constant_ranges: &[],
            });

        let targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.drop_shadow = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Drop Shadow Effect Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_drop_shadow"),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        if self.buffers.drop_shadow_uniforms.is_none() {
            self.buffers.drop_shadow_uniforms =
                Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Drop Shadow Uniforms Buffer"),
                    size: std::mem::size_of::<DropShadowUniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
        }
    }

    /// Lazily create the glow effect pipeline and its uniform buffer
    pub(super) fn ensure_glow_pipeline(&mut self) {
        if self.effect_pipelines.glow.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Glow Effect Shader"),
                source: wgpu::ShaderSource::Wgsl(GLOW_SHADER.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Glow Effect Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.glow],
                push_constant_ranges: &[],
            });

        let targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.glow = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Glow Effect Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_glow"),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        if self.buffers.glow_uniforms.is_none() {
            self.buffers.glow_uniforms = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Glow Uniforms Buffer"),
                size: std::mem::size_of::<GlowUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
    }

    /// Lazily create the mask image effect pipeline
    pub(super) fn ensure_mask_image_pipeline(&mut self) {
        if self.effect_pipelines.mask_image.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Mask Image Shader"),
                source: wgpu::ShaderSource::Wgsl(MASK_IMAGE_SHADER.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Mask Image Effect Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.mask_image],
                push_constant_ranges: &[],
            });

        let targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.mask_image = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Mask Image Effect Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_mask"),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));
    }

    /// Clear a texture view to a solid color
    pub fn clear_target(&mut self, target: &wgpu::TextureView, color: wgpu::Color) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Clear Target Encoder"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Target Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
