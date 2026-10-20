//! MSAA resolves contain premultiplied colour; native effects read straight colour.
use crate::renderer::LayerTexture;
use wgpu::util::DeviceExt;

pub(super) const SHADER: &str = r#"
struct Params { opacity: f32, mode: u32, padding: vec2<f32> }
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var input: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vertex(@builtin(vertex_index) i: u32) -> Vertex {
    let uv=vec2<f32>(f32((i << 1u) & 2u),f32(i & 2u));
    var out: Vertex;
    out.position=vec4<f32>(uv*2.0-1.0,0.0,1.0);
    out.uv=vec2<f32>(uv.x,1.0-uv.y);
    return out;
}
@fragment fn premultiplied(v: Vertex) -> @location(0) vec4<f32> {
    return textureSample(input,linear_sampler,v.uv)*params.opacity;
}
@fragment fn straight(v: Vertex) -> @location(0) vec4<f32> {
    let color=textureSample(input,linear_sampler,v.uv);
    if color.a<=0.0 { return vec4<f32>(0.0); }
    return vec4<f32>(color.rgb/color.a,color.a);
}
"#;

pub(crate) struct AlphaPass {
    pub(crate) premultiplied: wgpu::RenderPipeline,
    straight: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
}

impl AlphaPass {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        bindings: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Overlay alpha"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Overlay alpha"),
            bind_group_layouts: &[bindings],
            push_constant_ranges: &[],
        });
        let pipeline = |entry, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        Self {
            premultiplied: pipeline(
                "premultiplied",
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            ),
            straight: pipeline("straight", None),
            uniforms: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Overlay alpha"),
                contents: bytemuck::cast_slice(&[1.0_f32, 0.0, 0.0, 0.0]),
                usage: wgpu::BufferUsages::UNIFORM,
            }),
        }
    }

    pub(crate) fn straighten(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        input: &LayerTexture,
        output: &LayerTexture,
    ) {
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Overlay straight alpha"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&input.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Overlay straight alpha"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay straight alpha"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.straight);
            pass.set_bind_group(0, &bindings, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));
    }
}
