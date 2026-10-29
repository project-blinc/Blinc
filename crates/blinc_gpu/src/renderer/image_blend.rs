//! Image blending variants share the same geometry and clipping shader.
use super::{GpuImageInstance, GpuRenderer, IMAGE_DT_SHADER, IMAGE_SHADER};

#[derive(Clone, Copy)]
pub(super) enum ImageBlend {
    Over,
    Erase,
    Add,
}

impl GpuRenderer {
    pub(super) fn create_image_render_pipeline(
        &self,
        shader: &wgpu::ShaderModule,
        pipeline_layout: &wgpu::PipelineLayout,
        blend_state: wgpu::BlendState,
    ) -> wgpu::RenderPipeline {
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Image Pipeline"),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuImageInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            // dst_rect
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 0,
                                shader_location: 0,
                            },
                            // src_uv
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 16,
                                shader_location: 1,
                            },
                            // tint
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 32,
                                shader_location: 2,
                            },
                            // params (border_radius, opacity, border_width, packed_border_color)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 48,
                                shader_location: 3,
                            },
                            // clip_bounds (x, y, width, height)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 64,
                                shader_location: 4,
                            },
                            // clip_radius (tl, tr, br, bl)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 80,
                                shader_location: 5,
                            },
                            // filter_a (grayscale, invert, sepia, hue_rotate_rad)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 96,
                                shader_location: 6,
                            },
                            // filter_b (brightness, contrast, saturate, unused)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 112,
                                shader_location: 7,
                            },
                            // transform (a, b, c, d) - 2x2 affine matrix
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 128,
                                shader_location: 8,
                            },
                            // clip2_bounds (x, y, width, height) - secondary sharp clip
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 144,
                                shader_location: 9,
                            },
                            // mask_params (gradient geometry)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 160,
                                shader_location: 10,
                            },
                            // mask_info (type, start_alpha, end_alpha, 0)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 176,
                                shader_location: 11,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 192,
                                shader_location: 12,
                            },
                        ],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.texture_format,
                        blend: Some(blend_state),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
    }

    pub(super) fn ensure_image_replacement_pipelines(&mut self) {
        self.ensure_image_pipeline();
        if self.image_pipeline.as_ref().unwrap().replacement.is_some() {
            return;
        }
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Clipped callback replacement"),
                source: wgpu::ShaderSource::Wgsl(
                    if self.has_storage_buffers {
                        IMAGE_SHADER
                    } else {
                        IMAGE_DT_SHADER
                    }
                    .into(),
                ),
            });
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Clipped callback replacement"),
                bind_group_layouts: &[&self.image_pipeline.as_ref().unwrap().bind_group_layout],
                push_constant_ranges: &[],
            });
        let blend = |src_factor, dst_factor| {
            let component = wgpu::BlendComponent {
                src_factor,
                dst_factor,
                operation: wgpu::BlendOperation::Add,
            };
            wgpu::BlendState {
                color: component,
                alpha: component,
            }
        };
        let erase = self.create_image_render_pipeline(
            &shader,
            &layout,
            blend(wgpu::BlendFactor::Zero, wgpu::BlendFactor::OneMinusSrcAlpha),
        );
        let add = self.create_image_render_pipeline(
            &shader,
            &layout,
            blend(wgpu::BlendFactor::One, wgpu::BlendFactor::One),
        );
        self.image_pipeline.as_mut().unwrap().replacement = Some([erase, add]);
    }
}
