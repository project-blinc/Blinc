//! Render pipelines

use rustc_hash::FxHashMap;
use std::sync::Arc;
use wgpu;

/// Vertex layout configurations
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VertexLayout {
    /// Position only
    Position,
    /// Position + Normal
    PositionNormal,
    /// Position + Normal + UV
    PositionNormalUV,
    /// Position + Normal + UV + Tangent
    PositionNormalUVTangent,
    /// Position + Color
    PositionColor,
    /// Position + UV (for terrain and water)
    PositionUV,
}

impl VertexLayout {
    /// Get vertex buffer layout for this configuration
    pub fn buffer_layout(&self) -> wgpu::VertexBufferLayout<'static> {
        match self {
            VertexLayout::Position => wgpu::VertexBufferLayout {
                array_stride: 12,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                }],
            },
            VertexLayout::PositionNormal => wgpu::VertexBufferLayout {
                array_stride: 24,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 12,
                        shader_location: 1,
                    },
                ],
            },
            VertexLayout::PositionNormalUV => wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 12,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 24,
                        shader_location: 2,
                    },
                ],
            },
            VertexLayout::PositionNormalUVTangent => wgpu::VertexBufferLayout {
                array_stride: 48,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 12,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 24,
                        shader_location: 2,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 32,
                        shader_location: 3,
                    },
                ],
            },
            VertexLayout::PositionColor => wgpu::VertexBufferLayout {
                array_stride: 28,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 12,
                        shader_location: 1,
                    },
                ],
            },
            VertexLayout::PositionUV => wgpu::VertexBufferLayout {
                array_stride: 20,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 12,
                        shader_location: 1,
                    },
                ],
            },
        }
    }
}

/// Key for identifying pipelines
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    /// Shader ID
    pub shader: super::ShaderId,
    /// Vertex layout
    pub vertex_layout: VertexLayout,
    /// Blend mode
    pub blend: BlendConfig,
    /// Depth/stencil configuration
    pub depth_stencil: bool,
    /// Cull mode
    pub cull_mode: CullMode,
    /// Wireframe mode
    pub wireframe: bool,
}

/// Blend configuration
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendConfig {
    /// No blending (opaque)
    Opaque,
    /// Alpha blending
    Alpha,
    /// Additive blending
    Additive,
    /// Multiply blending
    Multiply,
}

/// Cull mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CullMode {
    /// No culling
    None,
    /// Cull front faces
    Front,
    /// Cull back faces
    Back,
}

impl From<CullMode> for Option<wgpu::Face> {
    fn from(mode: CullMode) -> Self {
        match mode {
            CullMode::None => None,
            CullMode::Front => Some(wgpu::Face::Front),
            CullMode::Back => Some(wgpu::Face::Back),
        }
    }
}

/// Game render pipelines
pub struct GamePipelines {
    /// Mesh basic pipeline (unlit)
    pub mesh_basic: Option<wgpu::RenderPipeline>,
    /// Mesh PBR pipeline
    pub mesh_pbr: Option<wgpu::RenderPipeline>,
    /// Mesh Phong pipeline
    pub mesh_phong: Option<wgpu::RenderPipeline>,
    /// SDF raymarch pipeline
    pub sdf_raymarch: Option<wgpu::RenderPipeline>,
    /// SDF 3D text pipeline
    pub sdf_text_3d: Option<wgpu::RenderPipeline>,
    /// SDF particles pipeline
    pub sdf_particles: Option<wgpu::RenderPipeline>,
    /// Skybox pipeline
    pub skybox: Option<wgpu::RenderPipeline>,
    /// Shadow map pipeline
    pub shadow_map: Option<wgpu::RenderPipeline>,
    /// Post-process pipeline
    pub post_process: Option<wgpu::RenderPipeline>,
    /// Terrain rendering pipeline
    pub terrain: Option<wgpu::RenderPipeline>,
    /// Water body rendering pipeline
    pub water: Option<wgpu::RenderPipeline>,
    /// Particle compute pipeline (simulation)
    pub particle_compute: Option<wgpu::ComputePipeline>,
    /// Particle render pipeline (drawing)
    pub particle_render: Option<wgpu::RenderPipeline>,
    /// Custom pipelines cache
    custom_pipelines: FxHashMap<PipelineKey, wgpu::RenderPipeline>,
}

impl GamePipelines {
    /// Create new game pipelines (uninitialized)
    pub fn new() -> Self {
        Self {
            mesh_basic: None,
            mesh_pbr: None,
            mesh_phong: None,
            sdf_raymarch: None,
            sdf_text_3d: None,
            sdf_particles: None,
            skybox: None,
            shadow_map: None,
            post_process: None,
            terrain: None,
            water: None,
            particle_compute: None,
            particle_render: None,
            custom_pipelines: FxHashMap::default(),
        }
    }

    /// Initialize all default pipelines
    pub fn init_defaults(
        &mut self,
        device: &wgpu::Device,
        registry: &super::ShaderRegistry,
        surface_format: wgpu::TextureFormat,
    ) {
        // Initialize mesh basic pipeline
        if let Some(shader) = registry.get(super::ShaderId::MeshBasic) {
            self.mesh_basic = Some(Self::create_mesh_pipeline(
                device,
                shader,
                surface_format,
                "mesh_basic",
                BlendConfig::Opaque,
            ));
        }

        // Initialize mesh PBR pipeline
        if let Some(shader) = registry.get(super::ShaderId::MeshPbr) {
            self.mesh_pbr = Some(Self::create_mesh_pipeline(
                device,
                shader,
                surface_format,
                "mesh_pbr",
                BlendConfig::Opaque,
            ));
        }

        // Initialize mesh Phong pipeline
        if let Some(shader) = registry.get(super::ShaderId::MeshPhong) {
            self.mesh_phong = Some(Self::create_mesh_pipeline(
                device,
                shader,
                surface_format,
                "mesh_phong",
                BlendConfig::Opaque,
            ));
        }

        // Initialize skybox pipeline
        if let Some(shader) = registry.get(super::ShaderId::Skybox) {
            self.skybox = Some(Self::create_fullscreen_pipeline(
                device,
                shader,
                surface_format,
                "skybox",
            ));
        }

        // Initialize SDF raymarch pipeline
        if let Some(shader) = registry.get(super::ShaderId::SdfRaymarch) {
            self.sdf_raymarch = Some(Self::create_fullscreen_pipeline(
                device,
                shader,
                surface_format,
                "sdf_raymarch",
            ));
        }

        // Initialize terrain pipeline
        if let Some(shader) = registry.get(super::ShaderId::Terrain) {
            self.terrain = Some(Self::create_terrain_pipeline(
                device,
                shader,
                surface_format,
            ));
        }

        // Initialize water pipeline
        if let Some(shader) = registry.get(super::ShaderId::Water) {
            self.water = Some(Self::create_water_pipeline(
                device,
                shader,
                surface_format,
            ));
        }

        // Initialize particle compute pipeline
        if let Some(shader) = registry.get(super::ShaderId::ParticleCompute) {
            self.particle_compute = Some(Self::create_particle_compute_pipeline(
                device,
                shader,
            ));
        }

        // Initialize particle render pipeline
        if let Some(shader) = registry.get(super::ShaderId::ParticleRender) {
            self.particle_render = Some(Self::create_particle_render_pipeline(
                device,
                shader,
                surface_format,
            ));
        }
    }

    /// Create a mesh rendering pipeline
    fn create_mesh_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        surface_format: wgpu::TextureFormat,
        label: &str,
        blend: BlendConfig,
    ) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{}_layout", label)),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let blend_state = match blend {
            BlendConfig::Opaque => None,
            BlendConfig::Alpha => Some(wgpu::BlendState::ALPHA_BLENDING),
            BlendConfig::Additive => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
            BlendConfig::Multiply => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Dst,
                    dst_factor: wgpu::BlendFactor::Zero,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
        };

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[VertexLayout::PositionNormalUV.buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: blend_state,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Create a fullscreen pipeline (for post-processing, skybox, SDF)
    fn create_fullscreen_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        surface_format: wgpu::TextureFormat,
        label: &str,
    ) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{}_layout", label)),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Create terrain rendering pipeline
    fn create_terrain_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        surface_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain_layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[VertexLayout::PositionUV.buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Create water rendering pipeline
    fn create_water_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        surface_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water_layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[VertexLayout::PositionUV.buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Water is visible from both sides
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false, // Water doesn't write depth (transparent)
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Create particle compute pipeline (simulation)
    fn create_particle_compute_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
    ) -> wgpu::ComputePipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle_compute_layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("particle_compute"),
            layout: Some(&layout),
            module: shader,
            entry_point: Some("cs_update"),
            compilation_options: Default::default(),
            cache: None,
        })
    }

    /// Create particle render pipeline (billboard drawing)
    fn create_particle_render_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        surface_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle_render_layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particle_render"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[], // Particles are generated from storage buffer
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Billboards visible from both sides
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false, // Particles are transparent
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Get or create a custom pipeline
    pub fn get_or_create(
        &mut self,
        device: &wgpu::Device,
        registry: &super::ShaderRegistry,
        surface_format: wgpu::TextureFormat,
        key: PipelineKey,
    ) -> Option<&wgpu::RenderPipeline> {
        if !self.custom_pipelines.contains_key(&key) {
            if let Some(shader) = registry.get(key.shader) {
                let pipeline = Self::create_mesh_pipeline(
                    device,
                    shader,
                    surface_format,
                    &format!("custom_{:?}", key.shader),
                    key.blend,
                );
                self.custom_pipelines.insert(key.clone(), pipeline);
            }
        }
        self.custom_pipelines.get(&key)
    }
}

impl Default for GamePipelines {
    fn default() -> Self {
        Self::new()
    }
}
