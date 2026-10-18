//! A layer's effects, drawn as `blinc_abi` defines them.
//!
//! A layer is its content in a premultiplied texture. Its blur is a Gaussian
//! of a stated deviation in device pixels, taken as a pass over rows and then
//! one over columns; its drop shadow is the blurred alpha of the same content,
//! shifted and tinted; its colour filter is an affine map of straight RGB. The
//! three are applied by one composite that draws the layer over its parent
//! with straight alpha blending, after at most two passes over rows.
//!
//! The Gaussian's weights come from its deviation and are worked out once, on
//! the CPU, and its taps are paired so each fetch takes two texels' worth. The
//! cost of a pass grows with the deviation, so a blur wider than
//! [`FULL_RESOLUTION_DEVIATION`] is taken on the content shrunk by a power of
//! two until its deviation in the shrunk pixels is no wider, and drawn back at
//! full size. A Gaussian is smooth, so the result stays within a few levels of
//! the exact one: against a CPU Gaussian, at most 2 of 255 up to the limit and 5
//! beyond it, in the scene the tests draw. Every pass is confined to the
//! layer's bounds, so a small layer costs what its area does.

/// The widest deviation, in pixels, a blur is taken at without shrinking the content.
pub const FULL_RESOLUTION_DEVIATION: f32 = 8.0;

/// Pairs of taps a pass has room for: the pairs of a deviation of
/// [`FULL_RESOLUTION_DEVIATION`], which reaches three deviations either way.
const MAX_PAIRS: usize = 26;

/// Textures kept for the next composite.
const MAX_POOLED: usize = 12;

const SHADER: &str = r#"
struct Params {
    // The blur's deviation in the pixels of its content as shrunk, the layer's
    // opacity, and the factors the blur's and the shadow's content were shrunk by.
    blur: vec4<f32>,
    // The colour filter: each row weighs r, g and b, then offsets.
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    // The drop shadow's offset in pixels at full size, then its deviation as shrunk.
    shadow: vec4<f32>,
    // The drop shadow's colour.
    tint: vec4<f32>,
    // The content texture's size, and where on the target its top-left falls.
    size: vec4<f32>,
    origin: vec4<f32>,
    // How many pairs of taps the blur has, and the shadow.
    counts: vec4<u32>,
    // Each pair is the offset of one fetch and its weight; two to a vec4.
    blur_taps: array<vec4<f32>, 13>,
    shadow_taps: array<vec4<f32>, 13>,
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

// The blur's pairs of taps on premultiplied pixels; `direction` selects rows or columns.
fn blur_taps(tex: texture_2d<f32>, pixel: vec2<f32>, direction: vec2<f32>) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(tex));
    var sum = vec4<f32>(0.0);
    for (var p = 0u; p < params.counts.x; p = p + 1u) {
        let t = params.blur_taps[p / 2u];
        var pair = t.xy;
        if (p % 2u) == 1u {
            pair = t.zw;
        }
        sum = sum + textureSampleLevel(tex, linear_sampler, (pixel + direction * pair.x) / size, 0.0) * pair.y;
    }
    return sum;
}

fn shadow_taps(tex: texture_2d<f32>, pixel: vec2<f32>, direction: vec2<f32>) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(tex));
    var sum = vec4<f32>(0.0);
    for (var p = 0u; p < params.counts.y; p = p + 1u) {
        let t = params.shadow_taps[p / 2u];
        var pair = t.xy;
        if (p % 2u) == 1u {
            pair = t.zw;
        }
        sum = sum + textureSampleLevel(tex, linear_sampler, (pixel + direction * pair.x) / size, 0.0) * pair.y;
    }
    return sum;
}

// Half the size, each texel the mean of the four it covers.
@fragment
fn fs_down(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(layer));
    return textureSampleLevel(layer, linear_sampler, at.xy * 2.0 / size, 0.0);
}

// The layer's rows blurred by the blur's deviation.
@fragment
fn fs_rows(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return blur_taps(layer, at.xy, vec2<f32>(1.0, 0.0));
}

// The layer's alpha, its rows blurred by the shadow's deviation, as a shadow's alpha.
@fragment
fn fs_shadow_rows(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    var alpha = textureLoad(layer, vec2<i32>(at.xy), 0).a;
    if params.shadow.z > 0.25 {
        alpha = shadow_taps(layer, at.xy, vec2<f32>(1.0, 0.0)).a;
    }
    return vec4<f32>(0.0, 0.0, 0.0, alpha);
}

@fragment
fn fs_composite(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let at = position.xy;
    // Where this pixel is in the content, which covers only the layer's region.
    let local = at - params.origin.xy;
    var texel = vec4<f32>(0.0);
    if params.counts.x > 0u {
        // The layer here is the blurred rows, at the size the blur was taken at.
        texel = blur_taps(layer, local / params.blur.z, vec2<f32>(0.0, 1.0));
    } else {
        texel = textureLoad(layer, vec2<i32>(local), 0);
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
        let source = floor(local) - floor(params.shadow.xy + vec2<f32>(0.5));
        var fallen = 0.0;
        if source.x >= 0.0 && source.x < params.size.x {
            fallen = shadow_taps(shadow, (source + vec2<f32>(0.5)) / params.blur.w, vec2<f32>(0.0, 1.0)).a;
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

/// Where a layer is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerFrame {
    /// The size, in pixels, of the content texture.
    pub size: (u32, u32),
    /// Where the content texture's top-left falls on the target.
    pub origin: [u32; 2],
    /// `[x, y, width, height]` on the target: the part the layer changes, the
    /// content widened by the blur's and the shadow's reach. It lies inside the
    /// content texture, which also covers where a shadow is read from.
    pub region: [u32; 4],
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

/// How a blur of `deviation` is taken: the power of two the content is shrunk
/// by, and the deviation in the shrunk pixels.
fn reduction(deviation: f32) -> (u32, f32) {
    let mut factor = 1u32;
    while deviation / factor as f32 > FULL_RESOLUTION_DEVIATION {
        factor *= 2;
    }
    (factor, deviation / factor as f32)
}

/// The pairs of a Gaussian's taps: where each fetch falls from the pixel, and
/// its weight, the weights summing to one. Each fetch lies between two texels
/// and stands for both, so the pairs are the Gaussian over whole texels.
fn taps(deviation: f32) -> Vec<[f32; 2]> {
    let sigma = deviation.max(1e-4);
    let reach = (sigma * 3.0).ceil();
    let mut pairs = Vec::new();
    let mut i = -reach;
    while i <= reach {
        let w0 = (-(i * i) / (2.0 * sigma * sigma)).exp();
        let w1 = (-((i + 1.0) * (i + 1.0)) / (2.0 * sigma * sigma)).exp();
        let w = w0 + w1;
        pairs.push([i + w1 / w.max(1e-30), w]);
        i += 2.0;
    }
    let total: f32 = pairs.iter().map(|p| p[1]).sum();
    pairs.iter().map(|p| [p[0], p[1] / total]).collect()
}

/// `pairs` packed two to a vec4, with how many there are.
fn pack(pairs: &[[f32; 2]]) -> ([[f32; 4]; MAX_PAIRS / 2], u32) {
    assert!(
        pairs.len() <= MAX_PAIRS,
        "a blur reaches past the taps there is room for"
    );
    let mut out = [[0.0; 4]; MAX_PAIRS / 2];
    for (n, pair) in pairs.iter().enumerate() {
        let slot = &mut out[n / 2];
        slot[(n % 2) * 2] = pair[0];
        slot[(n % 2) * 2 + 1] = pair[1];
    }
    (out, pairs.len() as u32)
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
    size: [f32; 4],
    origin: [f32; 4],
    counts: [u32; 4],
    blur_taps: [[f32; 4]; MAX_PAIRS / 2],
    shadow_taps: [[f32; 4]; MAX_PAIRS / 2],
}

/// `region`, `[x, y, width, height]`, as it falls on a texture `by` times smaller of `limit`
/// texels: whole texels, taken a texel wider each way so rounding never cuts it.
fn shrunk(region: [u32; 4], by: u32, limit: (u32, u32)) -> [u32; 4] {
    let x0 = (region[0] / by).saturating_sub(1);
    let y0 = (region[1] / by).saturating_sub(1);
    let x1 = ((region[0] + region[2]).div_ceil(by) + 1).min(limit.0);
    let y1 = ((region[1] + region[3]).div_ceil(by) + 1).min(limit.1);
    [
        x0,
        y0,
        x1.saturating_sub(x0).max(1),
        y1.saturating_sub(y0).max(1),
    ]
}

/// A texture to draw into and then sample.
struct Scratch {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
}

/// The pipelines that draw a layer's effects, made once for a target format.
pub struct LayerEffects {
    /// Textures a composite is done with, to draw into again.
    pool: Vec<Scratch>,
    uniforms: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    down: wgpu::RenderPipeline,
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
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Display List Layer Effects Params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pool: Vec::new(),
            uniforms,
            down: pipeline("fs_down", None),
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

    /// A texture of `size`, from the pool when one of that size is free.
    fn scratch(&mut self, device: &wgpu::Device, size: (u32, u32)) -> Scratch {
        if let Some(at) = self.pool.iter().position(|s| s.size == size) {
            return self.pool.swap_remove(at);
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Display List Layer Effects Scratch"),
            size: wgpu::Extent3d {
                width: size.0.max(1),
                height: size.1.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Scratch {
            _texture: texture,
            view,
            size,
        }
    }

    /// Draws `content`, a premultiplied layer texture, over `target` as `fx` says,
    /// changing only the frame's region. The passes over rows are drawn over the
    /// content texture, scaled down with it, so a small layer costs what its area
    /// does, not the target's.
    ///
    /// The content texture must hold the content widened by the blur's and the
    /// shadow's reach, as a layer record's bounds do; what lies outside it is not
    /// blurred in, and its edge texels are repeated past it.
    pub fn composite(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        content: &wgpu::TextureView,
        target: &wgpu::TextureView,
        frame: LayerFrame,
        fx: &LayerFx,
    ) {
        let LayerFrame {
            size,
            origin,
            region,
        } = frame;
        // The whole content texture is drawn, as it is tight.
        let extent = [0, 0, size.0, size.1];
        let blur = (fx.blur > 0.0).then(|| reduction(fx.blur));
        let shadow = fx
            .shadow
            .filter(|s| s.color[3] > 0.0)
            .map(|s| (s, reduction(s.deviation)));

        let (blur_taps, blur_count) = blur.map_or(([[0.0; 4]; MAX_PAIRS / 2], 0), |(_, sigma)| {
            pack(&taps(sigma))
        });
        let (shadow_taps, shadow_count) = shadow
            .map_or(([[0.0; 4]; MAX_PAIRS / 2], 0), |(_, (_, sigma))| {
                pack(&taps(sigma))
            });
        let params = Params {
            blur: [
                blur.map_or(0.0, |(_, sigma)| sigma),
                fx.opacity,
                blur.map_or(1.0, |(factor, _)| factor as f32),
                shadow.map_or(1.0, |(_, (factor, _))| factor as f32),
            ],
            m0: fx.matrix[0],
            m1: fx.matrix[1],
            m2: fx.matrix[2],
            shadow: shadow.map_or([0.0; 4], |(s, (_, sigma))| {
                [s.offset[0], s.offset[1], sigma, 0.0]
            }),
            tint: shadow.map_or([0.0; 4], |(s, _)| s.color),
            size: [size.0 as f32, size.1 as f32, 0.0, 0.0],
            origin: [origin[0] as f32, origin[1] as f32, 0.0, 0.0],
            counts: [blur_count, shadow_count, 0, 0],
            blur_taps,
            shadow_taps,
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&params));

        // The textures the passes draw into, taken first: each shrink halves the
        // size, rounding up, and the rows are at the size the last one reached.
        let mut take = |this: &mut Self, factor: u32| {
            let (mut steps, mut from, mut f) = (Vec::new(), size, factor);
            while f > 1 {
                from = (from.0.div_ceil(2), from.1.div_ceil(2));
                steps.push(this.scratch(device, from));
                f /= 2;
            }
            let rows = this.scratch(device, from);
            (steps, rows)
        };
        let shadow_textures = shadow.map(|(_, (factor, _))| take(self, factor));
        let blur_textures = blur.map(|(factor, _)| take(self, factor));
        let group = |layer: &wgpu::TextureView, shadow: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Display List Layer Effects"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniforms.as_entire_binding(),
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
                    clear: bool,
                    within: [u32; 4]| {
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
            pass.set_scissor_rect(within[0], within[1], within[2], within[3]);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..6, 0..1);
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Display List Layer Effects"),
        });
        // The content shrunk by halvings, then its rows blurred, at the reduced size,
        // each pass within the region as it comes down to that size.
        let rows_of = |encoder: &mut wgpu::CommandEncoder,
                       rows_pipeline: &wgpu::RenderPipeline,
                       (steps, rows): &(Vec<Scratch>, Scratch)| {
            let mut source = content;
            for (n, step) in steps.iter().enumerate() {
                let within = shrunk(extent, 2u32 << n, step.size);
                pass(
                    encoder,
                    &self.down,
                    &group(source, source),
                    &step.view,
                    true,
                    within,
                );
                source = &step.view;
            }
            let within = shrunk(extent, 1u32 << steps.len(), rows.size);
            pass(
                encoder,
                rows_pipeline,
                &group(source, source),
                &rows.view,
                true,
                within,
            );
        };
        if let Some(textures) = &shadow_textures {
            rows_of(&mut encoder, &self.shadow_rows, textures);
        }
        if let Some(textures) = &blur_textures {
            rows_of(&mut encoder, &self.rows, textures);
        }

        let layer = blur_textures.as_ref().map_or(content, |t| &t.1.view);
        let shadow_view = shadow_textures.as_ref().map_or(content, |t| &t.1.view);
        if region[2] > 0 && region[3] > 0 {
            pass(
                &mut encoder,
                &self.composite,
                &group(layer, shadow_view),
                target,
                false,
                region,
            );
        }
        queue.submit(std::iter::once(encoder.finish()));

        // Free to draw into again; the queue keeps them until this is done.
        for (steps, rows) in shadow_textures.into_iter().chain(blur_textures) {
            self.pool.extend(steps);
            self.pool.push(rows);
        }
        let excess = self.pool.len().saturating_sub(MAX_POOLED);
        self.pool.drain(..excess);
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
        for entry in [
            "vs_main",
            "fs_down",
            "fs_rows",
            "fs_shadow_rows",
            "fs_composite",
        ] {
            assert!(entries.contains(&entry), "{entry}");
        }
    }

    #[test]
    fn the_params_are_laid_out_as_the_shader_reads_them() {
        // Eight vec4s, the counts, then two arrays of thirteen.
        assert_eq!(std::mem::size_of::<Params>(), (9 + 26) * 16);
    }

    #[test]
    fn a_blur_is_shrunk_until_its_deviation_fits() {
        assert_eq!(reduction(1.0), (1, 1.0));
        assert_eq!(reduction(8.0), (1, 8.0));
        assert_eq!(reduction(8.5), (2, 4.25));
        assert_eq!(reduction(16.0), (2, 8.0));
        assert_eq!(reduction(32.0), (4, 8.0));
        assert_eq!(reduction(100.0), (16, 6.25));
    }

    #[test]
    fn a_region_shrinks_with_the_content_and_stays_inside_it() {
        // Whole, it is the region itself, a texel wider each way.
        assert_eq!(shrunk([10, 20, 30, 40], 1, (100, 100)), [9, 19, 32, 42]);
        // Halved, 10..40 by 20..60 covers 5..20 by 10..30, a texel wider.
        assert_eq!(shrunk([10, 20, 30, 40], 2, (100, 100)), [4, 9, 17, 22]);
        // Never past the texture, and never empty.
        assert_eq!(shrunk([90, 90, 20, 20], 1, (100, 100)), [89, 89, 11, 11]);
        let [_, _, w, h] = shrunk([0, 0, 0, 0], 4, (10, 10));
        assert!(w >= 1 && h >= 1);
    }

    #[test]
    fn the_widest_deviation_taken_whole_fits_the_taps() {
        let widest = taps(FULL_RESOLUTION_DEVIATION);
        assert!(widest.len() <= MAX_PAIRS, "{} pairs", widest.len());
        let total: f32 = widest.iter().map(|p| p[1]).sum();
        assert!((total - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_taps_are_the_gaussian_over_whole_texels() {
        // Each fetch lies between two texels and carries both their weights, so
        // splitting them back out gives weights that fall off as a Gaussian does.
        let sigma = 3.0f32;
        let pairs = taps(sigma);
        let reach = (sigma * 3.0).ceil() as i32;
        let mut i = -reach;
        let mut texels = Vec::new();
        for pair in &pairs {
            let (w0, w1) = (
                (-((i * i) as f32) / (2.0 * sigma * sigma)).exp(),
                (-(((i + 1) * (i + 1)) as f32) / (2.0 * sigma * sigma)).exp(),
            );
            // The fetch's offset divides its weight between the texels it lies between.
            let fraction = pair[0] - i as f32;
            assert!((fraction - w1 / (w0 + w1)).abs() < 1e-5);
            texels.push((i, w0));
            texels.push((i + 1, w1));
            i += 2;
        }
        let peak = texels.iter().find(|t| t.0 == 0).unwrap().1;
        assert!(
            (peak - 1.0).abs() < 1e-6,
            "the weights are not yet normalised there"
        );
    }
}
