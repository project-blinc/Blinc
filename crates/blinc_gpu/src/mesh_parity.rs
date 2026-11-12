//! The mesh shaders draw what their formulas say: the full one with storage buffers and the
//! reduced one for devices without, each through its colour entry and its transparency entry.

use crate::parity_harness::{self as harness};

const SIZE: u32 = 192;
const MAP: u32 = 8;
const SHADOW: u32 = 256;
const CUBE: u32 = 16;
/// The clear colour, as the 8-bit pixel it reads back as.
const CLEAR: [u8; 4] = [26, 26, 38, 255];

/// A directional light as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Light {
    direction: [f32; 3],
    intensity: f32,
    color: [f32; 3],
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [f32; 16],
    model: [f32; 16],
    light_view_proj: [f32; 16],
    camera_pos: [f32; 3],
    _pad: f32,
    lights: [Light; 4],
    light_count: u32,
    _pad_lc: [f32; 3],
    viewport_size: [f32; 2],
    has_texture: f32,
    has_normal_map: f32,
    shadow_enabled: f32,
    displacement_scale: f32,
    normal_scale: f32,
    has_skinning: f32,
    morph_target_count: u32,
    morph_vertex_count: u32,
    _pad_morph: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Material {
    base_color: [f32; 4],
    metallic_roughness: [f32; 2],
    _pad_mr: [f32; 2],
    emissive: [f32; 3],
    unlit: f32,
    has_metallic_roughness_texture: f32,
    has_emissive_texture: f32,
    has_occlusion_texture: f32,
    occlusion_strength: f32,
    alpha_mode: f32,
    alpha_cutoff: f32,
    _pad_am: [f32; 2],
    uv_transform_matrix: [f32; 4],
    uv_transform_offset: [f32; 2],
    _pad_uv: [f32; 2],
}

/// A vertex: position, normal, uv, colour, tangent, joints and weights.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
    tangent: [f32; 4],
    joints: [u32; 4],
    weights: [f32; 4],
}

/// One object: its uniforms and material, its vertices, and how it is skinned and morphed.
struct Object {
    uniforms: Uniforms,
    material: Material,
    vertices: Vec<Vertex>,
}

type Matrix = [f32; 16];

/// `a * b` for column-major matrices.
fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum();
        }
    }
    out
}

fn translation(x: f32, y: f32, z: f32) -> Matrix {
    let mut m = [0.0; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m[12] = x;
    m[13] = y;
    m[14] = z;
    m
}

/// A perspective projection with depth 0 to 1.
fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Matrix {
    let f = 1.0 / (fov_y * 0.5).tan();
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = far / (near - far);
    m[11] = -1.0;
    m[14] = near * far / (near - far);
    m
}

/// A texture of `MAP` square texels from `texel(x, y)`.
fn map(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texel: impl Fn(u32, u32) -> [u8; 4],
) -> wgpu::TextureView {
    let bytes: Vec<u8> = (0..MAP)
        .flat_map(|y| (0..MAP).flat_map(|x| texel(x, y)).collect::<Vec<_>>())
        .collect();
    harness::texture(
        device,
        queue,
        (MAP, MAP),
        wgpu::TextureFormat::Rgba8Unorm,
        4,
        &bytes,
    )
}

/// The environment cubemap: a colour for each face that runs across it, with mip levels.
fn cubemap(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let levels = 5;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: CUBE,
            height: CUBE,
            depth_or_array_layers: 6,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for level in 0..levels {
        let size = CUBE >> level;
        for face in 0..6u32 {
            let bytes: Vec<u8> = (0..size)
                .flat_map(|y| {
                    (0..size).flat_map(move |x| {
                        let (u, v) = (x * 255 / size.max(2), y * 255 / size.max(2));
                        [
                            (u + face * 30) as u8,
                            (v + face * 20) as u8,
                            (255 - face * 40) as u8,
                            255,
                        ]
                    })
                })
                .collect();
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: face,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    })
}

/// A shadow map whose depth runs across it, so a fragment is lit in some places and in shadow in
/// others. A depth texture cannot be written from the CPU, so the shadow pass draws it: one big
/// triangle whose depth runs from 0.35 to 0.65.
fn shadow_map(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SHADOW,
            height: SHADOW,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(crate::shaders_russl::shadow_depth::WGSL.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 12,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                }],
            }],
            compilation_options: Default::default(),
        },
        fragment: None,
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    let identity = translation(0.0, 0.0, 0.0);
    let uniforms = harness::buffer(
        device,
        bytemuck::cast_slice(&[identity, identity]),
        wgpu::BufferUsages::UNIFORM,
    );
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniforms.as_entire_binding(),
        }],
    });
    let vertices = harness::buffer(
        device,
        bytemuck::cast_slice(&[[-1.0_f32, -1.0, 0.35], [3.0, -1.0, 0.65], [-1.0, 3.0, 0.65]]),
        wgpu::BufferUsages::VERTEX,
    );
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.draw(0..3, 0..1);
    }
    queue.submit([encoder.finish()]);
    view
}

/// Which fragment entry draws: the colour, or the transparency accumulation and reveal.
#[derive(Clone, Copy, PartialEq)]
enum Entry {
    Colour,
    Oit,
}

/// Draw `objects` with `source` and read back what `entry` writes: one image for the colour entry,
/// the accumulation (`RGBA16F`) and the reveal (`R8`) for the transparency one. `full` is the
/// shader with storage buffers: joints in one and morph targets in two more. Without it the joints
/// are an `RGBA32F` texture.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    full: bool,
    entry: Entry,
    objects: &[Object],
) -> Vec<Vec<u8>> {
    use wgpu::{BindingType as B, ShaderStages as S};
    let layout_entry = |binding, stage, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stage,
        ty,
        count: None,
    };
    let uniform = B::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
    };
    let texture = |dimension, sample_type| B::Texture {
        sample_type,
        view_dimension: dimension,
        multisampled: false,
    };
    let filterable = wgpu::TextureSampleType::Float { filterable: true };
    let d2 = wgpu::TextureViewDimension::D2;
    let storage = B::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: None,
    };
    let mut entries = vec![
        layout_entry(0, S::VERTEX | S::FRAGMENT, uniform),
        layout_entry(1, S::FRAGMENT, uniform),
        layout_entry(2, S::FRAGMENT, texture(d2, filterable)),
        layout_entry(
            3,
            S::FRAGMENT,
            B::Sampler(wgpu::SamplerBindingType::Filtering),
        ),
        layout_entry(4, S::FRAGMENT, texture(d2, filterable)),
        layout_entry(5, S::FRAGMENT, texture(d2, wgpu::TextureSampleType::Depth)),
        layout_entry(
            6,
            S::FRAGMENT,
            B::Sampler(wgpu::SamplerBindingType::Comparison),
        ),
        layout_entry(7, S::FRAGMENT, texture(d2, filterable)),
        layout_entry(
            8,
            S::VERTEX,
            if full {
                storage
            } else {
                texture(d2, wgpu::TextureSampleType::Float { filterable: false })
            },
        ),
        layout_entry(9, S::FRAGMENT, texture(d2, filterable)),
        layout_entry(10, S::FRAGMENT, texture(d2, filterable)),
        layout_entry(11, S::FRAGMENT, texture(d2, filterable)),
        layout_entry(
            12,
            S::FRAGMENT,
            texture(wgpu::TextureViewDimension::Cube, filterable),
        ),
        layout_entry(
            13,
            S::FRAGMENT,
            B::Sampler(wgpu::SamplerBindingType::Filtering),
        ),
    ];
    if full {
        entries.push(layout_entry(14, S::VERTEX, storage));
        entries.push(layout_entry(15, S::VERTEX, storage));
    }
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &entries,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let attributes: Vec<wgpu::VertexAttribute> = [
        (wgpu::VertexFormat::Float32x3, 0),
        (wgpu::VertexFormat::Float32x3, 12),
        (wgpu::VertexFormat::Float32x2, 24),
        (wgpu::VertexFormat::Float32x4, 32),
        (wgpu::VertexFormat::Float32x4, 48),
        (wgpu::VertexFormat::Uint32x4, 64),
        (wgpu::VertexFormat::Float32x4, 80),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (format, offset))| wgpu::VertexAttribute {
        format,
        offset,
        shader_location: i as u32,
    })
    .collect();
    let add = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    };
    let reveal = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::OneMinusSrc,
        operation: wgpu::BlendOperation::Add,
    };
    let (entry_point, targets): (&str, Vec<Option<wgpu::ColorTargetState>>) = match entry {
        Entry::Colour => (
            "fs_main",
            vec![Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        ),
        Entry::Oit => (
            "fs_main_oit",
            vec![
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState {
                        color: add,
                        alpha: add,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: reveal,
                        alpha: reveal,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ],
        ),
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            }),
        ),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(entry_point),
            targets: &targets,
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    // The textures every object shares.
    let base = map(device, queue, |x, y| {
        [
            (40 + x * 25) as u8,
            (60 + y * 20) as u8,
            150,
            (110 + y * 20) as u8,
        ]
    });
    let normal = map(device, queue, |x, y| {
        [
            (128 + x * 12 - 40) as u8,
            (128 + y * 12 - 40) as u8,
            200,
            255,
        ]
    });
    let displacement = map(device, queue, |x, y| {
        let h = ((x * 31 + y * 17) % 100 + 60) as u8;
        [h, h, h, 255]
    });
    let metallic_roughness = map(device, queue, |x, y| {
        [0, (60 + x * 25) as u8, (y * 32) as u8, 255]
    });
    let emissive = map(device, queue, |x, y| {
        [(x * 30) as u8, (y * 20) as u8, 60, 255]
    });
    let occlusion = map(device, queue, |x, y| {
        let v = (255 - x * 20 - y * 10) as u8;
        [v, v, v, 255]
    });
    let environment = cubemap(device, queue);
    let shadow = shadow_map(device, queue);
    let linear = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let compare = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });

    // Two joints: the identity, and a move and a shrink.
    let mut shrunk = translation(0.15, -0.1, 0.0);
    shrunk[0] = 0.8;
    shrunk[5] = 0.8;
    let joints = [translation(0.0, 0.0, 0.0), shrunk];
    let joint_bytes: &[u8] = bytemuck::cast_slice(&joints);
    let joint_buffer;
    let joint_view;
    let joint_binding = if full {
        joint_buffer = harness::buffer(device, joint_bytes, wgpu::BufferUsages::STORAGE);
        joint_buffer.as_entire_binding()
    } else {
        joint_view = harness::texture(
            device,
            queue,
            (4, 2),
            wgpu::TextureFormat::Rgba32Float,
            16,
            joint_bytes,
        );
        wgpu::BindingResource::TextureView(&joint_view)
    };
    // Two morph targets over the six vertices of a quad: a position delta then a normal delta.
    let deltas: Vec<[f32; 4]> = (0..2u32)
        .flat_map(|t| {
            (0..6u32).flat_map(move |v| {
                let s = 0.05 * (t + 1) as f32;
                [
                    [s, s * v as f32 * 0.2, 0.0, 0.0],
                    [0.0, 0.1 * (t + 1) as f32, 0.0, 0.0],
                ]
            })
        })
        .collect();
    let morph_deltas = harness::buffer(
        device,
        bytemuck::cast_slice(&deltas),
        wgpu::BufferUsages::STORAGE,
    );
    let morph_weights = harness::buffer(
        device,
        bytemuck::cast_slice(&[0.7_f32, 0.4]),
        wgpu::BufferUsages::STORAGE,
    );

    let prepared: Vec<_> = objects
        .iter()
        .map(|o| {
            let uniforms = harness::buffer(
                device,
                bytemuck::bytes_of(&o.uniforms),
                wgpu::BufferUsages::UNIFORM,
            );
            let material = harness::buffer(
                device,
                bytemuck::bytes_of(&o.material),
                wgpu::BufferUsages::UNIFORM,
            );
            let vertices = harness::buffer(
                device,
                bytemuck::cast_slice(&o.vertices),
                wgpu::BufferUsages::VERTEX,
            );
            let mut bindings = vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: material.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&base),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&linear),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&normal),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&shadow),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&compare),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&displacement),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: joint_binding.clone(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(&metallic_roughness),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(&emissive),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::TextureView(&occlusion),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::TextureView(&environment),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: wgpu::BindingResource::Sampler(&linear),
                },
            ];
            if full {
                bindings.push(wgpu::BindGroupEntry {
                    binding: 14,
                    resource: morph_deltas.as_entire_binding(),
                });
                bindings.push(wgpu::BindGroupEntry {
                    binding: 15,
                    resource: morph_weights.as_entire_binding(),
                });
            }
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &bindings,
            });
            (uniforms, material, vertices, bind, o.vertices.len() as u32)
        })
        .collect();
    let colour_target = [(
        wgpu::TextureFormat::Rgba8Unorm,
        4,
        CLEAR.map(|c| c as f64 / 255.0),
    )];
    let oit_targets = [
        (wgpu::TextureFormat::Rgba16Float, 8, [0.0; 4]),
        (wgpu::TextureFormat::R8Unorm, 1, [1.0, 0.0, 0.0, 0.0]),
    ];
    harness::render_targets(
        device,
        queue,
        SIZE,
        if entry == Entry::Colour {
            &colour_target
        } else {
            &oit_targets
        },
        |pass| {
            pass.set_pipeline(&pipeline);
            for (_, _, vertices, bind, count) in &prepared {
                pass.set_bind_group(0, bind, &[]);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.draw(0..*count, 0..1);
            }
        },
    )
}

/// A quad of 0.7 on a side facing the camera, its uv running 0 to 1, with a joint pair and weights
/// that move the top and bottom differently.
fn quad(color: [f32; 4]) -> Vec<Vertex> {
    let corner = |u: f32, v: f32| Vertex {
        position: [(u - 0.5) * 0.7, (0.5 - v) * 0.7, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [u, v],
        color,
        tangent: [1.0, 0.0, 0.0, 1.0],
        joints: [0, 1, 0, 0],
        weights: [1.0 - v * 0.8, v * 0.8, 0.0, 0.0],
    };
    vec![
        corner(0.0, 0.0),
        corner(1.0, 0.0),
        corner(0.0, 1.0),
        corner(1.0, 0.0),
        corner(1.0, 1.0),
        corner(0.0, 1.0),
    ]
}

/// The uniforms of an object at `(x, y)`: a perspective camera three units back, three lights, and
/// a light's view for the shadow whose depth puts the quad's plane at 0.5.
fn uniforms(x: f32, y: f32) -> Uniforms {
    let view_proj = multiply(
        &perspective(50.0_f32.to_radians(), 1.0, 0.1, 20.0),
        &translation(0.0, 0.0, -3.0),
    );
    let mut light_view_proj = [0.0; 16];
    light_view_proj[0] = 1.0 / 1.5;
    light_view_proj[5] = 1.0 / 1.5;
    light_view_proj[10] = 0.5;
    light_view_proj[14] = 0.5;
    light_view_proj[15] = 1.0;
    let light = |direction, intensity, color| Light {
        direction,
        intensity,
        color,
        _pad: 0.0,
    };
    Uniforms {
        view_proj,
        model: translation(x, y, 0.0),
        light_view_proj,
        camera_pos: [0.0, 0.0, 3.0],
        _pad: 0.0,
        lights: [
            light([-0.4, -0.6, -0.7], 2.5, [1.0, 0.95, 0.9]),
            light([0.5, -0.3, -0.4], 1.0, [0.6, 0.7, 1.0]),
            light([0.0, 0.8, -0.5], 0.6, [1.0, 0.6, 0.5]),
            light([0.0, -1.0, 0.0], 0.0, [0.0; 3]),
        ],
        light_count: 3,
        _pad_lc: [0.0; 3],
        viewport_size: [SIZE as f32; 2],
        has_texture: 0.0,
        has_normal_map: 0.0,
        shadow_enabled: 0.0,
        displacement_scale: 0.0,
        normal_scale: 1.0,
        has_skinning: 0.0,
        morph_target_count: 0,
        morph_vertex_count: 0,
        _pad_morph: [0; 2],
    }
}

/// A plain opaque material.
fn material() -> Material {
    Material {
        base_color: [1.0; 4],
        metallic_roughness: [0.6, 0.5],
        _pad_mr: [0.0; 2],
        emissive: [0.0; 3],
        unlit: 0.0,
        has_metallic_roughness_texture: 0.0,
        has_emissive_texture: 0.0,
        has_occlusion_texture: 0.0,
        occlusion_strength: 1.0,
        alpha_mode: 0.0,
        alpha_cutoff: 0.5,
        _pad_am: [0.0; 2],
        uv_transform_matrix: [1.0, 0.0, 0.0, 1.0],
        uv_transform_offset: [0.0; 2],
        _pad_uv: [0.0; 2],
    }
}

/// Nine objects in a grid: every texture, normal mapping and a shadow; parallax with a rotated
/// texture transform; skinning; morph targets; an unlit alpha mask; a blended material with a
/// scaled transform; a plain metal; a blended material that glows; and one light without a shadow.
fn scene() -> Vec<Object> {
    let at = |col: i32, row: i32| (0.9 * col as f32, -0.9 * row as f32);
    let object = |cell: (f32, f32), color: [f32; 4]| Object {
        uniforms: uniforms(cell.0, cell.1),
        material: material(),
        vertices: quad(color),
    };
    let mut lit = object(at(-1, -1), [1.0, 0.95, 0.9, 1.0]);
    lit.uniforms.has_texture = 1.0;
    lit.uniforms.has_normal_map = 1.0;
    lit.uniforms.shadow_enabled = 1.0;
    lit.material.has_metallic_roughness_texture = 1.0;
    lit.material.has_emissive_texture = 1.0;
    lit.material.emissive = [0.5; 3];
    lit.material.has_occlusion_texture = 1.0;
    lit.material.occlusion_strength = 0.8;

    let mut parallax = object(at(0, -1), [1.0; 4]);
    parallax.uniforms.has_texture = 1.0;
    parallax.uniforms.displacement_scale = 0.15;
    let (s, c) = 20.0_f32.to_radians().sin_cos();
    parallax.material.uv_transform_matrix = [c * 1.2, -s * 1.2, s * 1.2, c * 1.2];
    parallax.material.uv_transform_offset = [-0.1, 0.05];

    let mut skinned = object(at(1, -1), [1.0; 4]);
    skinned.uniforms.has_skinning = 1.0;
    skinned.material.base_color = [0.8, 0.6, 0.3, 1.0];
    skinned.material.metallic_roughness = [0.9, 0.25];

    let mut morphed = object(at(-1, 0), [0.7, 0.9, 0.8, 1.0]);
    morphed.uniforms.morph_target_count = 2;
    morphed.uniforms.morph_vertex_count = 6;

    let mut mask = object(at(0, 0), [1.0; 4]);
    mask.uniforms.has_texture = 1.0;
    mask.material.unlit = 1.0;
    mask.material.alpha_mode = 1.0;
    mask.material.alpha_cutoff = 0.6;
    mask.material.uv_transform_matrix = [1.5, 0.0, 0.0, 1.5];
    mask.material.uv_transform_offset = [-0.2, 0.1];

    let mut blend = object(at(1, 0), [1.0; 4]);
    blend.uniforms.has_texture = 1.0;
    blend.material.base_color = [0.9, 0.4, 0.4, 0.55];
    blend.material.alpha_mode = 2.0;
    blend.material.uv_transform_matrix = [2.0, 0.0, 0.0, 2.0];
    blend.material.uv_transform_offset = [0.1, 0.1];

    let mut plain = object(at(-1, 1), [1.0; 4]);
    plain.uniforms.shadow_enabled = 1.0;
    plain.material.metallic_roughness = [0.0, 0.9];
    plain.material.emissive = [0.1, 0.0, 0.0];

    let mut glow = object(at(0, 1), [1.0; 4]);
    glow.uniforms.has_texture = 1.0;
    glow.material.base_color = [0.5, 0.7, 1.0, 0.3];
    glow.material.alpha_mode = 2.0;
    glow.material.has_emissive_texture = 1.0;
    glow.material.emissive = [1.0, 0.5, 0.0];
    glow.material.has_occlusion_texture = 1.0;

    let mut single = object(at(1, 1), [1.0; 4]);
    single.uniforms.light_count = 1;
    single.uniforms.has_normal_map = 1.0;
    single.uniforms.normal_scale = 2.0;
    single.material.metallic_roughness = [0.3, 0.2];

    vec![
        lit, parallax, skinned, morphed, mask, blend, plain, glow, single,
    ]
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    harness::pixel(pixels, SIZE, x, y)
}

/// A half-precision float as an `f32`.
fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits >> 15 == 1 { -1.0 } else { 1.0 };
    let exponent = ((bits >> 10) & 0x1f) as i32;
    let fraction = (bits & 0x3ff) as f32;
    match exponent {
        0 => sign * fraction * 2.0_f32.powi(-24),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + fraction / 1024.0) * 2.0_f32.powi(exponent - 15),
    }
}

/// The world point of an object's plane under the pixel `(px, py)`, as a position on the quad: its
/// uv before the material's transform.
fn quad_uv(px: u32, py: u32, cell: (f32, f32)) -> [f32; 2] {
    // The camera is three units back with a 50 degree field of view, and the plane is at depth 0.
    let f = 1.0 / 25.0_f32.to_radians().tan();
    let ndc_x = (px as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
    let ndc_y = 1.0 - (py as f32 + 0.5) / SIZE as f32 * 2.0;
    let (x, y) = (ndc_x * 3.0 / f, ndc_y * 3.0 / f);
    [(x - cell.0) / 0.7 + 0.5, 0.5 - (y - cell.1) / 0.7]
}

/// The base-colour map's texel at `(x, y)`.
fn base_texel(x: u32, y: u32) -> [f32; 4] {
    [
        (40 + x * 25) as f32 / 255.0,
        (60 + y * 20) as f32 / 255.0,
        150.0 / 255.0,
        (110 + y * 20) as f32 / 255.0,
    ]
}

/// The base-colour map sampled bilinearly at `(u, v)`, clamped at the edges.
fn sample_base(u: f32, v: f32) -> [f32; 4] {
    let (fx, fy) = (u * MAP as f32 - 0.5, v * MAP as f32 - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (wx, wy) = (fx - x0, fy - y0);
    let at = |x: f32, y: f32| {
        let max = (MAP - 1) as f32;
        base_texel(x.clamp(0.0, max) as u32, y.clamp(0.0, max) as u32)
    };
    let mut out = [0.0; 4];
    for (dx, dy, w) in [
        (0.0, 0.0, (1.0 - wx) * (1.0 - wy)),
        (1.0, 0.0, wx * (1.0 - wy)),
        (0.0, 1.0, (1.0 - wx) * wy),
        (1.0, 1.0, wx * wy),
    ] {
        let c = at(x0 + dx, y0 + dy);
        for k in 0..4 {
            out[k] += c[k] * w;
        }
    }
    out
}

fn assert_near(pixel: [u8; 4], expected: [f32; 3], tolerance: f32, what: &str) {
    for c in 0..3 {
        assert!(
            (pixel[c] as f32 - expected[c]).abs() <= tolerance,
            "{what}: channel {c} is {} and should be {:.1}",
            pixel[c],
            expected[c]
        );
    }
}

/// The unlit alpha-mask object: the base colour times the vertex colour and the texel at the
/// transformed uv, drawn opaque where the texel's alpha passes the cutoff and not at all below it.
#[test]
fn an_unlit_alpha_mask_draws_the_texel_through_the_transform_and_the_cutoff() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let objects = scene();
    let mask = &objects[4];
    let cell = (0.0, 0.0);
    let (matrix, offset) = (
        mask.material.uv_transform_matrix,
        mask.material.uv_transform_offset,
    );
    let cutoff = mask.material.alpha_cutoff;
    for (source, full) in [
        (crate::sdf_russl::mesh::WGSL, true),
        (crate::sdf_russl::mesh_dt::WGSL, false),
    ] {
        let pixels = draw(&device, &queue, source, full, Entry::Colour, &objects).remove(0);
        let (mut kept, mut cut) = (0, 0);
        for (px, py) in [
            (86, 86),
            (106, 86),
            (86, 106),
            (106, 106),
            (96, 76),
            (84, 76),
            (108, 76),
            (96, 112),
            (80, 96),
            (112, 100),
        ] {
            let uv = quad_uv(px, py, cell);
            let uv = [
                matrix[0] * uv[0] + matrix[1] * uv[1] + offset[0],
                matrix[2] * uv[0] + matrix[3] * uv[1] + offset[1],
            ];
            // Out of the quad's own 0..1 is not drawn; near the cutoff the rounding decides.
            let texel = sample_base(uv[0], uv[1]);
            if (texel[3] - cutoff).abs() < 0.03 {
                continue;
            }
            if texel[3] < cutoff {
                assert_eq!(
                    pixel(&pixels, px, py),
                    CLEAR,
                    "below the cutoff at {px},{py}"
                );
                cut += 1;
            } else {
                assert_near(
                    pixel(&pixels, px, py),
                    [texel[0] * 255.0, texel[1] * 255.0, texel[2] * 255.0],
                    3.0,
                    &format!("unlit texel at {px},{py}"),
                );
                kept += 1;
            }
        }
        assert!(
            kept >= 2 && cut >= 1,
            "full={full}: {kept} kept and {cut} cut"
        );
    }
}

/// How far a pinned pixel may differ. Software rasterisers differ from a GPU by a few levels in
/// the lighting maths; a missing term moves a pixel far more.
const PIN_TOLERANCE: f32 = 8.0;

/// Pixels as the shaders drew them when they were checked against the hand-written WGSL they
/// replaced: the colour of each object at its centre and near its edge, for the full shader and
/// the reduced one.
const PINS: &[(bool, u32, u32, [u8; 3])] = &[
    // The full shader: textured, normal mapped, shadowed.
    (true, 34, 34, [162, 124, 127]),
    (true, 48, 25, [252, 97, 140]),
    (true, 22, 46, [52, 82, 77]),
    // The full shader: parallax with a rotated transform.
    (true, 96, 34, [95, 152, 135]),
    (true, 110, 25, [191, 160, 152]),
    (true, 84, 46, [26, 26, 38]),
    // The full shader: skinned metal.
    (true, 157, 34, [212, 130, 54]),
    (true, 171, 25, [223, 153, 67]),
    (true, 145, 46, [204, 114, 45]),
    // The full shader: morph targets.
    (true, 34, 96, [149, 149, 162]),
    (true, 48, 87, [156, 154, 165]),
    (true, 22, 108, [143, 146, 158]),
    // The full shader: unlit alpha mask.
    (true, 96, 96, [141, 188, 150]),
    (true, 110, 87, [215, 143, 150]),
    (true, 84, 108, [66, 200, 150]),
    // The full shader: blend with a scaled transform.
    (true, 157, 96, [63, 32, 33]),
    (true, 171, 87, [55, 27, 33]),
    (true, 145, 108, [48, 31, 32]),
    // The full shader: plain rough, shadowed.
    (true, 34, 157, [145, 80, 96]),
    (true, 48, 148, [145, 80, 96]),
    (true, 22, 169, [145, 79, 96]),
    // The full shader: blend that glows.
    (true, 96, 157, [45, 30, 33]),
    (true, 110, 148, [54, 26, 33]),
    (true, 84, 169, [34, 38, 33]),
    // The full shader: one light with a scaled normal map.
    (true, 157, 157, [158, 174, 141]),
    (true, 171, 148, [131, 147, 204]),
    (true, 145, 169, [166, 134, 153]),
    // The reduced shader: textured, normal mapped, shadowed.
    (false, 34, 34, [145, 111, 111]),
    (false, 48, 25, [227, 89, 130]),
    (false, 22, 46, [52, 82, 77]),
    // The reduced shader: parallax with a rotated transform.
    (false, 96, 34, [87, 142, 126]),
    (false, 110, 25, [178, 152, 146]),
    (false, 84, 46, [26, 26, 38]),
    // The reduced shader: skinned metal.
    (false, 157, 34, [210, 130, 55]),
    (false, 171, 25, [221, 155, 69]),
    (false, 145, 46, [202, 114, 45]),
    // The reduced shader: morph targets.
    (false, 34, 96, [113, 116, 108]),
    (false, 48, 87, [119, 119, 112]),
    (false, 22, 108, [107, 114, 106]),
    // The reduced shader: unlit alpha mask.
    (false, 96, 96, [141, 188, 150]),
    (false, 110, 87, [215, 143, 150]),
    (false, 84, 108, [66, 200, 150]),
    // The reduced shader: blend with a scaled transform.
    (false, 157, 96, [59, 30, 32]),
    (false, 171, 87, [52, 26, 32]),
    (false, 145, 108, [45, 29, 31]),
    // The reduced shader: plain rough, shadowed.
    (false, 34, 157, [145, 80, 96]),
    (false, 48, 148, [145, 80, 96]),
    (false, 22, 169, [145, 79, 96]),
    // The reduced shader: blend that glows.
    (false, 96, 157, [45, 30, 33]),
    (false, 110, 148, [54, 26, 33]),
    (false, 84, 169, [33, 37, 33]),
    // The reduced shader: one light with a scaled normal map.
    (false, 157, 157, [158, 177, 148]),
    (false, 171, 148, [132, 150, 210]),
    (false, 145, 169, [166, 138, 160]),
];

#[test]
fn the_scene_keeps_its_pinned_pixels() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    // CI's software GL adapter shades a shadowed pixel up to 14 levels away from the Metal and
    // Vulkan ones; the tier test and the goldens still run there.
    if harness::on_gl() {
        eprintln!("GL adapter: pins skipped");
        return;
    }
    let objects = scene();
    for (source, full) in [
        (crate::sdf_russl::mesh::WGSL, true),
        (crate::sdf_russl::mesh_dt::WGSL, false),
    ] {
        let pixels = draw(&device, &queue, source, full, Entry::Colour, &objects).remove(0);
        for &(for_full, x, y, expected) in PINS.iter().filter(|p| p.0 == full) {
            let _ = for_full;
            assert_near(
                pixel(&pixels, x, y),
                expected.map(f32::from),
                PIN_TOLERANCE,
                &format!("full={full}, pinned pixel {x},{y}"),
            );
        }
    }
}

/// Each shader's colour image and transparency targets against this machine's goldens, the output
/// of the shader it replaced. The accumulation is compressed to 8 bits as `x / (1 + x)`.
#[test]
fn the_scenes_match_this_machines_goldens() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let objects = scene();
    for (source, full, name) in [
        (crate::sdf_russl::mesh::WGSL, true, "mesh"),
        (crate::sdf_russl::mesh_dt::WGSL, false, "mesh_dt"),
    ] {
        harness::check_golden(name, SIZE, || {
            draw(&device, &queue, source, full, Entry::Colour, &objects).remove(0)
        });
        harness::check_golden(&format!("{name}_accum"), SIZE, || {
            let accum = draw(&device, &queue, source, full, Entry::Oit, &objects).remove(0);
            accum
                .as_chunks::<2>()
                .0
                .iter()
                .map(|h| {
                    let x = f16_to_f32(u16::from_le_bytes([h[0], h[1]])).max(0.0);
                    (x / (1.0 + x) * 255.0) as u8
                })
                .collect()
        });
        harness::check_golden(&format!("{name}_reveal"), SIZE, || {
            draw(&device, &queue, source, full, Entry::Oit, &objects)
                .remove(1)
                .iter()
                .flat_map(|&r| [r, r, r, 255])
                .collect()
        });
    }
}

#[test]
fn the_cpu_mesh_types_lay_out_as_the_shader_reads_them() {
    use crate::sdf_russl::mesh_common::{DirLight, MaterialUniforms, MeshUniforms};
    use std::mem::{offset_of, size_of};
    assert_eq!(size_of::<Uniforms>(), MeshUniforms::SIZE);
    assert_eq!(size_of::<Material>(), MaterialUniforms::SIZE);
    assert_eq!(size_of::<Light>(), DirLight::SIZE);
    macro_rules! same_offsets {
        ($ty:ty, $shader:ty, $($field:ident),* $(,)?) => {
            $(
                assert_eq!(
                    offset_of!($ty, $field),
                    offset_of!($shader, $field),
                    concat!("`", stringify!($field), "` sits elsewhere in the shader"),
                );
            )*
        };
    }
    same_offsets!(
        Uniforms,
        MeshUniforms,
        view_proj,
        model,
        light_view_proj,
        camera_pos,
        lights,
        light_count,
        viewport_size,
        has_texture,
        has_normal_map,
        shadow_enabled,
        displacement_scale,
        normal_scale,
        has_skinning,
        morph_target_count,
        morph_vertex_count,
    );
    same_offsets!(
        Material,
        MaterialUniforms,
        base_color,
        metallic_roughness,
        emissive,
        unlit,
        has_metallic_roughness_texture,
        has_emissive_texture,
        has_occlusion_texture,
        occlusion_strength,
        alpha_mode,
        alpha_cutoff,
        uv_transform_matrix,
        uv_transform_offset,
    );
    same_offsets!(Light, DirLight, direction, intensity, color);
}
