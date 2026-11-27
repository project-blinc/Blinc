//! Image blending variants share the same geometry and clipping shader.
use super::{GpuImageInstance, GpuRenderer, IMAGE_DT_SHADER, IMAGE_SHADER};
use crate::image::IMAGE_INSTANCE_ATTRIBUTES;

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
                        attributes: &IMAGE_INSTANCE_ATTRIBUTES,
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
