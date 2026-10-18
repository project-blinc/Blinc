//! A layer's effects, drawn as `blinc_abi` defines them.
//!
//! A layer is its content in a premultiplied texture. Its blur is a Gaussian
//! of a stated deviation in device pixels, taken as a pass over rows and then
//! one over columns; its drop shadow is the blurred alpha of the same content,
//! shifted and tinted; its colour filter is an affine map of straight RGB. The
//! three are applied by one composite that draws the layer over its parent
//! with straight alpha blending, after at most two passes over rows.
//!
//! The Gaussian is exact, with weights from the deviation and not an
//! approximation of one, and its taps are paired so each fetch takes two
//! texels' worth. Its cost grows with the deviation.

use wgpu::util::DeviceExt;

const SHADER: &str = r#"
struct Params {
    // Blur deviation, then the layer's opacity.
    blur: vec4<f32>,
    // The colour filter: each row weighs r, g and b, then offsets.
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    // The drop shadow's offset in pixels, then its deviation.
    shadow: vec4<f32>,
    // The drop shadow's colour.
    tint: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var layer: texture_2d<f32>;
@group(0) @binding(2) var shadow: texture_2d<f32>;
@group(0) @binding(3) var linear_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    return vec4<f32>(corners[index], 0.0, 1.0);
}

// Paired Gaussian taps on premultiplied pixels; `direction` selects rows or columns.
fn gaussian(tex: texture_2d<f32>, pixel: vec2<f32>, direction: vec2<f32>, deviation: f32) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(tex));
    let sigma = max(deviation, 0.0001);
    let reach = ceil(sigma * 3.0);
    var sum = vec4<f32>(0.0);
    var weights = 0.0;
    var i = -reach;
    while i <= reach {
        let w0 = exp((-i * i) / (2.0 * sigma * sigma));
        let w1 = exp((-(i + 1.0) * (i + 1.0)) / (2.0 * sigma * sigma));
        let w = w0 + w1;
        let uv = (pixel + direction * (i + w1 / max(w, 1e-30))) / size;
        sum = sum + textureSampleLevel(tex, linear_sampler, uv, 0.0) * w;
        weights = weights + w;
        i = i + 2.0;
    }
    return sum / weights;
}

// The layer's rows blurred by its deviation.
@fragment
fn fs_rows(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return gaussian(layer, at.xy, vec2<f32>(1.0, 0.0), params.blur.x);
}

// The layer's alpha, its rows blurred by the shadow's deviation, as a shadow's alpha.
@fragment
fn fs_shadow_rows(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let sigma = params.shadow.z;
    var alpha = textureLoad(layer, vec2<i32>(at.xy), 0).a;
    if sigma > 0.25 {
        alpha = gaussian(layer, at.xy, vec2<f32>(1.0, 0.0), sigma).a;
    }
    return vec4<f32>(0.0, 0.0, 0.0, alpha);
}

@fragment
fn fs_composite(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let at = position.xy;
    var texel = textureLoad(layer, vec2<i32>(at), 0);
    if params.blur.x > 0.0 {
        texel = gaussian(layer, at, vec2<f32>(0.0, 1.0), params.blur.x);
    }
    var rgb = vec3<f32>(0.0);
    if texel.a > 0.0001 {
        rgb = texel.rgb / texel.a;
    }
    let c = vec4<f32>(rgb, 1.0);
    rgb = clamp(
        vec3<f32>(dot(params.m0, c), dot(params.m1, c), dot(params.m2, c)),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    var alpha = texel.a;
    if params.tint.a > 0.0 {
        let source = floor(at) - floor(params.shadow.xy + vec2<f32>(0.5));
        let size = vec2<f32>(textureDimensions(shadow));
        var fallen = 0.0;
        if source.x >= 0.0 && source.x < size.x {
            fallen = gaussian(shadow, source + vec2<f32>(0.5), vec2<f32>(0.0, 1.0), params.shadow.z).a;
        }
        let sa = fallen * params.tint.a;
        let out_alpha = alpha + sa * (1.0 - alpha);
        let premult = rgb * alpha + params.tint.rgb * (sa * (1.0 - alpha));
        rgb = vec3<f32>(0.0);
        if out_alpha > 0.0001 {
            rgb = premult / out_alpha;
        }
        alpha = out_alpha;
    }
    return vec4<f32>(rgb, alpha * params.blur.y);
}
"#;

/// A drop shadow: what a layer's alpha casts, shifted and tinted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropShadow {
    /// Where the shadow falls from the layer, in device pixels.
    pub offset: [f32; 2],
    /// The Gaussian deviation of its blur, in device pixels.
    pub deviation: f32,
    /// Its colour, straight alpha.
    pub color: [f32; 4],
}

/// What a layer does when it is drawn over its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerFx {
    /// The Gaussian deviation of its blur in device pixels; zero is none.
    pub blur: f32,
    /// Its opacity.
    pub opacity: f32,
    /// A colour filter on straight RGB: each row weighs r, g and b, then offsets.
    pub matrix: [[f32; 4]; 3],
    pub shadow: Option<DropShadow>,
}

impl LayerFx {
    pub const IDENTITY_MATRIX: [[f32; 4]; 3] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ];

    /// Only a fade.
    pub fn fade(opacity: f32) -> Self {
        Self {
            blur: 0.0,
            opacity,
            matrix: Self::IDENTITY_MATRIX,
            shadow: None,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    blur: [f32; 4],
    m0: [f32; 4],
    m1: [f32; 4],
    m2: [f32; 4],
    shadow: [f32; 4],
    tint: [f32; 4],
}

/// The pipelines that draw a layer's effects, made once for a target format.
pub struct LayerEffects {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    rows: wgpu::RenderPipeline,
    shadow_rows: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
}

impl LayerEffects {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Display List Layer Effects"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Display List Layer Effects Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture(1),
                texture(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Display List Layer Effects Pipeline Layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = |entry: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Display List Layer Effects Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            rows: pipeline("fs_rows", None),
            shadow_rows: pipeline("fs_shadow_rows", None),
            composite: pipeline("fs_composite", Some(wgpu::BlendState::ALPHA_BLENDING)),
            layout,
            sampler,
            format,
        }
    }

    /// The format of the textures this draws to and from.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Draws `content`, a premultiplied layer texture of the target's size,
    /// over `target` as `fx` says.
    ///
    /// `scratch` is two textures of that size and format for the passes over
    /// rows: the content's blurred rows, and the shadow's.
    pub fn composite(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        content: &wgpu::TextureView,
        scratch: [&wgpu::TextureView; 2],
        target: &wgpu::TextureView,
        fx: &LayerFx,
    ) {
        let shadow = fx.shadow.filter(|s| s.color[3] > 0.0);
        let params = Params {
            blur: [fx.blur, fx.opacity, 0.0, 0.0],
            m0: fx.matrix[0],
            m1: fx.matrix[1],
            m2: fx.matrix[2],
            shadow: shadow.map_or([0.0; 4], |s| [s.offset[0], s.offset[1], s.deviation, 0.0]),
            tint: shadow.map_or([0.0; 4], |s| s.color),
        };
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Display List Layer Effects Params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = |layer: &wgpu::TextureView, shadow: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Display List Layer Effects"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(layer),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(shadow),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        let pass = |encoder: &mut wgpu::CommandEncoder,
                    pipeline: &wgpu::RenderPipeline,
                    group: &wgpu::BindGroup,
                    into: &wgpu::TextureView,
                    clear: bool| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Display List Layer Effects"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: into,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: if clear {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..6, 0..1);
        };

        // Any texture does for the one a pass does not read: the content.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Display List Layer Effects"),
        });
        let [rows, shadow_rows] = scratch;
        if shadow.is_some() {
            pass(
                &mut encoder,
                &self.shadow_rows,
                &group(content, content),
                shadow_rows,
                true,
            );
        }
        if fx.blur > 0.0 {
            pass(
                &mut encoder,
                &self.rows,
                &group(content, content),
                rows,
                true,
            );
        }
        let layer = if fx.blur > 0.0 { rows } else { content };
        let shadow_view = if shadow.is_some() {
            shadow_rows
        } else {
            content
        };
        pass(
            &mut encoder,
            &self.composite,
            &group(layer, shadow_view),
            target,
            false,
        );
        queue.submit(std::iter::once(encoder.finish()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_effects_shader_validates() {
        let module = naga::front::wgsl::parse_str(SHADER)
            .map_err(|e| e.emit_to_string(SHADER))
            .unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        for entry in ["vs_main", "fs_rows", "fs_shadow_rows", "fs_composite"] {
            assert!(entries.contains(&entry), "{entry}");
        }
    }

    #[test]
    fn the_params_are_six_vec4s() {
        assert_eq!(std::mem::size_of::<Params>(), 6 * 16);
    }
}
